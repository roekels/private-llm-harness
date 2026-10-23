//! The only door from the webview into Rust. No command returns a secret.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;
use tauri::ipc::Channel;
use zeroize::Zeroizing;

use crate::chat::{self, ChatEvent, Generation};
use crate::error::{Result, msg};
use crate::net::{self, Probe};
use crate::profile::Profile;
use crate::secrets::{self, KEYCHAIN_SERVICE, KeyRef, SecretCache};
use crate::store::{MessageRow, SessionRow, Store};
use crate::tools::{Approvals, ToolSettings, Workspace};
use crate::setup::{Report, SelfTest, Setup};
use crate::updates::{self, Available, UpdateEvent, Updates};
use crate::vm::{EnsureOpts, Patch, Vm, VmStatus};

pub struct AppState {
    pub store: Mutex<Store>,
    pub secrets: SecretCache,
    pub env_file: PathBuf,
    pub generation: Generation,
    pub approvals: Approvals,
    /// One folder per session lives here.
    pub workspaces: PathBuf,
    pub home: PathBuf,
    pub vm: Vm,
    pub setup: Setup,
    pub updates: Updates,
}

impl AppState {
    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    profiles: Vec<Profile>,
    active_id: Option<String>,
    env_file: String,
    keychain_service: &'static str,
    tool_settings: ToolSettings,
    /// App version the setup assistant was last seen in (to re-open after updates).
    setup_seen_version: Option<String>,
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Result<Bootstrap> {
    let store = state.store();
    Ok(Bootstrap {
        profiles: store.profiles()?,
        active_id: store.active_profile_id()?,
        env_file: state.env_file.display().to_string(),
        keychain_service: KEYCHAIN_SERVICE,
        tool_settings: store.tool_settings()?,
        setup_seen_version: store.setting(SETUP_SEEN)?,
    })
}

#[tauri::command]
pub fn profile_save(state: State<'_, AppState>, profile: Profile) -> Result<Profile> {
    let p = profile.validate()?;
    let store = state.store();
    store.save_profile(&p)?;
    if store.active_profile_id()?.is_none() {
        store.set_active_profile(&p.id)?;
    }
    state.secrets.clear();
    Ok(p)
}

#[tauri::command]
pub fn profile_delete(state: State<'_, AppState>, id: String) -> Result<Option<String>> {
    let store = state.store();
    store.delete_profile(&id)?;
    state.secrets.clear();
    store.active_profile_id()
}

#[tauri::command]
pub fn profile_activate(state: State<'_, AppState>, id: String) -> Result<()> {
    let store = state.store();
    if store.profile(&id)?.is_none() {
        return Err(msg("No such profile."));
    }
    store.set_active_profile(&id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyCheck {
    source: &'static str,
    length: usize,
}

/// Resolve a reference fresh and report where it came from and its length.
#[tauri::command]
pub async fn key_check(state: State<'_, AppState>, key_ref: String) -> Result<KeyCheck> {
    let source = KeyRef::parse(&key_ref)?.source();
    state.secrets.forget(&key_ref);
    let s = state.secrets.get(&key_ref, &state.env_file).await?;
    Ok(KeyCheck { source, length: s.char_count() })
}

/// Store a key typed in Settings. The value travels webview -> Rust once and
/// is never sent back.
#[tauri::command]
pub async fn keychain_set(state: State<'_, AppState>, account: String, value: String) -> Result<String> {
    let value = Zeroizing::new(value);
    let account = check_account(&account)?;
    let trimmed = Zeroizing::new(value.trim().to_string());
    if trimmed.is_empty() {
        return Err(msg("The key is empty."));
    }
    let acct = account.clone();
    tokio::task::spawn_blocking(move || secrets::keychain_set(&acct, &trimmed))
        .await
        .map_err(|e| msg(format!("Keychain task failed: {e}")))??;
    state.secrets.clear();
    Ok(format!("keychain:{KEYCHAIN_SERVICE}/{account}"))
}

#[tauri::command]
pub async fn keychain_delete(state: State<'_, AppState>, account: String) -> Result<()> {
    let account = check_account(&account)?;
    tokio::task::spawn_blocking(move || secrets::keychain_delete(&account))
        .await
        .map_err(|e| msg(format!("Keychain task failed: {e}")))??;
    state.secrets.clear();
    Ok(())
}

/// Probe a profile as given, so unsaved drafts can be tested too.
#[tauri::command]
pub async fn connection_probe(state: State<'_, AppState>, profile: Profile) -> Result<Probe> {
    let p = profile.validate()?;
    let r = net::probe(&p, &state.secrets, &state.env_file).await;
    #[cfg(debug_assertions)]
    eprintln!("[probe] {} {:?}: {}", r.url, r.status, r.message);
    Ok(r)
}

// ---- Sessions and chat (M2) ----

#[tauri::command]
pub fn sessions_list(state: State<'_, AppState>) -> Result<Vec<SessionRow>> {
    state.store().sessions()
}

#[tauri::command]
pub fn session_create(state: State<'_, AppState>) -> Result<SessionRow> {
    let store = state.store();
    let profile = store.active_profile_id()?;
    let system = chat::system_prompt(crate::clock::now_ms());
    store.create_session(&uuid::Uuid::new_v4().to_string(), profile.as_deref(), &system)
}

#[tauri::command]
pub fn session_rename(state: State<'_, AppState>, id: String, title: String) -> Result<()> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 80 {
        return Err(msg("Title must be 1 to 80 characters."));
    }
    state.store().rename_session(&id, title)
}

#[tauri::command]
pub fn session_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    state.store().delete_session(&id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagesView {
    messages: Vec<MessageRow>,
    trim_seq: i64,
    strip_seq: i64,
    memory: String,
    compactions: i64,
    /// Where this session's files go (created on first tool use).
    workspace: String,
}

#[tauri::command]
pub fn messages_list(state: State<'_, AppState>, session_id: String) -> Result<MessagesView> {
    let store = state.store();
    let (row, _) = store.session(&session_id)?.ok_or_else(|| msg("No such session."))?;
    Ok(MessagesView {
        messages: store.messages(&session_id, 0)?,
        trim_seq: row.trim_seq,
        strip_seq: row.strip_seq,
        memory: row.memory.clone(),
        compactions: row.compactions,
        workspace: state.workspaces.join(&row.id).display().to_string(),
    })
}

/// Runs one turn and streams events on `on_event`. Resolves when the turn ends.
#[tauri::command]
pub async fn chat_send(
    state: State<'_, AppState>,
    session_id: String,
    text: String,
    images: Vec<String>,
    think: bool,
    on_event: Channel<ChatEvent>,
) -> Result<()> {
    let (profile, tools) = {
        let store = state.store();
        let id = store.active_profile_id()?.ok_or_else(|| msg("Choose a profile in Settings."))?;
        let p = store.profile(&id)?.ok_or_else(|| msg("The active profile is missing."))?;
        (p, store.tool_settings()?)
    };
    let (_slot, stop) = state.generation.start()?;
    let ctx = chat::Ctx {
        store: &state.store,
        secrets: &state.secrets,
        env_file: &state.env_file,
        approvals: &state.approvals,
        workspaces: &state.workspaces,
        home: &state.home,
        vm: &state.vm,
    };
    let input = chat::TurnInput { session_id, text, images, think, tools };
    chat::run_turn(ctx, &profile, input, stop, &on_event).await
}

/// "Compact now": write the session memory and shrink the context.
#[tauri::command]
pub async fn session_compact(state: State<'_, AppState>, session_id: String, on_event: Channel<ChatEvent>) -> Result<()> {
    let (profile, tools) = {
        let store = state.store();
        let id = store.active_profile_id()?.ok_or_else(|| msg("Choose a profile in Settings."))?;
        let p = store.profile(&id)?.ok_or_else(|| msg("The active profile is missing."))?;
        (p, store.tool_settings()?)
    };
    let (_slot, _stop) = state.generation.start()?; // the server has one slot
    let ctx = chat::Ctx {
        store: &state.store,
        secrets: &state.secrets,
        env_file: &state.env_file,
        approvals: &state.approvals,
        workspaces: &state.workspaces,
        home: &state.home,
        vm: &state.vm,
    };
    chat::compact_now(ctx, &profile, &session_id, tools, &on_event).await
}

/// Edited in the Memory pane. Takes effect on the next request.
#[tauri::command]
pub fn session_set_memory(state: State<'_, AppState>, id: String, memory: String) -> Result<()> {
    if memory.chars().count() > 8000 {
        return Err(msg("The memory must be under 8,000 characters."));
    }
    state.store().set_memory(&id, memory.trim())
}

#[tauri::command]
pub fn chat_stop(state: State<'_, AppState>) -> bool {
    state.generation.stop()
}

/// Answer a pending approval prompt. Returns false if nothing was waiting.
#[tauri::command]
pub fn tool_decide(state: State<'_, AppState>, call_id: String, approve: bool) -> bool {
    state.approvals.decide(&call_id, approve)
}

#[tauri::command]
pub fn tool_settings_set(state: State<'_, AppState>, settings: ToolSettings) -> Result<()> {
    state.store().set_tool_settings(&settings)?;
    state.vm.set_rate_limit(settings.rate_per_sec());
    Ok(())
}

/// Show the session's workspace folder in Finder.
#[tauri::command]
pub fn workspace_reveal(state: State<'_, AppState>, session_id: String) -> Result<()> {
    let ws = Workspace::open(&state.workspaces, &session_id).map_err(msg)?;
    std::process::Command::new("/usr/bin/open")
        .arg(&ws.root)
        .spawn()
        .map_err(|e| msg(format!("Could not open Finder: {e}")))?;
    Ok(())
}

#[tauri::command]
pub async fn vm_status(state: State<'_, AppState>) -> Result<VmStatus> {
    Ok(state.vm.status().await)
}

/// Start the container system, build the image if needed, start the VM.
/// Progress lines arrive on `on_progress`.
#[tauri::command]
pub async fn vm_setup(state: State<'_, AppState>, on_progress: Channel<String>) -> Result<VmStatus> {
    let report = |s: &str| {
        let _ = on_progress.send(s.to_string());
    };
    let (patches, network) = {
        let store = state.store();
        (store.patches()?, store.tool_settings()?.network)
    };
    let runs = state.vm.ensure(EnsureOpts { build: true, network, patches }, &report).await.map_err(msg)?;
    state.store().record_patch_runs(&runs)?;
    Ok(state.vm.status().await)
}

#[tauri::command]
pub fn patches_list(state: State<'_, AppState>) -> Result<Vec<Patch>> {
    state.store().patches()
}

/// Save the list. Built-ins keep their script; only "run at start" changes.
#[tauri::command]
pub fn patches_save(state: State<'_, AppState>, patches: Vec<Patch>) -> Result<Vec<Patch>> {
    let mut seen = std::collections::HashSet::new();
    let mut clean = Vec::with_capacity(patches.len());
    for p in patches {
        let p = p.validate().map_err(msg)?;
        if !seen.insert(p.id.clone()) {
            return Err(msg(format!("Two patches share the id \"{}\".", p.id)));
        }
        clean.push(p);
    }
    let store = state.store();
    store.save_patches(&clean)?;
    store.patches()
}

/// Run one patch now. Output lines arrive on `on_line`. Returns the updated patch.
#[tauri::command]
pub async fn patch_run(state: State<'_, AppState>, id: String, on_line: Channel<String>) -> Result<Patch> {
    let (patch, network) = {
        let store = state.store();
        let p = store.patches()?.into_iter().find(|p| p.id == id).ok_or_else(|| msg("No such patch."))?;
        (p, store.tool_settings()?.network)
    };
    let send = |s: &str| {
        let _ = on_line.send(s.to_string());
    };
    let run = state.vm.run_patch(&patch, network, &send).await;
    let store = state.store();
    store.record_patch_runs(&[(id.clone(), run)])?;
    store.patches()?.into_iter().find(|p| p.id == id).ok_or_else(|| msg("No such patch."))
}

/// Rebuild the image from a fresh base, then start the VM again.
#[tauri::command]
pub async fn vm_rebuild(state: State<'_, AppState>, on_line: Channel<String>) -> Result<VmStatus> {
    let send = |s: &str| {
        let _ = on_line.send(s.to_string());
    };
    state.vm.rebuild(&send).await.map_err(msg)?;
    let (patches, network) = {
        let store = state.store();
        (store.patches()?, store.tool_settings()?.network)
    };
    let runs = state.vm.ensure(EnsureOpts { build: true, network, patches }, &send).await.map_err(msg)?;
    state.store().record_patch_runs(&runs)?;
    Ok(state.vm.status().await)
}

#[tauri::command]
pub async fn vm_stop(state: State<'_, AppState>) -> Result<VmStatus> {
    state.vm.stop().await.map_err(msg)?;
    Ok(state.vm.status().await)
}

/// The browser pane: current page as an image, or `{"running": false}`.
#[tauri::command]
pub async fn browser_view(state: State<'_, AppState>) -> Result<serde_json::Value> {
    if state.store().tool_settings()?.backend != crate::tools::Backend::Vm || !state.vm.is_ready() {
        return Ok(serde_json::json!({ "running": false }));
    }
    let _shared = state.vm.tool_lock().await;
    let out = state.vm.browser_user(&serde_json::json!({ "action": "view" }), false).await.map_err(msg)?;
    serde_json::from_str(out.trim()).map_err(|e| msg(format!("Bad browser view: {e}")))
}

/// The user acting in the browser pane (open a URL, click, type, back). No
/// approval: it's the user, not the model. The VM firewall still applies.
#[tauri::command]
pub async fn browser_user(state: State<'_, AppState>, args: serde_json::Value) -> Result<()> {
    let (patches, settings) = {
        let store = state.store();
        (store.patches()?, store.tool_settings()?)
    };
    if settings.backend != crate::tools::Backend::Vm {
        return Err(msg("The browser runs only in the Linux VM sandbox."));
    }
    let runs = state
        .vm
        .ensure(EnsureOpts { build: false, network: settings.network, patches }, &|_| {})
        .await
        .map_err(msg)?;
    state.store().record_patch_runs(&runs)?;
    let _shared = state.vm.tool_lock().await;
    state.vm.browser_user(&args, settings.network).await.map_err(msg)?;
    Ok(())
}

// ---- Setup assistant ----

const SETUP_SEEN: &str = "setup_seen_version";
const SELFTEST: &str = "selftest";

fn last_selftest(store: &Store) -> Option<SelfTest> {
    store.setting(SELFTEST).ok().flatten().and_then(|s| serde_json::from_str(&s).ok())
}

#[tauri::command]
pub async fn setup_report(state: State<'_, AppState>) -> Result<Report> {
    let (use_vm, test) = {
        let store = state.store();
        (store.tool_settings()?.backend == crate::tools::Backend::Vm, last_selftest(&store))
    };
    Ok(state.setup.report(&state.vm, use_vm, test.as_ref()).await)
}

/// Download, verify and install or upgrade Apple container (macOS asks for the password).
#[tauri::command]
pub async fn setup_install_container(state: State<'_, AppState>, on_line: Channel<String>) -> Result<String> {
    let say = |s: &str| {
        let _ = on_line.send(s.to_string());
    };
    let data = state.workspaces.parent().map(Path::to_path_buf).unwrap_or_default();
    // A running VM goes down with the container system during an upgrade.
    state.vm.forget_ready();
    state.setup.install_container(&crate::setup::downloads_dir(&data), &say).await.map_err(msg)
}

/// Start the container system and the VM (building the image if `build`).
/// Old sandbox images are removed after a successful build.
#[tauri::command]
pub async fn setup_vm(state: State<'_, AppState>, build: bool, on_line: Channel<String>) -> Result<String> {
    let say = |s: &str| {
        let _ = on_line.send(s.to_string());
    };
    let (patches, network) = {
        let store = state.store();
        (store.patches()?, store.tool_settings()?.network)
    };
    let runs = state.vm.ensure(EnsureOpts { build, network, patches }, &say).await.map_err(msg)?;
    state.store().record_patch_runs(&runs)?;
    if build {
        let removed = state.vm.remove_old_images().await;
        if removed > 0 {
            say(&format!("Removed {removed} old sandbox image(s)."));
        }
    }
    Ok("The sandbox VM is running.".into())
}

#[tauri::command]
pub async fn setup_selftest(state: State<'_, AppState>) -> Result<SelfTest> {
    let network = state.store().tool_settings()?.network;
    let checks = state.vm.selftest(network).await.map_err(msg)?;
    let t = SelfTest { at: crate::clock::now_ms(), passed: checks.iter().all(|c| c.passed), checks };
    state.store().set_setting(SELFTEST, &serde_json::to_string(&t)?)?;
    Ok(t)
}

#[tauri::command]
pub fn setup_last_selftest(state: State<'_, AppState>) -> Option<SelfTest> {
    last_selftest(&state.store())
}

/// Remember that the assistant was seen in this app version.
#[tauri::command]
pub fn setup_seen(state: State<'_, AppState>) -> Result<()> {
    state.store().set_setting(SETUP_SEEN, env!("CARGO_PKG_VERSION"))
}

/// Links the UI may open in the default browser. Fixed on purpose: the
/// webview can't open arbitrary URLs.
const LINKS: [(&str, &str); 6] = [
    ("github", "https://github.com/roekels"),
    ("repo", "https://github.com/roekels/private-llm-harness"),
    ("model", "https://huggingface.co/HauhauCS/Qwen3.6-35B-A3B-Uncensored-HauhauCS-Aggressive"),
    ("base-model", "https://huggingface.co/Qwen/Qwen3.6-35B-A3B"),
    ("llama-cpp", "https://github.com/ggml-org/llama.cpp"),
    ("container", "https://github.com/apple/container"),
];

#[tauri::command]
pub fn open_link(name: String) -> Result<()> {
    let url = LINKS.iter().find(|(n, _)| *n == name).map(|(_, u)| *u).ok_or_else(|| msg("Unknown link."))?;
    std::process::Command::new("/usr/bin/open")
        .arg(url)
        .spawn()
        .map_err(|e| msg(format!("Could not open the browser: {e}")))?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    macos: String,
    chip: String,
    memory_gb: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutInfo {
    version: &'static str,
    data_dir: String,
    host: HostInfo,
    vm: VmStatus,
    vm_details: Option<crate::vm::VmDetails>,
    skills: Vec<&'static str>,
}

/// Live facts for the About view.
#[tauri::command]
pub async fn about_info(state: State<'_, AppState>) -> Result<AboutInfo> {
    let run = |cmd: &str, args: &[&str]| {
        std::process::Command::new(cmd)
            .args(args)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    let host = HostInfo {
        macos: run("/usr/bin/sw_vers", &["-productVersion"]),
        chip: run("/usr/sbin/sysctl", &["-n", "machdep.cpu.brand_string"]),
        memory_gb: run("/usr/sbin/sysctl", &["-n", "hw.memsize"]).parse::<u64>().unwrap_or(0) / (1 << 30),
    };
    let vm = state.vm.status().await;
    let vm_details = if vm.state == crate::vm::VmState::Running { state.vm.details().await } else { None };
    Ok(AboutInfo {
        version: env!("CARGO_PKG_VERSION"),
        data_dir: state.workspaces.parent().map(|p| p.display().to_string()).unwrap_or_default(),
        host,
        vm,
        vm_details,
        skills: crate::tools::SKILLS.iter().map(|(n, _)| *n).collect(),
    })
}

fn check_account(account: &str) -> Result<String> {
    let a = account.trim();
    if a.is_empty() || a.len() > 64 || !a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(msg("Keychain account must be 1 to 64 letters, digits or dashes."));
    }
    Ok(a.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    current: &'static str,
    /// False for dev builds and builds without an update key; `reason` says why.
    supported: bool,
    reason: Option<String>,
    auto_check: bool,
    /// Last successful check, ms since the epoch.
    last_check: Option<i64>,
}

#[tauri::command]
pub fn update_status(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<UpdateStatus> {
    let store = state.store();
    let reason = updates::unsupported_reason(&app);
    Ok(UpdateStatus {
        current: env!("CARGO_PKG_VERSION"),
        supported: reason.is_none(),
        reason,
        auto_check: store.setting(updates::AUTO_CHECK)?.as_deref() != Some("0"),
        last_check: store.setting(updates::LAST_CHECK)?.and_then(|v| v.parse().ok()),
    })
}

/// Ask GitHub for a newer release. Nothing is downloaded yet.
#[tauri::command]
pub async fn update_check(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Option<Available>> {
    let found = state.updates.check(&app).await.map_err(msg)?;
    state.store().set_setting(updates::LAST_CHECK, &crate::clock::now_ms().to_string())?;
    Ok(found)
}

/// Download, verify and install the update from the last check. Restart to run it.
#[tauri::command]
pub async fn update_install(app: tauri::AppHandle, state: State<'_, AppState>, on_event: Channel<UpdateEvent>) -> Result<String> {
    let on = |e: UpdateEvent| {
        let _ = on_event.send(e);
    };
    state.updates.install(&app, &on).await.map_err(msg)
}

#[tauri::command]
pub fn update_restart(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<()> {
    if state.generation.busy() {
        return Err(msg("A reply is still being written. Stop it or wait, then restart."));
    }
    app.restart()
}

#[tauri::command]
pub fn update_settings_set(state: State<'_, AppState>, auto_check: bool) -> Result<()> {
    state.store().set_setting(updates::AUTO_CHECK, if auto_check { "1" } else { "0" })
}
