# Security

The harness gives a language model a terminal and a browser. Assume the model can be wrong, or be steered by a web page or file it reads (prompt injection). The design keeps the damage inside a box you can throw away.

## What it protects

| Asset | How |
| --- | --- |
| Your Mac's files | Tools run in a Linux VM that sees only the session folders at `/work` |
| Your home network, VPN and the Mac itself | The VM firewall refuses private, Tailscale, link-local and multicast ranges |
| API keys | Kept in the Keychain and in Rust memory only. Never sent to the webview, the VM or the model |
| Your public IP's reputation | The anti-block guard caps bursts to any one site |
| The model server | Only the app talks to it. The VM can't reach it |

## The sandbox VM

- Its own Linux kernel, through Apple's `container` (Virtualization framework). Not a process on your Mac.
- The model's commands run as user `agent`: no sudo, no capabilities, can't change the firewall.
- One shared folder (`/work`). No `/Users`.
- The firewall must fail closed: if the rules don't load, the command is not run.
- Network modes, set by the app as root before each command:
  - **Off:** loopback only.
  - **Internet only:** DNS to the VM's resolver, then everything except `0/8`, `10/8`, `100.64/10`, `127/8`, `169.254/16`, `172.16/12`, `192.0.0/24`, `192.168/16`, `198.18/15`, `224/4`, `240/4`, and IPv6 `::1`, `fc00::/7`, `fe80::/10`, `ff00::/8`.
  - **Anti-block guard** (on by default): at most 20 new connections per second to one address (burst 40), refused inside the VM.
- Patches and rebuilds run as root, started only by you from Settings. The model can't see, edit or start them.

### Isolation self-test

Setup runs these checks in the VM as the model's user:

1. Your Mac's files are hidden (no `/Users`).
2. No admin rights (no sudo, can't read the firewall).
3. Only the session folders are shared (one mount).
4. No network when it's off.
5. Internet works when allowed.
6. This Mac is blocked (the VM's gateway).
7. The tailnet is blocked (`100.100.100.100`).
8. Your home network is blocked (`192.168.1.1`).
9. Bursts to one site are held back (80 fast connections to a documentation address that goes nowhere; the guard's own counter must stop at least 20).

## The macOS sandbox (fallback)

When you choose it in Settings, commands run under `sandbox-exec` with a deny-by-default profile: read system paths but not your home folder, write only the session folder, no Keychain or clipboard services, no network unless allowed, 60 s limit, the whole process group killed on timeout or Stop. It is weaker than the VM. Use the VM when you can.

`read` and `write` on the macOS backend run in the app behind a path guard: a lexical check, then the real path of the deepest existing folder must be inside the session folder (no symlink escape).

## Approvals

By default every `sh`, `write`, and browser `open`, `click`, `type` and `eval` call shows a card you approve or deny. Auto-approve is a setting. Denying ends the turn, so the model can't keep retrying.

## Prompt injection

The system prompt tells the model that only the user's messages are instructions, that pages, files and command output are untrusted data, never to reveal secrets, and to ask before irreversible steps. Browser text is marked as untrusted web content. This lowers the risk but doesn't remove it. The sandbox is what limits the damage.

The UI renders model and web output as text, never as HTML.

## Browser DevTools redaction

`network` shows request lines and headers, never bodies. Headers named like cookie, auth, token, secret, session, password, API key, CSRF or signature are redacted, and so are query parameters with those names or `code`, `key`, `sig`, `state`, including inside `:path`, `referer` and `location`. `eval` always asks first.

## Keys

- Stored as references only: `keychain:` or `env:`. The database never holds a key.
- Resolved in Rust, cached in memory, zeroed on drop, dropped after a 401 or any key change.
- Sent only to the profile's pinned address: no proxy, no redirects, no public hosts unless allowed, and then only over https.
- Masked in errors and logs.

## App updates

Updates come from this repo's GitHub releases. Each one is signed with a minisign key that is kept in 1Password, never in the repo. The app has only the public key and installs an update only when its signature is valid, the signature names the same version, and the version is newer. Install asks first, and macOS asks for a password if /Applications isn't writable. Details: [updates.md](updates.md).

## Installing Apple container

Setup downloads the installer only from GitHub hosts over https. Before installing it checks the size, the SHA-256 against GitHub's digest, Apple's Developer ID team (`UPBK2H6LZM`, pinned), notarization and Gatekeeper. It installs through macOS's own administrator dialog. A package that fails any check is never installed.

## Limits

- The app is ad-hoc signed, not notarized. Build it yourself, or check the source before you open a copy from someone else.
- The VM shares one folder for all sessions, so sessions can see each other's files.
- The browser's cookies are shared by sessions in the VM.
- With internet allowed, the model can reach any public site, with your public IP.

## Reporting a problem

Please open an issue on GitHub, or for a security problem, contact the maintainer through GitHub ([@roekels](https://github.com/roekels)) before posting details.
