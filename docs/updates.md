# Updates

Since 0.8.0 the app finds a new version on GitHub, downloads it, checks that it is genuine, swaps itself in /Applications and restarts. No manual download and drag.

## How it works

```
GitHub release vX.Y.Z (roekels/private-llm-harness)
  Ruans-Harness_X.Y.Z_aarch64.dmg      for new installs
  Ruans-Harness.app.tar.gz             the update
  Ruans-Harness.app.tar.gz.sig         minisign signature of the update
  latest.json                          version, notes, date, url, signature

App (Rust only)
  1. GET https://github.com/roekels/private-llm-harness/releases/latest/download/latest.json
  2. Newer than the running version? Show it: version and notes.
  3. You select Install: download the .tar.gz, verify the signature with the public key built into the app.
  4. Swap the .app bundle in one step (rename swap). If /Applications isn't writable, macOS asks for your password.
  5. Restart. Setup opens by itself if the sandbox needs a rebuild (already in place).
```

We use the official Tauri updater plugin (`tauri-plugin-updater`) from Rust. The webview gets no updater permission: it calls three narrow commands of ours.

## Why it is safe

- **Signature, not just a checksum.** Each update is signed with a private key that never leaves 1Password. The app has only the public key (`plugins.updater.pubkey` in `tauri.conf.json`). Someone who takes over the GitHub account or the download can't make an update the app accepts.
- **https only.** The plugin refuses plain HTTP in release builds.
- **Newer only.** The app never installs the same or an older version.
- **Verified before install.** A bad signature stops the update, and the app stays as it is. The old bundle is kept until the new one is in place.
- **No quarantine flag.** The app downloads the update itself, so macOS doesn't block the new version with Gatekeeper the way it blocks a downloaded `.dmg`.

## In the app

- **About > Updates:** current version, **Check for updates**, then version, release notes and **Install update** with download progress, then **Restart now**.
- **Automatic check** (on by default, a checkbox in About > Updates): once at start and then once a day. When an update is found, a dot appears next to **About** in the sidebar and a small banner offers it. Nothing installs without your click.
- **Restart now** waits while a reply is being written. The VM keeps running; the new app takes it over.
- `tauri dev` builds never update (they aren't the installed app). The Updates card says so.

## Rust

- `updates.rs`: `check()` (returns version, notes, date or none), `install(progress)` (download, verify, swap), using `app.updater()`.
- Commands: `update_status`, `update_check`, `update_install` (progress on a `Channel`), `update_restart`, `update_settings_set`.
- Settings key `auto_update_check` (default true) and `last_update_check`.
- Errors in plain words: offline, no release yet, bad signature ("The update didn't pass the signature check, so nothing was installed").

## Releasing

`scripts/release.sh X.Y.Z`, run on this Mac:

1. Checks the tree is clean, on `main`, pushed, and the version in the three files is X.Y.Z.
2. Builds with `CI=true npm run tauri build` and the signing key from 1Password (`op-claude run --env-file .env`, `TAURI_SIGNING_PRIVATE_KEY=op://...`). `bundle.createUpdaterArtifacts: true` makes the `.tar.gz` and `.sig`.
3. Writes `latest.json` (`darwin-aarch64`), with notes from `CHANGELOG.md`.
4. `gh release create vX.Y.Z` with the four files (asks before publishing).

## One-time setup

1. Generate the key pair: `npx tauri signer generate -w ~/Desktop/harness-updater.key`. Put the private key file and its password in 1Password, then delete the file.
2. Put the public key in `tauri.conf.json` (public, committed).
3. Add `.env` with the two `op://` references (gitignored).
4. Install 0.8.0 by hand once (0.7.1 has no updater). From then on the app updates itself.

## Limits

- The app stays ad-hoc signed. After each update macOS may ask again for Keychain access. Choose **Always Allow**. Signing with an Apple developer account would fix this.
- Losing the private key means users have to install the next version by hand.

## Test plan

- Unit: version compare, `latest.json` parsing, signature failure (tampered file) refuses to install.
- Real, step 1: publish 0.8.0, install it by hand in `~/Applications` (writable, no password), publish 0.8.1, update from the app.
- Real, step 2: the same from `/Applications`, including the password prompt case.
