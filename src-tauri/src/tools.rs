//! Tools the model can call: `sh`, `read`, `write`. Everything is confined to
//! the session workspace. `sh` runs under macOS `sandbox-exec`: it can write
//! only the workspace, can't read the user's home folder, and has no network
//! unless allowed. `read` and `write` run in the app, behind a path guard.

use std::collections::{HashMap, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::{oneshot, watch};

/// Most tool rounds per user message.
pub const MAX_STEPS: usize = 16;
const SH_TIMEOUT: Duration = Duration::from_secs(60);
/// Bytes kept per stream: the first CAPTURE_HEAD and the last CAPTURE_TAIL.
const CAPTURE_HEAD: usize = 48 * 1024;
const CAPTURE_TAIL: usize = 16 * 1024;
/// What the model sees of long output: head and tail.
const CLIP_HEAD: usize = 1024;
const CLIP_TAIL: usize = 1024;
const READ_MAX_LINES: usize = 200;
const READ_MAX_BYTES: usize = 8 * 1024;
const READ_MAX_FILE: u64 = 8 * 1024 * 1024;
const WRITE_MAX_BYTES: usize = 1024 * 1024;

/// Where tools run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    /// The app's Linux VM (Apple container). Full isolation.
    #[default]
    Vm,
    /// macOS sandbox-exec on this Mac. Less isolated.
    Macos,
}

/// Global tool settings (Settings screen).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolSettings {
    pub enabled: bool,
    /// Run sh and write without asking.
    pub auto_approve: bool,
    /// Let sh use the network. In the VM this means internet only.
    pub network: bool,
    pub backend: Backend,
    /// Anti-block guard in the VM: at most 20 new connections per second per site.
    pub rate_limit: bool,
}

impl ToolSettings {
    /// New connections per second the VM firewall allows per address (0 = no limit).
    pub fn rate_per_sec(&self) -> u32 {
        if self.rate_limit { crate::vm::RATE_LIMIT } else { 0 }
    }
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self { enabled: true, auto_approve: false, network: true, backend: Backend::Vm, rate_limit: true }
    }
}

/// Best-practice notes the model can look up (`skills/*.md`), compiled in.
pub const SKILLS: [(&str, &str); 6] = [
    ("terminal", include_str!("../../skills/terminal.md")),
    ("files", include_str!("../../skills/files.md")),
    ("browser", include_str!("../../skills/browser.md")),
    ("screenshot", include_str!("../../skills/screenshot.md")),
    ("research", include_str!("../../skills/research.md")),
    ("python", include_str!("../../skills/python.md")),
];

pub fn skill_text(name: &str) -> ToolResult {
    match SKILLS.iter().find(|(n, _)| *n == name.trim()) {
        Some((_, body)) => ToolResult { text: body.trim().to_string(), ..ToolResult::simple(Outcome::Ok, "") },
        None => {
            let names: Vec<&str> = SKILLS.iter().map(|(n, _)| *n).collect();
            ToolResult::simple(Outcome::Failed, format!("No skill \"{name}\". Skills: {}.", names.join(", ")))
        }
    }
}

/// Tool schemas. Fixed text, so the prompt prefix (and llama.cpp's cache)
/// stays the same on every request. Keep them short: they are sent every turn.
pub fn definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "sh",
                "description": "Run a shell command in the workspace folder (the current directory). Network may be blocked. 60 s limit. Long output is clipped; the full text is saved to a file you can read.",
                "parameters": {
                    "type": "object",
                    "properties": { "cmd": { "type": "string" } },
                    "required": ["cmd"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read",
                "description": "Read a text file in the workspace. Returns up to 200 lines starting at line `offset` (default 1).",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" }, "offset": { "type": "integer" } },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "browser",
                "description": "Use a web browser in the sandbox. Actions: open (url), snapshot, click (ref), type (ref, text, submit), scroll (direction), back, screenshot. DevTools: network (filter, or id for headers), console (filter), html (selector), eval (code). Pages come back as text; [n] marks things you can click or type into.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["open", "snapshot", "click", "type", "scroll", "back", "screenshot", "network", "console", "html", "eval"] },
                        "url": { "type": "string" },
                        "ref": { "type": "integer" },
                        "text": { "type": "string" },
                        "submit": { "type": "boolean" },
                        "direction": { "type": "string", "enum": ["up", "down"] },
                        "id": { "type": "integer" },
                        "filter": { "type": "string" },
                        "selector": { "type": "string" },
                        "code": { "type": "string" }
                    },
                    "required": ["action"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "skill",
                "description": "Short best-practice notes for doing something well and cheaply. Names: terminal, files, browser, screenshot, research, python.",
                "parameters": {
                    "type": "object",
                    "properties": { "name": { "type": "string", "enum": ["terminal", "files", "browser", "screenshot", "research", "python"] } },
                    "required": ["name"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write",
                "description": "Create or replace a text file in the workspace.",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" }, "content": { "type": "string" } },
                    "required": ["path", "content"]
                }
            }
        }
    ])
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// JSON text, as the model produced it.
    pub arguments: String,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Sh { cmd: String },
    Read { path: String, offset: usize },
    Write { path: String, content: String },
    /// Validated JSON object passed to `qhb` as-is.
    Browser { action: String, args: String },
    Skill { name: String },
}

impl Action {
    pub fn parse(call: &ToolCall) -> Result<Self, String> {
        let args: Value = if call.arguments.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&call.arguments).map_err(|e| format!("Arguments are not valid JSON: {e}"))?
        };
        let s = |k: &str| args[k].as_str().map(str::to_string).ok_or_else(|| format!("Missing \"{k}\"."));
        match call.name.as_str() {
            "sh" => Ok(Self::Sh { cmd: s("cmd")? }),
            "read" => Ok(Self::Read {
                path: s("path")?,
                offset: args["offset"].as_u64().unwrap_or(1).max(1) as usize,
            }),
            "write" => Ok(Self::Write { path: s("path")?, content: s("content")? }),
            "browser" => {
                let action = s("action")?;
                const OK: [&str; 11] =
                    ["open", "snapshot", "click", "type", "scroll", "back", "screenshot", "network", "console", "html", "eval"];
                if !OK.contains(&action.as_str()) {
                    return Err(format!("Unknown browser action \"{action}\". Use one of: {}.", OK.join(", ")));
                }
                if action == "open" && args["url"].as_str().is_none_or(|u| u.trim().is_empty()) {
                    return Err("open needs a url.".into());
                }
                if matches!(action.as_str(), "click" | "type") && args["ref"].as_u64().is_none() {
                    return Err(format!("{action} needs a ref number from the last snapshot."));
                }
                let len = |k: &str| args[k].as_str().map_or(0, |s| s.trim().chars().count());
                if action == "html" && !(1..=300).contains(&len("selector")) {
                    return Err("html needs a CSS selector (under 300 characters).".into());
                }
                if action == "eval" && !(1..=4000).contains(&len("code")) {
                    return Err("eval needs JavaScript code (under 4,000 characters).".into());
                }
                if len("filter") > 100 {
                    return Err("filter must be under 100 characters.".into());
                }
                Ok(Self::Browser { action, args: args.to_string() })
            }
            "skill" => Ok(Self::Skill { name: s("name")? }),
            other => Err(format!("Unknown tool \"{other}\". Use sh, read, write, browser or skill.")),
        }
    }

    /// `read` stays inside the workspace and changes nothing, so it runs freely.
    /// `read`, and browser actions that only look, run freely.
    pub fn needs_approval(&self) -> bool {
        match self {
            Self::Read { .. } | Self::Skill { .. } => false,
            // eval runs code in the page, so it always asks (unless auto-approve).
            Self::Browser { action, .. } => matches!(action.as_str(), "open" | "click" | "type" | "eval"),
            _ => true,
        }
    }
}

/// How a tool run ended.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Ok,
    Failed,
    Denied,
    TimedOut,
    Stopped,
}

#[derive(Debug)]
pub struct ToolResult {
    /// What the model sees (already clipped).
    pub text: String,
    pub outcome: Outcome,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    /// Workspace-relative path of the full output, when clipped.
    pub full_output: Option<String>,
    /// A screenshot for the model (data URI), from the browser tool.
    pub image: Option<String>,
}

impl ToolResult {
    pub fn simple(outcome: Outcome, text: impl Into<String>) -> Self {
        Self { text: text.into(), outcome, exit_code: None, duration_ms: 0, full_output: None, image: None }
    }
}

// ---------- Approvals ----------

/// Pending approval prompts, answered by the `tool_decide` command.
#[derive(Default)]
pub struct Approvals {
    map: Mutex<HashMap<String, oneshot::Sender<bool>>>,
}

impl Approvals {
    pub fn register(&self, call_id: &str) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        self.lock().insert(call_id.to_string(), tx);
        rx
    }

    pub fn decide(&self, call_id: &str, approve: bool) -> bool {
        match self.lock().remove(call_id) {
            Some(tx) => tx.send(approve).is_ok(),
            None => false,
        }
    }

    pub fn forget(&self, call_id: &str) {
        self.lock().remove(call_id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, oneshot::Sender<bool>>> {
        self.map.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// ---------- Workspace ----------

pub struct Workspace {
    /// Canonical path, no symlinks.
    pub root: PathBuf,
}

impl Workspace {
    /// `<base>/<session id>`, created if missing.
    pub fn open(base: &Path, session_id: &str) -> Result<Self, String> {
        if session_id.is_empty() || !session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err("Bad session id.".into());
        }
        let dir = base.join(session_id);
        std::fs::create_dir_all(dir.join(".tmp")).map_err(|e| format!("Could not create the workspace: {e}"))?;
        let root = dir.canonicalize().map_err(|e| format!("Workspace: {e}"))?;
        Ok(Self { root })
    }

    /// Lexically resolve a model-supplied path. Rejects anything outside.
    fn resolve(&self, raw: &str) -> Result<PathBuf, String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("Path is empty.".into());
        }
        let p = Path::new(raw);
        let joined = if p.is_absolute() { p.to_path_buf() } else { self.root.join(p) };
        let mut out = PathBuf::new();
        for c in joined.components() {
            match c {
                Component::Prefix(_) | Component::RootDir => out.push(c.as_os_str()),
                Component::CurDir => {}
                Component::ParentDir => {
                    if !out.pop() {
                        return Err("Path is outside the workspace.".into());
                    }
                }
                Component::Normal(x) => out.push(x),
            }
        }
        if !out.starts_with(&self.root) {
            return Err("Path is outside the workspace. Use a path relative to the current folder.".into());
        }
        Ok(out)
    }

    /// The deepest existing ancestor must really be inside (no symlink escape).
    fn check_real(&self, path: &Path) -> Result<(), String> {
        let mut probe = path.to_path_buf();
        while !probe.exists() {
            if !probe.pop() {
                return Err("Path is outside the workspace.".into());
            }
        }
        let real = probe.canonicalize().map_err(|e| e.to_string())?;
        if !real.starts_with(&self.root) {
            return Err("Path leads outside the workspace through a link.".into());
        }
        Ok(())
    }

    pub fn display(&self, path: &Path) -> String {
        path.strip_prefix(&self.root).map(|p| p.display().to_string()).unwrap_or_else(|_| path.display().to_string())
    }
}

pub fn read_file(ws: &Workspace, raw: &str, offset: usize) -> ToolResult {
    let start = Instant::now();
    let result = (|| -> Result<String, String> {
        let path = ws.resolve(raw)?;
        ws.check_real(&path)?;
        let meta = std::fs::metadata(&path).map_err(|e| format!("Can't read {raw}: {e}"))?;
        if meta.is_dir() {
            return Err(format!("{raw} is a folder. Use sh with ls to list it."));
        }
        if meta.len() > READ_MAX_FILE {
            return Err(format!("{raw} is larger than 8 MB. Use sh with head, tail or grep."));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("Can't read {raw}: {e}"))?;
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        let total = lines.len();
        if total == 0 {
            return Ok(format!("[{raw} is empty]"));
        }
        if offset > total {
            return Ok(format!("[{raw} has only {total} lines]"));
        }
        let mut out = String::new();
        let mut last = offset - 1;
        for (i, line) in lines.iter().enumerate().skip(offset - 1).take(READ_MAX_LINES) {
            if out.len() + line.len() + 1 > READ_MAX_BYTES && i > offset - 1 {
                break;
            }
            out.push_str(line);
            out.push('\n');
            last = i;
        }
        if offset > 1 || last + 1 < total {
            out.push_str(&format!("[lines {}-{} of {total}]", offset, last + 1));
        }
        Ok(out)
    })();
    match result {
        Ok(text) => ToolResult { text, outcome: Outcome::Ok, exit_code: None, duration_ms: ms(start), full_output: None, image: None },
        Err(e) => ToolResult::simple(Outcome::Failed, e),
    }
}

pub fn write_file(ws: &Workspace, raw: &str, content: &str) -> ToolResult {
    let start = Instant::now();
    let result = (|| -> Result<String, String> {
        if content.len() > WRITE_MAX_BYTES {
            return Err("Content is larger than 1 MB.".into());
        }
        let path = ws.resolve(raw)?;
        if path == ws.root {
            return Err("Give a file name.".into());
        }
        ws.check_real(&path)?;
        if let Ok(m) = std::fs::symlink_metadata(&path) {
            if m.file_type().is_symlink() {
                return Err(format!("{raw} is a link. Write to a real file."));
            }
            if m.is_dir() {
                return Err(format!("{raw} is a folder."));
            }
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Can't create the folder: {e}"))?;
            ws.check_real(parent)?;
        }
        std::fs::write(&path, content).map_err(|e| format!("Can't write {raw}: {e}"))?;
        Ok(format!("Wrote {} bytes to {}.", content.len(), ws.display(&path)))
    })();
    match result {
        Ok(text) => ToolResult { text, outcome: Outcome::Ok, exit_code: None, duration_ms: ms(start), full_output: None, image: None },
        Err(e) => ToolResult::simple(Outcome::Failed, e),
    }
}

// ---------- sh ----------

fn sb_str(p: &Path) -> String {
    let s = p.display().to_string();
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Seatbelt profile: deny by default; read the system but not the user's home;
/// write only the workspace; no Keychain, clipboard or 1Password services.
pub fn sandbox_profile(workspace: &Path, home: &Path, network: bool) -> String {
    let ws = sb_str(workspace);
    let h = sb_str(home);
    let cargo = sb_str(&home.join(".cargo"));
    let rustup = sb_str(&home.join(".rustup"));
    let mut p = format!(
        r#"(version 1)
(deny default)
(allow process-exec process-fork)
(allow signal (target same-sandbox))
(allow sysctl-read)
(allow user-preference-read)
(allow ipc-posix-shm* ipc-posix-sem)
(allow file-map-executable)
(allow mach-lookup)
(deny mach-lookup
  (global-name "com.apple.SecurityServer")
  (global-name "com.apple.pasteboard.1")
  (global-name-prefix "com.1password")
  (global-name-prefix "2BUA8C4S2C.com.1password"))
(allow file-read*)
(deny file-read* (subpath {h}))
(allow file-read-metadata (subpath {h}))
(allow file-read* (subpath {cargo}) (subpath {rustup}))
(allow file-read* file-write* (subpath {ws}))
(allow file-read* file-write* (literal "/dev/null") (literal "/dev/zero") (regex #"^/dev/fd/"))
(allow file-ioctl (literal "/dev/null"))
"#
    );
    if network {
        p.push_str("(allow network* system-socket)\n");
    }
    p
}

pub async fn run_sh(
    ws: &Workspace,
    cmd: &str,
    call_id: &str,
    home: &Path,
    network: bool,
    stop: &mut watch::Receiver<bool>,
) -> ToolResult {
    let start = Instant::now();
    let path = format!(
        "{}/.cargo/bin:/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        home.display()
    );
    let mut c = tokio::process::Command::new("/usr/bin/sandbox-exec");
    c.arg("-p")
        .arg(sandbox_profile(&ws.root, home, network))
        .args(["/bin/zsh", "-f", "-o", "nobgnice", "-c", cmd])
        .current_dir(&ws.root)
        // A clean environment: the app's own env may hold keys (op run, .env).
        .env_clear()
        .env("PATH", path)
        .env("HOME", &ws.root)
        .env("TMPDIR", ws.root.join(".tmp"))
        .env("CARGO_HOME", ws.root.join(".cargo"))
        .env("RUSTUP_HOME", home.join(".rustup"))
        .env("LANG", "en_US.UTF-8")
        .env("TERM", "dumb")
        .env("NO_COLOR", "1")
        .env("PAGER", "cat")
        .env("GIT_PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Own session and process group, so a timeout or Stop kills everything it started.
    // SAFETY: setsid is async-signal-safe and touches no Rust state.
    unsafe {
        c.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => return ToolResult::simple(Outcome::Failed, format!("Could not start the sandbox: {e}")),
    };
    let pgid = child.id().map(|p| p as i32);
    let out_task = tokio::spawn(capture(child.stdout.take()));
    let err_task = tokio::spawn(capture(child.stderr.take()));

    let mut ended = None;
    let mut outcome = Outcome::Ok;
    tokio::select! {
        s = child.wait() => ended = s.ok(),
        _ = tokio::time::sleep(SH_TIMEOUT) => outcome = Outcome::TimedOut,
        _ = stop.changed() => outcome = Outcome::Stopped,
    }
    // Kill the whole group: leftovers (servers started with &) and, on
    // timeout or Stop, the command itself.
    if let Some(g) = pgid {
        // SAFETY: plain syscall on a process group we created.
        unsafe { libc::killpg(g, libc::SIGKILL) };
    }
    if ended.is_none() {
        ended = child.wait().await.ok();
    }
    let join = |t: tokio::task::JoinHandle<Captured>| async move {
        tokio::time::timeout(Duration::from_secs(2), t).await.ok().and_then(|r| r.ok()).unwrap_or_default()
    };
    let (out, err) = (join(out_task).await, join(err_task).await);

    let exit_code = ended.and_then(|s| s.code());
    if outcome == Outcome::Ok && exit_code != Some(0) {
        outcome = Outcome::Failed;
    }
    let mut body = out.text();
    let err_text = err.text();
    if !err_text.trim().is_empty() {
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
        body.push_str("[stderr]\n");
        body.push_str(&err_text);
    }
    let duration_ms = ms(start);
    let head = match outcome {
        Outcome::TimedOut => "Timed out after 60 s and was stopped.".to_string(),
        Outcome::Stopped => "Stopped by the user.".to_string(),
        _ => format!("exit={} time={:.1}s", exit_code.map_or("killed".into(), |c| c.to_string()), duration_ms as f64 / 1000.0),
    };
    let (clipped, full_output) = clip(ws, call_id, &body);
    let text = if clipped.trim().is_empty() { format!("{head}\n(no output)") } else { format!("{head}\n{clipped}") };
    ToolResult { text, outcome, exit_code, duration_ms, full_output, image: None }
}

#[derive(Default)]
struct Captured {
    head: Vec<u8>,
    tail: VecDeque<u8>,
    skipped: usize,
}

impl Captured {
    fn text(&self) -> String {
        let mut s = String::from_utf8_lossy(&self.head).into_owned();
        if self.skipped > 0 {
            s.push_str(&format!("\n[... {} bytes not kept ...]\n", self.skipped));
        }
        let tail: Vec<u8> = self.tail.iter().copied().collect();
        s.push_str(&String::from_utf8_lossy(&tail));
        s
    }
}

async fn capture<R: AsyncRead + Unpin>(stream: Option<R>) -> Captured {
    let mut c = Captured::default();
    let Some(mut r) = stream else { return c };
    let mut buf = [0u8; 8192];
    while let Ok(n) = r.read(&mut buf).await {
        if n == 0 {
            break;
        }
        for &b in &buf[..n] {
            if c.head.len() < CAPTURE_HEAD {
                c.head.push(b);
            } else {
                if c.tail.len() == CAPTURE_TAIL {
                    c.tail.pop_front();
                    c.skipped += 1;
                }
                c.tail.push_back(b);
            }
        }
    }
    c
}

/// Head and tail for the model; the full text goes to `.harness/out/<id>.txt`.
pub fn clip(ws: &Workspace, call_id: &str, body: &str) -> (String, Option<String>) {
    if body.len() <= CLIP_HEAD + CLIP_TAIL + 256 {
        return (body.to_string(), None);
    }
    let safe_id: String = call_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(64).collect();
    let name = if safe_id.is_empty() { uuid::Uuid::new_v4().to_string() } else { safe_id };
    let rel = format!(".harness/out/{name}.txt");
    let saved = std::fs::create_dir_all(ws.root.join(".harness/out"))
        .and_then(|_| std::fs::write(ws.root.join(&rel), body))
        .is_ok();

    let head = cut_at_line(body, CLIP_HEAD, true);
    let tail = cut_at_line(body, CLIP_TAIL, false);
    let skipped = body.len() - head.len() - tail.len();
    let note = if saved {
        format!("\n[... {skipped} bytes clipped. Full output: {rel} (use read with offset) ...]\n")
    } else {
        format!("\n[... {skipped} bytes clipped ...]\n")
    };
    (format!("{head}{note}{tail}"), saved.then_some(rel))
}

/// Up to `max` bytes from the start (or end), on a char boundary, preferring a line break.
fn cut_at_line(s: &str, max: usize, from_start: bool) -> &str {
    if from_start {
        let mut end = max.min(s.len());
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        let cut = &s[..end];
        match cut.rfind('\n') {
            Some(i) if i > max / 2 => &s[..=i],
            _ => cut,
        }
    } else {
        let mut start = s.len().saturating_sub(max);
        while !s.is_char_boundary(start) {
            start += 1;
        }
        let cut = &s[start..];
        match cut.find('\n') {
            Some(i) if i < max / 2 => &cut[i + 1..],
            _ => cut,
        }
    }
}

fn ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> (Workspace, PathBuf) {
        let base = std::env::temp_dir().join(format!("qh-ws-{}-{}", std::process::id(), uuid::Uuid::new_v4()));
        (Workspace::open(&base, "s1").unwrap(), base)
    }

    fn home() -> PathBuf {
        PathBuf::from(std::env::var("HOME").unwrap())
    }

    #[test]
    fn parses_actions() {
        let c = |n: &str, a: &str| ToolCall { id: "1".into(), name: n.into(), arguments: a.into() };
        assert_eq!(Action::parse(&c("sh", r#"{"cmd":"ls"}"#)).unwrap(), Action::Sh { cmd: "ls".into() });
        assert_eq!(
            Action::parse(&c("read", r#"{"path":"a","offset":0}"#)).unwrap(),
            Action::Read { path: "a".into(), offset: 1 }
        );
        assert!(Action::parse(&c("sh", "{}")).is_err());
        assert!(Action::parse(&c("rm", "{}")).is_err());
        assert!(Action::parse(&c("sh", "not json")).is_err());
        assert!(!Action::Read { path: "a".into(), offset: 1 }.needs_approval());
        assert!(Action::Sh { cmd: "ls".into() }.needs_approval());
        let b = |a: &str| Action::parse(&c("browser", a));
        assert!(b(r#"{"action":"open","url":"example.com"}"#).unwrap().needs_approval());
        assert!(!b(r#"{"action":"snapshot"}"#).unwrap().needs_approval());
        assert!(b(r#"{"action":"open"}"#).is_err());
        assert!(b(r#"{"action":"click"}"#).is_err());
        assert!(b(r#"{"action":"download"}"#).is_err());
        assert!(!b(r#"{"action":"network","id":3}"#).unwrap().needs_approval());
        assert!(!b(r#"{"action":"console"}"#).unwrap().needs_approval());
        assert!(!b(r#"{"action":"html","selector":"h1"}"#).unwrap().needs_approval());
        assert!(b(r#"{"action":"eval","code":"1+1"}"#).unwrap().needs_approval());
        assert!(b(r#"{"action":"eval"}"#).is_err());
        assert!(b(r#"{"action":"html"}"#).is_err());
    }

    #[test]
    fn skills() {
        let r = skill_text("browser");
        assert_eq!(r.outcome, Outcome::Ok);
        assert!(r.text.contains("snapshot"));
        assert_eq!(skill_text("hacking").outcome, Outcome::Failed);
        // The schema's enum must list every skill.
        let defs = definitions();
        let skill = defs.as_array().unwrap().iter().find(|d| d["function"]["name"] == "skill").unwrap();
        let names: Vec<&str> = skill["function"]["parameters"]["properties"]["name"]["enum"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(names, SKILLS.iter().map(|(n, _)| *n).collect::<Vec<_>>());
    }

    #[test]
    fn path_guard() {
        let (w, base) = ws();
        assert!(w.resolve("a/b.txt").is_ok());
        assert!(w.resolve("./a/../b.txt").is_ok());
        assert!(w.resolve("../escape.txt").is_err());
        assert!(w.resolve("a/../../escape.txt").is_err());
        assert!(w.resolve("/etc/passwd").is_err());
        let abs = w.root.join("x.txt");
        assert!(w.resolve(abs.to_str().unwrap()).is_ok());

        // A link inside the workspace that points outside is refused.
        std::os::unix::fs::symlink("/tmp", w.root.join("out")).unwrap();
        assert_eq!(write_file(&w, "out/x.txt", "hi").outcome, Outcome::Failed);
        assert_eq!(read_file(&w, "out", 1).outcome, Outcome::Failed);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn write_then_read() {
        let (w, base) = ws();
        let r = write_file(&w, "notes/a.txt", "one\ntwo\nthree\n");
        assert_eq!(r.outcome, Outcome::Ok, "{}", r.text);
        assert_eq!(read_file(&w, "notes/a.txt", 1).text, "one\ntwo\nthree\n");
        assert_eq!(read_file(&w, "notes/a.txt", 2).text, "two\nthree\n[lines 2-3 of 3]");
        let big: String = (0..500).map(|i| format!("line {i}\n")).collect();
        write_file(&w, "big.txt", &big);
        assert!(read_file(&w, "big.txt", 1).text.ends_with("[lines 1-200 of 500]"));
        assert_eq!(read_file(&w, "missing.txt", 1).outcome, Outcome::Failed);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn clips_long_output() {
        let (w, base) = ws();
        let body: String = (0..1000).map(|i| format!("row {i}\n")).collect();
        let (text, full) = clip(&w, "call_1", &body);
        assert!(text.len() < 2600, "{}", text.len());
        assert!(text.starts_with("row 0\n"));
        assert!(text.trim_end().ends_with("row 999"));
        let rel = full.unwrap();
        assert_eq!(std::fs::read_to_string(w.root.join(&rel)).unwrap(), body);
        let (short, none) = clip(&w, "c2", "hello\n");
        assert_eq!((short.as_str(), none), ("hello\n", None));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn sh_runs_sandboxed() {
        let (w, base) = ws();
        let (_tx, mut stop) = watch::channel(false);
        let h = home();

        let r = run_sh(&w, "echo hi > a.txt && cat a.txt", "t1", &h, false, &mut stop).await;
        assert_eq!(r.outcome, Outcome::Ok, "{}", r.text);
        assert!(r.text.starts_with("exit=0") && r.text.contains("hi"), "{}", r.text);

        let r = run_sh(&w, "echo x > /tmp/qh-escape-test.txt", "t2", &h, false, &mut stop).await;
        assert_eq!(r.outcome, Outcome::Failed, "{}", r.text);
        assert!(!Path::new("/tmp/qh-escape-test.txt").exists());

        let r = run_sh(&w, &format!("ls {}", h.join("Documents").display()), "t3", &h, false, &mut stop).await;
        assert_eq!(r.outcome, Outcome::Failed, "home must not be readable: {}", r.text);

        let r = run_sh(&w, "env", "t4", &h, false, &mut stop).await;
        assert!(!r.text.contains("QWEN35_HARNESS_KEY"), "env must be clean");

        let r = run_sh(&w, "exit 3", "t5", &h, false, &mut stop).await;
        assert_eq!((r.outcome, r.exit_code), (Outcome::Failed, Some(3)));

        let r = run_sh(&w, "seq 1 20000", "t6", &h, false, &mut stop).await;
        assert!(r.full_output.is_some() && r.text.len() < 2600);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn sh_stop_kills_group() {
        let (w, base) = ws();
        let (tx, mut stop) = watch::channel(false);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = tx.send(true);
        });
        let t = Instant::now();
        let r = run_sh(&w, "sleep 30 & sleep 30", "t7", &home(), false, &mut stop).await;
        assert_eq!(r.outcome, Outcome::Stopped);
        assert!(t.elapsed() < Duration::from_secs(5));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn approvals() {
        let a = Approvals::default();
        let mut rx = a.register("c1");
        assert!(a.decide("c1", true));
        assert!(rx.try_recv().unwrap());
        assert!(!a.decide("c1", true), "only once");
    }
}
