<script lang="ts">
  import { onMount } from "svelte";
  import { api, errText } from "../api";
  import { app } from "../state.svelte";
  import type { BrowserView } from "../types";

  const POLL_MS = 2000;
  let view = $state<BrowserView>({ running: false });
  let url = $state("");
  let urlFocused = $state(false);
  let typed = $state("");
  let busy = $state(false);
  let error = $state("");
  let img: HTMLImageElement | undefined = $state();

  const vmBackend = $derived(app.tools.backend === "vm");

  async function refresh(): Promise<void> {
    if (document.hidden || !vmBackend) return;
    try {
      view = await api.browserView();
      if (!urlFocused && view.url) url = view.url === "about:blank" ? "" : view.url;
    } catch (e) {
      error = errText(e);
    }
  }

  async function act(args: Record<string, unknown>): Promise<void> {
    busy = true;
    error = "";
    try {
      await api.browserUser(args);
    } catch (e) {
      error = errText(e);
    } finally {
      busy = false;
      await refresh();
    }
  }

  function go(): void {
    const u = url.trim();
    if (u) void act({ action: "open", url: u });
  }

  function clickPage(e: MouseEvent): void {
    if (!img || !view.width || !view.height || busy) return;
    const r = img.getBoundingClientRect();
    const x = ((e.clientX - r.left) / r.width) * view.width;
    const y = ((e.clientY - r.top) / r.height) * view.height;
    void act({ action: "click-xy", x: Math.round(x), y: Math.round(y) });
  }

  function sendText(enter: boolean): void {
    if (!typed) return;
    const text = typed;
    typed = "";
    void act({ action: "insert", text, enter });
  }

  onMount(() => {
    void refresh();
    const t = setInterval(() => {
      if (!busy) void refresh();
    }, POLL_MS);
    return () => clearInterval(t);
  });
</script>

<aside class="pane" aria-label="Sandbox browser">
  <header>
    <div class="row">
      <strong>Browser</strong>
      <span class="muted">in the sandbox VM</span>
      <span class="spacer"></span>
      <button class="btn-secondary small" onclick={() => (app.browserOpen = false)}>Close</button>
    </div>
    <form
      class="row"
      onsubmit={(e) => {
        e.preventDefault();
        go();
      }}
    >
      <button type="button" class="btn-secondary small" disabled={busy || !view.running} onclick={() => act({ action: "back" })} title="Back">Back</button>
      <button type="button" class="btn-secondary small" disabled={busy || !view.running} onclick={() => act({ action: "reload" })} title="Reload">Reload</button>
      <label class="visually-hidden" for="pane-url">Address</label>
      <input
        id="pane-url"
        class="url"
        placeholder="Type a web address"
        spellcheck="false"
        bind:value={url}
        onfocus={() => (urlFocused = true)}
        onblur={() => (urlFocused = false)}
      />
      <button type="submit" class="btn-primary small" disabled={busy || !vmBackend}>Go</button>
    </form>
  </header>

  <div class="screen">
    {#if !vmBackend}
      <p class="empty">The browser runs only in the Linux VM sandbox. Switch to it in Settings &gt; Tools.</p>
    {:else if view.running && view.image}
      <button class="shot" onclick={clickPage} disabled={busy} aria-label="Click on the page">
        <img bind:this={img} src={view.image} alt={view.title ? `Page: ${view.title}` : "Browser page"} />
      </button>
      <p class="title">{view.title || view.url}</p>
    {:else}
      <p class="empty">The browser starts when the model or you open a page. It runs inside the VM, with the same firewall as commands: internet only, no home network or tailnet.</p>
    {/if}
  </div>

  <footer>
    <form
      class="row"
      onsubmit={(e) => {
        e.preventDefault();
        sendText(true);
      }}
    >
      <label class="visually-hidden" for="pane-type">Type into the page</label>
      <input id="pane-type" class="url" placeholder="Type into the selected field" bind:value={typed} disabled={!view.running} />
      <button type="button" class="btn-secondary small" disabled={busy || !typed} onclick={() => sendText(false)}>Type</button>
      <button type="submit" class="btn-secondary small" disabled={busy || !typed}>Type + Enter</button>
    </form>
    <div class="row">
      <button class="btn-secondary small" disabled={busy || !view.running} onclick={() => act({ action: "scroll", direction: "up" })}>Scroll up</button>
      <button class="btn-secondary small" disabled={busy || !view.running} onclick={() => act({ action: "scroll", direction: "down" })}>Scroll down</button>
      <span class="spacer"></span>
      {#if busy}<span class="muted">Working…</span>{/if}
    </div>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
  </footer>
</aside>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    border-left: 1px solid var(--border);
    background: var(--surface);
  }

  header,
  footer {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
  }

  header {
    border-bottom: 1px solid var(--border);
  }

  footer {
    border-top: 1px solid var(--border);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: 13px;
  }

  .spacer {
    flex: 1;
  }

  .muted {
    color: var(--text-muted);
    font-size: 12px;
  }

  .small {
    padding: 2px var(--space-2);
    font-size: 12px;
  }

  .url {
    flex: 1;
    min-width: 0;
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--blue-25);
    border-radius: var(--radius);
    background: var(--surface);
    font-size: 13px;
  }

  .screen {
    flex: 1;
    overflow: auto;
    padding: var(--space-3);
    background: var(--mist-25);
  }

  .shot {
    display: block;
    width: 100%;
    padding: 0;
    border: 1px solid var(--border);
    cursor: crosshair;
  }

  .shot img {
    display: block;
    width: 100%;
    height: auto;
  }

  .title {
    margin: var(--space-2) 0 0;
    font-size: 12px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty {
    margin: var(--space-5) auto;
    max-width: 320px;
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
  }

  .err {
    margin: 0;
    font-size: 12px;
    padding-left: var(--space-2);
    border-left: 3px solid var(--danger);
    overflow-wrap: anywhere;
  }
</style>
