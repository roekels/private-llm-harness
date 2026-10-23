//! Run chat turns from the command line, through the same code as the app.
//! Uses an in-memory store. Prints events, never the key.
//!
//!   cargo run --example chat -- [options] "first message" "second message" ...
//!
//! Options:
//!   --url URL        server base URL (default http://127.0.0.1:8080/v1)
//!   --model ID       model id (default "local")
//!   --key REF        key reference, e.g. keychain:qwen35-harness/<profile id> (default: none)
//!   --think          Think on
//!   --stop-ms N      press Stop N ms into each turn
//!   --ctx N          pretend the context is N tokens (to test trimming)
//!   --image PATH     attach a PNG or JPEG to the first message
//!   --tools          turn tools on (calls are approved automatically)
//!   --deny           with --tools: deny every call that needs approval
//!   --net            with --tools: allow network in sh
//!   --macos          with --tools: use the macOS sandbox instead of the VM
//!   --until-done     with --tools: answer the first approval with "Approve until done"
//!   --steps N        tool rounds before the turn pauses (default 16)
//!   --skill NAME=TEXT add a custom skill (repeatable); listed in the system prompt
//!   --export DIR     write the session's audit CSV to DIR at the end
//!
//! With --tools the workspace is a temp folder, removed at the end.
//!
//! Example, against a server on your network that needs a key from the Keychain:
//!   cargo run --example chat -- --url http://my-gpu-box:8080/v1 --model local \
//!       --key keychain:qwen35-harness/my-server "Hello"

use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qwen35_harness_lib::chat::{self, ChatEvent, Generation};
use qwen35_harness_lib::profile::Profile;
use qwen35_harness_lib::secrets::SecretCache;
use qwen35_harness_lib::store::Store;
use qwen35_harness_lib::tools::{Approvals, Backend, ToolSettings};
use qwen35_harness_lib::vm::{EnsureOpts, Vm};
use tauri::ipc::{Channel, InvokeResponseBody};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1).peekable();
    let (mut tools, mut deny, mut net, mut macos, mut until_done) = (false, false, false, false, false);
    let mut steps = None::<u32>;
    let mut skills = Vec::new();
    let mut export = None::<String>;
    let (mut url, mut model) = (None::<String>, None::<String>);
    let (mut key, mut think, mut stop_ms, mut ctx, mut image) = (
        "none".to_string(),
        false,
        None::<u64>,
        None::<u32>,
        None::<String>,
    );
    let mut prompts = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--key" => key = args.next().expect("--key REF"),
            "--url" => url = args.next(),
            "--model" => model = args.next(),
            "--think" => think = true,
            "--stop-ms" => stop_ms = args.next().and_then(|v| v.parse().ok()),
            "--ctx" => ctx = args.next().and_then(|v| v.parse().ok()),
            "--image" => image = args.next(),
            "--tools" => tools = true,
            "--deny" => deny = true,
            "--net" => net = true,
            "--macos" => macos = true,
            "--until-done" => until_done = true,
            "--export" => export = args.next(),
            "--steps" => steps = args.next().and_then(|v| v.parse().ok()),
            "--skill" => {
                let kv = args.next().expect("--skill NAME=TEXT");
                let (n, b) = kv.split_once('=').expect("--skill NAME=TEXT");
                skills.push(qwen35_harness_lib::tools::CustomSkill { name: n.into(), body: b.into() });
            }
            _ => prompts.push(a),
        }
    }

    let mut p = Profile::local_default();
    p.key_ref = (key != "none").then_some(key);
    if let Some(u) = url {
        p.base_url = u;
    }
    if let Some(m) = model {
        p.model = m;
    }
    if let Some(c) = ctx {
        p.context_tokens = c;
    }
    let p = p.validate().expect("profile");

    let store = Mutex::new(Store::open_in_memory().expect("store"));
    let names: Vec<String> = skills.iter().map(|s| s.name.clone()).collect();
    let system = chat::system_prompt(qwen35_harness_lib::clock::now_ms(), &names);
    store.lock().unwrap().set_custom_skills(&skills).expect("skills");
    store.lock().unwrap().create_session("cli", Some(&p.id), &system).expect("session");
    let secrets = SecretCache::default();
    let generation = Arc::new(Generation::default());
    let approvals = Arc::new(Approvals::default());
    let workspaces = std::env::temp_dir().join(format!("qh-cli-{}", std::process::id()));
    let home = std::path::PathBuf::from(std::env::var("HOME").expect("HOME"));
    let backend = if macos { Backend::Macos } else { Backend::Vm };
    let settings = ToolSettings { enabled: tools, auto_approve: false, network: net, backend, rate_limit: true, max_steps: steps.unwrap_or(16) };
    let vm = Vm::new(workspaces.clone(), std::env::temp_dir().join("qh-cli-build"));
    if tools && !macos {
        let opts = EnsureOpts { build: true, network: net, patches: Vec::new() };
        vm.ensure(opts, &|s| println!("[vm] {s}")).await.expect("sandbox VM");
    }
    let ap = approvals.clone();

    let channel: Channel<ChatEvent> = Channel::new(move |body| {
        if let InvokeResponseBody::Json(s) = body {
            let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
            match v["type"].as_str() {
                Some("content") => print!("{}", v["text"].as_str().unwrap_or("")),
                Some("reasoning") => eprint!("\x1b[2m{}\x1b[0m", v["text"].as_str().unwrap_or("")),
                Some("done") => println!("\n[done] meta={}", v["message"]["meta"]),
                Some("titled") => println!("[titled] {}", v["title"]),
                Some("compacting") => println!("\n[compacting…]"),
                Some("compacted") => println!(
                    "[compacted] window from seq {}, strip below {}, ~{} -> ~{} tokens\n--- memory ---\n{}\n--------------",
                    v["trimSeq"], v["stripSeq"], v["beforeTokens"], v["afterTokens"], v["memory"].as_str().unwrap_or("")
                ),
                Some("trimmed") => println!("[trimmed] window now starts at seq {}", v["trimSeq"]),
                Some("error") => println!("\n[error] {}", v["message"]),
                Some("paused") => println!("\n[paused] after {} tool rounds", v["steps"]),
                Some("userSaved") => println!("[user seq {}]", v["message"]["seq"]),
                Some("toolPending") => {
                    println!("\n[tool] {} {}", v["name"].as_str().unwrap_or(""), v["arguments"].as_str().unwrap_or(""));
                    if v["needsApproval"] == true {
                        // Answer from another thread, like the UI would.
                        let (ap, id) = (ap.clone(), v["callId"].as_str().unwrap_or("").to_string());
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(50));
                            ap.decide(&id, !deny, until_done);
                            println!("[{}]", if deny { "denied" } else if until_done { "approved until done" } else { "approved" });
                        });
                    }
                }
                Some("toolDone") => {
                    let m = &v["message"];
                    let text = m["content"].as_str().unwrap_or("");
                    let short: String = text.chars().take(300).collect();
                    println!("[result {} via {}] {}", m["meta"]["outcome"], m["meta"]["approval"], short.replace('\n', " | "));
                }
                _ => {}
            }
            let _ = std::io::stdout().flush();
        }
        Ok(())
    });

    for (i, text) in prompts.into_iter().enumerate() {
        println!("\n=== turn {} ===", i + 1);
        let images = match (&image, i) {
            (Some(path), 0) => vec![data_uri(path)],
            _ => vec![],
        };
        let (slot, stop) = generation.start().expect("slot");
        if let Some(ms) = stop_ms {
            let g = generation.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(ms)).await;
                if g.stop() {
                    println!("\n[stop pressed]");
                }
            });
        }
        let ctx = chat::Ctx {
            store: &store,
            secrets: &secrets,
            env_file: Path::new("/nonexistent/.env"),
            approvals: &approvals,
            workspaces: &workspaces,
            home: &home,
            vm: &vm,
        };
        let input = chat::TurnInput { session_id: "cli".into(), text, images, think, tools: settings };
        if let Err(e) = chat::run_turn(ctx, &p, input, stop, &channel).await {
            println!("[failed] {e}");
        }
        drop(slot);
    }
    if let Some(dir) = export {
        let st = store.lock().unwrap();
        let (session, system) = st.session("cli").expect("db").expect("session");
        let msgs = st.messages("cli", 0).expect("messages");
        let path = qwen35_harness_lib::audit::export(Path::new(&dir), &session, &system, &msgs).expect("export");
        println!("[exported] {}", path.display());
    }
    let _ = std::fs::remove_dir_all(&workspaces);
}

fn data_uri(path: &str) -> String {
    let bytes = std::fs::read(path).expect("read image");
    let mime = if path.ends_with(".png") { "image/png" } else { "image/jpeg" };
    // Small base64 encoder, to avoid a dependency for an example.
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = format!("data:{mime};base64,");
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for (i, shift) in [18, 12, 6, 0].iter().enumerate() {
            if i <= c.len() {
                out.push(T[(n >> shift & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
