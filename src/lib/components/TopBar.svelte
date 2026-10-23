<script lang="ts">
  import { api, errText } from "../api";
  import { checkNow } from "../connection.svelte";
  import { firewallText, fmtInt } from "../format";
  import { activeProfile, app } from "../state.svelte";
  import type { Status } from "../types";

  const LABELS: Record<Status, string> = {
    unknown: "Not connected",
    checking: "Checking",
    "dns-failed": "Host not found",
    unreachable: "Unreachable",
    starting: "Server starting",
    loading: "Model loading",
    ready: "Ready",
    "auth-error": "Key rejected",
    "key-missing": "Key missing",
    blocked: "Blocked",
    error: "Error",
  };

  const title = $derived(app.sessions.find((s) => s.id === app.activeSessionId)?.title ?? "No session");
  const profile = $derived(activeProfile());
  const tone = $derived(
    app.status === "ready"
      ? app.modelFound
        ? "ok"
        : "warn"
      : ["loading", "starting", "checking"].includes(app.status)
        ? "warn"
        : app.status === "unknown"
          ? "idle"
          : "bad",
  );
  const tip = $derived(
    [app.statusMessage, app.latencyMs != null ? `${app.latencyMs} ms` : ""].filter(Boolean).join(" "),
  );

  // VM light: green up, orange idle or not set up, red missing.
  const vm = $derived.by((): { label: string; tone: string; tip: string } => {
    const s = app.vm;
    if (!s) return { label: "VM ?", tone: "idle", tip: "Checking the sandbox VM" };
    switch (s.state) {
      case "running":
        return { label: "VM up", tone: "ok", tip: `Running. Firewall: ${firewallText(s.network)}` };
      case "stopped":
        return { label: "VM idle", tone: "warn", tip: "Stopped. It starts on the next tool call." };
      case "no-image":
      case "system-stopped":
        return { label: "VM not set up", tone: "warn", tip: s.message };
      case "not-installed":
        return { label: "No VM", tone: "bad", tip: s.message };
    }
  });

  const chip = $derived(
    `${app.tools.autoApprove ? "Auto" : "Ask"} · ${app.tools.backend === "vm" ? "VM" : "Mac"}${app.tools.network ? " · Net" : ""}`,
  );

  // Turning tools on or off changes the prompt start, so warn when it costs a re-read.
  let confirming = $state(false);
  let toggleError = $state("");
  const contextTokens = $derived.by(() => {
    const m = [...app.messages].reverse().find((x) => x.role === "assistant" && x.meta?.promptTokens != null)?.meta;
    return m ? (m.promptTokens ?? 0) + (m.completionTokens ?? 0) : 0;
  });

  function onToggle(): void {
    if (app.messages.length > 0) confirming = !confirming;
    else void applyToggle();
  }

  async function applyToggle(): Promise<void> {
    confirming = false;
    toggleError = "";
    const before = app.tools.enabled;
    app.tools.enabled = !before;
    try {
      await api.setToolSettings($state.snapshot(app.tools));
    } catch (e) {
      app.tools.enabled = before;
      toggleError = errText(e);
    }
  }
</script>

<div class="bar">
  <h1>{title}</h1>

  <div class="right">
    {#if profile && !app.browserOpen}
      <span class="model" title={profile.modelInfo || (app.models.length ? `Server models: ${app.models.join(", ")}` : undefined)}>{profile.model}</span>
    {/if}
    <button class="pill" data-tone={tone} title={tip || undefined} onclick={() => void checkNow()}>
      <span class="dot" aria-hidden="true"></span>{LABELS[app.status]}
      <span class="visually-hidden">. {tip} Select to check again.</span>
    </button>

    {#if app.tools.backend === "vm"}
      <button class="pill" data-tone={vm.tone} title={vm.tip} onclick={() => (app.view = "settings")}>
        <span class="dot" aria-hidden="true"></span>{vm.label}
      </button>
    {/if}

    <div class="tools">
      <button
        class="switch"
        role="switch"
        aria-checked={app.tools.enabled}
        aria-label="Tools"
        title={app.tools.enabled ? "Tools on. Select to turn off." : "Tools off. Select to turn on."}
        disabled={app.generating}
        onclick={onToggle}><span class="knob"></span></button
      >
      <button class="chip" title="Tool settings" onclick={() => (app.view = "settings")}>
        {app.tools.enabled ? chip : "Tools off"}
      </button>

      {#if confirming}
        <div class="pop" role="dialog" aria-label="Change tools">
          <p>
            Turning tools {app.tools.enabled ? "off" : "on"} changes the start of the prompt. The next reply re-reads
            this whole conversation{contextTokens ? ` (about ${fmtInt(contextTokens)} tokens)` : ""} instead of using the cache.
          </p>
          <div class="pop-actions">
            <button class="btn-primary" onclick={applyToggle}>Turn {app.tools.enabled ? "off" : "on"}</button>
            <button class="btn-secondary" onclick={() => (confirming = false)}>Cancel</button>
          </div>
        </div>
      {/if}
      {#if toggleError}<div class="pop err" role="alert">{toggleError}</div>{/if}
    </div>

    <label class="think">
      <input type="checkbox" bind:checked={app.think} />
      Think
    </label>
  </div>
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-5);
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }

  h1 {
    font-size: 16px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 80px;
    flex: 1;
  }

  .right {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    font-size: 13px;
    flex: none;
  }

  .model {
    font-family: var(--font-mono);
    color: var(--text-muted);
  }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-color: var(--border);
    font-size: 13px;
    white-space: nowrap;
  }

  .pill:hover,
  .chip:hover {
    background: var(--surface-muted);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-muted);
  }

  [data-tone="ok"] .dot {
    background: var(--ok);
  }
  [data-tone="warn"] .dot {
    background: var(--warn);
  }
  [data-tone="bad"] .dot {
    background: var(--danger);
  }

  .tools {
    position: relative;
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding-left: var(--space-2);
  }

  .switch {
    width: 30px;
    height: 18px;
    padding: 0;
    border-radius: 9px;
    background: var(--blue-25);
    position: relative;
  }

  .switch[aria-checked="true"] {
    background: var(--primary);
  }

  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--white);
    transition: left 0.12s;
  }

  .switch[aria-checked="true"] .knob {
    left: 14px;
  }

  .chip {
    padding: var(--space-1) var(--space-2);
    font-size: 13px;
    white-space: nowrap;
  }

  .pop {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 10;
    width: 300px;
    padding: var(--space-3);
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 4px solid var(--warn);
    border-radius: var(--radius);
    box-shadow: 0 4px 16px color-mix(in srgb, var(--navy) 18%, transparent);
  }

  .pop p {
    margin: 0 0 var(--space-3);
    font-size: 13px;
  }

  .pop-actions {
    display: flex;
    gap: var(--space-2);
  }

  .pop.err {
    border-left-color: var(--danger);
    font-size: 13px;
  }

  .think {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    cursor: pointer;
  }

  .think input {
    accent-color: var(--primary);
  }

  @media (prefers-reduced-motion: reduce) {
    .knob {
      transition: none;
    }
  }
</style>
