<script lang="ts">
  import { fmtInt } from "../format";
  import type { PendingTool } from "../state.svelte";
  import type { Message, MessageMeta, Role, ToolCall } from "../types";
  import Markdown from "./Markdown.svelte";
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
    {#if role === "user"}
      <span class="text">{content}</span>
    {:else}
      <Markdown source={content} />
    {/if}
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
