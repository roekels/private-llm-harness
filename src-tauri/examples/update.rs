//! End-to-end check of the self-update path (docs/updates.md) with a throwaway key.
//! Runs the real updater plugin on Tauri's mock runtime, so no window opens and
//! the app's data is never touched. It installs only into the .app you name.
//!
//!   cargo run --example update -- <latest.json URL> <public key> <path/to/Test.app> [current version]
//!
//! The URL may be plain http on 127.0.0.1 (allowed here only). The current version defaults to 0.8.0.

use tauri_plugin_updater::UpdaterExt;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [url, pubkey, app_path] = [0, 1, 2].map(|i| a.get(i).cloned().unwrap_or_else(usage));
    let current = a.get(3).cloned().unwrap_or_else(|| "0.8.0".into());
    let app_path = std::path::PathBuf::from(app_path);
    if app_path.starts_with("/Applications") || app_path.extension().is_none_or(|e| e != "app") {
        eprintln!("Give a test .app outside /Applications.");
        std::process::exit(2);
    }

    let mut ctx = tauri::test::mock_context(tauri::test::noop_assets());
    ctx.package_info_mut().version = current.parse().expect("version");
    ctx.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({
            "pubkey": pubkey,
            "endpoints": [url],
            "requireSignedVersion": true,
            "dangerousInsecureTransportProtocol": true
        }),
    );
    let app = tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(ctx)
        .expect("mock app");
    let handle = app.handle().clone();

    tauri::async_runtime::block_on(async move {
        let exe = app_path.join("Contents/MacOS/qwen35-harness");
        let updater = handle.updater_builder().executable_path(&exe).build().expect("updater");
        println!("current: {current}");
        match updater.check().await {
            Ok(Some(u)) => {
                println!("found: {}", u.version);
                let mut got = 0usize;
                match u.download_and_install(|n, _| got += n, || {}).await {
                    Ok(()) => println!("installed: {} bytes, into {}", got, app_path.display()),
                    Err(e) => println!("refused: {e}"),
                }
            }
            Ok(None) => println!("no newer version"),
            Err(e) => println!("check failed: {e}"),
        }
    });
}

fn usage() -> String {
    eprintln!("usage: cargo run --example update -- <latest.json URL> <public key> <path/to/Test.app> [current version]");
    std::process::exit(2);
}
