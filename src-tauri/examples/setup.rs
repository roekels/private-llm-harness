//! Exercise the setup assistant from the command line, against the app's real
//! data folder (so it reuses the app's VM). Installs nothing.
//!
//!   cargo run --example setup -- report | download | selftest

use std::path::PathBuf;

use qwen35_harness_lib::setup::Setup;
use qwen35_harness_lib::vm::Vm;

#[tokio::main]
async fn main() {
    let what = std::env::args().nth(1).unwrap_or_else(|| "report".into());
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let data = home.join("Library/Application Support/com.dewit.qwen35-harness");
    let vm = Vm::new(data.join("workspaces"), data.join("sandbox-build"));
    let setup = Setup::default();
    let say = |s: &str| println!("  | {s}");
    match what.as_str() {
        "report" => {
            let r = setup.report(&vm, true, None).await;
            for s in &r.steps {
                println!("{:<18} {:<11} {:<18} {}", s.title, format!("{:?}", s.state), s.action.unwrap_or("-"), s.detail);
            }
            println!("needs attention: {}", r.needs_attention);
        }
        "download" => {
            let r = setup.latest_release().await.expect("release");
            let dir = std::env::temp_dir().join("qh-setup-download");
            match setup.fetch_and_verify(&r, &dir, &say).await {
                Ok(f) => println!("== verified {} ({} bytes); not installing", f.display(), std::fs::metadata(&f).unwrap().len()),
                Err(e) => println!("== refused: {e}"),
            }
            let _ = std::fs::remove_dir_all(dir);
        }
        "selftest" => {
            use qwen35_harness_lib::vm::EnsureOpts;
            vm.ensure(EnsureOpts { build: true, network: true, patches: vec![] }, &|_| {}).await.expect("ensure");
            for rate in [qwen35_harness_lib::vm::RATE_LIMIT, 0] {
                vm.set_rate_limit(rate);
                println!("== rate limit {}", if rate > 0 { format!("{rate}/s") } else { "off".into() });
                match vm.selftest(true).await {
                    Ok(checks) => {
                        for c in checks {
                            println!("{} {:<38} {}", if c.passed { "PASS" } else { "FAIL" }, c.name, c.detail);
                        }
                    }
                    Err(e) => println!("== error: {e}"),
                }
            }
            vm.set_rate_limit(qwen35_harness_lib::vm::RATE_LIMIT);
            println!("== firewall now: {:?}", vm.status().await.network);
        }
        "devtools" => {
            use qwen35_harness_lib::vm::EnsureOpts;
            vm.ensure(EnsureOpts { build: true, network: true, patches: vec![] }, &say).await.expect("ensure");
            let (_tx, mut stop) = tokio::sync::watch::channel(false);
            let steps = [
                r#"{"action":"open","url":"https://httpbin.org/"}"#,
                r#"{"action":"network"}"#,
                r#"{"action":"network","filter":"spec"}"#,
                r#"{"action":"eval","code":"fetch('/status/404').then(r => r.status)"}"#,
                r#"{"action":"eval","code":"console.error('qh test error', 42); document.querySelectorAll('a').length"}"#,
                r#"{"action":"network","filter":"404"}"#,
                r#"{"action":"console"}"#,
                r#"{"action":"html","selector":"h2"}"#,
                r#"{"action":"html","selector":"[[bad"}"#,
                r#"{"action":"eval","code":"throw new Error('boom')"}"#,
            ];
            for s in steps {
                let r = vm.browser(s, true, &mut stop).await;
                println!("\n>>> {s}\n[{:?}, {} chars]\n{}", r.outcome, r.text.len(), r.text.chars().take(900).collect::<String>());
            }
            // Headers of the spec request, found from the list.
            let list = vm.browser(r#"{"action":"network","filter":"spec"}"#, true, &mut stop).await.text;
            if let Some(id) = list.lines().find_map(|l| l.strip_prefix('#').and_then(|r| r.split_whitespace().next())) {
                let r = vm.browser(&format!(r#"{{"action":"network","id":{id}}}"#), true, &mut stop).await;
                println!("\n>>> network id {id}\n[{:?}, {} chars]\n{}", r.outcome, r.text.len(), r.text);
            }
        }
        "redact" => {
            use qwen35_harness_lib::vm::EnsureOpts;
            vm.ensure(EnsureOpts { build: true, network: true, patches: vec![] }, &|_| {}).await.expect("ensure");
            let (_tx, mut stop) = tokio::sync::watch::channel(false);
            for s in [r#"{"action":"open","url":"https://httpbin.org/cookies/set?session=SECRET-abc123&lang=en"}"#,
                      r#"{"action":"open","url":"https://httpbin.org/headers"}"#] {
                vm.browser(s, true, &mut stop).await;
            }
            let all = vm.browser(r#"{"action":"network","all":true}"#, true, &mut stop).await.text;
            println!("{all}");
            for id in all.lines().filter_map(|l| l.strip_prefix('#').and_then(|r| r.split_whitespace().next())) {
                let r = vm.browser(&format!(r#"{{"action":"network","id":{id}}}"#), true, &mut stop).await.text;
                for l in r.lines().filter(|l| l.to_lowercase().contains("cookie") || l.contains("SECRET")) {
                    println!("  #{id} {}", l.trim());
                }
            }
            let mut leaked = all.contains("SECRET");
            for id in all.lines().filter_map(|l| l.strip_prefix('#').and_then(|r| r.split_whitespace().next())) {
                leaked |= vm.browser(&format!(r#"{{"action":"network","id":{id}}}"#), true, &mut stop).await.text.contains("SECRET");
            }
            println!("leaked anywhere (list or headers): {leaked}");
        }
        other => eprintln!("unknown: {other}"),
    }
}
