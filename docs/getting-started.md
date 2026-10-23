# Getting started

This guide takes you from nothing to a working harness: a model server, the app, a saved key and the sandbox VM. Each step ends with a check, so you (or your coding agent) know it worked before moving on.

## 1. Check your Mac

You need a Mac with Apple silicon. The sandbox VM needs macOS 26 or newer. Chat without tools works on macOS 13 or newer.

```bash
uname -m          # must print arm64
sw_vers -productVersion
```

## 2. Install the build tools

```bash
xcode-select --install                      # Xcode Command Line Tools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust (stable)
brew install node@22                        # or use nvm, fnm or the installer from nodejs.org
```

Check:

```bash
node --version    # v22 or newer
cargo --version
```

## 3. Run a model server

The app talks to any server with an OpenAI-compatible API (`/v1/chat/completions` and `/v1/models`). It can run on the same Mac, on another computer on your LAN, or on a machine you reach over a VPN such as Tailscale.

### llama.cpp (recommended)

The app is tuned for llama.cpp's `llama-server`, which reuses its cache when a prompt starts the same way. Download a build from [llama.cpp releases](https://github.com/ggml-org/llama.cpp/releases) and a GGUF model, then run:

```bash
llama-server -m your-model.gguf --jinja --alias local -c 32768 -np 1 --port 8080
```

What the flags do:

- `--jinja` is required. Without it, tool calls don't work.
- `--alias local` is the model name the app asks for.
- `-c 32768` is the context window in tokens. Use what fits your memory.
- `-np 1` gives one slot, which reuses the prompt cache best.
- Add `--mmproj your-projector.gguf` for a vision model, so the model can see screenshots and images.
- On a GPU with little memory, a mixture-of-experts model can keep some experts in system RAM with `--n-cpu-moe N`.
- On another computer, also pass `--host` with an address your Mac can reach, and `--api-key` with a long random key. Without a key, anyone on that network can use the server.
- Never expose the server to the internet. The app refuses public addresses unless you allow them, and then only over https.

Check from your Mac (change the host if the server runs elsewhere):

```bash
curl -s http://127.0.0.1:8080/health          # {"status":"ok"}
curl -s http://127.0.0.1:8080/v1/models       # lists "local"
```

### Other servers

Ollama (`http://127.0.0.1:11434/v1`), LM Studio (`http://127.0.0.1:1234/v1`) and vLLM work too. Set **Model id** to one that `/v1/models` lists. They may not reuse the prompt cache as well as llama.cpp, and their health check may live at another path (set **Health path** in Settings, or `/v1/models`).

## 4. Get the app

### Download a release

Download `Ruans-Harness_<version>_aarch64.dmg` from [Releases](https://github.com/roekels/private-llm-harness/releases), open it and drag **Ruan's Harness** to **Applications**. See the Gatekeeper note below for the first launch.

After that, the app updates itself: **About > Updates** shows a new version, and **Install update** downloads it, checks its signature and replaces the app.

### Build from source

```bash
git clone https://github.com/roekels/private-llm-harness.git
cd private-llm-harness
npm install
npm run tauri dev
```

The first build takes a few minutes. The app window opens when it's done.

### Or build an installer

```bash
CI=true npm run tauri build
```

This makes `src-tauri/target/release/bundle/dmg/Ruan's Harness_<version>_aarch64.dmg`. Open it and drag **Ruan's Harness** to **Applications**.

The app is ad-hoc signed, not signed with an Apple developer account. A copy built on your own Mac opens normally. A copy from another Mac is blocked the first time: open **System Settings > Privacy & Security** and select **Open Anyway**, or run:

```bash
xattr -dr com.apple.quarantine "/Applications/Ruan's Harness.app"
```

## 5. Connect to your model

1. In the app, open **Settings**.
2. The first profile, **Local llama-server**, points at `http://127.0.0.1:8080/v1` with model `local`. Change **Base URL** and **Model id** to match your server. Add a **Fallback URL** if you want a second address to try (for example an IP when a host name fails).
3. Set **Context size** to the server's context size (`-c`).
4. Under **Key**, choose:
   - **No key** if your server has no `--api-key`.
   - **Keychain** to paste the key once and select **Save to Keychain**. It is stored under the service `qwen35-harness`. macOS may ask whether the app may read it: choose **Always Allow**.
   - **.env or environment** for development: the app reads the named variable from its environment, then from `~/Library/Application Support/com.dewit.qwen35-harness/.env`. See [.env.example](../.env.example).
5. Select **Test connection**.

Check: the top bar shows **Ready**. If it shows something else, see [Troubleshooting](#troubleshooting).

## 6. Set up the sandbox

Tools run in a Linux VM made with Apple's open-source [`container`](https://github.com/apple/container) tool. The app installs it for you.

1. Open **Setup** in the sidebar (it opens by itself the first time).
2. Select **Set up everything**. The app:
   - downloads the latest Apple `container` installer from GitHub, checks its SHA-256, Apple's signature (team `UPBK2H6LZM`), notarization and Gatekeeper, and asks for your Mac password in macOS's own dialog to install it;
   - starts the container system, which downloads a Linux kernel once;
   - builds the sandbox image (Debian, Python, Node, git, Chromium; about 2 minutes);
   - starts the VM and runs the isolation self-test.

Check: Setup says **Isolation check: all passed.** and lists **9 of 9 passed**. The top bar's VM light is green.

Prefer the terminal? Install `container` yourself from [Apple's releases](https://github.com/apple/container/releases), then run `container system start --enable-kernel-install` and use **Set up everything** for the rest.

## 7. First session

1. Select **New session** and ask something, such as "What's in this folder? Make a file hello.txt with today's date."
2. Each command shows as a card. Select **Approve** or **Deny**. Denying ends that reply.
3. Select **Files** in the bottom bar to open the session's folder in Finder.
4. Select **Browser** in the bottom bar to watch the sandbox browser, or ask the model to open a page.

Useful settings in **Settings > Tools**: auto-approve commands, allow internet in the VM, the anti-block guard (at most 20 new connections per second to one site), and the sandbox choice (Linux VM or the older macOS sandbox).

## For coding agents

If you are an agent setting this up for a user, do it in this order and stop at the first failed check:

1. `uname -m` is `arm64`, and `sw_vers -productVersion` is 26 or newer for the sandbox.
2. `node --version` (22+) and `cargo --version` work. Install what is missing (step 2).
3. Ask the user for the model server's base URL and model name. Don't guess hosts on their network. Check `curl -s <base without /v1>/health` and `curl -s <base>/models`. If the server needs a key, the user enters it in the app (step 5). Never ask for it in chat or write it to a file in the repo.
4. In the repo: `npm install`, then `npm run check` and `cd src-tauri && cargo test`. All must pass.
5. Optional, without the UI: `cd src-tauri && cargo run --example probe -- none <base URL>` prints the connection status. `cargo run --example chat -- --url <base URL> --model <model> "Hello"` sends one message.
6. `npm run tauri dev` to start the app. The user completes steps 5 and 6 in the window (the Keychain and the admin password prompt need them).

## Troubleshooting

| Status | What it means | Try |
| --- | --- | --- |
| Host not found | The host name doesn't resolve | Check the name. On Tailscale, check `tailscale status` |
| Unreachable | Nothing answers at that address | Is the server running? Is `--host` set to an address the Mac can reach? Firewall on the server? |
| Server starting, Model loading | The server is up but the model is still loading | Wait. The app checks every 5 seconds |
| Key missing | The profile points at a key that isn't saved | Save it in **Settings** |
| Key rejected | The server refused the key | Check the key matches `--api-key` |
| Blocked | The host is a public address | Use a private address, or turn on **Allow public host** (https only) |
| Ready, but orange | `/v1/models` doesn't list the profile's model | Set **Model id** to a listed name, or use `--alias` |

Sandbox problems:

- **VM not set up** after an app update: the sandbox files changed. Open **Setup** and select **Set up everything** to rebuild.
- **container not found**: install it from Setup, or from Apple's releases.
- A site shows a bot check: some sites block headless browsers. The app doesn't work around that.

Where your data lives: `~/Library/Application Support/com.dewit.qwen35-harness/` holds the database (`harness.db`, profiles and sessions), the session folders (`workspaces/`) and the sandbox build files. Keys are in the Keychain, never in that folder (unless you use `.env`).
