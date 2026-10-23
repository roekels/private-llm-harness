# Development

## Run and check

```bash
npm install
npm run tauri dev        # run the app with hot reload
npm run check            # svelte-check + TypeScript
npm test                 # Markdown parser checks
npm run build            # frontend build only
cd src-tauri && cargo clippy --all-targets && cargo test
```

Run `npm run check` and `cargo clippy --all-targets && cargo test` after every change.

`npm run dev` previews the UI in a browser, but Tauri commands (`invoke`) only work inside the app.

The installed app and `npm run tauri dev` share the same data folder (same bundle id). Don't run both at once: that means two VM managers and two writers on one database.

## Command-line examples

These talk to your server or the VM without the UI. They print status and replies, never a key.

```bash
cd src-tauri

# Connection check. Key ref: none, keychain:service/account or env:NAME
cargo run --example probe -- none
cargo run --example probe -- keychain:qwen35-harness/local http://192.168.1.20:8080/v1

# Chat. Each extra argument is a new user turn.
cargo run --example chat -- --url http://127.0.0.1:8080/v1 --model local "Hello" "And again"
#   --key REF      key reference (default: none)
#   --think        thinking on
#   --stop-ms N    press Stop after N ms
#   --ctx N        pretend the context is N tokens (tests trimming and compaction)
#   --image FILE   attach an image to the first turn
#   --tools        tools on, auto-approved   --deny   deny every tool call
#   --net          allow internet in the sandbox   --macos   use the macOS sandbox
#   --until-done   answer the first approval with Approve until done
#   --steps N      tool rounds before the turn pauses
#   --skill N=TEXT add your own skill      --export DIR   write the audit CSV to DIR

# Sandbox VM with temporary folders (removes the VM after)
cargo run --example vm -- status | patches | rebuild | browser

# The app's real VM and setup checks (download verifies but never installs)
cargo run --example setup -- report | download | selftest | devtools | redact

# Self-update on a mock runtime (no window, no app data): check, verify, swap a test .app
cargo run --example update -- <latest.json URL> <public key> <path/to/Test.app> [current version]
```

With a `keychain:` key, macOS asks once to let the example read the item. Choose **Allow**.

## Build an installer

```bash
CI=true npm run tauri build
```

`CI=true` skips the Finder styling of the disk image, which would need Automation permission. The `.app` and `.dmg` are in `src-tauri/target/release/bundle/`. This build has no update files, so it needs no signing key.

## Release

Releases on GitHub are what the app updates itself to. See [updates.md](updates.md) for how it works.

1. Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` (numbers only; macOS bundle versions must be numeric).
2. Set `CHANNEL` in `src/lib/release.ts` (for example `"Beta"`, or `""` for a stable release).
3. Add a `## X.Y.Z` section to `CHANGELOG.md`. It becomes the release notes in the app.
4. Commit and push to `main`.
5. Run `scripts/release.sh X.Y.Z`. It checks the versions and the tree, builds with the signing key, writes `latest.json`, shows the files and asks before it publishes the GitHub release.

The signing key comes from `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. With `op-claude` and a `.env` of `op://` references, the script reads them from 1Password.

The bundle is ad-hoc signed (`bundle.macOS.signingIdentity: "-"`), so it is sealed as a whole but has no Apple developer identity. Keep that setting: without it, another Mac may call the app "damaged" instead of offering **Open Anyway**.

## Before you commit

`scripts/check-public.sh` blocks commits that contain secret-shaped strings (API keys, tokens, private keys), plus any strings you list in `private/denylist.txt` (gitignored). Install it as a hook once:

```bash
ln -sf ../../scripts/check-public.sh .git/hooks/pre-commit
```

`scripts/check-public.sh --all` checks every file that would be published.

## Adding things

- **A skill:** add `skills/<name>.md`, add it to `tools::SKILLS` and to the `skill` schema enum. A test checks they match.
- **A tool action:** change `tools::definitions()` rarely. The schemas are part of the cached prompt start. Keep descriptions short, never put changing state in them.
- **A sandbox change:** any edit in `sandbox/` changes the image tag. After it, the app shows "VM not set up" until **Set up everything** runs (tool calls never build).
- **A command:** add a narrow `#[tauri::command]` in `commands.rs`, register it in `lib.rs`, wrap it in `src/lib/api.ts`. Don't add Tauri shell, fs or http plugins.
- **A database change:** add a migration in `store.rs` and bump `user_version`.
