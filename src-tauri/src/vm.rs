//! The sandbox VM: one Linux container, in its own lightweight VM, run with
//! Apple `container`. Tools run in there as the unprivileged user `agent`.
//! Only the app talks to it, through `container exec`. Inference never
//! touches the VM. See docs/architecture.md and docs/security.md.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex as AsyncMutex, RwLock, RwLockReadGuard, watch};

use crate::tools::{Outcome, ToolResult};

pub const VM_NAME: &str = "qwen35-sandbox";
pub const CLI: &str = "/usr/local/bin/container";
const AGENT: &str = "agent";
const SH_TIMEOUT_S: &str = "60";
const CPUS: &str = "4";
const MEMORY: &str = "2g";
/// The anti-block guard's default: new connections per second to any one address.
pub const RATE_LIMIT: u32 = 20;

/// The image definition, compiled into the app. Its hash names the image tag.
const FILES: [(&str, &str); 5] = [
    ("Containerfile", include_str!("../../sandbox/Containerfile")),
    ("qh", include_str!("../../sandbox/qh")),
    ("qhb", include_str!("../../sandbox/qhb")),
    ("qh-net", include_str!("../../sandbox/qh-net")),
    ("entrypoint", include_str!("../../sandbox/entrypoint")),
];

/// `qwen35-sandbox:<hash>`. FNV-1a, so it's stable across builds of the app.
pub fn image_tag() -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (name, body) in FILES {
        for b in name.bytes().chain([0u8]).chain(body.bytes()) {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{VM_NAME}:{:012x}", h & 0xffff_ffff_ffff)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VmState {
    NotInstalled,
    SystemStopped,
    NoImage,
    Stopped,
    Running,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VmStatus {
    pub state: VmState,
    pub message: String,
    pub image: String,
    pub version: Option<String>,
    /// Firewall mode in the VM: "on", "off", "failed", "unknown".
    pub network: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VmDetails {
    pub os: String,
    pub kernel: String,
    pub arch: String,
    pub cpus: String,
    pub memory_mb: String,
    pub python: String,
    pub node: String,
    pub chromium: String,
}

pub struct Out {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Out {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }

    /// The most useful line of an error, for messages.
    pub fn reason(&self) -> String {
        let s = if self.stderr.trim().is_empty() { &self.stdout } else { &self.stderr };
        s.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("no details").chars().take(300).collect()
    }
}

pub struct Vm {
    /// Host folder mounted at /work.
    pub workspaces: PathBuf,
    /// Where the image files are written before a build.
    build_dir: PathBuf,
    setup: AsyncMutex<()>,
    /// Tool calls hold it shared; patches and rebuilds hold it alone, so a
    /// model command never runs while the firewall is opened for a patch.
    maint: RwLock<()>,
    ready: AtomicBool,
    /// Firewall state last applied: (internet on, rate limit per second).
    net: Mutex<Option<(bool, u32)>>,
    /// Anti-block guard: new connections per second per address; 0 = off.
    rate: AtomicU32,
}

/// What `ensure` should do besides starting the VM.
pub struct EnsureOpts {
    /// Build the image if it's missing (takes minutes the first time).
    pub build: bool,
    /// Firewall mode to leave the VM in.
    pub network: bool,
    /// Patches to run if the VM had to be started (it starts fresh from the image).
    pub patches: Vec<Patch>,
}

impl Vm {
    pub fn new(workspaces: PathBuf, build_dir: PathBuf) -> Self {
        Self {
            workspaces,
            build_dir,
            setup: AsyncMutex::new(()),
            maint: RwLock::new(()),
            ready: AtomicBool::new(false),
            net: Mutex::new(None),
            rate: AtomicU32::new(RATE_LIMIT),
        }
    }

    pub fn installed() -> bool {
        Path::new(CLI).is_file()
    }

    fn forget(&self) {
        self.ready.store(false, Ordering::SeqCst);
        *self.net.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub async fn status(&self) -> VmStatus {
        let image = image_tag();
        let mut st = VmStatus { state: VmState::NotInstalled, message: String::new(), image, version: None, network: None };
        if !Self::installed() {
            st.message = "Apple container is not installed.".into();
            return st;
        }
        st.version = cli(&["--version"], None, Duration::from_secs(10)).await.ok().map(|o| o.stdout.trim().to_string());
        if !system_running().await {
            st.state = VmState::SystemStopped;
            st.message = "The container system is not running.".into();
            return st;
        }
        if !image_present(&st.image).await {
            st.state = VmState::NoImage;
            st.message = "The sandbox image is not built yet.".into();
            return st;
        }
        if !vm_running(&st.image, &self.workspaces).await {
            self.forget();
            st.state = VmState::Stopped;
            st.message = "The sandbox VM is stopped. It starts on the next tool call.".into();
            return st;
        }
        st.state = VmState::Running;
        st.network = self
            .exec("root", "/", &[], &["/usr/local/sbin/qh-net", "status"], None, Duration::from_secs(10))
            .await
            .ok()
            .map(|o| o.stdout.trim().to_string());
        st.message = "The sandbox VM is running.".into();
        st
    }

    /// Make sure the VM runs. Builds the image only when `build` is true,
    /// because that takes minutes the first time.
    /// Returns the results of any auto-run patches, by patch id.
    pub async fn ensure(&self, opts: EnsureOpts, progress: &(dyn Fn(&str) + Sync)) -> Result<Vec<(String, PatchRun)>, String> {
        if self.ready.load(Ordering::SeqCst) {
            return Ok(Vec::new());
        }
        let _guard = self.setup.lock().await;
        if self.ready.load(Ordering::SeqCst) {
            return Ok(Vec::new());
        }
        let build = opts.build;
        let mut runs = Vec::new();
        if !Self::installed() {
            return Err("Apple container is not installed. See Settings > Tools.".into());
        }
        if !system_running().await {
            progress("Starting the container system…");
            let o = cli(&["system", "start", "--enable-kernel-install"], None, Duration::from_secs(600)).await?;
            if !o.ok() && !system_running().await {
                return Err(format!("Could not start the container system: {}", o.reason()));
            }
        }
        let tag = image_tag();
        if !image_present(&tag).await {
            if !build {
                return Err("The sandbox isn't set up yet. Open Settings > Tools and select Set up sandbox.".into());
            }
            progress("Building the sandbox image. The first build downloads Debian and Chromium, a few minutes…");
            self.write_build_files().map_err(|e| format!("Could not write the image files: {e}"))?;
            let ctx = self.build_dir.display().to_string();
            let file = self.build_dir.join("Containerfile").display().to_string();
            let o = cli(&["build", "--tag", &tag, "--file", &file, &ctx], None, Duration::from_secs(1800)).await?;
            let _ = cli(&["builder", "stop"], None, Duration::from_secs(60)).await; // frees its memory
            if !o.ok() {
                return Err(format!("The image build failed: {}", o.reason()));
            }
        }
        if !vm_running(&tag, &self.workspaces).await {
            progress("Starting the sandbox VM…");
            // An old or stopped container with our name is replaced.
            let _ = cli(&["delete", "--force", VM_NAME], None, Duration::from_secs(60)).await;
            std::fs::create_dir_all(&self.workspaces).map_err(|e| e.to_string())?;
            let mount = format!("{}:/work", self.workspaces.display());
            let o = cli(
                &[
                    "run", "--detach", "--name", VM_NAME, "--cpus", CPUS, "--memory", MEMORY, "--volume", &mount,
                    // Root (used only by the app) sets the firewall. The model's user has no capabilities.
                    "--cap-add", "CAP_NET_ADMIN", &tag,
                ],
                None,
                Duration::from_secs(180),
            )
            .await?;
            if !o.ok() {
                return Err(format!("Could not start the sandbox VM: {}", o.reason()));
            }
            *self.net.lock().unwrap_or_else(|e| e.into_inner()) = None;
            let started = Instant::now();
            loop {
                match self.exec("root", "/", &[], &["true"], None, Duration::from_secs(10)).await {
                    Ok(o) if o.ok() => break,
                    _ if started.elapsed() > Duration::from_secs(60) => {
                        return Err("The sandbox VM started but does not answer.".into());
                    }
                    _ => tokio::time::sleep(Duration::from_millis(500)).await,
                }
            }
            // Fresh VM: re-apply the patches marked "run at start".
            for p in opts.patches.iter().filter(|p| p.auto_run) {
                progress(&format!("Running patch: {}", p.name));
                let run = self.run_patch_locked(p, progress).await;
                if !run.ok {
                    progress(&format!("Patch failed: {} ({})", p.name, run.summary));
                }
                runs.push((p.id.clone(), run));
            }
        }
        self.set_network(opts.network).await?;
        self.ready.store(true, Ordering::SeqCst);
        Ok(runs)
    }

    /// Shared lock for tool calls (see `maint`).
    pub async fn tool_lock(&self) -> RwLockReadGuard<'_, ()> {
        self.maint.read().await
    }

    /// Run one patch now, as root. The VM must be set up. Internet is open
    /// during the patch, then the firewall goes back to `network`.
    pub async fn run_patch(&self, p: &Patch, network: bool, on_line: &(dyn Fn(&str) + Sync)) -> PatchRun {
        let started = Instant::now();
        let opts = EnsureOpts { build: false, network, patches: Vec::new() };
        if let Err(e) = self.ensure(opts, on_line).await {
            return PatchRun::failed(started, e);
        }
        let _w = self.maint.write().await;
        let run = self.run_patch_locked(p, on_line).await;
        if let Err(e) = self.set_network(network).await {
            on_line(&e);
        }
        run
    }

    async fn run_patch_locked(&self, p: &Patch, on_line: &(dyn Fn(&str) + Sync)) -> PatchRun {
        let started = Instant::now();
        if let Err(e) = self.set_network(true).await {
            return PatchRun::failed(started, e);
        }
        on_line(&format!("$ {}", p.name));
        let env = format!("QH_PATCH={}", p.script);
        let args = [
            "exec", "--user", "root", "--workdir", "/root", "--env", &env, VM_NAME,
            "bash", "-c", "set -e -o pipefail\neval \"$QH_PATCH\"",
        ];
        match cli_stream(&args, on_line, PATCH_TIMEOUT).await {
            Ok(o) => PatchRun {
                at: now_ms(),
                ok: o.ok(),
                ms: started.elapsed().as_millis() as u64,
                summary: if o.ok() { "Done".into() } else { format!("Exit {}. Last output: {}", o.code.unwrap_or(-1), o.reason()) },
            },
            Err(e) => PatchRun::failed(started, e),
        }
    }

    /// Fresh base image, no cache, then restart the VM (patches re-run at start).
    pub async fn rebuild(&self, on_line: &(dyn Fn(&str) + Sync)) -> Result<(), String> {
        let _guard = self.setup.lock().await;
        let _w = self.maint.write().await;
        if !Self::installed() {
            return Err("Apple container is not installed.".into());
        }
        if !system_running().await {
            return Err("The container system is not running. Select Start VM first.".into());
        }
        self.write_build_files().map_err(|e| format!("Could not write the image files: {e}"))?;
        let tag = image_tag();
        let ctx = self.build_dir.display().to_string();
        let file = self.build_dir.join("Containerfile").display().to_string();
        on_line("Rebuilding the image from a fresh Debian base…");
        let args = ["build", "--no-cache", "--pull", "--progress", "plain", "--tag", &tag, "--file", &file, &ctx];
        let o = cli_stream(&args, on_line, Duration::from_secs(1800)).await?;
        let _ = cli(&["builder", "stop"], None, Duration::from_secs(60)).await; // frees its memory
        if !o.ok() {
            return Err(format!("The rebuild failed: {}", o.reason()));
        }
        on_line("Replacing the running VM…");
        self.forget();
        let _ = cli(&["stop", VM_NAME], None, Duration::from_secs(60)).await;
        let _ = cli(&["delete", "--force", VM_NAME], None, Duration::from_secs(60)).await;
        let _ = cli(&["image", "prune"], None, Duration::from_secs(120)).await;
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let _guard = self.setup.lock().await;
        self.forget();
        if !Self::installed() {
            return Ok(());
        }
        let _ = cli(&["stop", VM_NAME], None, Duration::from_secs(60)).await;
        let _ = cli(&["delete", "--force", VM_NAME], None, Duration::from_secs(60)).await;
        Ok(())
    }

    fn write_build_files(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.build_dir)?;
        for (name, body) in FILES {
            std::fs::write(self.build_dir.join(name), body)?;
        }
        Ok(())
    }

    /// Turn the anti-block guard on (`per_sec` > 0) or off. Applies on the
    /// next firewall change, which the next command or browser action makes.
    pub fn set_rate_limit(&self, per_sec: u32) {
        self.rate.store(per_sec, Ordering::SeqCst);
    }

    /// Firewall in the VM: `true` = internet only (with the rate limit),
    /// `false` = loopback only.
    async fn set_network(&self, on: bool) -> Result<(), String> {
        let rate = if on { self.rate.load(Ordering::SeqCst) } else { 0 };
        if *self.net.lock().unwrap_or_else(|e| e.into_inner()) == Some((on, rate)) {
            return Ok(());
        }
        let mode = if on { "on" } else { "off" };
        let rate_arg = rate.to_string();
        let o = self
            .exec("root", "/", &[], &["/usr/local/sbin/qh-net", mode, &rate_arg], None, Duration::from_secs(20))
            .await?;
        if !o.ok() || o.stdout.split_whitespace().next() != Some(mode) {
            return Err(format!("The sandbox firewall did not load, so the command was not run: {}", o.reason()));
        }
        *self.net.lock().unwrap_or_else(|e| e.into_inner()) = Some((on, rate));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn exec(
        &self,
        user: &str,
        workdir: &str,
        env: &[(&str, &str)],
        cmd: &[&str],
        stdin: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<Out, String> {
        let envs: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let mut args: Vec<&str> = vec!["exec", "--user", user, "--workdir", workdir];
        for e in &envs {
            args.extend(["--env", e.as_str()]);
        }
        if stdin.is_some() {
            args.push("--interactive");
        }
        args.push(VM_NAME);
        args.extend(cmd);
        let out = cli(&args, stdin, timeout).await;
        if let Ok(o) = &out
            && !o.ok()
            && o.stderr.to_lowercase().contains("not found")
        {
            self.forget(); // the VM went away; start it again next time
        }
        out
    }

    // ---------- Tools ----------

    fn workdir(&self, session_id: &str) -> Result<String, String> {
        // Created from the Mac side so it exists before exec sets it as cwd.
        crate::tools::Workspace::open(&self.workspaces, session_id)?;
        Ok(format!("/work/{session_id}"))
    }

    pub async fn sh(
        &self,
        session_id: &str,
        call_id: &str,
        cmd: &str,
        network: bool,
        stop: &mut watch::Receiver<bool>,
    ) -> ToolResult {
        let wd = match self.workdir(session_id) {
            Ok(w) => w,
            Err(e) => return ToolResult::simple(Outcome::Failed, e),
        };
        if let Err(e) = self.set_network(network).await {
            return ToolResult::simple(Outcome::Failed, e);
        }
        let env = [("QH_CMD", cmd)];
        let args = ["qh", "run", call_id, SH_TIMEOUT_S];
        let run = self.exec(AGENT, &wd, &env, &args, None, Duration::from_secs(120));
        tokio::pin!(run);
        let mut stopped = false;
        let out = tokio::select! {
            r = &mut run => r,
            _ = stop.changed() => {
                stopped = true;
                let _ = self.exec("root", "/", &[], &["qh", "kill", call_id], None, Duration::from_secs(10)).await;
                run.await
            }
        };
        match out {
            Ok(o) => parse_run(&o, stopped),
            Err(e) => ToolResult::simple(if stopped { Outcome::Stopped } else { Outcome::Failed }, e),
        }
    }

    pub async fn read(&self, session_id: &str, path: &str, offset: usize) -> ToolResult {
        let start = Instant::now();
        let wd = match self.workdir(session_id) {
            Ok(w) => w,
            Err(e) => return ToolResult::simple(Outcome::Failed, e),
        };
        let off = offset.to_string();
        let out = self.exec(AGENT, &wd, &[], &["qh", "read", path, &off], None, Duration::from_secs(30)).await;
        simple_result(out, start)
    }

    pub async fn write(&self, session_id: &str, path: &str, content: &str) -> ToolResult {
        let start = Instant::now();
        let wd = match self.workdir(session_id) {
            Ok(w) => w,
            Err(e) => return ToolResult::simple(Outcome::Failed, e),
        };
        let out = self.exec(AGENT, &wd, &[], &["qh", "write", path], Some(content.as_bytes()), Duration::from_secs(30)).await;
        simple_result(out, start)
    }
}

impl Vm {
    /// One browser action for the model. `args` is the validated JSON object.
    pub async fn browser(&self, args: &str, network: bool, stop: &mut watch::Receiver<bool>) -> ToolResult {
        let start = Instant::now();
        if let Err(e) = self.set_network(network).await {
            return ToolResult::simple(Outcome::Failed, e);
        }
        let env = [("QH_ARGS", args)];
        let run = self.exec(AGENT, "/work", &env, &["qhb"], None, Duration::from_secs(60));
        tokio::pin!(run);
        let out = tokio::select! {
            r = &mut run => r,
            _ = stop.changed() => return ToolResult::simple(Outcome::Stopped, "Stopped by the user."),
        };
        let mut res = simple_result(out, start);
        // A screenshot comes back as a first line {"image": "data:..."}.
        if res.outcome == Outcome::Ok && res.text.starts_with("{\"image\"") {
            let (first, rest) = res.text.split_once('\n').unwrap_or((res.text.as_str(), ""));
            let image = serde_json::from_str::<serde_json::Value>(first)
                .ok()
                .and_then(|v| v["image"].as_str().map(str::to_string));
            res = ToolResult { text: rest.to_string(), image, ..res };
        }
        res
    }

    /// Actions from the app's browser pane (the user, not the model). Returns qhb's stdout.
    pub async fn browser_user(&self, args: &serde_json::Value, network: bool) -> Result<String, String> {
        const OK: [&str; 7] = ["view", "open", "back", "reload", "scroll", "click-xy", "insert"];
        let action = args["action"].as_str().unwrap_or("");
        if !OK.contains(&action) {
            return Err("Unknown browser action.".into());
        }
        if action != "view" {
            self.set_network(network).await?;
        }
        let json = args.to_string();
        let o = self.exec(AGENT, "/work", &[("QH_ARGS", &json)], &["qhb"], None, Duration::from_secs(60)).await?;
        if o.ok() { Ok(o.stdout) } else { Err(if o.stdout.is_empty() { o.reason() } else { o.stdout }) }
    }

    /// Isolation checks inside the VM, as the model's user. Opens the firewall
    /// (internet only) for the network checks, then restores `network`.
    pub async fn selftest(&self, network: bool) -> Result<Vec<crate::setup::Check>, String> {
        const CHECK: &str = r#"c() { if eval "$2" >/dev/null 2>&1; then echo "$1|1|$3"; else echo "$1|0|$4"; fi; }
"#;
        const LOCAL: &str = r#"c "Your Mac's files are hidden" "test ! -e /Users" "No /Users folder in the VM." "/Users is visible in the VM."
c "No admin rights" "! command -v sudo && ! iptables -L" "No sudo; the firewall can't be changed." "The model's user has admin tools."
c "Only the session folders are shared" "[ "$(mount | grep -c virtiofs)" = 1 ]" "One shared folder: /work." "More than one folder is shared."
c "No network when it's off" "! curl -s -m 5 -o /dev/null https://example.com" "Nothing got out." "A request got out with the network off."
"#;
        const NET: &str = r#"gw=$(ip route | awk '/default/ {print $3; exit}')
c "Internet works when allowed" "curl -s -m 10 -o /dev/null https://example.com" "example.com answered." "example.com didn't answer. Is this Mac online?"
c "This Mac is blocked" "! curl -s -m 4 -o /dev/null http://$gw/" "This Mac ($gw) refused the connection." "This Mac answered."
c "The tailnet is blocked" "! curl -s -m 4 -o /dev/null http://100.100.100.100/" "The tailnet (100.100.100.100) refused." "The tailnet answered."
c "Your home network is blocked" "! curl -s -m 4 -o /dev/null http://192.168.1.1/" "192.168.1.1 refused." "192.168.1.1 answered."
"#;
        // 80 connection attempts at once to 203.0.113.10, a documentation
        // address that goes nowhere, so no real site is hit. That address is
        // refused anyway, so the check counts what the guard rule itself
        // stopped (its packet counter, read as root before and after).
        const BURST: &str = r#"for i in $(seq 1 80); do curl -s -m 2 -o /dev/null http://203.0.113.10/ & done; wait"#;
        const COUNTER: &str = r#"iptables -L OUTPUT -v -n -x | awk '/limit: above/ {print $1; exit}'"#;
        let _w = self.maint.write().await;
        let mut checks = Vec::new();
        let limited = self.rate.load(Ordering::SeqCst) > 0;
        for (open, body) in [(false, LOCAL), (true, NET)] {
            self.set_network(open).await?;
            let script = format!("{CHECK}{body}");
            let o = self.exec(AGENT, "/work", &[], &["bash", "-c", &script], None, Duration::from_secs(90)).await?;
            for line in o.stdout.lines() {
                let mut p = line.splitn(3, '|');
                if let (Some(name), Some(ok), Some(detail)) = (p.next(), p.next(), p.next()) {
                    checks.push(crate::setup::Check { name: name.into(), passed: ok == "1", detail: detail.into() });
                }
            }
        }
        if limited {
            // Network is on with the guard (set by the loop above).
            let read = || self.exec("root", "/", &[], &["bash", "-c", COUNTER], None, Duration::from_secs(20));
            let before: u64 = read().await.map(|o| o.stdout.trim().parse().unwrap_or(0)).unwrap_or(0);
            let _ = self.exec(AGENT, "/work", &[], &["bash", "-c", BURST], None, Duration::from_secs(60)).await;
            let after: u64 = read().await.map(|o| o.stdout.trim().parse().unwrap_or(0)).unwrap_or(0);
            let stopped = after.saturating_sub(before);
            checks.push(crate::setup::Check {
                name: "Bursts to one site are held back".into(),
                passed: stopped >= 20,
                detail: if stopped >= 20 {
                    format!("The guard stopped {stopped} of 80 rapid connections to one address.")
                } else {
                    format!("The guard stopped only {stopped} of 80 rapid connections.")
                },
            });
        } else {
            checks.push(crate::setup::Check {
                name: "Bursts to one site are held back".into(),
                passed: true,
                detail: "The rate limit is off in Settings, so this wasn't tested.".into(),
            });
        }
        self.set_network(network).await?;
        if checks.len() < 9 {
            return Err("The self-test didn't finish inside the VM.".into());
        }
        Ok(checks)
    }

    /// Versions inside the VM, for the About view. None if the VM isn't running.
    pub async fn details(&self) -> Option<VmDetails> {
        let script = r#". /etc/os-release; echo "$PRETTY_NAME"; uname -r; uname -m; nproc; \
awk '/MemTotal/ {print int($2/1024)}' /proc/meminfo; python3 --version 2>&1 | cut -d' ' -f2; \
node --version 2>&1; chromium --version 2>&1 | cut -d' ' -f2"#;
        let o = self.exec("root", "/", &[], &["bash", "-c", script], None, Duration::from_secs(20)).await.ok()?;
        if !o.ok() {
            return None;
        }
        let l: Vec<String> = o.stdout.lines().map(|s| s.trim().to_string()).collect();
        let get = |i: usize| l.get(i).cloned().unwrap_or_default();
        Some(VmDetails {
            os: get(0),
            kernel: get(1),
            arch: get(2),
            cpus: get(3),
            memory_mb: get(4),
            python: get(5),
            node: get(6),
            chromium: get(7),
        })
    }

    /// After the container system is stopped from outside (an upgrade).
    pub fn forget_ready(&self) {
        self.forget();
    }

    /// Delete sandbox images other than the current one. Returns how many.
    pub async fn remove_old_images(&self) -> usize {
        let current = image_tag();
        let current = current.rsplit(':').next().unwrap_or("");
        let mut n = 0;
        for t in sandbox_image_tags().await.into_iter().filter(|t| t != current) {
            let r = format!("{VM_NAME}:{t}");
            if cli(&["image", "delete", &r], None, Duration::from_secs(120)).await.is_ok_and(|o| o.ok()) {
                n += 1;
            }
        }
        n
    }

    /// True when the VM runs (cheap: no CLI call when already known).
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }
}

fn simple_result(out: Result<Out, String>, start: Instant) -> ToolResult {
    match out {
        Ok(o) if o.ok() => ToolResult {
            text: o.stdout,
            outcome: Outcome::Ok,
            exit_code: None,
            duration_ms: start.elapsed().as_millis() as u64,
            full_output: None,
            image: None,
        },
        Ok(o) if o.code == Some(1) && !o.stdout.is_empty() => ToolResult::simple(Outcome::Failed, o.stdout),
        Ok(o) => ToolResult::simple(Outcome::Failed, format!("Sandbox error: {}", o.reason())),
        Err(e) => ToolResult::simple(Outcome::Failed, e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunHeader {
    exit: Option<i32>,
    timed_out: bool,
    ms: u64,
    full: Option<String>,
}

/// `qh run` output: one JSON header line, then the text for the model.
pub fn parse_run(o: &Out, stopped: bool) -> ToolResult {
    let (first, rest) = o.stdout.split_once('\n').unwrap_or((o.stdout.as_str(), ""));
    let Ok(h) = serde_json::from_str::<RunHeader>(first) else {
        let outcome = if stopped { Outcome::Stopped } else { Outcome::Failed };
        return ToolResult::simple(outcome, format!("Sandbox error: {}", o.reason()));
    };
    let outcome = if stopped {
        Outcome::Stopped
    } else if h.timed_out {
        Outcome::TimedOut
    } else if h.exit == Some(0) {
        Outcome::Ok
    } else {
        Outcome::Failed
    };
    let head = match outcome {
        Outcome::TimedOut => "Timed out after 60 s and was stopped.".to_string(),
        Outcome::Stopped => "Stopped by the user.".to_string(),
        _ => format!("exit={} time={:.1}s", h.exit.map_or("killed".into(), |c| c.to_string()), h.ms as f64 / 1000.0),
    };
    let text = if rest.trim().is_empty() { format!("{head}\n(no output)") } else { format!("{head}\n{rest}") };
    ToolResult { text, outcome, exit_code: h.exit, duration_ms: h.ms, full_output: h.full, image: None }
}

/// Tags of all qwen35-sandbox images on this Mac.
pub(crate) async fn sandbox_image_tags() -> Vec<String> {
    let Ok(o) = cli(&["image", "list", "--format", "json"], None, Duration::from_secs(30)).await else { return Vec::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&o.stdout) else { return Vec::new() };
    let mut tags = Vec::new();
    // Each entry carries its reference somewhere in its JSON; match on the name.
    for entry in v.as_array().into_iter().flatten() {
        let text = entry.to_string();
        let mut rest = text.as_str();
        while let Some(i) = rest.find("qwen35-sandbox:") {
            let tail = &rest[i + "qwen35-sandbox:".len()..];
            let tag: String = tail.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            if !tag.is_empty() && !tags.contains(&tag) {
                tags.push(tag);
            }
            rest = tail;
        }
    }
    tags
}

/// `container --version` as (major, minor, patch), if installed.
pub(crate) async fn container_version() -> Option<(u32, u32, u32)> {
    if !Vm::installed() {
        return None;
    }
    let o = cli(&["--version"], None, Duration::from_secs(10)).await.ok()?;
    parse_version(&o.stdout)
}

/// First "x.y.z" in the text.
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    s.split(|c: char| !(c.is_ascii_digit() || c == '.')).find_map(|w| {
        let p: Vec<u32> = w.split('.').filter_map(|n| n.parse().ok()).collect();
        (p.len() >= 3 && w.split('.').count() >= 3).then(|| (p[0], p[1], p[2]))
    })
}

// ---------- Patches ----------

const PATCH_TIMEOUT: Duration = Duration::from_secs(1800);
pub const SYSTEM_UPDATES_ID: &str = "system-updates";
const SYSTEM_UPDATES: &str = "export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get -y -o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold upgrade
apt-get -y autoremove
apt-get clean";

/// A script run as root in the VM, from Settings only. The model can't see,
/// change or start patches.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
    pub id: String,
    pub name: String,
    /// Bash, run with `set -e` as root in /root.
    pub script: String,
    /// Run again each time the VM starts (it starts fresh from the image).
    #[serde(default)]
    pub auto_run: bool,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub last_run: Option<PatchRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchRun {
    pub at: i64,
    pub ok: bool,
    pub ms: u64,
    pub summary: String,
}

impl PatchRun {
    fn failed(started: Instant, why: String) -> Self {
        Self { at: now_ms(), ok: false, ms: started.elapsed().as_millis() as u64, summary: why }
    }
}

pub fn builtin_patches() -> Vec<Patch> {
    vec![Patch {
        id: SYSTEM_UPDATES_ID.into(),
        name: "System updates (Debian security and package updates)".into(),
        script: SYSTEM_UPDATES.into(),
        auto_run: false,
        builtin: true,
        last_run: None,
    }]
}

impl Patch {
    pub fn validate(mut self) -> Result<Self, String> {
        self.id = self.id.trim().to_string();
        if self.id.is_empty() || self.id.len() > 64 || !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err("Patch id must be letters, digits or dashes.".into());
        }
        self.name = self.name.trim().to_string();
        if self.name.is_empty() || self.name.chars().count() > 80 {
            return Err("Patch name must be 1 to 80 characters.".into());
        }
        if self.script.trim().is_empty() || self.script.len() > 20_000 {
            return Err(format!("\"{}\": the script must be 1 to 20,000 characters.", self.name));
        }
        Ok(self)
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

// ---------- container CLI ----------

/// Like `cli`, but hands each output line (stdout and stderr) to `on_line`
/// as it arrives. Keeps the last lines for error messages.
pub(crate) async fn cli_stream(args: &[&str], on_line: &(dyn Fn(&str) + Sync), timeout: Duration) -> Result<Out, String> {
    let mut child = tokio::process::Command::new(CLI)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Could not run container: {e}"))?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    for pipe in [child.stdout.take().map(|p| Box::new(p) as Box<dyn tokio::io::AsyncRead + Send + Unpin>),
                 child.stderr.take().map(|p| Box::new(p) as Box<dyn tokio::io::AsyncRead + Send + Unpin>)]
        .into_iter()
        .flatten()
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(pipe).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                if tx.send(l).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    let mut tail: VecDeque<String> = VecDeque::new();
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            line = rx.recv() => match line {
                Some(l) => {
                    let l: String = l.chars().take(500).collect();
                    on_line(&l);
                    if tail.len() == 40 {
                        tail.pop_front();
                    }
                    tail.push_back(l);
                }
                None => break,
            },
            _ = &mut deadline => {
                let _ = child.kill().await;
                return Err(format!("container {} took longer than {} min.", args.first().unwrap_or(&""), timeout.as_secs() / 60));
            }
        }
    }
    let status = child.wait().await.map_err(|e| format!("container failed: {e}"))?;
    let text = tail.into_iter().collect::<Vec<_>>().join("\n");
    Ok(Out { code: status.code(), stdout: String::new(), stderr: text })
}

pub(crate) async fn cli(args: &[&str], stdin: Option<&[u8]>, timeout: Duration) -> Result<Out, String> {
    let mut c = tokio::process::Command::new(CLI);
    c.args(args)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = c.spawn().map_err(|e| format!("Could not run container: {e}"))?;
    if let (Some(data), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(data).await.map_err(|e| format!("Could not send input: {e}"))?;
        drop(pipe); // EOF
    }
    let out = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("container {} took longer than {} s.", args.first().unwrap_or(&""), timeout.as_secs()))?
        .map_err(|e| format!("container failed: {e}"))?;
    Ok(Out {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

pub(crate) async fn system_running() -> bool {
    cli(&["system", "status"], None, Duration::from_secs(15)).await.is_ok_and(|o| o.ok())
}

pub(crate) async fn image_present(tag: &str) -> bool {
    let hash = tag.rsplit(':').next().unwrap_or(tag);
    cli(&["image", "list", "--format", "json"], None, Duration::from_secs(30))
        .await
        .is_ok_and(|o| o.ok() && o.stdout.contains(VM_NAME) && o.stdout.contains(hash))
}

/// Running, from the current image (an older image means restart), with our
/// workspaces folder mounted (another tool may have started it with a different one).
async fn vm_running(tag: &str, workspaces: &Path) -> bool {
    let hash = tag.rsplit(':').next().unwrap_or(tag);
    let Ok(o) = cli(&["inspect", VM_NAME], None, Duration::from_secs(15)).await else { return false };
    let mount = format!("\"{}\"", workspaces.display());
    o.ok() && o.stdout.contains("\"running\"") && o.stdout.contains(hash) && o.stdout.contains(&mount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("container CLI version 1.6.0 (build: release)"), Some((1, 6, 0)));
        assert_eq!(parse_version("1.10.2"), Some((1, 10, 2)));
        assert_eq!(parse_version("v2"), None);
        assert!(parse_version("1.5.9").unwrap() < (1, 6, 0));
    }

    #[test]
    fn tag_is_stable_and_named() {
        let t = image_tag();
        assert_eq!(t, image_tag());
        assert!(t.starts_with("qwen35-sandbox:") && t.len() == "qwen35-sandbox:".len() + 12);
    }

    fn out(stdout: &str) -> Out {
        Out { code: Some(0), stdout: stdout.into(), stderr: String::new() }
    }

    #[test]
    fn parses_run_output() {
        let r = parse_run(&out("{\"exit\":0,\"signal\":null,\"timedOut\":false,\"ms\":120,\"bytes\":3,\"full\":null}\nhi\n"), false);
        assert_eq!(r.outcome, Outcome::Ok);
        assert_eq!(r.text, "exit=0 time=0.1s\nhi\n");

        let r = parse_run(&out("{\"exit\":3,\"timedOut\":false,\"ms\":5,\"full\":\".harness/out/c.txt\"}\nx"), false);
        assert_eq!((r.outcome, r.exit_code, r.full_output.as_deref()), (Outcome::Failed, Some(3), Some(".harness/out/c.txt")));

        let r = parse_run(&out("{\"exit\":null,\"timedOut\":true,\"ms\":60000,\"full\":null}\n"), false);
        assert_eq!(r.outcome, Outcome::TimedOut);
        assert!(r.text.contains("(no output)"));

        let r = parse_run(&out("{\"exit\":null,\"timedOut\":false,\"ms\":10,\"full\":null}\n"), true);
        assert_eq!(r.outcome, Outcome::Stopped);

        let bad = Out { code: Some(1), stdout: String::new(), stderr: "Error: container not found".into() };
        assert_eq!(parse_run(&bad, false).outcome, Outcome::Failed);
    }
}
