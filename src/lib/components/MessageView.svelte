<script lang="ts">
  import { boldRuns, fmtInt, segments } from "../format";
  import type { PendingTool } from "../state.svelte";
  import type { Message, MessageMeta, Role, ToolCall } from "../types";
  import ToolCard from "./ToolCard.svelte";

  interface Props {
    role: Role;
    content: string;
    images?: string[];
    reasoning?: string | null;
    meta?: MessageMeta | null;
    streaming?: boolean;
    faded?: boolean;
    toolCalls?: ToolCall[] | null;
    results?: Map<string, Message>;
    pending?: Record<string, PendingTool>;
  }
  let {
    role,
    content,
    images = [],
    reasoning = null,
    meta = null,
    streaming = false,
    faded = false,
    toolCalls = null,
    results,
    pending,
  }: Props = $props();

  const parts = $derived(segments(content));
  let copied = $state(-1);

  const metaLine = $derived.by(() => {
    if (!meta) return "";
    const bits: string[] = [];
    if (meta.tokensPerSecond) bits.push(`${meta.tokensPerSecond.toFixed(1)} tok/s`);
    if (meta.promptTokens != null) {
      const cached = meta.cachedTokens ? `, ${fmtInt(meta.cachedTokens)} cached` : "";
      bits.push(`${fmtInt(meta.promptTokens)} prompt${cached}`);
    }
    if (meta.completionTokens != null) bits.push(`${fmtInt(meta.completionTokens)} out`);
    if (meta.think) bits.push("Think");
    if (meta.stopped) bits.push("Stopped");
    else if (meta.finishReason === "length") bits.push("Cut off at the reply limit");
    return bits.join(" · ");
  });

  async function copy(text: string, i: number): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      copied = i;
      setTimeout(() => (copied = -1), 1200);
    } catch {
      /* clipboard can be blocked; nothing else to do */
    }
  }
</script>

<article class="msg" data-role={role} class:faded>
  <span class="role">{role === "user" ? "You" : "Model"}</span>

  {#if images.length}
    <div class="images">
      {#each images as src, i (i)}
        <img {src} alt="Attachment {i + 1}" />
      {/each}
    </div>
  {/if}

  {#if reasoning}
    <details open={streaming && !content}>
      <summary>Reasoning{streaming && !content ? "…" : ""}</summary>
      <pre class="reasoning">{reasoning}</pre>
    </details>
  {/if}

  {#if content || streaming || !toolCalls?.length}
  <div class="content">
    {#each parts as p, i (i)}
      {#if p.kind === "code"}
        <div class="code">
          <div class="code-bar">
            <span>{p.lang || "code"}</span>
            <button type="button" onclick={() => copy(p.text, i)}>{copied === i ? "Copied" : "Copy"}</button>
          </div>
          <pre><code>{p.text}</code></pre>
        </div>
      {:else}
        <span class="text">{#each boldRuns(p.text) as r, j (j)}{#if r.bold}<strong>{r.text}</strong>{:else}{r.text}{/if}{/each}</span>
      {/if}
    {/each}
    {#if streaming}<span class="caret" aria-hidden="true"></span>{/if}
  </div>
  {/if}

  {#each toolCalls ?? [] as call (call.id)}
    <ToolCard {call} result={results?.get(call.id)} pending={pending?.[call.id]} />
  {/each}

  {#if metaLine}
    <div class="meta">{metaLine}</div>
  {/if}
</article>

<style>
  .msg {
    max-width: 820px;
    width: 100%;
    margin: 0 auto;
    padding: var(--space-3) var(--space-4);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .msg[data-role="user"] {
    background: var(--surface-muted);
  }

  .faded {
    opacity: 0.6;
  }

  .role {
    display: block;
    font-family: var(--font-micro);
    font-size: 12px;
    font-weight: 700;
    color: var(--text-muted);
    margin-bottom: var(--space-1);
  }

  .images {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }

  .images img {
    max-height: 160px;
    max-width: 240px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
  }

  .content {
    overflow-wrap: anywhere;
  }

  .text {
    white-space: pre-wrap;
  }

  .code {
    margin: var(--space-2) 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .code-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 2px var(--space-2);
    background: var(--mist);
    font-family: var(--font-micro);
    font-size: 12px;
  }

  .code-bar button {
    padding: 2px var(--space-2);
    font-size: 12px;
    color: var(--link);
  }

  .code pre {
    margin: 0;
    padding: var(--space-3);
    overflow-x: auto;
    background: var(--mist-25);
  }

  details {
    margin-bottom: var(--space-2);
    color: var(--text-muted);
  }

  summary {
    cursor: pointer;
    font-size: 13px;
  }

  .reasoning {
    white-space: pre-wrap;
    font-family: var(--font-text);
    font-size: 13px;
    margin: var(--space-2) 0 0;
    padding-left: var(--space-3);
    border-left: 2px solid var(--border);
  }

  .caret {
    display: inline-block;
    width: 7px;
    height: 14px;
    margin-left: 2px;
    vertical-align: text-bottom;
    background: var(--primary);
    animation: blink 1s steps(2) infinite;
  }

  @keyframes blink {
    50% {
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .caret {
      animation: none;
    }
  }

  .meta {
    margin-top: var(--space-2);
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--text-muted);
  }
</style>
