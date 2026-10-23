//! Self-update from this repo's GitHub releases (docs/updates.md).
//!
//! Rust only: the webview has no updater permission. It calls the commands in
//! commands.rs. The Tauri updater plugin checks `latest.json`, verifies the
//! download against the public key in tauri.conf.json (including the signed
//! version), and swaps the .app bundle, asking for the password only when
//! /Applications isn't writable.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::{Error as UpError, Update, UpdaterExt};
use tokio::sync::Mutex;

/// Settings keys (store.rs settings table).
pub const AUTO_CHECK: &str = "auto_update_check";
pub const LAST_CHECK: &str = "last_update_check";

const PLACEHOLDER_KEY: &str = "REPLACE_WITH_UPDATER_PUBLIC_KEY";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    pub version: String,
    pub notes: Option<String>,
    /// RFC 3339, from the release.
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum UpdateEvent {
    Downloading { received: u64, total: Option<u64> },
    Installing,
    Installed { version: String },
}

/// The update found by the last check, kept for Install.
#[derive(Default)]
pub struct Updates {
    pending: Mutex<Option<Update>>,
    busy: std::sync::atomic::AtomicBool,
}

/// Why this build can't update itself, if it can't.
pub fn unsupported_reason<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    if cfg!(debug_assertions) {
        return Some("Development builds don't update themselves. Updates work in the installed app.".into());
    }
    let key = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .unwrap_or_default();
    if key.is_empty() || key == PLACEHOLDER_KEY {
        return Some("This build has no update key, so it can't check for signed updates.".into());
    }
    match std::env::current_exe() {
        Ok(p) if p.to_string_lossy().contains(".app/Contents/MacOS/") => None,
        _ => Some("Updates work only when the app runs from its .app bundle.".into()),
    }
}

impl Updates {
    /// Ask GitHub for the latest release. Returns it if it is newer.
    pub async fn check<R: Runtime>(&self, app: &AppHandle<R>) -> Result<Option<Available>, String> {
        if let Some(why) = unsupported_reason(app) {
            return Err(why);
        }
        let updater = app
            .updater_builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| plain(&e))?;
        let found = updater.check().await.map_err(|e| plain(&e))?;
        let info = found.as_ref().map(|u| Available {
            version: u.version.clone(),
            notes: u.body.clone().filter(|b| !b.trim().is_empty()),
            date: u.date.and_then(|d| d.format(&time::format_description::well_known::Rfc3339).ok()),
        });
        *self.pending.lock().await = found;
        Ok(info)
    }

    /// Download, verify and install the update found by the last check.
    /// The new version runs after a restart.
    pub async fn install<R: Runtime>(&self, app: &AppHandle<R>, on: &(dyn Fn(UpdateEvent) + Sync)) -> Result<String, String> {
        use std::sync::atomic::Ordering;
        if let Some(why) = unsupported_reason(app) {
            return Err(why);
        }
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err("An update is already being installed.".into());
        }
        let result = self.install_inner(on).await;
        self.busy.store(false, Ordering::SeqCst);
        result
    }

    async fn install_inner(&self, on: &(dyn Fn(UpdateEvent) + Sync)) -> Result<String, String> {
        let update = self.pending.lock().await.clone().ok_or("Check for updates first.")?;
        let mut received = 0u64;
        let mut reported = 0u64;
        // `download` verifies the signature (and the signed version) before it returns.
        let bytes = update
            .download(
                |chunk, total| {
                    received += chunk as u64;
                    // About 100 progress events per download.
                    let step = total.map_or(256 * 1024, |t| (t / 100).max(1));
                    if received - reported >= step || total == Some(received) {
                        reported = received;
                        on(UpdateEvent::Downloading { received, total });
                    }
                },
                || {},
            )
            .await
            .map_err(|e| plain(&e))?;
        on(UpdateEvent::Installing);
        let version = update.version.clone();
        // Install may ask for the password on the main thread, so it must not run there.
        tokio::task::spawn_blocking(move || update.install(bytes))
            .await
            .map_err(|e| format!("The installer stopped: {e}"))?
            .map_err(|e| plain(&e))?;
        *self.pending.lock().await = None;
        on(UpdateEvent::Installed { version: version.clone() });
        Ok(version)
    }
}

/// Updater errors in plain words.
fn plain(e: &UpError) -> String {
    match e {
        UpError::ReleaseNotFound => "No release found on GitHub yet, or GitHub couldn't be reached.".into(),
        UpError::Reqwest(_) | UpError::Network(_) => format!("Couldn't reach GitHub. Are you online? ({e})"),
        UpError::Minisign(_) | UpError::Base64(_) | UpError::SignatureUtf8(_) => {
            "The update didn't pass the signature check, so nothing was installed.".into()
        }
        UpError::SignedVersionMismatch { .. } | UpError::MissingSignedVersion => {
            "The update's signature is for a different version, so nothing was installed.".into()
        }
        UpError::TargetNotFound(_) | UpError::TargetsNotFound(_) => {
            "The latest release has no update for this Mac (Apple silicon).".into()
        }
        UpError::AuthenticationFailed => "The password prompt was cancelled. Nothing was changed.".into(),
        UpError::PreviousAppNotRestored(p) => {
            format!("The update failed and the old app couldn't be put back. It was left at {}.", p.display())
        }
        other => format!("The update failed: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_errors_say_nothing_was_installed() {
        let e = UpError::MissingSignedVersion;
        assert!(plain(&e).contains("nothing was installed"));
        assert!(plain(&UpError::ReleaseNotFound).contains("No release"));
    }
}
