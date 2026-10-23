<script lang="ts">
  import { api, errText } from "../api";
  import { fmtInt } from "../format";
  import { activeProfile, app } from "../state.svelte";

  const ctx = $derived(activeProfile()?.contextTokens ?? 0);
  // The last reply that saw the current window gives the exact context use.
  const last = $derived(
    [...app.messages].reverse().find((m) => m.role === "assistant" && m.meta?.windowStart === app.trimSeq)?.meta ?? null,
  );
  const used = $derived((last?.promptTokens ?? 0) + (last?.completionTokens ?? 0));
  const pct = $derived(Math.min(100, ctx ? (used / ctx) * 100 : 0));
  // Prompt speed is only meaningful when a real chunk was processed; with a
  // warm cache most turns process ~20 new tokens and overhead dominates.
  const newPromptTokens = $derived((last?.promptTokens ?? 0) - (last?.cachedTokens ?? 0));
  const showPromptSpeed = $derived(!!last?.promptPerSecond && newPromptTokens >= 200);

  let exporting = $state(false);
  let auditNote = $state("");
  let noteTimer: ReturnType<typeof setTimeout> | undefined;
  async function exportAudit(): Promise<void> {
    const id = app.activeSessionId;
    if (!id) return;
    exporting = true;
    try {
      const path = await api.exportSession(id);
      auditNote = `Saved to Downloads: ${path.split("/").pop() ?? path}`;
    } catch (e) {
      auditNote = `Export failed: ${errText(e)}`;
    } finally {
      exporting = false;
      clearTimeout(noteTimer);
      noteTimer = setTimeout(() => (auditNote = ""), 8000);
    }
  }
</script>

<footer class="bar">
  <div
    class="meter"
    role="meter"
    aria-label="Context used"
    aria-valuemin="0"
    aria-valuemax={ctx}
    aria-valuenow={used}
    title="Compacts automatically above 50%"
  >
    <div class="fill" class:warn={pct >= 50} class:high={pct >= 80} style:width="{pct}%"></div>
    <div class="mark" aria-hidden="true"></div>
  </div>
  <span>{fmtInt(used)} / {fmtInt(ctx)} tokens ({Math.round(pct)}%)</span>
  {#if !app.browserOpen && !app.memoryOpen}<span>Cached {fmtInt(last?.cachedTokens ?? 0)}</span>{/if}
  <span>{(last?.tokensPerSecond ?? 0).toFixed(1)} tok/s</span>
  {#if showPromptSpeed && last?.promptPerSecond && !app.browserOpen && !app.memoryOpen}<span>Prompt {fmtInt(last.promptPerSecond)} tok/s</span>{/if}
  <span class="spacer"></span>
  {#if auditNote}<span class="note" role="status">{auditNote}</span>{/if}
  {#if app.activeSessionId}
    <button
      class="files"
      title="Export this session's full history (messages, tool calls and output) as a CSV file to Downloads"
      disabled={exporting}
      onclick={() => void exportAudit()}>Audit</button
    >
  {/if}
  {#if app.untilDone}<span class="live" title="Steps run without asking until the model's final answer">Approving until done</span>{/if}
  {#if app.compacting}<span class="live">Compacting</span>{/if}
  {#if app.generating}<span class="live">Generating</span>{/if}
  <button
    class="files"
    aria-pressed={app.memoryOpen}
    title="Session memory and compaction"
    onclick={() => {
      app.memoryOpen = !app.memoryOpen;
      if (app.memoryOpen) app.browserOpen = false;
    }}>{app.memoryOpen ? "Hide memory" : `Memory${app.compactions ? ` (${app.compactions})` : ""}`}</button
  >
  <button
    class="files"
    aria-pressed={app.browserOpen}
    title="Show the sandbox browser beside the chat"
    onclick={() => {
      app.browserOpen = !app.browserOpen;
      if (app.browserOpen) app.memoryOpen = false;
    }}>{app.browserOpen ? "Hide browser" : "Browser"}</button
  >
  {#if app.activeSessionId}
    <button
      class="files"
      title={app.workspace ? `Open ${app.workspace} in Finder` : "Open this session's folder in Finder"}
      onclick={() => {
        const id = app.activeSessionId;
        if (id) api.revealWorkspace(id).catch((e) => (app.chatError = errText(e)));
      }}>Files</button
    >
  {/if}
</footer>

<style>
  .bar > * {
    white-space: nowrap;
  }

  .bar {
    display: flex;
    align-items: center;
    overflow: hidden;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-5);
    border-top: 1px solid var(--border);
    background: var(--surface);
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--text-muted);
  }

  .meter {
    position: relative;
    width: 120px;
    height: 6px;
    background: var(--mist);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .mark {
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--text-muted);
  }

  .fill.warn {
    background: var(--warn);
  }

  .fill {
    height: 100%;
    background: var(--primary);
  }

  .fill.high {
    background: var(--danger);
  }

  .spacer {
    flex: 1;
  }

  .files {
    padding: 2px var(--space-2);
    font-size: 12px;
    color: var(--link);
  }

  .note {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    flex-shrink: 1;
  }

  .live {
    color: var(--text);
    font-weight: 700;
  }
</style>
