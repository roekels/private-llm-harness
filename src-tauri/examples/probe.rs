//! Probe a profile from the command line. Prints status only, never the key.
//!
//!   cargo run --example probe -- [key-ref|none] [base-url]
//!
//! Defaults to the local preset (llama-server on 127.0.0.1:8080). Examples:
//!   cargo run --example probe -- none
//!   cargo run --example probe -- keychain:qwen35-harness/my-server http://my-gpu-box:8080/v1

use std::path::Path;

use qwen35_harness_lib::{net, profile::Profile, secrets::SecretCache};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut p = Profile::local_default();
    if let Some(k) = args.first() {
        p.key_ref = (k != "none").then(|| k.clone());
    }
    if let Some(u) = args.get(1) {
        p.base_url = u.clone();
        p.fallback_url = None;
    }
    let p = match p.validate() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Invalid profile: {e}");
            std::process::exit(2);
        }
    };
    let r = net::probe(&p, &SecretCache::default(), Path::new("/nonexistent/.env")).await;
    println!("{}", serde_json::to_string_pretty(&r).expect("json"));
}
