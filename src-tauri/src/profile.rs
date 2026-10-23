use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Result, msg};
use crate::secrets::KeyRef;

/// Sampling fields use the request body names (snake_case).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sampling {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub min_p: f32,
    pub presence_penalty: f32,
}

impl Sampling {
    /// Qwen authors' non-thinking preset (the model card).
    pub fn chat() -> Self {
        Self { temperature: 0.7, top_p: 0.8, top_k: 20, min_p: 0.0, presence_penalty: 1.5 }
    }

    /// Qwen authors' thinking preset, used while Think is on.
    pub fn think() -> Self {
        Self { temperature: 1.0, top_p: 0.95, top_k: 20, min_p: 0.0, presence_penalty: 1.5 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_url: Option<String>,
    /// `keychain:service/account` or `env:NAME`. Never the key itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_ref: Option<String>,
    pub model: String,
    pub context_tokens: u32,
    pub health_path: String,
    /// Off by default: only loopback, LAN and tailnet hosts are allowed.
    #[serde(default)]
    pub allow_public: bool,
    /// Shown as the model's tooltip, for example the exact GGUF file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_info: Option<String>,
    pub sampling: Sampling,
}

impl Profile {
    /// First-run profile: llama.cpp's llama-server on this Mac with its
    /// defaults, started with `--alias local` (see docs/getting-started.md).
    pub fn local_default() -> Self {
        Self {
            id: "local".into(),
            name: "Local llama-server".into(),
            base_url: "http://127.0.0.1:8080/v1".into(),
            fallback_url: None,
            key_ref: None,
            model: "local".into(),
            context_tokens: 32768,
            health_path: "/health".into(),
            allow_public: false,
            model_info: None,
            sampling: Sampling::chat(),
        }
    }

    /// Trim and check every field. Called before anything is saved or used.
    pub fn validate(mut self) -> Result<Self> {
        self.id = self.id.trim().to_string();
        if self.id.is_empty()
            || self.id.len() > 64
            || !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(msg("Profile id must be 1 to 64 letters, digits or dashes."));
        }

        self.name = self.name.trim().to_string();
        if self.name.is_empty() || self.name.chars().count() > 80 {
            return Err(msg("Name must be 1 to 80 characters."));
        }

        self.base_url = clean_url(&self.base_url, "Base URL")?;
        self.fallback_url = match self.fallback_url.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(u) => Some(clean_url(u, "Fallback URL")?),
        };

        self.key_ref = match self.key_ref.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(k) => {
                KeyRef::parse(k)?;
                Some(k.to_string())
            }
        };

        self.model = self.model.trim().to_string();
        if self.model.is_empty() || self.model.len() > 128 {
            return Err(msg("Model id must be 1 to 128 characters."));
        }

        if !(512..=2_000_000).contains(&self.context_tokens) {
            return Err(msg("Context size must be between 512 and 2,000,000 tokens."));
        }

        self.model_info = match self.model_info.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(s) if s.chars().count() > 200 => return Err(msg("Model description must be under 200 characters.")),
            Some(s) => Some(s.to_string()),
        };

        self.health_path = self.health_path.trim().to_string();
        if !self.health_path.starts_with('/') || self.health_path.len() > 128 {
            return Err(msg("Health path must start with / and be under 128 characters."));
        }

        let s = &self.sampling;
        if !(0.0..=2.0).contains(&s.temperature)
            || !(0.0..=1.0).contains(&s.top_p)
            || !(0.0..=1.0).contains(&s.min_p)
            || !(-2.0..=2.0).contains(&s.presence_penalty)
            || s.top_k > 1000
        {
            return Err(msg("A sampling value is out of range."));
        }

        Ok(self)
    }
}

/// http or https only, no credentials, query or fragment. Trailing slash removed.
fn clean_url(raw: &str, label: &str) -> Result<String> {
    let url = Url::parse(raw.trim()).map_err(|e| msg(format!("{label}: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(msg(format!("{label} must start with http:// or https://")));
    }
    if url.host().is_none() {
        return Err(msg(format!("{label} needs a host.")));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(msg(format!("{label} must not contain a user or password. Use the key field.")));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(msg(format!("{label} must not have a query or fragment.")));
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_is_valid() {
        let p = Profile::local_default().validate().unwrap();
        assert_eq!(p.base_url, "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn rejects_credentials_in_url() {
        let mut p = Profile::local_default();
        p.base_url = "http://user:pw@host:8081/v1".into();
        assert!(p.validate().is_err());
    }

    #[test]
    fn rejects_bad_key_ref() {
        let mut p = Profile::local_default();
        p.key_ref = Some("sk-plain-key".into());
        assert!(p.validate().is_err());
    }
}
