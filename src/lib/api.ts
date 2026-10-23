// Typed wrappers for the Rust commands in src-tauri/src/commands.rs.
import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AboutInfo,
  Bootstrap,
  ChatEvent,
  KeyCheck,
  MessagesView,
  Probe,
  Profile,
  BrowserView,
  Patch,
  SelfTest,
  SetupReport,
  SessionSummary,
  ToolSettings,
  VmStatus,
} from "./types";

function lineChannel(onLine: (line: string) => void): Channel<string> {
  const c = new Channel<string>();
  c.onmessage = onLine;
  return c;
}

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  saveProfile: (profile: Profile) => invoke<Profile>("profile_save", { profile }),
  /** Returns the new active profile id. */
  deleteProfile: (id: string) => invoke<string | null>("profile_delete", { id }),
  activateProfile: (id: string) => invoke<void>("profile_activate", { id }),
  checkKey: (keyRef: string) => invoke<KeyCheck>("key_check", { keyRef }),
  /** Returns the keychain: reference for the saved key. */
  keychainSet: (account: string, value: string) => invoke<string>("keychain_set", { account, value }),
  keychainDelete: (account: string) => invoke<void>("keychain_delete", { account }),
  probe: (profile: Profile) => invoke<Probe>("connection_probe", { profile }),

  sessions: () => invoke<SessionSummary[]>("sessions_list"),
  createSession: () => invoke<SessionSummary>("session_create"),
  renameSession: (id: string, title: string) => invoke<void>("session_rename", { id, title }),
  deleteSession: (id: string) => invoke<void>("session_delete", { id }),
  messages: (sessionId: string) => invoke<MessagesView>("messages_list", { sessionId }),
  /** Resolves when the turn is over. Events arrive on `onEvent` meanwhile. */
  send: (sessionId: string, text: string, images: string[], think: boolean, onEvent: (e: ChatEvent) => void) => {
    const channel = new Channel<ChatEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("chat_send", { sessionId, text, images, think, onEvent: channel });
  },
  stop: () => invoke<boolean>("chat_stop"),
  compact: (sessionId: string, onEvent: (e: ChatEvent) => void) => {
    const channel = new Channel<ChatEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("session_compact", { sessionId, onEvent: channel });
  },
  setMemory: (id: string, memory: string) => invoke<void>("session_set_memory", { id, memory }),
  decideTool: (callId: string, approve: boolean) => invoke<boolean>("tool_decide", { callId, approve }),
  setToolSettings: (settings: ToolSettings) => invoke<void>("tool_settings_set", { settings }),
  revealWorkspace: (sessionId: string) => invoke<void>("workspace_reveal", { sessionId }),
  vmStatus: () => invoke<VmStatus>("vm_status"),
  vmSetup: (onProgress: (line: string) => void) => {
    const channel = new Channel<string>();
    channel.onmessage = onProgress;
    return invoke<VmStatus>("vm_setup", { onProgress: channel });
  },
  vmStop: () => invoke<VmStatus>("vm_stop"),
  /** Opens one of a fixed list of links (commands.rs LINKS). */
  openLink: (name: "github" | "repo" | "model" | "base-model" | "llama-cpp" | "container" | string) => invoke<void>("open_link", { name }),
  aboutInfo: () => invoke<AboutInfo>("about_info"),
  setupReport: () => invoke<SetupReport>("setup_report"),
  setupInstallContainer: (onLine: (l: string) => void) => invoke<string>("setup_install_container", { onLine: lineChannel(onLine) }),
  setupVm: (build: boolean, onLine: (l: string) => void) => invoke<string>("setup_vm", { build, onLine: lineChannel(onLine) }),
  setupSelftest: () => invoke<SelfTest>("setup_selftest"),
  setupLastSelftest: () => invoke<SelfTest | null>("setup_last_selftest"),
  setupSeen: () => invoke<void>("setup_seen"),
  browserView: () => invoke<BrowserView>("browser_view"),
  browserUser: (args: Record<string, unknown>) => invoke<void>("browser_user", { args }),
  patches: () => invoke<Patch[]>("patches_list"),
  savePatches: (patches: Patch[]) => invoke<Patch[]>("patches_save", { patches }),
  runPatch: (id: string, onLine: (line: string) => void) =>
    invoke<Patch>("patch_run", { id, onLine: lineChannel(onLine) }),
  vmRebuild: (onLine: (line: string) => void) => invoke<VmStatus>("vm_rebuild", { onLine: lineChannel(onLine) }),
};

/** Tauri rejects with the Rust error string. */
export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
