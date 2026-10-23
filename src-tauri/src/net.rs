//! Reachability and key checks for a profile (see docs/architecture.md, "Connection check").

use std::error::Error as _;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::profile::Profile;
use crate::secrets::{KeyRef, SecretCache};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const DNS_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Host name does not resolve (Tailscale down, typo).
    DnsFailed,
    /// No TCP connection (machine off, port closed).
    Unreachable,
    /// TCP accepted then closed: the server is starting.
    Starting,
    /// `/health` is 503: model loading.
    Loading,
    Ready,
    /// 401 or 403 on `/v1/models`.
    AuthError,
    /// The key reference could not be resolved.
    KeyMissing,
    /// Host is outside the allowed address ranges.
    Blocked,
    Error,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub status: Status,
    pub message: String,
    pub url: String,
    pub models: Vec<String>,
    pub model_found: bool,
    pub latency_ms: Option<u64>,
    pub key_source: Option<&'static str>,
}

impl Probe {
    fn new(url: &str) -> Self {
        Self {
            status: Status::Error,
            message: String::new(),
            url: url.to_string(),
            models: Vec::new(),
            model_found: false,
            latency_ms: None,
            key_source: None,
        }
    }

    fn with(mut self, status: Status, message: impl Into<String>) -> Self {
        self.status = status;
        self.message = message.into();
        self
    }
}

/// Loopback, LAN (RFC 1918), link-local, Tailscale CGNAT and IPv6 ULA.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_loopback() || v.is_private() || v.is_link_local() || (o[0] == 100 && (o[1] & 0xc0) == 64)
        }
        IpAddr::V6(v) => {
            let s0 = v.segments()[0];
            v.is_loopback()
                || (s0 & 0xfe00) == 0xfc00
                || (s0 & 0xffc0) == 0xfe80
                || v.to_ipv4_mapped().is_some_and(|m| is_private_ip(IpAddr::V4(m)))
        }
    }
}

/// A URL with its host resolved once. Requests are pinned to `addr`, so the
/// address we checked is the address we talk to.
pub struct Target {
    pub url: Url,
    pub domain: Option<String>,
    pub addr: SocketAddr,
    pub private: bool,
}

pub async fn resolve_target(raw: &str) -> Result<Target, (Status, String)> {
    let url = Url::parse(raw).map_err(|e| (Status::Error, format!("Bad URL: {e}")))?;
    let port = url.port_or_known_default().unwrap_or(80);
    let (domain, addrs): (Option<String>, Vec<SocketAddr>) = match url.host() {
        Some(Host::Ipv4(ip)) => (None, vec![SocketAddr::new(ip.into(), port)]),
        Some(Host::Ipv6(ip)) => (None, vec![SocketAddr::new(ip.into(), port)]),
        Some(Host::Domain(d)) => {
            let found = tokio::time::timeout(DNS_TIMEOUT, tokio::net::lookup_host((d, port)))
                .await
                .ok()
                .and_then(|r| r.ok())
                .map(|it| it.collect::<Vec<_>>())
                .unwrap_or_default();
            if found.is_empty() {
                let hint = if d.ends_with(".ts.net") { " Is Tailscale connected?" } else { "" };
                return Err((Status::DnsFailed, format!("Can't find {d}.{hint}")));
            }
            (Some(d.to_string()), found)
        }
        None => return Err((Status::Error, "URL has no host.".into())),
    };
    let private = addrs.iter().all(|a| is_private_ip(a.ip()));
    let addr = *addrs.iter().find(|a| a.is_ipv4()).unwrap_or(&addrs[0]);
    Ok(Target { url, domain, addr, private })
}

/// Client for one target: no proxy, no redirects (a redirect could carry the
/// key elsewhere), short connect timeout.
pub fn client_for(
    t: &Target,
    timeout: Option<Duration>,
    read_timeout: Option<Duration>,
) -> reqwest::Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .user_agent(concat!("qwen35-harness/", env!("CARGO_PKG_VERSION")));
    if let Some(t) = timeout {
        b = b.timeout(t);
    }
    if let Some(t) = read_timeout {
        b = b.read_timeout(t);
    }
    if let Some(d) = &t.domain {
        b = b.resolve(d, t.addr);
    }
    b.build()
}

/// Refuse hosts outside the private ranges unless the profile allows them,
/// and never send a key to a public host over plain HTTP.
pub fn check_policy(p: &Profile, t: &Target) -> Result<(), String> {
    if t.private {
        return Ok(());
    }
    let host = t.url.host_str().unwrap_or("host");
    if !p.allow_public {
        return Err(format!("{host} is not a private address. Turn on \"Allow public host\" to use it."));
    }
    if t.url.scheme() != "https" {
        return Err("Public hosts need https, so the key is never sent in plain text.".into());
    }
    Ok(())
}

/// Base URL, or the fallback when the base host can't be resolved. Checks the
/// host policy. Returns the target and the base URL string it came from.
pub async fn pick_target(p: &Profile) -> Result<(Target, String), String> {
    let (t, base) = match resolve_target(&p.base_url).await {
        Ok(t) => (t, p.base_url.clone()),
        Err((_, first)) => match &p.fallback_url {
            Some(f) => (resolve_target(f).await.map_err(|(_, m)| m)?, f.clone()),
            None => return Err(first),
        },
    };
    check_policy(p, &t)?;
    Ok((t, base))
}

pub async fn probe(p: &Profile, cache: &SecretCache, env_file: &Path) -> Probe {
    let first = probe_url(p, &p.base_url, cache, env_file).await;
    let offline = |s: Status| matches!(s, Status::DnsFailed | Status::Unreachable);
    if !offline(first.status) {
        return first;
    }
    let Some(fallback) = &p.fallback_url else { return first };
    let second = probe_url(p, fallback, cache, env_file).await;
    if offline(second.status) {
        let (status, message) = (first.status, format!("{} The fallback URL failed too.", first.message));
        return first.with(status, message);
    }
    second
}

async fn probe_url(p: &Profile, base: &str, cache: &SecretCache, env_file: &Path) -> Probe {
    let out = Probe::new(base);
    let t = match resolve_target(base).await {
        Ok(t) => t,
        Err((s, m)) => return out.with(s, m),
    };
    if let Err(m) = check_policy(p, &t) {
        return out.with(Status::Blocked, m);
    }
    let client = match client_for(&t, Some(PROBE_TIMEOUT), None) {
        Ok(c) => c,
        Err(e) => return out.with(Status::Error, format!("HTTP client: {e}")),
    };

    // 1. Health, no auth. Resolves against the server root, not /v1.
    let mut out = out;
    let health = match t.url.join(&p.health_path) {
        Ok(u) => u,
        Err(e) => return out.with(Status::Error, format!("Health path: {e}")),
    };
    let start = Instant::now();
    match client.get(health).send().await {
        Err(e) => {
            let (s, m) = classify(&e);
            return out.with(s, m);
        }
        Ok(r) => {
            out.latency_ms = Some(start.elapsed().as_millis() as u64);
            match r.status().as_u16() {
                503 => return out.with(Status::Loading, "Model is loading."),
                s if s >= 500 => return out.with(Status::Error, format!("Health check returned {s}.")),
                // 404 is fine: some servers have no health route.
                _ => {}
            }
        }
    }

    // 2. Key.
    let key = match &p.key_ref {
        None => None,
        Some(r) => {
            out.key_source = KeyRef::parse(r).ok().map(|k| k.source());
            match cache.get(r, env_file).await {
                Ok(s) => Some(s),
                Err(e) => return out.with(Status::KeyMissing, e.to_string()),
            }
        }
    };

    // 3. Models, with the key. Proves the key works.
    let models_url = format!("{}/models", base.trim_end_matches('/'));
    let mut req = client.get(models_url);
    if let Some(k) = &key {
        req = req.bearer_auth(k.expose());
    }
    let r = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let (s, m) = classify(&e);
            return out.with(s, m);
        }
    };
    match r.status().as_u16() {
        200 => {}
        401 | 403 => {
            if let Some(kr) = &p.key_ref {
                cache.forget(kr); // so a fixed key is picked up on the next check
            }
            let m = if key.is_some() {
                "The server rejected the key. Check it in Settings."
            } else {
                "The server needs a key. Add one in Settings."
            };
            return out.with(Status::AuthError, m);
        }
        503 => return out.with(Status::Loading, "Model is loading."),
        s => return out.with(Status::Error, format!("/models returned {s}.")),
    }

    #[derive(Deserialize)]
    struct ModelList {
        data: Vec<ModelEntry>,
    }
    #[derive(Deserialize)]
    struct ModelEntry {
        id: String,
    }
    let list: ModelList = match r.json().await {
        Ok(l) => l,
        Err(e) => return out.with(Status::Error, format!("Unexpected /models reply: {e}")),
    };
    out.models = list.data.into_iter().map(|m| m.id).take(50).collect();
    out.model_found = out.models.iter().any(|m| m == &p.model);
    let message = if out.model_found {
        "Ready.".to_string()
    } else {
        format!("Connected, but \"{}\" is not on the server. It has: {}.", p.model, out.models.join(", "))
    };
    out.with(Status::Ready, message)
}

/// Map a transport error to a state from the table in docs/architecture.md.
pub fn classify(e: &reqwest::Error) -> (Status, String) {
    if e.is_connect() {
        if e.is_timeout() || chain_has(e, "timed out") {
            return (Status::Unreachable, "No answer from the host. Is the machine on?".into());
        }
        if chain_has(e, "refused") {
            return (Status::Unreachable, "Connection refused. Is the server running?".into());
        }
        return (Status::Unreachable, "Could not connect to the host.".into());
    }
    if e.is_timeout() {
        return (Status::Starting, "The host answered, but the server is not responding yet.".into());
    }
    // tailscale serve accepts the TCP connection, then closes it while the server is down.
    (Status::Starting, "Connection opened, then closed. The server may be starting.".into())
}

fn chain_has(e: &reqwest::Error, needle: &str) -> bool {
    let mut src = e.source();
    while let Some(s) = src {
        if s.to_string().to_lowercase().contains(needle) {
            return true;
        }
        src = s.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn private_ranges() {
        for s in ["127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.5", "100.100.100.100", "169.254.1.1", "::1", "fd00::1", "fe80::1"] {
            assert!(is_private_ip(ip(s)), "{s} should be private");
        }
        for s in ["8.8.8.8", "100.128.0.1", "172.32.0.1", "2001:4860::8888"] {
            assert!(!is_private_ip(ip(s)), "{s} should be public");
        }
    }

    #[tokio::test]
    async fn closed_local_port_is_unreachable() {
        let mut p = Profile::local_default();
        p.base_url = "http://127.0.0.1:9/v1".into();
        p.fallback_url = None;
        p.key_ref = None;
        let r = probe(&p, &SecretCache::default(), Path::new("/nonexistent")).await;
        assert_eq!(r.status, Status::Unreachable, "{}", r.message);
    }

    #[tokio::test]
    async fn public_host_is_blocked() {
        let mut p = Profile::local_default();
        p.base_url = "http://8.8.8.8/v1".into();
        p.fallback_url = None;
        let r = probe(&p, &SecretCache::default(), Path::new("/nonexistent")).await;
        assert_eq!(r.status, Status::Blocked);
        p.allow_public = true;
        let r = probe(&p, &SecretCache::default(), Path::new("/nonexistent")).await;
        assert_eq!(r.status, Status::Blocked, "plain http to a public host must stay blocked");
    }
}
