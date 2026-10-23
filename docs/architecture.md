# Architecture

Ruan's Harness is a Tauri v2 app. A Rust core does everything privileged. A Svelte 5 + TypeScript UI in a locked-down webview shows it.

```
Svelte + TS (webview)                     Rust (Tauri core)
---------------------                     -----------------
chat, sessions, settings,   <-- IPC -->   secrets: keychain: | env:
setup, about, browser pane                chat: history, token budget, compaction, SSE stream
streamed replies (Channel)                store: SQLite (profiles, sessions, messages, patches)
approval cards                            net: private-host policy, connection check
image scaling (1024 px)                   tools: approvals, path guard, clipping
                                          vm: Apple container, firewall, patches, self-test
                                          setup: verified install of Apple container
```

Rule: keys, model HTTP, processes, files and the browser live in Rust. The webview asks through narrow `#[tauri::command]`s and gets results, never secrets. It has no shell, file system or HTTP plugin, a strict CSP, and `freezePrototype`.

## Source map

| Path | What |
| --- | --- |
| `src-tauri/src/commands.rs` | Every IPC command |
| `src-tauri/src/chat.rs` | System prompt, turn loop, token budget, compaction, SSE parsing |
| `src-tauri/src/tools.rs` | Tool schemas, approvals, macOS sandbox, path guard, output clipping, skills |
| `src-tauri/src/vm.rs` | Sandbox VM: build, start, exec, firewall, patches, self-test |
| `src-tauri/src/setup.rs` | Setup assistant: checks, verified download and install of Apple container |
| `src-tauri/src/updates.rs` | Self-update from GitHub releases (Tauri updater plugin, Rust only) |
| `src-tauri/src/audit.rs` | Audit export: a session as CSV in ~/Downloads |
| `src/lib/markdown.ts` | Markdown parser for replies; `Markdown.svelte` renders it as elements, never HTML |
| `src-tauri/src/net.rs` | Host policy and connection check |
| `src-tauri/src/secrets.rs` | Key references and resolution |
| `src-tauri/src/store.rs` | SQLite store, migrations by `user_version` |
| `src-tauri/src/profile.rs` | Connection profiles and sampling presets |
| `src/lib/` | UI state, API wrapper (`api.ts`), chat actions, formatting |
| `src/lib/components/` | Svelte views |
| `sandbox/` | VM image: `Containerfile`, `qh` (tool helper), `qhb` (browser helper), `qh-net` (firewall), `entrypoint` |
| `skills/` | Short best-practice notes the model can fetch with its `skill` tool |

The `sandbox/` and `skills/` files are compiled into the app with `include_str!`. The image tag is a hash of the sandbox files, so a change triggers a rebuild in Setup.

## Connection profiles

| Field | Example |
| --- | --- |
| name | Local llama-server |
| base_url | `http://127.0.0.1:8080/v1` |
| fallback_url | `http://192.168.1.20:8080/v1` (optional, tried when the base host fails to resolve or answer) |
| key_ref | none, `keychain:qwen35-harness/<profile id>`, or `env:LLM_API_KEY` |
| model | `local` |
| context_tokens | 32768 |
| health_path | `/health` (no auth) |
| allow_public | false |

Only private hosts are allowed by default: loopback, RFC 1918, link-local, CGNAT `100.64.0.0/10` (Tailscale) and IPv6 ULA. A host name is allowed when it resolves to one of those. With **Allow public host**, other hosts work, but only over https.

### Connection check

`/health` (no auth), then `/v1/models` (with the key). States: `dns-failed`, `unreachable`, `starting`, `loading`, `ready`, `auth-error`, `key-missing`, `blocked`, `error`. The UI polls every 5 s while loading or starting, 15 s while offline, 30 s when ready, and waits for the user on key problems.

Requests are pinned to the address that passed the host check (no DNS rebinding), use no proxy, and don't follow redirects, so a key can't be carried to another host.

### Key references

| Prefix | Resolved by |
| --- | --- |
| `keychain:service/account` | macOS Keychain. The app only writes under service `qwen35-harness` |
| `env:NAME` | The app's environment, then `.env` in the app data folder |

Resolved keys are cached in Rust memory, zeroed on drop, and dropped on any profile or key change or after a 401.

## Token budget

The design goal is to spend as few tokens per turn as possible. llama-server with one slot reuses its KV cache when a new prompt starts with the same tokens as the last one. So:

1. **Stable prefix.** The system prompt and tool schemas are fixed per session. No timestamps or changing state in them. The system prompt is saved with the session when it is created.
2. **Append only.** History is only added to. Rendering depends only on stored messages, so every turn renders the same prefix.
3. **Times on messages.** Each user message carries its send time (`[Fri 23 Oct 2026 10:42 CEST]`) from the stored time, so the model knows the date without breaking the cache.
4. **Short tool schemas.** Five tools (`sh`, `read`, `write`, `browser`, `skill`), short descriptions. About 470 tokens, cached after the first turn.
5. **Clipped tool output.** The model sees 1 KB head and 1 KB tail. The full output is saved to `.harness/out/<call>.txt` in the session folder, and the model is told the path.
6. **Thinking off by default** (`chat_template_kwargs.enable_thinking=false`). The Think toggle turns it on per message. Old reasoning isn't sent back.
7. **Compaction at 50%.** Before a request, if the context is above half the window:
   - the model writes a session memory (Goal, Key facts and decisions, Progress, Variables and values, Open items; at most 400 words), using the same cached prefix plus one instruction;
   - the window moves to the recent turns (about 20% of the window, never inside the turn in progress);
   - older messages lose heavy parts: images, tool output beyond 300 characters, long call arguments. The newest three tool results and the newest user message stay whole;
   - the memory goes into the system prompt.
   The window start (`trim_seq`), strip point (`strip_seq`) and memory are saved, so the cache breaks once per compaction, not every turn.
8. **Trim at 90%** as a safety net: drop the oldest whole turns down to 60%. A context overflow from the server trims to 45% and retries once.
9. **Side requests reuse the prefix.** The automatic title (after five messages) and the memory both send the session's exact prompt plus one message, with `tool_choice: none`.
10. **One request at a time.** Send is disabled while generating. Stop closes the stream.

The status bar shows prompt tokens, cached tokens and tokens per second from llama.cpp's `timings`.

## Tools

| Tool | What it does | Approval |
| --- | --- | --- |
| `sh` | Run a bash command in the session folder (60 s limit) | Yes, unless auto-approve |
| `read` | Read a file in the session folder | No |
| `write` | Write a file in the session folder | Yes, unless auto-approve |
| `browser` | `open`, `snapshot`, `click`, `type`, `scroll`, `back`, `screenshot`, `network`, `console`, `html`, `eval` | `open`, `click`, `type` and `eval` ask |
| `skill` | Fetch a short best-practice note: built-in (terminal, files, browser, screenshot, research, python) or your own from Settings | No |

A denial ends the turn. Retrying without the tool would change the prompt and lose the cache.

**Approve until done** approves the call and every later one in the same turn, until the model's final answer. The flag lives in Rust (`Approvals`) and resets at the start and end of every turn. Those results are marked `turn` in their approval field.

Up to 16 tool rounds run per message (Settings > Tools, 1 to 200). Then the turn ends with a `paused` event and the UI offers **Continue**, which sends "Continue." as a new message.

Your own skills are stored in the settings table. A new session's system prompt lists their names (only when you have some, so the prompt for everyone else is unchanged), and `skill` looks the text up when it is called. The `skill` schema has no enum, so adding a skill never changes the cached tool schemas.

## Sandbox VM

Tools run in one Linux VM that only this app uses, made with Apple's [`container`](https://github.com/apple/container) (one lightweight VM per container, Apple silicon, macOS 26+).

- **VM:** `qwen35-sandbox`, 4 CPUs, 2 GB. Started on demand, left running, stoppable from Settings.
- **Image:** `qwen35-sandbox:<hash>`, built locally from `sandbox/Containerfile` (Debian trixie slim, bash, Python, Node, git, curl, ripgrep, jq, Chromium, iptables).
- **Users:** the model's commands run as `agent` (no sudo, no capabilities). Only the app runs things as root: the firewall, patches, stopping processes.
- **Files:** `<app data>/workspaces` is mounted at `/work`. A session works in `/work/<session id>`. Nothing else from the Mac is visible.
- **Helpers:** `qh` runs commands and file actions with limits and clipping. `qhb` drives headless Chromium over DevTools on `127.0.0.1:9222` inside the VM, and a recorder keeps the last 400 requests and 200 console entries. The Mac never talks to the browser.
- **Firewall:** `qh-net` sets iptables before each command. See [security.md](security.md).
- **Patches:** bash scripts run as root in the VM. Built-in: Debian security updates. Run them now, or at every VM start (the VM starts fresh from its image). **Rebuild image** starts over from a fresh Debian base.

The older **macOS sandbox** (`sandbox-exec`) is still available in Settings. It is faster to start but runs on your Mac with limits rather than in a separate system, and it has no browser.

### Browser pages as text

`snapshot` walks the DOM and returns visible text and controls with `[n]` refs, at most two screens and 4,000 characters (about 1,000 tokens). Screenshots go to the model as an image (about 700 tokens at 1024 px). Page text starts with an "untrusted web content" line.

## Data

| What | Where |
| --- | --- |
| Profiles, sessions, messages, settings, patches | `~/Library/Application Support/com.dewit.qwen35-harness/harness.db` (SQLite) |
| Session folders | `.../workspaces/<session id>/` |
| Sandbox build context | `.../sandbox-build/` |
| Keys | macOS Keychain, service `qwen35-harness` |

The bundle id and Keychain service keep their original names so existing installs keep their data.
