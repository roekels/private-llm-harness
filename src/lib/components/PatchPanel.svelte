<script lang="ts">
  import { onMount } from "svelte";
  import { api, errText } from "../api";
  import type { Patch, VmStatus } from "../types";

  interface Props {
    /** Called with the new status after a rebuild. */
    onstatus: (s: VmStatus) => void;
    /** Patches need the image to exist. */
    ready: boolean;
  }
  let { onstatus, ready }: Props = $props();

  const MAX_LOG = 300;
  let patches = $state<Patch[]>([]);
  let busy = $state<string | null>(null); // what is running, for the log title
  let log = $state<string[]>([]);
  let error = $state("");
  let editing = $state<Patch | null>(null);
  let confirmDelete = $state<string | null>(null);
  let logBox: HTMLElement | undefined = $state();

  onMount(() => {
    api.patches().then((p) => (patches = p)).catch((e) => (error = errText(e)));
  });

  function addLine(line: string): void {
    log.push(line);
    if (log.length > MAX_LOG) log.splice(0, log.length - MAX_LOG);
    queueMicrotask(() => logBox?.scrollTo({ top: logBox.scrollHeight }));
  }

  async function save(list: Patch[]): Promise<void> {
    error = "";
    try {
      patches = await api.savePatches(list);
    } catch (e) {
      error = errText(e);
    }
  }

  async function runPatch(p: Patch): Promise<void> {
    busy = p.name;
    log = [];
    error = "";
    try {
      const updated = await api.runPatch(p.id, addLine);
      patches = patches.map((x) => (x.id === updated.id ? updated : x));
      addLine(updated.lastRun?.ok ? "Finished." : `Failed: ${updated.lastRun?.summary ?? ""}`);
    } catch (e) {
      error = errText(e);
    } finally {
      busy = null;
    }
  }

  async function rebuild(): Promise<void> {
    busy = "Rebuild image";
    log = [];
    error = "";
    try {
      onstatus(await api.vmRebuild(addLine));
      addLine("Finished. The VM runs on the new image.");
      patches = await api.patches();
    } catch (e) {
      error = errText(e);
    } finally {
      busy = null;
    }
  }

  function startAdd(): void {
    editing = { id: crypto.randomUUID(), name: "", script: "", autoRun: false, builtin: false, lastRun: null };
  }

  async function saveEdit(): Promise<void> {
    const e = editing;
    if (!e) return;
    const exists = patches.some((p) => p.id === e.id);
    await save(exists ? patches.map((p) => (p.id === e.id ? e : p)) : [...patches, e]);
    if (!error) editing = null;
  }

  async function remove(id: string): Promise<void> {
    if (confirmDelete !== id) {
      confirmDelete = id;
      return;
    }
    confirmDelete = null;
    await save(patches.filter((p) => p.id !== id));
  }

  const when = (ms: number) =>
    new Date(ms).toLocaleString("en-US", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit", hour12: false });
</script>

<div class="patches">
  <div class="title">
    <strong>Maintenance</strong>
    <span class="spacer"></span>
    <button type="button" class="btn-secondary" disabled={!!busy || !ready} onclick={rebuild}
      title="Pull a fresh Debian base, rebuild without cache, restart the VM">Rebuild image</button
    >
    <button type="button" class="btn-secondary" disabled={!!busy || !!editing} onclick={startAdd}>Add patch</button>
  </div>
  <p class="muted">
    Patches are scripts you write. They run as root inside the VM, with internet on while they run. Model commands wait
    until they finish. The VM starts fresh from its image, so tick <em>Run at VM start</em> for anything that should
    always be there. Use <em>Rebuild image</em> for a full update.
  </p>

  <ul>
    {#each patches as p (p.id)}
      <li>
        <div class="row">
          <span class="name">{p.name}{#if p.builtin}&nbsp;<span class="tag">Built-in</span>{/if}</span>
          <label class="auto">
            <input
              type="checkbox"
              checked={p.autoRun}
              disabled={!!busy}
              onchange={(e) => save(patches.map((x) => (x.id === p.id ? { ...x, autoRun: e.currentTarget.checked } : x)))}
            />
            Run at VM start
          </label>
          <button type="button" class="btn-primary small" disabled={!!busy || !ready} onclick={() => runPatch(p)}>Run now</button>
          {#if !p.builtin}
            <button type="button" class="link" disabled={!!busy} onclick={() => (editing = { ...p })}>Edit</button>
            <button type="button" class="link danger" disabled={!!busy} onblur={() => (confirmDelete = null)} onclick={() => remove(p.id)}
              >{confirmDelete === p.id ? "Sure?" : "Delete"}</button
            >
          {/if}
        </div>
        {#if p.lastRun}
          <p class="last" data-ok={p.lastRun.ok}>
            Last run {when(p.lastRun.at)} · {p.lastRun.ok ? "OK" : "Failed"} · {(p.lastRun.ms / 1000).toFixed(0)} s{p.lastRun.ok
              ? ""
              : ` · ${p.lastRun.summary}`}
          </p>
        {/if}
      </li>
    {/each}
  </ul>

  {#if editing}
    <div class="edit">
      <label>Name <input bind:value={editing.name} maxlength="80" placeholder="Install ffmpeg" /></label>
      <label
        >Script (bash, runs as root with <code>set -e</code>)
        <textarea bind:value={editing.script} rows="5" spellcheck="false" placeholder="apt-get update && apt-get install -y ffmpeg"></textarea>
      </label>
      <label class="auto"><input type="checkbox" bind:checked={editing.autoRun} /> Run at VM start</label>
      <div class="row">
        <button type="button" class="btn-primary" disabled={!editing.name.trim() || !editing.script.trim()} onclick={saveEdit}>Save patch</button>
        <button type="button" class="btn-secondary" onclick={() => (editing = null)}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if busy || log.length}
    <div class="logwrap">
      <div class="logtitle">{busy ? `Running: ${busy}…` : "Last output"}</div>
      <pre class="log" bind:this={logBox} aria-live="polite">{log.join("\n")}</pre>
    </div>
  {/if}
  {#if error}<p class="err" role="alert">{error}</p>{/if}
</div>

<style>
  .patches {
    border-top: 1px solid var(--border);
    padding-top: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .title,
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
    margin: 0;
    font-size: 12px;
    color: var(--text-muted);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  li {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--mist-25);
  }

  li .row {
    flex-wrap: wrap;
  }

  .name {
    font-weight: 700;
    flex: 1 1 220px;
  }

  li .row > :not(.name) {
    white-space: nowrap;
  }

  .tag {
    font-family: var(--font-micro);
    font-size: 11px;
    padding: 0 6px;
    border-radius: var(--radius);
    background: var(--mist);
  }

  .auto {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-size: 12px;
    font-weight: 400;
    flex-direction: row;
  }

  .auto input {
    accent-color: var(--primary);
  }

  .small {
    padding: 2px var(--space-2);
    font-size: 12px;
  }

  .link {
    padding: 2px var(--space-1);
    font-size: 12px;
    color: var(--link);
  }

  .link.danger {
    color: var(--danger);
  }

  .last {
    margin: var(--space-1) 0 0;
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--text-muted);
  }

  .last[data-ok="false"] {
    color: var(--danger);
  }

  .edit {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--primary);
    border-radius: var(--radius);
  }

  .edit label:not(.auto) {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: 13px;
    font-weight: 700;
  }

  .edit input:not([type="checkbox"]),
  textarea {
    font-weight: 400;
    padding: var(--space-2);
    border: 1px solid var(--blue-25);
    border-radius: var(--radius);
    background: var(--surface);
  }

  textarea {
    font-family: var(--font-mono);
    font-size: 12px;
    resize: vertical;
  }

  .logwrap {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .logtitle {
    padding: 2px var(--space-2);
    background: var(--mist);
    font-family: var(--font-micro);
    font-size: 12px;
  }

  .log {
    margin: 0;
    padding: var(--space-2);
    max-height: 220px;
    overflow: auto;
    font-size: 11px;
    background: var(--mist-25);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .err {
    margin: 0;
    font-size: 13px;
    padding-left: var(--space-2);
    border-left: 3px solid var(--danger);
    overflow-wrap: anywhere;
  }
</style>
