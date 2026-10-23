// Session and chat actions. Rust owns history; this keeps the view in sync.
import { api, errText } from "./api";
import { app } from "./state.svelte";
import { refreshVm } from "./vm.svelte";
import type { ChatEvent } from "./types";

export async function loadSessions(): Promise<void> {
  app.sessions = await api.sessions();
  const first = app.sessions[0];
  if (!app.activeSessionId && first) await openSession(first.id);
}

export async function openSession(id: string): Promise<void> {
  if (app.generating) return;
  app.activeSessionId = id;
  app.view = "chat";
  app.chatError = "";
  const view = await api.messages(id);
  if (app.activeSessionId !== id) return;
  app.messages = view.messages;
  app.trimSeq = view.trimSeq;
  app.stripSeq = view.stripSeq;
  app.memory = view.memory;
  app.compactions = view.compactions;
  app.lastCompaction = null;
  app.workspace = view.workspace;
}

export async function newSession(): Promise<void> {
  if (app.generating) return;
  // Reuse an empty session at the top instead of piling them up.
  const top = app.sessions[0];
  if (top && top.id === app.activeSessionId && app.messages.length === 0) {
    app.view = "chat";
    return;
  }
  const s = await api.createSession();
  app.sessions.unshift(s);
  app.activeSessionId = s.id;
  app.messages = [];
  app.trimSeq = 0;
  app.stripSeq = 0;
  app.memory = "";
  app.compactions = 0;
  app.lastCompaction = null;
  app.chatError = "";
  app.view = "chat";
}

export async function renameSession(id: string, title: string): Promise<void> {
  await api.renameSession(id, title);
  const s = app.sessions.find((x) => x.id === id);
  if (s) s.title = title.trim();
}

export async function deleteSession(id: string): Promise<void> {
  if (app.generating && id === app.activeSessionId) return;
  await api.deleteSession(id);
  app.sessions = app.sessions.filter((s) => s.id !== id);
  if (app.activeSessionId === id) {
    app.activeSessionId = null;
    app.messages = [];
    const next = app.sessions[0];
    if (next) await openSession(next.id);
  }
}

/** Returns false if nothing was sent (caller keeps the draft). */
export async function send(text: string, images: string[]): Promise<boolean> {
  if (app.generating || (!text.trim() && images.length === 0)) return false;
  if (!app.activeSessionId) await newSession();
  const sid = app.activeSessionId;
  if (!sid) return false;

  app.generating = true;
  app.chatError = "";
  app.paused = null;
  app.untilDone = false;
  app.streaming = { content: "", reasoning: "" };

  const onEvent = handler(sid);

  try {
    await api.send(sid, text, images, app.think, onEvent);
    return true;
  } catch (e) {
    // Rejected before the message was saved (validation, no profile, busy).
    app.chatError = errText(e);
    return false;
  } finally {
    app.generating = false;
    app.compacting = false;
    app.streaming = null;
    app.pendingTools = {};
    app.untilDone = false;
    if (app.tools.enabled && app.tools.backend === "vm") void refreshVm();
  }
}

/** "Compact now" from the Memory pane. */
export async function compactNow(): Promise<void> {
  const sid = app.activeSessionId;
  if (!sid || app.generating) return;
  app.generating = true;
  app.chatError = "";
  try {
    await api.compact(sid, handler(sid));
  } catch (e) {
    app.chatError = errText(e);
  } finally {
    app.generating = false;
    app.compacting = false;
  }
}

export async function saveMemory(memory: string): Promise<void> {
  const sid = app.activeSessionId;
  if (!sid) return;
  await api.setMemory(sid, memory);
  app.memory = memory.trim();
}

/** Applies chat events for one session to the view. */
function handler(sid: string): (e: ChatEvent) => void {
  return (e: ChatEvent) => {
    if (app.activeSessionId !== sid) return;
    switch (e.type) {
      case "userSaved":
        app.messages.push(e.message);
        bumpSession(sid, e.message.content, e.message.images.length > 0);
        break;
      case "trimmed":
        app.trimSeq = e.trimSeq;
        break;
      case "reasoning":
        (app.streaming ??= { content: "", reasoning: "" }).reasoning += e.text;
        break;
      case "content":
        (app.streaming ??= { content: "", reasoning: "" }).content += e.text;
        break;
      case "done":
        app.messages.push(e.message);
        app.streaming = null;
        for (const c of e.message.toolCalls ?? []) {
          app.pendingTools[c.id] = { name: c.name, arguments: c.arguments, state: "queued" };
        }
        break;
      case "compacting":
        app.compacting = true;
        break;
      case "compacted":
        app.compacting = false;
        app.trimSeq = e.trimSeq;
        app.stripSeq = e.stripSeq;
        app.memory = e.memory;
        app.compactions += 1;
        app.lastCompaction = { before: e.beforeTokens, after: e.afterTokens };
        break;
      case "titled": {
        const s = app.sessions.find((x) => x.id === sid);
        if (s) s.title = e.title;
        break;
      }
      case "toolPending":
        app.pendingTools[e.callId] = {
          name: e.name,
          arguments: e.arguments,
          state: e.needsApproval ? "waiting" : "running",
        };
        break;
      case "toolRunning": {
        const t = app.pendingTools[e.callId];
        if (t) t.state = "running";
        break;
      }
      case "toolDone":
        app.messages.push(e.message);
        if (e.message.toolCallId) delete app.pendingTools[e.message.toolCallId];
        break;
      case "paused":
        app.paused = e.steps;
        break;
      case "error":
        app.chatError = e.message;
        break;
    }
  };
}

/** `untilDone`: approve this and every later call until the model's final answer. */
export async function decideTool(callId: string, approve: boolean, untilDone = false): Promise<void> {
  const t = app.pendingTools[callId];
  if (t?.state !== "waiting") return;
  t.state = approve ? "running" : "queued";
  if (approve && untilDone) app.untilDone = true;
  await api.decideTool(callId, approve, untilDone);
}

/** After a pause at the round limit: let the model carry on. */
export async function continueTurn(): Promise<void> {
  if (app.paused == null || app.generating) return;
  await send("Continue.", []);
}

export function stop(): void {
  void api.stop();
}

/** Mirror the Rust side: move to top, name it after the first message. */
function bumpSession(id: string, text: string, hasImages: boolean): void {
  const i = app.sessions.findIndex((s) => s.id === id);
  if (i < 0) return;
  const [s] = app.sessions.splice(i, 1);
  if (!s) return;
  s.updatedAt = Date.now();
  if (s.title === "New session") {
    const line = text.split("\n").map((l) => l.trim()).find(Boolean) ?? "";
    s.title = line ? (line.length > 48 ? `${line.slice(0, 48)}…` : line) : hasImages ? "Image" : s.title;
  }
  app.sessions.unshift(s);
}
