//! Drive the sandbox VM from the command line, through the same code as the app.
//! Uses a temp workspaces folder and removes the VM at the end.
//!
//!   cargo run --example vm -- status
//!   cargo run --example vm -- patches        # setup, built-in update, custom auto patch, restart
//!   cargo run --example vm -- rebuild        # fresh base, no cache

use std::time::Instant;

use qwen35_harness_lib::vm::{EnsureOpts, Patch, Vm, builtin_patches};

#[tokio::main]
async fn main() {
    let what = std::env::args().nth(1).unwrap_or_else(|| "status".into());
    let ws = std::env::temp_dir().join(format!("qh-vm-{}", std::process::id()));
    let vm = Vm::new(ws.clone(), std::env::temp_dir().join("qh-vm-build"));
    let say = |s: &str| println!("  | {s}");
    let quiet = |_: &str| {};

    match what.as_str() {
        "status" => println!("{:?}", vm.status().await),
        "patches" => {
            let custom = Patch {
                id: "marker".into(),
                name: "Write a marker file".into(),
                script: "echo patched-$(date +%s) > /etc/qh-marker\ncat /etc/qh-marker".into(),
                auto_run: true,
                builtin: false,
                last_run: None,
            };
            let t = Instant::now();
            let runs = vm
                .ensure(EnsureOpts { build: true, network: false, patches: vec![custom.clone()] }, &say)
                .await
                .expect("ensure");
            println!("== setup with auto patch: {:?} in {:.1}s", runs.iter().map(|(id, r)| (id, r.ok)).collect::<Vec<_>>(), t.elapsed().as_secs_f32());
            println!("== firewall after setup: {:?}", vm.status().await.network);

            let t = Instant::now();
            let lines = std::sync::atomic::AtomicUsize::new(0);
            let count = |s: &str| {
                if lines.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 4 || s.contains("upgraded") {
                    println!("  | {s}");
                }
            };
            let run = vm.run_patch(&builtin_patches()[0], false, &count).await;
            println!(
                "== system updates: ok={} {} lines in {:.1}s ({})",
                run.ok,
                lines.load(std::sync::atomic::Ordering::SeqCst),
                t.elapsed().as_secs_f32(),
                run.summary
            );
            println!("== firewall after patch (should be off): {:?}", vm.status().await.network);

            let bad = Patch { id: "bad".into(), name: "Failing patch".into(), script: "echo before\nfalse\necho never".into(), auto_run: false, builtin: false, last_run: None };
            let run = vm.run_patch(&bad, false, &say).await;
            println!("== failing patch: ok={} summary={}", run.ok, run.summary);

            // Restart: the marker must come back through the auto patch.
            vm.stop().await.unwrap();
            let runs = vm.ensure(EnsureOpts { build: false, network: false, patches: vec![custom] }, &quiet).await.unwrap();
            println!("== after restart, auto patch re-ran: {:?}", runs.iter().map(|(id, r)| (id, r.ok)).collect::<Vec<_>>());
            vm.stop().await.unwrap();
        }
        "rebuild" => {
            vm.ensure(EnsureOpts { build: true, network: false, patches: vec![] }, &quiet).await.expect("ensure");
            let t = Instant::now();
            let lines = std::sync::atomic::AtomicUsize::new(0);
            let r = vm.rebuild(&|_| {
                lines.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .await;
            println!("== rebuild: {:?}, {} log lines, {:.0}s", r, lines.load(std::sync::atomic::Ordering::SeqCst), t.elapsed().as_secs_f32());
            vm.ensure(EnsureOpts { build: true, network: false, patches: vec![] }, &quiet).await.expect("restart");
            println!("== after rebuild: {:?}", vm.status().await.state);
            vm.stop().await.unwrap();
        }
        "browser" => {
            vm.ensure(EnsureOpts { build: true, network: true, patches: vec![] }, &say).await.expect("ensure");
            let t = Instant::now();
            let r = vm.browser_user(&serde_json::json!({"action": "open", "url": "https://en.wikipedia.org/wiki/Special:Random"}), true).await;
            println!("== user open: ok={} in {:.1}s", r.is_ok(), t.elapsed().as_secs_f32());
            let t = Instant::now();
            let v: serde_json::Value = serde_json::from_str(&vm.browser_user(&serde_json::json!({"action": "view"}), true).await.unwrap()).unwrap();
            println!("== view: running={} title={} image={} KB in {:.2}s", v["running"], v["title"], v["image"].as_str().map_or(0, |s| s.len() / 1024), t.elapsed().as_secs_f32());
            let r = vm.browser_user(&serde_json::json!({"action": "click-xy", "x": 640, "y": 300}), true).await;
            println!("== click-xy: ok={}", r.is_ok());
            let r = vm.browser_user(&serde_json::json!({"action": "rm -rf"}), true).await;
            println!("== bad action refused: {}", r.is_err());
            let (_tx, mut stop) = tokio::sync::watch::channel(false);
            let snap = vm.browser(r#"{"action":"snapshot"}"#, true, &mut stop).await;
            println!("== model snapshot: {:?}, {} chars (cap 4000 + header)", snap.outcome, snap.text.len());
            vm.stop().await.unwrap();
        }
        other => eprintln!("unknown: {other}"),
    }
    let _ = std::fs::remove_dir_all(ws);
}
