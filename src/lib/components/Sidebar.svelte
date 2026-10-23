<script lang="ts">
  import { errText } from "../api";
  import { deleteSession, newSession, openSession, renameSession } from "../chat.svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { onMount } from "svelte";
  import { api } from "../api";
  import { activeProfile, app } from "../state.svelte";
  import logo from "../../assets/logo.png";
  import { CHANNEL } from "../release";

  let version = $state("");
  onMount(() => {
    getVersion()
      .then((v) => (version = v))
      .catch(() => (version = ""));
  });

  let editingId = $state<string | null>(null);
  let editTitle = $state("");
  let confirmId = $state<string | null>(null);
  let error = $state("");

  function startRename(id: string, title: string): void {
    editingId = id;
    editTitle = title;
    confirmId = null;
  }

  async function finishRename(): Promise<void> {
    const id = editingId;
    editingId = null;
    if (!id || !editTitle.trim()) return;
    try {
      await renameSession(id, editTitle);
    } catch (e) {
      error = errText(e);
    }
  }

  async function onDelete(id: string): Promise<void> {
    if (confirmId !== id) {
      confirmId = id;
      return;
    }
    confirmId = null;
    try {
      await deleteSession(id);
    } catch (e) {
      error = errText(e);
    }
  }

  function focusSelect(el: HTMLInputElement): void {
    el.focus();
    el.select();
  }

  const run = (p: Promise<void>) => p.catch((e) => (error = errText(e)));
</script>

<nav aria-label="Sessions">
  <header>
    <img class="logo" src={logo} alt="" width="32" height="32" />
    <div class="brand">
      <span class="name">Ruan's Harness</span>
      <span class="profile">{activeProfile()?.name ?? "No profile"}</span>
    </div>
  </header>

  <button class="btn-primary new" disabled={app.generating} onclick={() => run(newSession())}>New session</button>

  <ul>
    {#each app.sessions as s (s.id)}
      {@const active = s.id === app.activeSessionId && app.view === "chat"}
      <li class="row" class:active>
        {#if editingId === s.id}
          <input
            class="rename"
            aria-label="Session title"
            maxlength="80"
            bind:value={editTitle}
            use:focusSelect
            onblur={finishRename}
            onkeydown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
              if (e.key === "Escape") editingId = null;
            }}
          />
        {:else}
          <button
            class="item"
            aria-current={active ? "page" : undefined}
            disabled={app.generating && s.id !== app.activeSessionId}
            ondblclick={() => startRename(s.id, s.title)}
            onclick={() => run(openSession(s.id))}>{s.title}</button
          >
          <span class="tools">
            <button class="tool" aria-label="Rename {s.title}" onclick={() => startRename(s.id, s.title)}>Rename</button>
            <button
              class="tool"
              class:confirm={confirmId === s.id}
              aria-label="Delete {s.title}"
              disabled={app.generating && s.id === app.activeSessionId}
              onblur={() => (confirmId = confirmId === s.id ? null : confirmId)}
              onclick={() => onDelete(s.id)}>{confirmId === s.id ? "Sure?" : "Delete"}</button
            >
          </span>
        {/if}
      </li>
    {:else}
      <li class="empty">No sessions yet</li>
    {/each}
  </ul>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  <footer>
    <button
      class="item"
      class:on={app.view === "setup"}
      aria-current={app.view === "setup" ? "page" : undefined}
      onclick={() => (app.view = app.view === "setup" ? "chat" : "setup")}
      >Setup{#if app.setupNeeded}<span class="attn" title="Something needs attention"></span>{/if}</button
    >
    <button
      class="item"
      class:on={app.view === "settings"}
      aria-current={app.view === "settings" ? "page" : undefined}
      onclick={() => (app.view = app.view === "settings" ? "chat" : "settings")}>Settings</button
    >
    <button
      class="item"
      class:on={app.view === "about"}
      aria-current={app.view === "about" ? "page" : undefined}
      onclick={() => (app.view = app.view === "about" ? "chat" : "about")}>About</button
    >
    <div class="credit">
      <span>Developed by Ruan · 2026</span>
      <span>
        {#if version}v{version}{CHANNEL ? ` ${CHANNEL}` : ""} · {/if}<button class="gh" title="Open github.com/roekels" onclick={() => void api.openLink("github")}>@Roekels</button>
      </span>
    </div>
  </footer>
</nav>

<style>
  nav {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-3);
    background: var(--nav-bg);
    color: var(--nav-text);
    min-height: 0;
  }

  header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2);
  }

  .logo {
    flex: none;
    border-radius: 50%;
  }

  .brand {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .credit {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: var(--space-3);
    padding: var(--space-3) var(--space-2) 0;
    border-top: 1px solid var(--nav-hover);
    font-family: var(--font-micro);
    font-size: 11px;
    color: var(--nav-text-muted);
  }

  .attn {
    display: inline-block;
    width: 8px;
    height: 8px;
    margin-left: var(--space-2);
    border-radius: 50%;
    background: var(--yellow);
    vertical-align: middle;
  }

  .gh {
    padding: 0;
    font-size: 11px;
    color: var(--nav-text);
    text-decoration: underline;
  }

  .name {
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 16px;
  }

  .profile {
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--nav-text-muted);
  }

  .new {
    width: 100%;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    flex: 1;
    overflow-y: auto;
  }

  .row {
    display: flex;
    align-items: center;
    border-radius: var(--radius);
  }

  .row:hover,
  .row:focus-within,
  .row.active {
    background: var(--nav-hover);
  }

  .item {
    flex: 1;
    min-width: 0;
    text-align: left;
    color: var(--nav-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row.active .item,
  .item.on {
    font-weight: 700;
  }

  footer .item {
    width: 100%;
  }

  footer .item:hover,
  footer .item.on {
    background: var(--nav-hover);
  }

  .tools {
    display: none;
    gap: 2px;
    padding-right: var(--space-1);
  }

  .row:hover .tools,
  .row:focus-within .tools {
    display: flex;
  }

  .tool {
    padding: 2px 6px;
    font-family: var(--font-micro);
    font-size: 11px;
    color: var(--nav-text-muted);
  }

  .tool:hover {
    color: var(--nav-text);
  }

  .tool.confirm {
    color: var(--white);
    background: var(--red);
  }

  .rename {
    width: 100%;
    padding: var(--space-2);
    border: 1px solid var(--blue-25);
    border-radius: var(--radius);
    background: var(--white);
    color: var(--navy);
  }

  .empty {
    padding: var(--space-2);
    color: var(--nav-text-muted);
    font-size: 13px;
  }

  .error {
    margin: 0;
    font-size: 12px;
    color: var(--nav-text);
    border-left: 3px solid var(--red);
    padding-left: var(--space-2);
  }

  nav :global(:focus-visible) {
    outline-color: var(--white);
  }
</style>
