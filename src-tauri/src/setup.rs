//! Setup assistant: checks everything the app needs on this Mac and installs
//! or upgrades what's missing. Every action is started by the user from the
//! Setup view; the admin password goes through macOS's own dialog.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::vm::{self, Vm, VmState};

/// Oldest Apple container version this app is tested with.
pub const CONTAINER_MIN: (u32, u32, u32) = (1, 6, 0);
const RELEASES_API: &str = "https://api.github.com/repos/apple/container/releases/latest";
/// Hosts the release check and download may touch (GitHub redirects downloads).
const ALLOWED_HOSTS: [&str; 4] =
    ["api.github.com", "github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com"];
/// Apple's Developer ID team for Containerization. The pkg must be signed by it.
const APPLE_TEAM: &str = "UPBK2H6LZM";
const RELEASE_CACHE: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepState {
    Ok,
    Missing,
    Outdated,
    Stopped,
    Unsupported,
    Warning,
    Skipped,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: &'static str,
    pub title: &'static str,
    pub state: StepState,
    pub detail: String,
    /// What the UI's button does: "install-container", "upgrade-container",
    /// "start-system", "build-image", "start-vm", "selftest".
    pub action: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfTest {
    pub at: i64,
    pub passed: bool,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub app_version: &'static str,
    pub steps: Vec<Step>,
    pub latest_container: Option<Release>,
    /// True when a step the chosen backend needs isn't ok.
    pub needs_attention: bool,
}

#[derive(Default)]
pub struct Setup {
    release: Mutex<Option<(Instant, Release)>>,
}

impl Setup {
    /// Latest Apple container release, cached for an hour.
    pub async fn latest_release(&self) -> Result<Release, String> {
        if let Some((at, r)) = self.release.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
            && at.elapsed() < RELEASE_CACHE
        {
            return Ok(r.clone());
        }
        let v: serde_json::Value = github_client()?
            .get(RELEASES_API)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| format!("Couldn't reach GitHub: {e}"))?
            .error_for_status()
            .map_err(|e| format!("GitHub said: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Unexpected reply from GitHub: {e}"))?;
        let version = v["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
        let asset = v["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with("-installer-signed.pkg")))
            .ok_or("The latest release has no signed installer.")?;
        let r = Release {
            version,
            name: asset["name"].as_str().unwrap_or("").to_string(),
            url: asset["browser_download_url"].as_str().unwrap_or("").to_string(),
            size: asset["size"].as_u64().unwrap_or(0),
            sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
        };
        if r.url.is_empty() || r.size == 0 || vm::parse_version(&r.version).is_none() {
            return Err("The latest release looks incomplete.".into());
        }
        *self.release.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), r.clone()));
        Ok(r)
    }

    pub async fn report(&self, vm: &Vm, use_vm: bool, selftest: Option<&SelfTest>) -> Report {
        let mut steps = Vec::new();
        let skip = |id, title| Step { id, title, state: StepState::Skipped, detail: "Tools use the macOS sandbox. Switch to the Linux VM in Settings to use this.".into(), action: None };

        // 1. This Mac
        let macos = run("/usr/bin/sw_vers", &["-productVersion"]);
        let major: u32 = macos.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
        let arm = std::env::consts::ARCH == "aarch64";
        let mac_ok = arm && major >= 26;
        steps.push(Step {
            id: "mac",
            title: "This Mac",
            state: if mac_ok { StepState::Ok } else { StepState::Unsupported },
            detail: if mac_ok {
                format!("Apple silicon, macOS {macos}.")
            } else {
                format!(
                    "The sandbox VM needs Apple silicon and macOS 26 or newer. This Mac: {}, macOS {macos}. Chat works; use the macOS sandbox for tools.",
                    if arm { "Apple silicon" } else { "Intel" }
                )
            },
            action: None,
        });

        if !use_vm || !mac_ok {
            for (id, title) in [("container", "Apple container"), ("system", "Container system"), ("image", "Sandbox image"), ("vm", "Sandbox VM"), ("selftest", "Isolation check")] {
                steps.push(skip(id, title));
            }
            return Report { app_version: env!("CARGO_PKG_VERSION"), steps, latest_container: None, needs_attention: use_vm && !mac_ok };
        }

        // 2. Apple container
        let installed = vm::container_version().await;
        let latest = self.latest_release().await;
        let latest_v = latest.as_ref().ok().and_then(|r| vm::parse_version(&r.version));
        let fmt = |v: (u32, u32, u32)| format!("{}.{}.{}", v.0, v.1, v.2);
        let (state, detail, action) = match (installed, latest_v) {
            (None, _) => (
                StepState::Missing,
                match &latest {
                    Ok(r) => format!("Not installed. Apple's latest is {} ({} MB, signed by Apple).", r.version, (r.size + 500_000) / 1_000_000),
                    Err(e) => format!("Not installed. {e}"),
                },
                Some("install-container"),
            ),
            (Some(v), _) if v < CONTAINER_MIN => (
                StepState::Outdated,
                format!("Version {} is older than this app needs ({}).", fmt(v), fmt(CONTAINER_MIN)),
                Some("upgrade-container"),
            ),
            (Some(v), Some(l)) if v < l => (
                StepState::Outdated,
                format!("Version {} installed. {} is available.", fmt(v), fmt(l)),
                Some("upgrade-container"),
            ),
            (Some(v), Some(_)) => (StepState::Ok, format!("Version {}, the latest.", fmt(v)), None),
            (Some(v), None) => (
                StepState::Warning,
                format!("Version {} installed. Couldn't check for updates: {}", fmt(v), latest.as_ref().err().cloned().unwrap_or_default()),
                None,
            ),
        };
        let container_ok = installed.is_some_and(|v| v >= CONTAINER_MIN);
        steps.push(Step { id: "container", title: "Apple container", state, detail, action });

        // 3..5 follow from the VM status.
        let status = if installed.is_some() { Some(vm.status().await) } else { None };
        let st = status.as_ref().map(|s| s.state);
        let system_ok = matches!(st, Some(VmState::NoImage | VmState::Stopped | VmState::Running));
        steps.push(Step {
            id: "system",
            title: "Container system",
            state: if system_ok { StepState::Ok } else if installed.is_some() { StepState::Stopped } else { StepState::Missing },
            detail: if system_ok { "Running, kernel installed.".into() } else { "Not running. Starting it also installs the Linux kernel the first time.".into() },
            action: (installed.is_some() && !system_ok).then_some("start-system"),
        });

        let tag = vm::image_tag();
        let tags = if system_ok { vm::sandbox_image_tags().await } else { Vec::new() };
        let current = tag.rsplit(':').next().unwrap_or("").to_string();
        let image_ok = system_ok && tags.contains(&current);
        let old: Vec<&String> = tags.iter().filter(|t| **t != current).collect();
        steps.push(Step {
            id: "image",
            title: "Sandbox image",
            state: if image_ok { if old.is_empty() { StepState::Ok } else { StepState::Warning } } else if old.is_empty() { StepState::Missing } else { StepState::Outdated },
            detail: if image_ok {
                if old.is_empty() { format!("Built for this version ({current}).") } else { format!("Built ({current}). {} older image(s) can be removed.", old.len()) }
            } else if old.is_empty() {
                "Not built yet. The first build downloads Debian, Python, Node and Chromium: about 2 minutes, 1 GB.".into()
            } else {
                "This app version needs a new image. Building it reuses most of the old one.".into()
            },
            action: (system_ok && (!image_ok || !old.is_empty())).then_some("build-image"),
        });

        let vm_ok = st == Some(VmState::Running);
        steps.push(Step {
            id: "vm",
            title: "Sandbox VM",
            state: if vm_ok { StepState::Ok } else { StepState::Stopped },
            detail: if vm_ok {
                format!("Running. Firewall: {}.", match status.as_ref().and_then(|s| s.network.as_deref()) {
                    Some(n) if n.starts_with("on limit=") => format!("internet only, max {} new connections/s per site", &n[9..]),
                    Some("on") => "internet only".into(),
                    Some("off") => "no network".into(),
                    Some(o) => o.to_string(),
                    None => "unknown".into(),
                })
            } else {
                "Stopped. It also starts by itself on the first tool call.".into()
            },
            action: (image_ok && !vm_ok).then_some("start-vm"),
        });

        steps.push(Step {
            id: "selftest",
            title: "Isolation check",
            state: match selftest {
                Some(t) if t.passed => StepState::Ok,
                Some(_) => StepState::Warning,
                None => StepState::Missing,
            },
            detail: match selftest {
                Some(t) if t.passed => format!("All {} checks passed on {}.", t.checks.len(), crate::clock::short_stamp(t.at)),
                Some(t) => format!("{} of {} checks failed. Run it again, or look at the details.", t.checks.iter().filter(|c| !c.passed).count(), t.checks.len()),
                None => "Not run yet. Checks inside the VM that your Mac, home network and tailnet are out of reach.".into(),
            },
            action: vm_ok.then_some("selftest"),
        });

        let needs_attention = !container_ok || !system_ok || !image_ok;
        Report { app_version: env!("CARGO_PKG_VERSION"), steps, latest_container: latest.ok(), needs_attention }
    }

    /// Download, verify and install (or upgrade) Apple container. macOS asks for
    /// the admin password. Progress lines go to `say`.
    pub async fn install_container(&self, downloads: &Path, say: &(dyn Fn(&str) + Sync)) -> Result<String, String> {
        let r = self.latest_release().await?;
        let new_v = vm::parse_version(&r.version).ok_or("Bad release version.")?;
        let old_v = vm::container_version().await;
        if old_v.is_some_and(|v| v >= new_v) {
            return Ok(format!("Apple container {} is already installed.", r.version));
        }
        let file = self.fetch_and_verify(&r, downloads, say).await?;

        if old_v.is_some() {
            say("Stopping the container system for the upgrade…");
            let _ = vm::cli(&["system", "stop"], None, Duration::from_secs(120)).await;
        }
        say("Installing. macOS will ask for your password…");
        let res = admin_install(&file).await;
        let _ = std::fs::remove_file(&file);
        res?;

        say("Starting the container system (first start installs the Linux kernel)…");
        let o = vm::cli(&["system", "start", "--enable-kernel-install"], None, Duration::from_secs(600)).await?;
        if !o.ok() && !vm::system_running().await {
            return Err(format!("Installed, but the container system didn't start: {}", o.reason()));
        }
        let now = vm::container_version().await.map(|v| format!("{}.{}.{}", v.0, v.1, v.2)).unwrap_or_default();
        Ok(format!("Apple container {now} is installed and running."))
    }
}

impl Setup {
    /// Download the release's installer and verify it. Deletes it if any check fails.
    pub async fn fetch_and_verify(&self, r: &Release, downloads: &Path, say: &(dyn Fn(&str) + Sync)) -> Result<PathBuf, String> {
        let expected = r.sha256.clone().ok_or("GitHub gave no checksum for the installer, so it won't be installed.")?;
        std::fs::create_dir_all(downloads).map_err(|e| e.to_string())?;
        let file = downloads.join(safe_name(&r.name));
        say(&format!("Downloading {} ({} MB) from Apple's GitHub release…", r.name, (r.size + 500_000) / 1_000_000));
        if let Err(e) = download(&r.url, &file, r.size, say).await {
            let _ = std::fs::remove_file(&file);
            return Err(e);
        }
        say("Checking the download…");
        if let Err(e) = verify_pkg(&file, &expected).await {
            let _ = std::fs::remove_file(&file);
            return Err(e);
        }
        say("Verified: checksum matches, signed by Apple (Containerization, UPBK2H6LZM), notarized.");
        Ok(file)
    }
}

/// HTTPS client limited to GitHub hosts, following only redirects that stay on them.
fn github_client() -> Result<reqwest::Client, String> {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        let ok = attempt.url().scheme() == "https"
            && attempt.url().host_str().is_some_and(|h| ALLOWED_HOSTS.contains(&h))
            && attempt.previous().len() < 5;
        if ok { attempt.follow() } else { attempt.stop() }
    });
    reqwest::Client::builder()
        .https_only(true)
        .redirect(policy)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(60))
        .user_agent(concat!("ruans-harness/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
}

async fn download(url: &str, to: &Path, expected_size: u64, say: &(dyn Fn(&str) + Sync)) -> Result<(), String> {
    let host_ok = url::Url::parse(url).ok().and_then(|u| u.host_str().map(|h| ALLOWED_HOSTS.contains(&h))).unwrap_or(false);
    if !host_ok {
        return Err("The installer URL isn't on GitHub.".into());
    }
    let mut resp = github_client()?.get(url).send().await.map_err(|e| format!("Download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }
    let mut f = tokio::fs::File::create(to).await.map_err(|e| e.to_string())?;
    let (mut got, mut last_pct) = (0u64, 0u64);
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("Download broke off: {e}"))? {
        got += chunk.len() as u64;
        if got > expected_size + 1024 {
            return Err("The download is bigger than GitHub said. Stopped.".into());
        }
        f.write_all(&chunk).await.map_err(|e| e.to_string())?;
        let pct = got * 100 / expected_size.max(1);
        if pct >= last_pct + 10 {
            last_pct = pct - pct % 10;
            say(&format!("Downloaded {last_pct}%"));
        }
    }
    f.flush().await.map_err(|e| e.to_string())?;
    if got != expected_size {
        return Err(format!("The download is incomplete ({got} of {expected_size} bytes)."));
    }
    Ok(())
}

/// Checksum, Apple signature with the pinned team, notarization, Gatekeeper.
pub async fn verify_pkg(file: &Path, expected_sha256: &str) -> Result<(), String> {
    let path = file.to_path_buf();
    let sha = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        use std::io::Read;
        let mut f = std::fs::File::open(path)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    if !sha.eq_ignore_ascii_case(expected_sha256) {
        return Err("The checksum doesn't match GitHub's. The file was not installed.".into());
    }
    let p = file.display().to_string();
    let sig = run("/usr/sbin/pkgutil", &["--check-signature", &p]);
    let signed = sig.contains("signed by a developer certificate issued by Apple")
        && sig.contains("Notarization: trusted by the Apple notary service")
        && sig.lines().any(|l| l.contains("Developer ID Installer: Apple Inc.") && l.contains(APPLE_TEAM));
    if !signed {
        return Err("The installer isn't signed and notarized by Apple's Containerization team. It was not installed.".into());
    }
    let gk = std::process::Command::new("/usr/sbin/spctl")
        .args(["--assess", "--type", "install", "-vv", &p])
        .output()
        .map_err(|e| e.to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&gk.stdout), String::from_utf8_lossy(&gk.stderr));
    if !gk.status.success() || !text.contains("accepted") || !text.contains("Notarized Developer ID") {
        return Err("Gatekeeper didn't accept the installer. It was not installed.".into());
    }
    Ok(())
}

/// Run Apple's installer with admin rights, through macOS's own password dialog.
async fn admin_install(file: &Path) -> Result<(), String> {
    let p = applescript_string(&file.display().to_string());
    let script = format!(
        "do shell script \"/usr/sbin/installer -pkg \" & quoted form of {p} & \" -target /\" \
         with prompt \"Ruan's Harness wants to install Apple container, the VM runtime for its sandbox.\" \
         with administrator privileges"
    );
    let out = tokio::process::Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .await
        .map_err(|e| format!("Could not start the installer: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    if err.contains("-128") || err.to_lowercase().contains("cancel") {
        return Err("Cancelled. Nothing was installed.".into());
    }
    Err(format!("The installer failed: {}", err.trim().chars().take(300).collect::<String>()))
}

/// An AppleScript string literal.
fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn safe_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
    if s.ends_with(".pkg") { s } else { "container-installer.pkg".into() }
}

fn run(cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .map(|o| format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)).trim().to_string())
        .unwrap_or_default()
}

/// Where downloads go before they're checked and installed.
pub fn downloads_dir(data: &Path) -> PathBuf {
    data.join("downloads")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_and_names() {
        assert_eq!(applescript_string(r#"/a "b"\c"#), r#""/a \"b\"\\c""#);
        assert_eq!(safe_name("container-1.6.0-installer-signed.pkg"), "container-1.6.0-installer-signed.pkg");
        assert_eq!(safe_name("../../evil.sh"), "container-installer.pkg");
        assert_eq!(safe_name("a b;rm.pkg"), "abrm.pkg");
    }

    #[tokio::test]
    async fn rejects_bad_checksum_and_unsigned_files() {
        let dir = std::env::temp_dir().join(format!("qh-setup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("fake.pkg");
        std::fs::write(&f, b"not a package").unwrap();
        let sha: String = Sha256::digest(b"not a package").iter().map(|b| format!("{b:02x}")).collect();
        assert!(verify_pkg(&f, "00").await.unwrap_err().contains("checksum"));
        assert!(verify_pkg(&f, &sha).await.unwrap_err().contains("signed"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
