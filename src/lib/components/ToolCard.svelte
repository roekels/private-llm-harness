<script lang="ts">
  import { decideTool } from "../chat.svelte";
  import { app, type PendingTool } from "../state.svelte";
  import type { Message, ToolCall, ToolMeta } from "../types";

  interface Props {
    call: ToolCall;
    result?: Message;
    pending?: PendingTool;
  }
  let { call, result, pending }: Props = $props();

  const args = $derived.by((): Record<string, unknown> => {
    try {
      const v: unknown = JSON.parse(call.arguments || "{}");
      return v && typeof v === "object" ? (v as Record<string, unknown>) : {};
    } catch {
      return {};
    }
  });
  const str = (k: string) => (typeof args[k] === "string" ? (args[k] as string) : "");

  const LABEL: Record<string, string> = { sh: "Run command", read: "Read file", write: "Write file", browser: "Browser", skill: "Skill" };
  const meta = $derived((result?.meta ?? null) as unknown as ToolMeta | null);

  const status = $derived.by((): { text: string; tone: "ok" | "bad" | "wait" | "idle" } => {
    if (meta) {
      const secs = meta.durationMs != null ? ` · ${(meta.durationMs / 1000).toFixed(1)} s` : "";
      const how = meta.approval === "auto" ? " · auto-approved" : meta.approval === "turn" ? " · approved until done" : "";
      switch (meta.outcome) {
        case "ok":
          return { text: `${call.name === "sh" ? `Exit 0${secs}` : "Done"}${how}`, tone: "ok" };
        case "failed":
          return { text: `Failed${meta.exitCode != null ? ` (exit ${meta.exitCode})` : ""}${secs}`, tone: "bad" };
        case "denied":
          return { text: "Declined", tone: "idle" };
        case "timed-out":
          return { text: "Timed out after 60 s", tone: "bad" };
        case "stopped":
          return { text: "Stopped", tone: "idle" };
      }
    }
    if (pending?.state === "waiting") return { text: "Waiting for your approval", tone: "wait" };
    if (pending?.state === "running") return { text: "Running…", tone: "wait" };
    if (pending?.state === "queued") return { text: "Queued", tone: "idle" };
    return { text: "No result", tone: "idle" };
  });

  const output = $derived(result?.content ?? "");
  const outputLines = $derived(output ? output.split("\n").length : 0);
</script>

<div class="card" data-tone={status.tone}>
  <div class="head">
    <span class="name">{LABEL[call.name] ?? call.name}</span>
    {#if result && result.seq < app.stripSeq && result.seq >= app.trimSeq}
      <span class="short" title="Older tool output is sent shortened to keep the context small">Shortened for the model</span>
    {/if}
    <span class="status">{status.text}</span>
  </div>

  {#if call.name === "sh"}
    <pre class="cmd"><code>{str("cmd") || call.arguments}</code></pre>
  {:else if call.name === "read" || call.name === "write"}
    <p class="path"><code>{str("path") || call.arguments}</code>{#if call.name === "read" && args.offset}&nbsp;from line {String(args.offset)}{/if}</p>
    {#if call.name === "write"}
      <details open={pending?.state === "waiting"}>
        <summary>Content ({str("content").length} characters)</summary>
        <pre>{str("content")}</pre>
      </details>
    {/if}
  {:else if call.name === "skill"}
    <p class="path"><strong>{str("name")}</strong> best-practice notes</p>
  {:else if call.name === "browser"}
    <p class="path">
      <strong>{str("action")}</strong>
      {#if str("url")}&nbsp;<code>{str("url")}</code>{/if}
      {#if args.ref != null}&nbsp;[{String(args.ref)}]{/if}
      {#if str("text")}&nbsp;"{str("text")}"{args.submit ? " + Enter" : ""}{/if}
      {#if str("direction")}&nbsp;{str("direction")}{/if}
      {#if str("selector")}&nbsp;<code>{str("selector")}</code>{/if}
      {#if str("filter")}&nbsp;filter "{str("filter")}"{/if}
      {#if args.id != null}&nbsp;#{String(args.id)}{/if}
    </p>
    {#if str("code")}<pre class="cmd"><code>{str("code")}</code></pre>{/if}
    {#if result?.images?.length}
      <img class="shot" src={result.images[0]} alt="Screenshot the model received" />
    {/if}
  {:else}
    <pre class="cmd"><code>{call.arguments}</code></pre>
  {/if}

  {#if pending?.state === "waiting"}
    <div class="decide">
      <button class="btn-primary" onclick={() => decideTool(call.id, true)}>Approve</button>
      <button class="btn-secondary" onclick={() => decideTool(call.id, false)}>Deny</button>
      <button
        class="btn-secondary"
        title="Approve this and every later step until the model gives its final answer. The next message asks again."
        onclick={() => decideTool(call.id, true, true)}>Approve until done</button
      >
      <span class="hint">Runs in this session's sandboxed folder.</span>
    </div>
  {/if}

  {#if result && (meta?.outcome === "denied" || meta?.outcome === "stopped")}
    <!-- The status says it all. -->
  {:else if result && call.name !== "write"}
    <details>
      <summary>Output ({outputLines} {outputLines === 1 ? "line" : "lines"}){meta?.fullOutput ? " · clipped" : ""}</summary>
      <pre>{output}</pre>
      {#if meta?.fullOutput}<p class="hint">Full output saved as <code>{meta.fullOutput}</code> in the workspace.</p>{/if}
    </details>
  {:else if result && meta?.outcome !== "ok"}
    <p class="hint">{output}</p>
  {/if}
</div>

<style>
  .card {
    margin-top: var(--space-2);
    border: 1px solid var(--border);
    border-left: 4px solid var(--blue-25);
    border-radius: var(--radius);
    padding: var(--space-2) var(--space-3);
    background: var(--mist-25);
  }

  .card[data-tone="ok"] {
    border-left-color: var(--ok);
  }
  .card[data-tone="bad"] {
    border-left-color: var(--danger);
  }
  .card[data-tone="wait"] {
    border-left-color: var(--warn);
    background: var(--surface);
  }

  .head {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    font-size: 13px;
  }

  .name {
    font-weight: 700;
  }

  .short {
    font-family: var(--font-micro);
    font-size: 11px;
    padding: 0 6px;
    border-radius: var(--radius);
    background: var(--mist);
    color: var(--text-muted);
  }

  .status {
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--text-muted);
  }

  .cmd,
  pre {
    margin: var(--space-2) 0 0;
    padding: var(--space-2);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 320px;
    overflow-y: auto;
  }

  .path {
    margin: var(--space-2) 0 0;
    font-size: 13px;
  }

  .shot {
    display: block;
    margin-top: var(--space-2);
    max-width: 100%;
    max-height: 220px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  details {
    margin-top: var(--space-2);
  }

  summary {
    cursor: pointer;
    font-size: 12px;
    color: var(--text-muted);
  }

  .decide {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .hint {
    margin: var(--space-1) 0 0;
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--text-muted);
    overflow-wrap: anywhere;
  }
</style>
