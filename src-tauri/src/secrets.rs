//! Resolves key references to values. Values stay in Rust memory, are zeroed on
//! drop, and are never logged, serialized or returned to the webview.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use zeroize::{Zeroize, Zeroizing};

use crate::error::{Result, msg};

/// The only Keychain service the app may write to.
pub const KEYCHAIN_SERVICE: &str = "qwen35-harness";

const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn char_count(&self) -> usize {
        self.0.chars().count()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeyRef {
    Env(String),
    Keychain { service: String, account: String },
}

impl KeyRef {
    pub fn parse(raw: &str) -> Result<Self> {
        let s = raw.trim();
        if s.len() > 512 || s.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(msg("Key reference is too long or has spaces."));
        }
        if s.starts_with("op://") {
            return Err(msg("1Password references are no longer supported. Save the key in the Keychain in Settings."));
        }
        if let Some(name) = s.strip_prefix("env:") {
            let mut chars = name.chars();
            let ok_first = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
            if !ok_first || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') || name.len() > 128 {
                return Err(msg("env: needs a variable name such as env:QWEN35_HARNESS_KEY."));
            }
            return Ok(Self::Env(name.to_string()));
        }
        if let Some(rest) = s.strip_prefix("keychain:") {
            return match rest.split_once('/') {
                Some((service, account))
                    if !service.is_empty()
                        && !account.is_empty()
                        && service.len() <= 128
                        && account.len() <= 128 =>
                {
                    Ok(Self::Keychain { service: service.into(), account: account.into() })
                }
                _ => Err(msg("Keychain reference must look like keychain:service/account.")),
            };
        }
        Err(msg("Key reference must start with env: or keychain:"))
    }

    pub fn source(&self) -> &'static str {
        match self {
            Self::Env(_) => "Environment",
            Self::Keychain { .. } => "Keychain",
        }
    }
}

/// Resolved keys, cached so the Keychain isn't asked on every request.
#[derive(Default)]
pub struct SecretCache {
    map: Mutex<HashMap<String, Arc<Secret>>>,
}

impl SecretCache {
    pub async fn get(&self, raw_ref: &str, env_file: &Path) -> Result<Arc<Secret>> {
        if let Some(s) = self.lock().get(raw_ref) {
            return Ok(s.clone());
        }
        let secret = Arc::new(resolve(&KeyRef::parse(raw_ref)?, env_file).await?);
        self.lock().insert(raw_ref.to_string(), secret.clone());
        Ok(secret)
    }

    pub fn forget(&self, raw_ref: &str) {
        self.lock().remove(raw_ref);
    }

    pub fn clear(&self) {
        self.lock().clear();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<Secret>>> {
        // A poisoned map only holds cached keys; start over rather than panic.
        self.map.lock().unwrap_or_else(|e| {
            let mut g = e.into_inner();
            g.clear();
            g
        })
    }
}

pub async fn resolve(key: &KeyRef, env_file: &Path) -> Result<Secret> {
    match key {
        KeyRef::Env(name) => {
            let value = std::env::var(name)
                .ok()
                .filter(|v| !v.is_empty())
                .or_else(|| read_env_file(env_file, name))
                .map(Zeroizing::new)
                .ok_or_else(|| {
                    msg(format!(
                        "{name} is not set in the app environment or in {}.",
                        env_file.display()
                    ))
                })?;
            if value.starts_with("op://") {
                return Err(msg(format!("{name} holds a 1Password reference, which is no longer supported. Use the Keychain.")));
            }
            Ok(Secret(value))
        }
        KeyRef::Keychain { service, account } => {
            let (service, account) = (service.clone(), account.clone());
            tokio::task::spawn_blocking(move || keychain_get(&service, &account))
                .await
                .map_err(|e| msg(format!("Keychain task failed: {e}")))?
        }
    }
}

fn keychain_get(service: &str, account: &str) -> Result<Secret> {
    use security_framework::passwords::get_generic_password;
    match get_generic_password(service, account) {
        Ok(bytes) => {
            let value = String::from_utf8(bytes).map_err(|e| {
                let mut b = e.into_bytes();
                b.zeroize();
                msg("Keychain item is not text.")
            })?;
            Ok(Secret(Zeroizing::new(value)))
        }
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => {
            Err(msg(format!("No Keychain item for {service}/{account}. Save the key in Settings.")))
        }
        Err(e) => Err(msg(format!("Keychain error {}.", e.code()))),
    }
}

pub fn keychain_set(account: &str, value: &str) -> Result<()> {
    security_framework::passwords::set_generic_password(KEYCHAIN_SERVICE, account, value.as_bytes())
        .map_err(|e| msg(format!("Could not save to Keychain (error {}).", e.code())))
}

pub fn keychain_delete(account: &str) -> Result<()> {
    match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, account) {
        Ok(()) => Ok(()),
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
        Err(e) => Err(msg(format!("Could not remove from Keychain (error {}).", e.code()))),
    }
}

/// Minimal .env reader: KEY=VALUE, optional `export`, quotes, # comments.
fn read_env_file(path: &Path, name: &str) -> Option<String> {
    let text = Zeroizing::new(std::fs::read_to_string(path).ok()?);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((k, v)) = line.split_once('=') else { continue };
        if k.trim() != name {
            continue;
        }
        let v = v.trim();
        let v = v
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(v);
        return (!v.is_empty()).then(|| v.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_refs() {
        assert!(KeyRef::parse("op://v/i/f").unwrap_err().to_string().contains("Keychain"));
        assert_eq!(KeyRef::parse("env:QWEN_KEY").unwrap(), KeyRef::Env("QWEN_KEY".into()));
        assert_eq!(
            KeyRef::parse("keychain:svc/acct").unwrap(),
            KeyRef::Keychain { service: "svc".into(), account: "acct".into() }
        );
        assert!(KeyRef::parse("env:1BAD").is_err());
        assert!(KeyRef::parse("env:A B").is_err());
        assert!(KeyRef::parse("keychain:noslash").is_err());
        assert!(KeyRef::parse("sk-raw-key").is_err());
    }

    #[test]
    fn debug_is_masked() {
        let s = Secret(Zeroizing::new("sk-very-secret".into()));
        assert_eq!(format!("{s:?}"), "Secret(***)");
    }

    #[test]
    fn reads_env_file() {
        let dir = std::env::temp_dir().join(format!("qh-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join(".env");
        std::fs::write(&f, "# c\nexport A=\"one\"\nB='two'\nC=three\nD=\n").unwrap();
        assert_eq!(read_env_file(&f, "A").as_deref(), Some("one"));
        assert_eq!(read_env_file(&f, "B").as_deref(), Some("two"));
        assert_eq!(read_env_file(&f, "C").as_deref(), Some("three"));
        assert_eq!(read_env_file(&f, "D"), None);
        assert_eq!(read_env_file(&f, "E"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn env_ref_resolves_from_process_env() {
        // SAFETY: test-only, unique variable name.
        unsafe { std::env::set_var("QH_TEST_KEY_123", "abc") };
        let s = resolve(&KeyRef::Env("QH_TEST_KEY_123".into()), Path::new("/nonexistent")).await.unwrap();
        assert_eq!(s.expose(), "abc");
        let missing = resolve(&KeyRef::Env("QH_TEST_MISSING_987".into()), Path::new("/nonexistent")).await;
        assert!(missing.is_err());
    }
}
