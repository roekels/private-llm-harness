// Privileged work lives here, behind narrow commands (see docs/architecture.md).
// The webview never receives a secret.

pub mod chat;
pub mod clock;
pub mod error;
pub mod net;
pub mod profile;
pub mod secrets;
pub mod setup;
pub mod store;
pub mod tools;
pub mod updates;
pub mod vm;

mod commands;

use std::sync::Mutex;

use tauri::Manager;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            let config = app.path().app_config_dir()?;
            std::fs::create_dir_all(&data)?;
            std::fs::create_dir_all(&config)?;
            let workspaces = data.join("workspaces");
            std::fs::create_dir_all(&workspaces)?;

            let store = store::Store::open(&data.join("harness.db"))?;
            store.seed()?;
            let rate = store.tool_settings()?.rate_per_sec();
            app.manage(AppState {
                store: Mutex::new(store),
                secrets: secrets::SecretCache::default(),
                env_file: config.join(".env"),
                generation: chat::Generation::default(),
                approvals: tools::Approvals::default(),
                vm: {
                    let vm = vm::Vm::new(workspaces.clone(), data.join("sandbox-build"));
                    vm.set_rate_limit(rate);
                    vm
                },
                setup: setup::Setup::default(),
                updates: updates::Updates::default(),
                workspaces,
                home: app.path().home_dir()?,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::profile_save,
            commands::profile_delete,
            commands::profile_activate,
            commands::key_check,
            commands::keychain_set,
            commands::keychain_delete,
            commands::connection_probe,
            commands::sessions_list,
            commands::session_create,
            commands::session_rename,
            commands::session_delete,
            commands::messages_list,
            commands::chat_send,
            commands::chat_stop,
            commands::session_compact,
            commands::session_set_memory,
            commands::tool_decide,
            commands::tool_settings_set,
            commands::workspace_reveal,
            commands::vm_status,
            commands::vm_setup,
            commands::vm_stop,
            commands::patches_list,
            commands::patches_save,
            commands::patch_run,
            commands::vm_rebuild,
            commands::open_link,
            commands::about_info,
            commands::setup_report,
            commands::setup_install_container,
            commands::setup_vm,
            commands::setup_selftest,
            commands::setup_last_selftest,
            commands::setup_seen,
            commands::browser_view,
            commands::browser_user,
            commands::update_status,
            commands::update_check,
            commands::update_install,
            commands::update_restart,
            commands::update_settings_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
