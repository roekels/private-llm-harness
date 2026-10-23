# Ruan's Harness (repo private-llm-harness)

A token-lean agent harness for a self-hosted, OpenAI-compatible model: chat, sessions, and terminal and browser tools in an isolated Linux VM. macOS desktop app on Apple silicon. Status: 0.7.1 Beta.

If `private/AGENTS-private.md` exists (it is gitignored), read it too: it holds the maintainer's own setup notes.

## Stack and layout

- Tauri v2. Rust core in `src-tauri/`, Svelte 5 + TypeScript UI in `src/`, built with Vite.
- Docs: `docs/getting-started.md` (setup), `docs/architecture.md` (design, token budget), `docs/security.md` (sandbox, keys), `docs/development.md` (checks, examples, release), `docs/roadmap.md`.
- Rust: `commands.rs` (all IPC), `chat.rs` (turn loop, budget, compaction, SSE), `tools.rs` (tools, macOS sandbox, path guard, clipping), `vm.rs` (sandbox VM, patches, self-test), `setup.rs` (verified install of Apple container), `secrets.rs`, `net.rs` (host policy, probe), `store.rs` (SQLite, migrations by `user_version`), `profile.rs`.
- UI: `api.ts` wraps every command; `chat.svelte.ts` session and send actions; `connection.svelte.ts` polls status; `state.svelte.ts` app state; `format.ts` renders text only; `theme.css` holds the color tokens.
- `sandbox/`: the VM image. `skills/`: notes for the model's `skill` tool. Both are compiled in with `include_str!`.

## Run and check

```bash
npm install
npm run tauri dev
npm run check
cd src-tauri && cargo clippy --all-targets && cargo test
```

Run the checks after every change. CLI examples (`probe`, `chat`, `vm`, `setup`) are in `docs/development.md`.

## Rules

- Privileged work (keys, model HTTP, processes, files, browser) lives in Rust. The webview never receives a secret.
- No Tauri shell, fs or http plugins in the webview. Add narrow `#[tauri::command]`s instead.
- Secrets: only `keychain:` or `env:` references are stored. Never log, print or commit a value. Mask keys in errors.
- Never commit hosts, IPs, tailnet names, keys or personal details. `scripts/check-public.sh` is the pre-commit guard; keep it passing.
- Keep the prompt prefix stable: the system prompt and `tools::definitions()` are cached. Change them rarely, keep them short, never put the time or changing state in them. Times go on user messages (`stamped`).
- Never edit or reorder sent history. Only `trim_seq` and `strip_seq` move, forward. `chat::render` depends only on stored messages and `strip_seq`.
- The system prompt is stored per session at creation. Changing it only affects new sessions.
- `skills/*.md`: adding one means the file, the `SKILLS` list and the schema enum (a test checks).
- Qwen ignores soft hints. The system prompt says to call `skill` once per topic before first use. Keep it that direct.
- `read`/`write` on the macOS backend run in the app: keep the path guard (`Workspace::resolve` + `check_real`) on every path.
- Render model and web output as text. No `{@html}`.
- Use the CSS variables from `theme.css`, never raw hex in components.
- Don't rename the bundle id `com.dewit.qwen35-harness` or the Keychain service `qwen35-harness`: existing data and keys depend on them.
- The container installer must pass every check in `setup::verify_pkg`. Never relax them.
- The model's VM user (`agent`) has no caps and no sudo. Only the app execs as root.

## Gotchas

- Qwen3.6 thinks by default. Without `chat_template_kwargs.enable_thinking=false`, small `max_tokens` gives empty replies.
- llama-server has one slot. Concurrent requests queue, and switching sessions throws away the KV cache. Side requests (title, memory) must reuse the session's exact prompt plus one message.
- `keyring` v4 changed its API. We use `security-framework` directly.
- Ad-hoc builds change code signature on every rebuild, so macOS may ask again for Keychain access.
- Root in the VM needs `--cap-add CAP_NET_ADMIN` for iptables.
- The firewall fails closed: if `qh-net` doesn't print the mode, `sh` refuses to run.
- 203.0.113.0/24 is refused by macOS for the VM anyway, so the burst self-test counts the guard rule's iptables counter. Don't test the guard against real hosts.
- `vm_running` also checks the mounted folder; the CLI examples may start the VM with a temp folder.
- `container build` leaves a `buildkit` VM running (2 GB). The app runs `container builder stop` after builds.
- Changing any file in `sandbox/` changes the image tag. Tool calls never build; Setup must run.
- Headless Chromium gets bot checks on some sites. Don't spoof the user agent or work around bot detection.
- `qhb` re-numbers `[n]` refs on every snapshot. Redaction lives in `is_secret`, `secret_param` and `redact_query`; run `cargo run --example setup -- redact` after touching them (must print `leaked anywhere: false`).
- When Stop is pressed, llama.cpp sends no usage. The meter falls back to estimates.
- Release builds need `[profile.release.build-override] strip = false` in Cargo.toml, or the build fails with "can't find crate for phf_macros".
- Keep `bundle.macOS.signingIdentity: "-"`, so the bundle is sealed.
- `CI=true` for `tauri build`, or the DMG step needs Finder Automation permission.
- `sandbox-exec` is deprecated but works. zsh needs `-o nobgnice` inside it.
- A tool denial ends the turn on purpose: retrying without tools would change the prompt start and lose the cache.
- `tauri dev` restarts the app when Rust files change, including DB migrations.
- Async Tauri commands must be `Send`. Callbacks passed across `.await` need `+ Sync`.
