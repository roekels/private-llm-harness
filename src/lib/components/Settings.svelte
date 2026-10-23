<script lang="ts">
  import { api, errText } from "../api";
  import { checkNow } from "../connection.svelte";
  import { app, blankProfile } from "../state.svelte";
  import SandboxPanel from "./SandboxPanel.svelte";
  import type { KeyRef, Profile } from "../types";

  type Source = "env" | "keychain" | "none";
  type Note = { kind: "ok" | "error" | "info"; text: string };

  const SOURCES: { id: Source; label: string }[] = [
    { id: "keychain", label: "Keychain" },
    { id: "env", label: ".env or environment" },
    { id: "none", label: "No key" },
  ];

  let selectedId = $state<string | null>(null);
  let draft = $state<Profile>(blankProfile());
  let isNew = $state(false);
  let source = $state<Source>("none");
  let envName = $state("");
  let keychainRef = $state("");
  let keyInput = $state("");
  let note = $state<Note | null>(null);
  let busy = $state(false);
  let confirmDelete = $state(false);

  load(app.activeId ?? app.profiles[0]?.id ?? null);

  function load(id: string | null): void {
    const p = app.profiles.find((x) => x.id === id);
    isNew = !p;
    draft = p ? structuredClone($state.snapshot(p)) : blankProfile();
    selectedId = draft.id;
    const ref = draft.keyRef ?? "";
    source = ref.startsWith("env:") ? "env" : ref.startsWith("keychain:") ? "keychain" : "none";
    envName = source === "env" ? ref.slice(4) : "";
    keychainRef = source === "keychain" ? ref : `keychain:${app.keychainService}/${draft.id}`;
    keyInput = "";
    note = null;
    confirmDelete = false;
  }

  function keyRef(): KeyRef | undefined {
    switch (source) {
      case "env":
        return envName.trim() ? `env:${envName.trim()}` : undefined;
      case "keychain":
        return keychainRef as KeyRef;
      case "none":
        return undefined;
    }
  }

  function snapshot(): Profile {
    const p = $state.snapshot(draft) as Profile;
    return { ...p, fallbackUrl: p.fallbackUrl?.trim() || undefined, keyRef: keyRef() };
  }

  async function run(task: () => Promise<Note | null>): Promise<void> {
    busy = true;
    try {
      note = await task();
    } catch (e) {
      note = { kind: "error", text: errText(e) };
    } finally {
      busy = false;
    }
  }

  async function persist(): Promise<Profile> {
    const saved = await api.saveProfile(snapshot());
    const i = app.profiles.findIndex((p) => p.id === saved.id);
    if (i >= 0) app.profiles[i] = saved;
    else app.profiles.push(saved);
    if (!app.activeId) app.activeId = saved.id;
    isNew = false;
    draft = structuredClone(saved);
    if (saved.id === app.activeId) void checkNow();
    return saved;
  }

  const save = () =>
    run(async () => {
      await persist();
      return { kind: "ok", text: "Saved." };
    });

  const use = () =>
    run(async () => {
      const saved = await persist();
      await api.activateProfile(saved.id);
      app.activeId = saved.id;
      void checkNow();
      return { kind: "ok", text: `Now using ${saved.name}.` };
    });

  const testConnection = () =>
    run(async () => {
      const r = await api.probe(snapshot());
      const models = r.models.length ? ` Models: ${r.models.join(", ")}.` : "";
      const ms = r.latencyMs != null ? ` ${r.latencyMs} ms.` : "";
      return { kind: r.status === "ready" ? "ok" : "error", text: `${r.message}${models}${ms}` };
    });

  const testKey = () =>
    run(async () => {
      const ref = keyRef();
      if (!ref) return { kind: "info", text: "No key set for this profile." };
      const k = await api.checkKey(ref);
      return { kind: "ok", text: `Found in ${k.source}: ${k.length} characters.` };
    });

  const saveToKeychain = () =>
    run(async () => {
      const value = keyInput;
      keyInput = ""; // clear right away, whatever happens next
      keychainRef = await api.keychainSet(draft.id, value);
      await persist();
      return { kind: "ok", text: "Key saved to your Keychain and profile saved." };
    });

  const removeFromKeychain = () =>
    run(async () => {
      await api.keychainDelete(draft.id);
      return { kind: "ok", text: "Key removed from your Keychain." };
    });

  // Tool settings are global and save as soon as they change.
  let toolNote = $state("");
  async function saveTools(): Promise<void> {
    try {
      await api.setToolSettings($state.snapshot(app.tools));
      toolNote = "Saved. Applies from the next message.";
    } catch (e) {
      toolNote = errText(e);
    }
  }

  const remove = () =>
    run(async () => {
      if (!confirmDelete) {
        confirmDelete = true;
        return { kind: "info", text: "Select Delete again to remove this profile." };
      }
      const name = draft.name;
      app.activeId = await api.deleteProfile(draft.id);
      app.profiles = app.profiles.filter((p) => p.id !== draft.id);
      load(app.activeId);
      void checkNow();
      return { kind: "ok", text: `Deleted ${name}.` };
    });
</script>

<section class="settings">
  <header class="head">
    <h1>Settings</h1>
    <button class="btn-secondary" onclick={() => (app.view = "chat")}>Back to chat</button>
  </header>

  <div class="body">
    <aside class="list" aria-label="Profiles">
      <h2>Profiles</h2>
      <ul>
        {#each app.profiles as p (p.id)}
          <li>
            <button class="item" class:selected={p.id === selectedId} onclick={() => load(p.id)}>
              <span class="pname">{p.name}</span>
              {#if p.id === app.activeId}<span class="tag">In use</span>{/if}
              <span class="purl">{p.baseUrl}</span>
            </button>
          </li>
        {/each}
      </ul>
      <button class="btn-secondary add" onclick={() => load(null)}>Add profile</button>
    </aside>

    <div class="right">
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <h2>{isNew ? "New profile" : draft.name || "Profile"}</h2>

      <fieldset>
        <legend>Connection</legend>
        <label>Name <input bind:value={draft.name} required maxlength="80" placeholder="My GPU server" /></label>
        <label
          >Base URL <input bind:value={draft.baseUrl} required placeholder="http://my-gpu-box:8080/v1" spellcheck="false" />
          <small>OpenAI-compatible, ending in /v1.</small>
        </label>
        <label
          >Fallback URL <input bind:value={draft.fallbackUrl} placeholder="Optional, for example http://100.x.y.z:8081/v1" spellcheck="false" />
          <small>Tried when the base URL can't be reached.</small>
        </label>
        <div class="row">
          <label>Model id <input bind:value={draft.model} required placeholder="qwen35" spellcheck="false" /></label>
          <label>Context size <input type="number" bind:value={draft.contextTokens} min="512" step="512" /></label>
          <label>Health path <input bind:value={draft.healthPath} spellcheck="false" /></label>
        </div>
        <label
          >Model description <input bind:value={draft.modelInfo} maxlength="200" placeholder="Shown when you point at the model id" spellcheck="false" />
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={draft.allowPublic} />
          Allow public host
          <small>Off keeps the app on localhost, your LAN and your tailnet. Public hosts also need https.</small>
        </label>
      </fieldset>

      <fieldset>
        <legend>Key</legend>
        <div class="seg" role="radiogroup" aria-label="Key source">
          {#each SOURCES as s (s.id)}
            <label class:on={source === s.id}>
              <input type="radio" name="source" value={s.id} bind:group={source} />
              {s.label}
            </label>
          {/each}
        </div>

        {#if source === "env"}
          <label
            >Variable name
            <input bind:value={envName} placeholder="QWEN35_HARNESS_KEY" spellcheck="false" />
            <small>Read from the app's environment first, then <code>{app.envFile}</code>. Mainly for development; the Keychain is the better choice.</small>
          </label>
        {:else if source === "keychain"}
          <p class="ref">Stored as <code>{keychainRef}</code></p>
          <label
            >Paste key
            <input type="password" bind:value={keyInput} autocomplete="off" placeholder="sk-..." />
            <small>Saved to your login Keychain. The app never shows it again.</small>
          </label>
          <div class="row-actions">
            <button type="button" class="btn-secondary" disabled={busy || !keyInput.trim()} onclick={saveToKeychain}>Save to Keychain</button>
            <button type="button" class="btn-secondary" disabled={busy || isNew} onclick={removeFromKeychain}>Remove from Keychain</button>
          </div>
        {:else}
          <p class="hint">Requests go out without an Authorization header.</p>
        {/if}

        {#if source !== "none"}
          <div class="row-actions">
            <button type="button" class="btn-secondary" disabled={busy} onclick={testKey}>Test key</button>
          </div>
        {/if}
      </fieldset>

      <details>
        <summary>Sampling</summary>
        <div class="row">
          <label>Temperature <input type="number" bind:value={draft.sampling.temperature} min="0" max="2" step="0.05" /></label>
          <label>Top p <input type="number" bind:value={draft.sampling.top_p} min="0" max="1" step="0.05" /></label>
          <label>Top k <input type="number" bind:value={draft.sampling.top_k} min="0" max="1000" step="1" /></label>
          <label>Min p <input type="number" bind:value={draft.sampling.min_p} min="0" max="1" step="0.01" /></label>
          <label>Presence penalty <input type="number" bind:value={draft.sampling.presence_penalty} min="-2" max="2" step="0.1" /></label>
        </div>
      </details>

      {#if note}
        <p class="note" data-kind={note.kind} role="status">{note.text}</p>
      {/if}

      <div class="actions">
        <button type="submit" class="btn-primary" disabled={busy}>Save</button>
        <button type="button" class="btn-secondary" disabled={busy || draft.id === app.activeId} onclick={use}>Save and use</button>
        <button type="button" class="btn-secondary" disabled={busy} onclick={testConnection}>Test connection</button>
        <span class="spacer"></span>
        {#if !isNew}
          <button type="button" class="danger" disabled={busy} onclick={remove}>{confirmDelete ? "Delete, I'm sure" : "Delete"}</button>
        {/if}
      </div>
    </form>

    <section class="form" aria-labelledby="tools-h">
      <h2 id="tools-h">Tools (all profiles)</h2>
      <fieldset>
        <legend>Terminal and files</legend>
        <label class="check">
          <input type="checkbox" bind:checked={app.tools.enabled} onchange={saveTools} />
          Let the model use tools
          <small>Commands, file reads and file writes, in a separate folder per session. Turning this on or off changes the start of the prompt, so the next reply reprocesses the whole conversation once.</small>
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={app.tools.autoApprove} onchange={saveTools} disabled={!app.tools.enabled} />
          Run commands and writes without asking
          <small>Off: you approve each one. Reading files in the session folder never asks.</small>
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={app.tools.network} onchange={saveTools} disabled={!app.tools.enabled} />
          {app.tools.backend === "vm" ? "Allow internet for commands" : "Allow network for commands"}
          <small
            >{app.tools.backend === "vm"
              ? "Internet only: the VM's firewall blocks your home network, the tailnet and this Mac. Off: no network at all."
              : "Full network, including your home network and tailnet. Off: no network."}{app.tools.autoApprove && app.tools.network
              ? " With auto-approve on too, the model can send data out without asking."
              : ""}</small
          >
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.tools.rateLimit}
            onchange={saveTools}
            disabled={!app.tools.enabled || app.tools.backend !== "vm" || !app.tools.network}
          />
          Limit bursts (anti-block)
          <small>At most 20 new connections per second to any one site; the rest is refused inside the VM. Normal browsing stays well under this, but a runaway loop can't hammer a site and get your address blocked.</small>
        </label>
        <p class="hint">
          {app.tools.backend === "vm"
            ? "Commands, reads and writes run in the Linux VM as an unprivileged user, in /work/<session>. Only that folder is shared with this Mac. Commands stop after 60 seconds."
            : "Commands run in the macOS sandbox. They can write only the session folder, can't read your home folder, Keychain or clipboard, and stop after 60 seconds."}
        </p>
        {#if toolNote}<p class="note" data-kind="ok" role="status">{toolNote}</p>{/if}
      </fieldset>
      <SandboxPanel onchange={saveTools} />
    </section>
    </div>
  </div>
</section>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-3) var(--space-5);
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }

  .head h1 {
    font-size: 16px;
  }

  .body {
    display: grid;
    grid-template-columns: 240px 1fr;
    gap: var(--space-5);
    padding: var(--space-5);
    overflow-y: auto;
    min-height: 0;
  }

  h2 {
    font-size: 14px;
    margin-bottom: var(--space-3);
  }

  .list ul {
    list-style: none;
    margin: 0 0 var(--space-3);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .item {
    width: 100%;
    text-align: left;
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 2px var(--space-2);
    background: var(--surface);
    border-color: var(--border);
  }

  .item.selected {
    border-color: var(--primary);
  }

  .pname {
    font-weight: 700;
  }

  .purl {
    grid-column: 1 / -1;
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag {
    font-family: var(--font-micro);
    font-size: 11px;
    font-weight: 700;
    color: var(--white);
    background: var(--ok);
    border-radius: var(--radius);
    padding: 1px 6px;
    align-self: center;
  }

  .add {
    width: 100%;
  }

  .right {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    min-width: 0;
  }

  .form {
    max-width: 720px;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  fieldset {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    padding: var(--space-4);
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  legend {
    font-family: var(--font-display);
    font-weight: 700;
    padding: 0 var(--space-1);
  }

  label {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-weight: 700;
    font-size: 13px;
  }

  input:not([type="checkbox"]):not([type="radio"]) {
    font-weight: 400;
    padding: var(--space-2);
    border: 1px solid var(--blue-25);
    border-radius: var(--radius);
    background: var(--surface);
    min-width: 0;
  }

  small,
  .hint {
    font-family: var(--font-micro);
    font-weight: 400;
    font-size: 12px;
    color: var(--text-muted);
    margin: 0;
  }

  .row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: var(--space-3);
  }

  .check {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    column-gap: var(--space-2);
  }

  .check small {
    grid-column: 2;
  }

  .check input,
  .seg input {
    accent-color: var(--primary);
  }

  .seg {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .seg label {
    flex-direction: row;
    align-items: center;
    gap: var(--space-1);
    font-weight: 400;
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--blue-25);
    border-radius: var(--radius);
    cursor: pointer;
  }

  .seg label.on {
    border-color: var(--primary);
    background: var(--mist-60);
    font-weight: 700;
  }

  .ref {
    margin: 0;
    font-size: 13px;
  }


  details {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: var(--space-3) var(--space-4);
  }

  summary {
    cursor: pointer;
    font-family: var(--font-display);
    font-weight: 700;
  }

  details .row {
    margin-top: var(--space-3);
  }

  .row-actions,
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .spacer {
    flex: 1;
  }

  .danger {
    color: var(--danger);
    border-color: var(--danger);
    background: var(--surface);
  }

  .note {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius);
    border-left: 4px solid var(--primary);
    background: var(--surface-muted);
    overflow-wrap: anywhere;
  }

  .note[data-kind="ok"] {
    border-left-color: var(--ok);
  }

  .note[data-kind="error"] {
    border-left-color: var(--danger);
  }
</style>
