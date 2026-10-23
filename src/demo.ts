// Demo mode for screenshots and UI work: `npm run dev`, then open
// http://127.0.0.1:1420/?demo (add &view=settings|setup|about, &pane=browser|memory).
// Made-up data only. Loaded only in dev builds (see main.ts), never in the app.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Message, Patch, Profile, SelfTest, SessionSummary, SetupReport, VmStatus } from "./lib/types";

const T0 = Date.UTC(2026, 9, 23, 8, 42);
const min = 60_000;

const profile: Profile = {
  id: "local",
  name: "Local llama-server",
  baseUrl: "http://127.0.0.1:8080/v1",
  model: "local",
  contextTokens: 65536,
  healthPath: "/health",
  allowPublic: false,
  keyRef: "keychain:qwen35-harness/local",
  modelInfo: "Qwen3.6-35B-A3B Q4_K_M",
  sampling: { temperature: 0.7, top_p: 0.8, top_k: 20, min_p: 0, presence_penalty: 1.5 },
};

const sessions: SessionSummary[] = [
  ["s1", "Mont Blanc elevation notes", 0],
  ["s2", "Clean up the sales CSV", 1],
  ["s3", "Why is my Flask app slow?", 2],
  ["s4", "Weekend reading list", 3],
].map(([id, title, i]) => ({
  id: id as string,
  title: title as string,
  profileId: "local",
  trimSeq: 0,
  stripSeq: 0,
  compactions: id === "s1" ? 1 : 0,
  memory: "",
  createdAt: T0 - (i as number) * 1440 * min,
  updatedAt: T0 - (i as number) * 1440 * min + 9 * min,
}));

const memory = `## Goal
Find the elevation of Mont Blanc and keep notes in notes.md.

## Key facts and decisions
- Source: the Wikipedia article on Mont Blanc.
- Elevation varies year to year with the summit ice cap.

## Progress
- Opened the article and read the summary box.
- Wrote notes.md with the elevation and the source.

## Variables and values
- elevation_m = 4805.59
- notes file = notes.md

## Open items
- None.`;

let seq = 0;
const msg = (m: Partial<Message> & Pick<Message, "role" | "content">): Message => ({
  id: `m${++seq}`,
  seq,
  images: [],
  createdAt: T0 + seq * 20_000,
  ...m,
});

const call = (id: string, name: string, args: object) => ({ id, name, arguments: JSON.stringify(args) });
const toolMeta = (name: string, durationMs: number, approval = "user") => ({ name, outcome: "ok", exitCode: 0, durationMs, approval });

const messages: Message[] = [
  msg({ role: "user", content: "Look up the elevation of Mont Blanc on Wikipedia and save it to notes.md with the source." }),
  msg({ role: "assistant", content: "", toolCalls: [call("c1", "skill", { name: "browser" })] }),
  msg({ role: "tool", toolCallId: "c1", content: "Browser: open, then snapshot to read. Use [n] refs to click…", meta: toolMeta("skill", 2, "free") as never }),
  msg({ role: "assistant", content: "", toolCalls: [call("c2", "browser", { action: "open", url: "https://en.wikipedia.org/wiki/Mont_Blanc" })] }),
  msg({
    role: "tool",
    toolCallId: "c2",
    content:
      "[untrusted web content]\nMont Blanc - Wikipedia\nhttps://en.wikipedia.org/wiki/Mont_Blanc\n[1] Main menu  [2] Search\nMont Blanc\nHighest point  Elevation 4,805.59 m (15,766 ft)\nProminence 4,696 m  Coordinates 45°49′58″N 6°51′54″E\nMont Blanc is the highest mountain in the Alps and Western Europe…",
    meta: toolMeta("browser", 1840) as never,
  }),
  msg({
    role: "assistant",
    content: "",
    toolCalls: [call("c3", "write", { path: "notes.md", content: "# Mont Blanc\n\n- Elevation: 4,805.59 m (15,766 ft)\n- Source: https://en.wikipedia.org/wiki/Mont_Blanc\n" })],
  }),
  msg({ role: "tool", toolCallId: "c3", content: "Wrote notes.md (104 bytes).", meta: toolMeta("write", 40) as never }),
  msg({ role: "assistant", content: "", toolCalls: [call("c4", "sh", { cmd: "cat notes.md && wc -c notes.md" })] }),
  msg({
    role: "tool",
    toolCallId: "c4",
    content: "# Mont Blanc\n\n- Elevation: 4,805.59 m (15,766 ft)\n- Source: https://en.wikipedia.org/wiki/Mont_Blanc\n104 notes.md",
    meta: toolMeta("sh", 210) as never,
  }),
  msg({
    role: "assistant",
    content:
      "Done. Mont Blanc is **4,805.59 m** (15,766 ft) high, according to Wikipedia. The summit is an ice cap, so the exact figure changes a little from year to year.\n\nI saved it to `notes.md` in this session's folder:\n\n```\n# Mont Blanc\n\n- Elevation: 4,805.59 m (15,766 ft)\n- Source: https://en.wikipedia.org/wiki/Mont_Blanc\n```",
    meta: { promptTokens: 14212, cachedTokens: 14088, completionTokens: 96, windowStart: 0, tokensPerSecond: 41.7, promptPerSecond: 812, finishReason: "stop", model: "local" },
  }),
];

const vm: VmStatus = { state: "running", message: "Running", image: "qwen35-sandbox:3f9c2a17", version: "container 1.6.0", network: "on limit=20" };

const checks: SelfTest = {
  at: T0 - 5 * min,
  passed: true,
  checks: [
    ["Your Mac's files are hidden", "No /Users folder in the VM."],
    ["No admin rights", "No sudo; the firewall can't be changed."],
    ["Only the session folders are shared", "One shared folder: /work."],
    ["No network when it's off", "Nothing got out."],
    ["Internet works when allowed", "example.com answered."],
    ["This Mac is blocked", "This Mac (192.168.64.1) refused the connection."],
    ["The tailnet is blocked", "The tailnet (100.100.100.100) refused."],
    ["Your home network is blocked", "192.168.1.1 refused."],
    ["Bursts to one site are held back", "The guard stopped 39 of 80 rapid connections to one address."],
  ].map(([name, detail]) => ({ name: name!, passed: true, detail: detail! })),
};

const report: SetupReport = {
  appVersion: "0.7.1",
  needsAttention: false,
  latestContainer: null,
  steps: [
    { id: "mac", title: "This Mac", state: "ok", detail: "Apple silicon, macOS 26.1." },
    { id: "container", title: "Apple container", state: "ok", detail: "Version 1.6.0, the latest release." },
    { id: "system", title: "Container system", state: "ok", detail: "Running, kernel installed." },
    { id: "image", title: "Sandbox image", state: "ok", detail: "qwen35-sandbox:3f9c2a17 is current." },
    { id: "vm", title: "Sandbox VM", state: "ok", detail: "Running." },
    { id: "selftest", title: "Isolation check", state: "ok", detail: "9 of 9 passed today." },
  ],
};

const patches: Patch[] = [
  {
    id: "system-updates",
    name: "System updates (Debian security and package updates)",
    script: "apt-get update && apt-get -y upgrade",
    autoRun: false,
    builtin: true,
    lastRun: { at: T0 - 60 * min, ok: true, ms: 6100, summary: "Done" },
  },
  { id: "ffmpeg", name: "Install ffmpeg", script: "apt-get install -y ffmpeg", autoRun: true, builtin: false, lastRun: null },
];

// A drawn stand-in for the browser pane's live view.
const page = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="768" viewBox="0 0 1024 768" font-family="Georgia, serif">
<rect width="1024" height="768" fill="#fff"/><rect width="1024" height="56" fill="#f8f9fa"/><circle cx="48" cy="28" r="14" fill="#c8ccd1"/><text x="72" y="35" font-size="19" font-family="Helvetica, Arial" fill="#202122">Free encyclopedia</text>
<rect x="620" y="14" width="360" height="30" rx="4" fill="#fff" stroke="#a2a9b1"/><text x="634" y="35" font-size="15" font-family="Helvetica, Arial" fill="#72777d">Search Wikipedia</text>
<text x="32" y="118" font-size="38" fill="#202122">Mont Blanc</text><line x1="32" y1="132" x2="600" y2="132" stroke="#a2a9b1"/>
<text x="32" y="168" font-size="17" font-family="Helvetica, Arial" fill="#202122">Mont Blanc is the highest mountain in the Alps and</text><text x="32" y="194" font-size="17" font-family="Helvetica, Arial" fill="#202122">Western Europe, rising 4,805.59 m above sea level.</text>
<text x="32" y="230" font-size="17" font-family="Helvetica, Arial" fill="#202122">It is the second-most prominent mountain in Europe,</text><text x="32" y="256" font-size="17" font-family="Helvetica, Arial" fill="#202122">after Mount Elbrus.</text>
<rect x="640" y="90" width="340" height="560" fill="#f8f9fa" stroke="#a2a9b1"/><text x="810" y="124" text-anchor="middle" font-size="22" fill="#202122">Mont Blanc</text>
<defs><linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#7fb2e5"/><stop offset="1" stop-color="#d6e8f7"/></linearGradient></defs>
<rect x="656" y="140" width="308" height="210" fill="url(#sky)"/><path d="M656 350 L740 250 L790 280 L850 180 L900 240 L964 210 L964 350 Z" fill="#eef3f8" stroke="#9fb3c8"/><path d="M820 222 L850 180 L880 222 Z" fill="#fff"/>
<g font-family="Helvetica, Arial" font-size="15" fill="#202122"><text x="660" y="390" font-weight="bold">Highest point</text>
<text x="660" y="422" font-weight="bold">Elevation</text><text x="790" y="422">4,805.59 m</text><text x="790" y="442">(15,766 ft)</text>
<text x="660" y="474" font-weight="bold">Prominence</text><text x="790" y="474">4,696 m</text>
<text x="660" y="506" font-weight="bold">Listing</text><text x="790" y="506">Seven Summits</text><text x="660" y="546" font-weight="bold">Geography</text>
<text x="660" y="578" font-weight="bold">Countries</text><text x="790" y="578">France, Italy</text><text x="660" y="610" font-weight="bold">Range</text><text x="790" y="610">Graian Alps</text></g>
<g fill="#eaecf0"><rect x="32" y="290" width="560" height="14"/><rect x="32" y="316" width="520" height="14"/><rect x="32" y="342" width="545" height="14"/><rect x="32" y="392" width="560" height="14"/><rect x="32" y="418" width="500" height="14"/><rect x="32" y="444" width="530" height="14"/></g>
</svg>`;

const handlers: Record<string, (args: Record<string, unknown>) => unknown> = {
  "plugin:app|version": () => "0.7.1",
  update_status: () => ({ current: "0.7.1", supported: true, autoCheck: true, lastCheck: T0 - 3 * min }),
  // Add &update to the URL to show an available update.
  update_check: () =>
    new URLSearchParams(location.search).has("update")
      ? { version: "0.8.0", date: "2026-10-23T12:00:00Z", notes: "- Check for updates: the app updates itself from GitHub releases.\n- Signed updates, verified before install." }
      : null,
  update_install: () => new Promise((r) => setTimeout(() => r("0.8.0"), 1500)),
  bootstrap: () => ({
    profiles: [profile],
    activeId: "local",
    envFile: "~/Library/Application Support/com.dewit.qwen35-harness/.env",
    keychainService: "qwen35-harness",
    toolSettings: { enabled: true, autoApprove: false, network: true, backend: "vm", rateLimit: true },
    setupSeenVersion: "0.7.1",
  }),
  connection_probe: () => ({ status: "ready", message: "Model local is loaded.", url: profile.baseUrl, models: ["local"], modelFound: true, latencyMs: 4, keySource: "Keychain" }),
  key_check: () => ({ source: "Keychain", length: 48 }),
  sessions_list: () => sessions,
  messages_list: ({ sessionId }) =>
    sessionId === "s1"
      ? { messages, trimSeq: 0, stripSeq: 3, memory, compactions: 1, workspace: "/work/s1" }
      : { messages: [], trimSeq: 0, stripSeq: 0, memory: "", compactions: 0, workspace: `/work/${String(sessionId)}` },
  vm_status: () => vm,
  setup_report: () => report,
  setup_last_selftest: () => checks,
  patches_list: () => patches,
  browser_view: () => ({ running: true, url: "https://en.wikipedia.org/wiki/Mont_Blanc", title: "Mont Blanc - Wikipedia", image: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(page)}`, width: 1024, height: 768 }),
  about_info: () => ({
    version: "0.7.1",
    dataDir: "~/Library/Application Support/com.dewit.qwen35-harness",
    host: { macos: "26.1", chip: "Apple M4", memoryGb: 24 },
    vm,
    vmDetails: { os: "Debian GNU/Linux 13 (trixie)", kernel: "6.12.28", arch: "aarch64", cpus: "4", memoryMb: "1987", python: "3.13.5", node: "20.19.2", chromium: "141.0.7390.54" },
    skills: ["terminal", "files", "browser", "screenshot", "research", "python"],
  }),
};

export function installDemo(): void {
  mockWindows("main");
  mockIPC((cmd, args) => {
    const h = handlers[cmd];
    return h ? h((args ?? {}) as Record<string, unknown>) : null;
  });
}

/** Optional starting view from the URL, applied after the app has loaded. */
export async function applyDemoView(app: { view: string; browserOpen: boolean; memoryOpen: boolean }): Promise<void> {
  const q = new URLSearchParams(location.search);
  await new Promise((r) => setTimeout(r, 300));
  const view = q.get("view");
  if (view === "settings" || view === "setup" || view === "about") app.view = view;
  if (q.get("pane") === "browser") app.browserOpen = true;
  if (q.get("pane") === "memory") app.memoryOpen = true;
}
