<script lang="ts">
  import { onMount } from "svelte";
  import { api, errText } from "../api";
  import { firewallText } from "../format";
  import { app } from "../state.svelte";
  import type { VmStatus } from "../types";
  import PatchPanel from "./PatchPanel.svelte";

  interface Props {
    onchange: () => void;
  }
  let { onchange }: Props = $props();

  // Shared with the top bar (vm.svelte.ts).
  let status = $derived(app.vm);
  const setStatus = (s: VmStatus | null) => (app.vm = s);
  let busy = $state(false);
  let progress = $state("");
  let error = $state("");

  const LABEL: Record<VmStatus["state"], string> = {
    "not-installed": "Not installed",
    "system-stopped": "Container system stopped",
    "no-image": "Not set up",
    stopped: "Stopped",
    running: "Running",
  };

  async function refresh(): Promise<void> {
    try {
      setStatus(await api.vmStatus());
    } catch (e) {
      error = errText(e);
    }
  }

  async function setup(): Promise<void> {
    busy = true;
    error = "";
    progress = "Starting…";
    try {
      setStatus(await api.vmSetup((line) => (progress = line)));
      progress = "";
    } catch (e) {
      error = errText(e);
      progress = "";
      await refresh();
    } finally {
      busy = false;
    }
  }

  async function stopVm(): Promise<void> {
    busy = true;
    error = "";
    try {
      setStatus(await api.vmStop());
    } catch (e) {
      error = errText(e);
    } finally {
      busy = false;
    }
  }

  onMount(() => void refresh());
</script>

<fieldset>
  <legend>Sandbox</legend>
  <div class="choice" role="radiogroup" aria-label="Where tools run">
    <label class:on={app.tools.backend === "vm"}>
      <input type="radio" name="backend" value="vm" bind:group={app.tools.backend} onchange={onchange} />
      <span><strong>Linux VM</strong> (recommended)<small>Its own system, disk and browser. Can't see your Mac. Network is internet only.</small></span>
    </label>
    <label class:on={app.tools.backend === "macos"}>
      <input type="radio" name="backend" value="macos" bind:group={app.tools.backend} onchange={onchange} />
      <span><strong>macOS sandbox</strong><small>Runs on this Mac with limits. Faster to start, less isolated.</small></span>
    </label>
  </div>

  {#if app.tools.backend === "vm"}
    <div class="vm" data-state={status?.state ?? "unknown"}>
      <div class="row">
        <span class="dot" aria-hidden="true"></span>
        <strong>{status ? LABEL[status.state] : "Checking…"}</strong>
        {#if status?.state === "running" && status.network}
          <span class="muted">Firewall: {firewallText(status.network)}</span>
        {/if}
        <span class="spacer"></span>
        <button type="button" class="btn-secondary" disabled={busy} onclick={refresh}>Refresh</button>
        {#if status?.state === "running"}
          <button type="button" class="btn-secondary" disabled={busy} onclick={stopVm}>Stop VM</button>
        {:else if status && status.state !== "not-installed"}
          <button type="button" class="btn-primary" disabled={busy} onclick={setup}>
            {status.state === "no-image" ? "Set up sandbox" : "Start VM"}
          </button>
        {/if}
      </div>

      {#if status?.state === "not-installed"}
        <p class="muted">
          Apple's <strong>container</strong> tool isn't installed.
          <button type="button" class="btn-primary" onclick={() => (app.view = "setup")}>Open Setup</button> downloads, checks and
          installs it for you.
        </p>
      {:else if status?.state === "no-image"}
        <p class="muted">Setting up builds the sandbox image once (Debian, Python, Node, Chromium). It takes a few minutes and about 1 GB of disk.</p>
      {/if}
      {#if progress}<p class="progress" role="status">{progress}</p>{/if}
      {#if error}<p class="err" role="alert">{error}</p>{/if}
      {#if status?.version}<p class="muted small">{status.version} · image {status.image}</p>{/if}
      {#if status && status.state !== "not-installed"}
        <PatchPanel ready={status.state === "running" || status.state === "stopped"} onstatus={setStatus} />
      {/if}
    </div>
  {/if}
</fieldset>

<style>
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

  .choice {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-2);
  }

  .choice label {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--blue-25);
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 13px;
  }

  .choice label.on {
    border-color: var(--primary);
    background: var(--mist-60);
  }

  .choice input {
    margin-top: 3px;
    accent-color: var(--primary);
  }

  .choice small {
    display: block;
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--text-muted);
  }

  .vm {
    border-top: 1px solid var(--border);
    padding-top: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
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

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-muted);
  }

  [data-state="running"] .dot {
    background: var(--ok);
  }
  [data-state="stopped"] .dot,
  [data-state="no-image"] .dot,
  [data-state="system-stopped"] .dot {
    background: var(--warn);
  }
  [data-state="not-installed"] .dot {
    background: var(--danger);
  }

  .muted,
  .progress {
    margin: 0;
    font-size: 13px;
    color: var(--text-muted);
  }

  .progress {
    color: var(--text);
  }

  .small {
    font-family: var(--font-micro);
    font-size: 11px;
  }

  .err {
    margin: 0;
    font-size: 13px;
    padding-left: var(--space-2);
    border-left: 3px solid var(--danger);
    overflow-wrap: anywhere;
  }
</style>
