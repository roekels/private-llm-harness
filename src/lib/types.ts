// Shared shapes. Rust mirrors these in src-tauri/src (profile.rs, net.rs).

/** Where a key comes from. The value itself never reaches the webview. */
export type KeyRef =
  | `env:${string}` // app environment, then .env in the app config folder
  | `keychain:${string}`; // macOS Keychain, service/account

export interface Sampling {
  temperature: number;
  top_p: number;
  top_k: number;
  min_p: number;
  presence_penalty: number;
}

export interface Profile {
  id: string;
  name: string;
  baseUrl: string;
  fallbackUrl?: string;
  keyRef?: KeyRef;
  model: string;
  contextTokens: number;
  healthPath: string;
  allowPublic: boolean;
  /** Tooltip for the model, for example the exact GGUF file. */
  modelInfo?: string;
  sampling: Sampling;
}

/** Connection state (docs/architecture.md, "Connection check"). "unknown" and "checking" are UI-only. */
export type Status =
  | "unknown"
  | "checking"
  | "dns-failed"
  | "unreachable"
  | "starting"
  | "loading"
  | "ready"
  | "auth-error"
  | "key-missing"
  | "blocked"
  | "error";

export interface Probe {
  status: Exclude<Status, "unknown" | "checking">;
  message: string;
  url: string;
  models: string[];
  modelFound: boolean;
  latencyMs: number | null;
  keySource: string | null;
}

export interface ToolSettings {
  enabled: boolean;
  /** Run sh and write without asking. */
  autoApprove: boolean;
  /** Let sh use the network. In the VM: internet only. */
  network: boolean;
  /** "vm": the app's Linux VM. "macos": sandbox-exec on this Mac. */
  backend: "vm" | "macos";
  /** Anti-block guard: at most 20 new connections per second per site (VM). */
  rateLimit: boolean;
  /** Tool rounds per message before the app asks to continue (1 to 200). */
  maxSteps: number;
}

export interface AboutInfo {
  version: string;
  dataDir: string;
  host: { macos: string; chip: string; memoryGb: number };
  vm: VmStatus;
  vmDetails?: {
    os: string;
    kernel: string;
    arch: string;
    cpus: string;
    memoryMb: string;
    python: string;
    node: string;
    chromium: string;
  } | null;
  skills: string[];
}

/** The browser pane's view of the VM browser. */
export interface BrowserView {
  running: boolean;
  url?: string;
  title?: string;
  /** JPEG data URI. */
  image?: string;
  width?: number;
  height?: number;
}

export interface PatchRun {
  at: number;
  ok: boolean;
  ms: number;
  summary: string;
}

/** A root script for the sandbox VM, managed in Settings only. */
export interface Patch {
  id: string;
  name: string;
  script: string;
  /** Run again each time the VM starts (it starts fresh from the image). */
  autoRun: boolean;
  builtin: boolean;
  lastRun?: PatchRun | null;
}

export interface VmStatus {
  state: "not-installed" | "system-stopped" | "no-image" | "stopped" | "running";
  message: string;
  image: string;
  version?: string | null;
  /** Firewall mode in the VM: "on", "off", "failed", "unknown". */
  network?: string | null;
}

export interface Bootstrap {
  profiles: Profile[];
  activeId: string | null;
  envFile: string;
  keychainService: string;
  toolSettings: ToolSettings;
  setupSeenVersion?: string | null;
}

export type StepState = "ok" | "missing" | "outdated" | "stopped" | "unsupported" | "warning" | "skipped";
export type SetupAction = "install-container" | "upgrade-container" | "start-system" | "build-image" | "start-vm" | "selftest";

export interface SetupStep {
  id: string;
  title: string;
  state: StepState;
  detail: string;
  action?: SetupAction | null;
}

export interface SetupReport {
  appVersion: string;
  steps: SetupStep[];
  latestContainer?: { version: string; name: string; url: string; size: number; sha256?: string | null } | null;
  needsAttention: boolean;
}

export interface SelfTest {
  at: number;
  passed: boolean;
  checks: { name: string; passed: boolean; detail: string }[];
}

export interface KeyCheck {
  source: string;
  length: number;
}

export type Role = "user" | "assistant" | "tool";

export interface ToolCall {
  id: string;
  name: string;
  /** JSON text from the model. */
  arguments: string;
}

/** Meta on role "tool" messages (chat.rs turn_loop). */
export interface ToolMeta {
  name: string;
  outcome: "ok" | "failed" | "denied" | "timed-out" | "stopped";
  exitCode?: number | null;
  durationMs?: number;
  fullOutput?: string | null;
  /** "turn": approved with Approve until done earlier in the turn. */
  approval: "free" | "auto" | "turn" | "user" | "none";
}

/** Usage and flags on assistant messages (chat.rs Acc::meta). */
export interface MessageMeta {
  promptTokens?: number | null;
  completionTokens?: number | null;
  cachedTokens?: number | null;
  tokensPerSecond?: number | null;
  promptPerSecond?: number | null;
  finishReason?: string | null;
  think?: boolean;
  stopped?: boolean;
  model?: string;
  windowStart?: number;
}

export interface Message {
  id: string;
  seq: number;
  role: Role;
  content: string;
  /** Data URIs, scaled down by the UI before sending. */
  images: string[];
  reasoning?: string | null;
  meta?: MessageMeta | null;
  createdAt: number;
  toolCalls?: ToolCall[] | null;
  toolCallId?: string | null;
}

export interface SessionSummary {
  id: string;
  title: string;
  profileId?: string | null;
  trimSeq: number;
  stripSeq: number;
  compactions: number;
  memory: string;
  createdAt: number;
  updatedAt: number;
}

export interface MessagesView {
  messages: Message[];
  trimSeq: number;
  stripSeq: number;
  memory: string;
  compactions: number;
  workspace: string;
}

export type ChatEvent =
  | { type: "userSaved"; message: Message }
  | { type: "trimmed"; trimSeq: number }
  | { type: "reasoning"; text: string }
  | { type: "content"; text: string }
  | { type: "done"; message: Message }
  | { type: "titled"; title: string }
  | { type: "compacting" }
  | {
      type: "compacted";
      trimSeq: number;
      stripSeq: number;
      memory: string;
      beforeTokens: number;
      afterTokens: number;
    }
  | { type: "toolPending"; callId: string; name: string; arguments: string; needsApproval: boolean }
  | { type: "toolRunning"; callId: string }
  | { type: "toolDone"; message: Message }
  | { type: "paused"; steps: number }
  | { type: "error"; message: string };

/** Self-update (updates.rs). */
export interface UpdateStatus {
  current: string;
  supported: boolean;
  reason?: string | null;
  autoCheck: boolean;
  lastCheck?: number | null;
}

export interface UpdateAvailable {
  version: string;
  notes?: string | null;
  /** RFC 3339. */
  date?: string | null;
}

export type UpdateEvent =
  | { type: "downloading"; received: number; total?: number | null }
  | { type: "installing" }
  | { type: "installed"; version: string };

/** A skill: built-in (skills/*.md) or the user's own (Settings). */
export interface Skill {
  name: string;
  body: string;
}

export interface SkillsView {
  builtin: Skill[];
  custom: Skill[];
}

export interface PromptView {
  /** What a session created now starts with. */
  newSession: string;
  /** The active session's stored prompt (fixed when it was created). */
  session?: string | null;
}
