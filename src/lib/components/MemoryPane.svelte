<script lang="ts">
  import { errText } from "../api";
  import { compactNow, saveMemory } from "../chat.svelte";
  import { boldRuns, fmtInt } from "../format";
  import { app } from "../state.svelte";

  let editing = $state(false);
  let draft = $state("");
  let error = $state("");
  let saved = $state(false);

  function startEdit(): void {
    draft = app.memory;
    editing = true;
    saved = false;
  }

  async function save(): Promise<void> {
    error = "";
    try {
      await saveMemory(draft);
      editing = false;
      saved = true;
    } catch (e) {
      error = errText(e);
    }
  }

  const lines = $derived(app.memory.split("\n"));
</script>

<aside class="pane" aria-label="Session memory">
  <header>
    <div class="row">
      <strong>Session memory</strong>
      <span class="spacer"></span>
      <button class="btn-secondary small" onclick={() => (app.memoryOpen = false)}>Close</button>
    </div>
    <p class="muted">
      When the context passes 50%, the model writes the key facts, progress and values here, then older messages leave its
      context (they stay in the chat). Compacted {app.compactions} {app.compactions === 1 ? "time" : "times"}.
    </p>
    <div class="row">
      <button class="btn-primary small" disabled={app.generating || !app.activeSessionId || editing} onclick={() => void compactNow()}>
        {app.compacting ? "Compacting…" : "Compact now"}
      </button>
      {#if !editing}
        <button class="btn-secondary small" disabled={!app.activeSessionId} onclick={startEdit}>Edit</button>
      {/if}
      {#if app.lastCompaction}
        <span class="muted">Last: about {fmtInt(app.lastCompaction.before)} → {fmtInt(app.lastCompaction.after)} tokens</span>
      {/if}
    </div>
  </header>

  <div class="body">
    {#if editing}
      <label class="visually-hidden" for="memory-edit">Session memory</label>
      <textarea id="memory-edit" bind:value={draft} spellcheck="false" maxlength="8000"></textarea>
      <div class="row">
        <button class="btn-primary small" onclick={save}>Save</button>
        <button class="btn-secondary small" onclick={() => (editing = false)}>Cancel</button>
        <span class="muted">Takes effect on the next message. The cache is rebuilt once.</span>
      </div>
    {:else if app.memory}
      <div class="memory">
        {#each lines as line, i (i)}
          {#if line.startsWith("## ")}
            <h3>{line.slice(3)}</h3>
          {:else if line.trim()}
            <p class:bullet={line.trimStart().startsWith("- ")}>
              {#each boldRuns(line.trimStart().replace(/^- /, "")) as r, j (j)}{#if r.bold}<strong>{r.text}</strong>{:else}{r.text}{/if}{/each}
            </p>
          {/if}
        {/each}
      </div>
      {#if saved}<p class="muted">Saved.</p>{/if}
    {:else}
      <p class="empty">No memory yet. It's written the first time this session is compacted, or when you select Compact now.</p>
    {/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
  </div>
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

  header {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border-bottom: 1px solid var(--border);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    font-size: 13px;
  }

  .spacer {
    flex: 1;
  }

  .muted {
    margin: 0;
    color: var(--text-muted);
    font-size: 12px;
  }

  .small {
    padding: 2px var(--space-2);
    font-size: 12px;
  }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-3) var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .memory h3 {
    font-size: 13px;
    margin: var(--space-3) 0 var(--space-1);
  }

  .memory h3:first-child {
    margin-top: 0;
  }

  .memory p {
    margin: 0 0 2px;
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .memory p.bullet {
    padding-left: var(--space-3);
    position: relative;
  }

  .memory p.bullet::before {
    content: "•";
    position: absolute;
    left: 0;
    color: var(--text-muted);
  }

  textarea {
    width: 100%;
    min-height: 360px;
    flex: 1;
    padding: var(--space-2);
    border: 1px solid var(--blue-25);
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 12px;
    resize: vertical;
  }

  .empty {
    margin: var(--space-5) auto;
    max-width: 300px;
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
  }

  .err {
    margin: 0;
    padding-left: var(--space-2);
    border-left: 3px solid var(--danger);
    font-size: 12px;
  }
</style>
