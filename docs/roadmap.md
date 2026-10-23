# Roadmap

## Done

| Version | What |
| --- | --- |
| Before 0.5 | Tauri app, locked-down webview, connection profiles, Keychain and `.env` keys, connection check, streaming chat, Think toggle, sessions, images, token meter, context trim, terminal and file tools with approvals, macOS sandbox |
| Before 0.5 | Sandbox VM with Apple `container`, VM firewall, patches and rebuild |
| 0.5 | Browser tool in the VM (text snapshots, clicks, typing, screenshots), browser pane, automatic titles, system prompt against prompt injection, message times, skills, About view |
| 0.6 | `.dmg` build, Setup assistant with verified install of Apple `container`, isolation self-test, browser DevTools (network, console, html, eval) with redaction |
| 0.7 | Compaction at 50% with a session memory, memory pane, 1Password support removed in favor of the Keychain, anti-block guard |

## Next

- **Check for updates:** the app checks this repo's GitHub releases, downloads the new version, verifies it and replaces itself, instead of a manual download.
- GitHub releases with a `.dmg` for each version.

## Later

- Sign and notarize with an Apple developer account, so there is no Gatekeeper prompt and the Keychain identity stays stable across updates.
- One VM folder per session, so sessions can't see each other's files.
- Optional masked egress (a VPN or WARP inside the VM) for a different public IP.

## Not planned

- Windows and Linux. The sandbox depends on Apple `container`.
