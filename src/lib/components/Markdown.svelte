<script lang="ts">
  // Renders model Markdown as elements and text nodes only (see markdown.ts).
  import { api } from "../api";
  import { parse, type Block, type Inline } from "../markdown";

  interface Props {
    source: string;
  }
  let { source }: Props = $props();

  const tree = $derived(parse(source));
  let copied = $state("");

  async function copy(text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      copied = text;
      setTimeout(() => (copied = ""), 1200);
    } catch {
      /* clipboard can be blocked; nothing else to do */
    }
  }

  function open(e: MouseEvent, href: string): void {
    e.preventDefault();
    void api.openUrl(href);
  }
</script>

{#snippet inlines(list: Inline[])}{#each list as n, i (i)}{#if n.t === "text"}{n.v}{:else if n.t === "code"}<code class="ic">{n.v}</code>{:else if n.t === "strong"}<strong>{@render inlines(n.c)}</strong>{:else if n.t === "em"}<em>{@render inlines(n.c)}</em>{:else if n.t === "del"}<del>{@render inlines(n.c)}</del>{:else if n.t === "link"}<a href={n.href} title={n.href} onclick={(e) => open(e, n.href)}>{@render inlines(n.c)}</a>{:else if n.t === "br"}<br />{/if}{/each}{/snippet}

{#snippet blocks(list: Block[])}
  {#each list as b, i (i)}
    {#if b.t === "p"}
      <p>{@render inlines(b.c)}</p>
    {:else if b.t === "h"}
      <svelte:element this={`h${Math.min(b.level + 2, 6)}`} class="h" data-level={b.level}>{@render inlines(b.c)}</svelte:element>
    {:else if b.t === "code"}
      <div class="code">
        <div class="code-bar">
          <span>{b.lang || "code"}</span>
          <button type="button" onclick={() => copy(b.v)}>{copied === b.v ? "Copied" : "Copy"}</button>
        </div>
        <pre><code>{b.v}</code></pre>
      </div>
    {:else if b.t === "quote"}
      <blockquote>{@render blocks(b.c)}</blockquote>
    {:else if b.t === "list"}
      <svelte:element this={b.ordered ? "ol" : "ul"} start={b.ordered && b.start !== 1 ? b.start : undefined} class:tasks={b.items.some((it) => it.task !== null)}>
        {#each b.items as it, j (j)}
          <li>
            {#if it.task !== null}<input type="checkbox" checked={it.task} disabled aria-label={it.task ? "Done" : "Not done"} />{/if}
            {@render blocks(it.c)}
          </li>
        {/each}
      </svelte:element>
    {:else if b.t === "table"}
      <div class="table">
        <table>
          <thead>
            <tr>{#each b.head as c, j (j)}<th style:text-align={b.align[j] ?? undefined}>{@render inlines(c)}</th>{/each}</tr>
          </thead>
          <tbody>
            {#each b.rows as r, k (k)}
              <tr>{#each b.head as _, j (j)}<td style:text-align={b.align[j] ?? undefined}>{@render inlines(r[j] ?? [])}</td>{/each}</tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else if b.t === "hr"}
      <hr />
    {/if}
  {/each}
{/snippet}

<div class="md">{@render blocks(tree)}</div>

<style>
  .md {
    line-height: 1.5;
  }

  .md :global(> :first-child),
  .md :global(li > :first-child),
  .md :global(blockquote > :first-child) {
    margin-top: 0;
  }

  .md :global(> :last-child),
  .md :global(li > :last-child),
  .md :global(blockquote > :last-child) {
    margin-bottom: 0;
  }

  p {
    margin: var(--space-2) 0;
  }

  .h {
    font-family: var(--font-display);
    margin: var(--space-4) 0 var(--space-2);
    line-height: 1.25;
  }

  .h[data-level="1"] {
    font-size: 19px;
  }
  .h[data-level="2"] {
    font-size: 17px;
  }
  .h[data-level="3"] {
    font-size: 15px;
  }
  .h[data-level="4"],
  .h[data-level="5"],
  .h[data-level="6"] {
    font-size: 14px;
  }

  ul,
  ol {
    margin: var(--space-2) 0;
    padding-left: 22px;
  }

  li {
    margin: 2px 0;
  }

  li > :global(p) {
    margin: 0;
  }

  ul.tasks {
    list-style: none;
    padding-left: 4px;
  }

  ul.tasks input {
    margin: 0 6px 0 0;
    vertical-align: -2px;
  }

  ul.tasks li > :global(p:first-of-type) {
    display: inline;
  }

  blockquote {
    margin: var(--space-2) 0;
    padding: 2px var(--space-3);
    border-left: 3px solid var(--blue-25);
    color: var(--text-muted);
  }

  hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: var(--space-3) 0;
  }

  a {
    color: var(--link);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }

  .ic {
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--mist-60);
    font-size: 0.92em;
  }

  .table {
    margin: var(--space-2) 0;
    overflow-x: auto;
  }

  table {
    border-collapse: collapse;
    font-size: 13px;
  }

  th,
  td {
    border: 1px solid var(--border);
    padding: 4px var(--space-2);
    text-align: left;
    vertical-align: top;
  }

  th {
    background: var(--surface-muted);
    font-weight: 700;
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
</style>
