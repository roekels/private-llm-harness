import type { Message, Profile, Sampling, SessionSummary, Status, ToolSettings, VmStatus } from "./types";

export type PendingTool = { name: string; arguments: string; state: "waiting" | "running" | "queued" };

// Qwen authors' presets (the model card).
export const SAMPLING = {
  chat: { temperature: 0.7, top_p: 0.8, top_k: 20, min_p: 0, presence_penalty: 1.5 },
  think: { temperature: 1.0, top_p: 0.95, top_k: 20, min_p: 0, presence_penalty: 1.5 },
  code: { temperature: 0.6, top_p: 0.95, top_k: 20, min_p: 0, presence_penalty: 0 },
} satisfies Record<string, Sampling>;

export function blankProfile(): Profile {
  return {
    id: crypto.randomUUID(),
    name: "",
    baseUrl: "http://",
    model: "",
    contextTokens: 32768,
    healthPath: "/health",
    allowPublic: false,
    modelInfo: "",
    sampling: { ...SAMPLING.chat },
  };
}

export const app = $state({
  view: "chat" as "chat" | "settings" | "about" | "setup",

  // Connection
  profiles: [] as Profile[],
  activeId: null as string | null,
  status: "unknown" as Status,
  statusMessage: "",
  models: [] as string[],
  modelFound: false,
  latencyMs: null as number | null,
  envFile: "",
  keychainService: "qwen35-harness",

  // Chat
  think: false,
  sessions: [] as SessionSummary[],
  activeSessionId: null as string | null,
  messages: [] as Message[],
  /** Messages below this seq are no longer sent to the model. */
  trimSeq: 0,
  /** Messages below this seq are sent shortened (compaction). */
  stripSeq: 0,
  /** Session memory written at compaction. */
  memory: "",
  compactions: 0,
  /** The model is writing the session memory. */
  compacting: false,
  /** Last compaction, for the Memory pane. */
  lastCompaction: null as { before: number; after: number } | null,
  /** The reply being streamed, if any. */
  streaming: null as { content: string; reasoning: string } | null,
  generating: false,
  chatError: "",

  // Tools (M3)
  tools: { enabled: true, autoApprove: false, network: true, backend: "vm", rateLimit: true } as ToolSettings,
  /** Calls in flight this turn, by call id. */
  pendingTools: {} as Record<string, PendingTool>,
  workspace: "",
  /** Sandbox VM status, polled (vm.svelte.ts). */
  vm: null as VmStatus | null,
  /** The browser pane beside the chat. */
  browserOpen: false,
  /** The session memory pane beside the chat. */
  memoryOpen: false,
  /** Setup has something to do (shown as a banner). */
  setupNeeded: false,
});

export function activeProfile(): Profile | null {
  return app.profiles.find((p) => p.id === app.activeId) ?? null;
}
