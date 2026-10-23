<script lang="ts">
  import { onMount } from "svelte";
  import { api, errText } from "../api";
  import { app } from "../state.svelte";
  import { refreshVm } from "../vm.svelte";
  import type { SelfTest, SetupAction, SetupReport, SetupStep } from "../types";

  let report = $state<SetupReport | null>(null);
  let selftest = $state<SelfTest | null>(null);
  let busy = $state<string | null>(null);
  let log = $state<string[]>([]);
  let error = $state("");
  let confirm = $state<SetupAction | null>(null);
  let logBox: HTMLElement | undefined = $state();

  const LABEL: Record<SetupStep["state"], string> = {
    ok: "Ready",
    missing: "Missing",
    outdated: "Update available",
    stopped: "Stopped",
    unsupported: "Not supported",
    warning: "Check",
    skipped: "Not used",
  };
  const BUTTON: Record<SetupAction, string> = {
    "install-container": "Install",
    "upgrade-container": "Upgrade",
    "start-system": "Start",
    "build-image": "Build",
    "start-vm": "Start",
    selftest: "Run check",
  };

  // The model connection comes from the app's own status check.
  const modelStep = $derived<SetupStep>({
    id: "model",
    title: "Model and key",
    state: app.status === "ready" ? "ok" : app.status === "checking" || app.status === "unknown" ? "warning" : "missing",
    detail: app.status === "ready" ? `Connected. ${app.statusMessage}` : app.statusMessage || "Not connected yet.",
  });
  const steps = $derived(report ? [...report.steps, modelStep] : []);
  const pending = $derived(steps.filter((s) => s.action && s.action !== "selftest"));
  const allGood = $derived(!!report && !report.needsAttention && pending.length === 0 && modelStep.state === "ok");

  function say(line: string): void {
    log.push(line);
    if (log.length > 300) log.splice(0, log.length - 300);
    queueMicrotask(() => logBox?.scrollTo({ top: logBox.scrollHeight }));
  }

  async function refresh(): Promise<void> {
    try {
      report = await api.setupReport();
      app.setupNeeded = report.needsAttention;
    } catch (e) {
      error = errText(e);
    }
  }

  async function run(action: SetupAction): Promise<boolean> {
    if ((action === "install-container" || action === "upgrade-container") && confirm !== action) {
      confirm = action; // ask first
      return false;
    }
    confirm = null;
    busy = BUTTON[action];
    error = "";
    try {
      switch (action) {
        case "install-container":
        case "upgrade-container":
          say(await api.setupInstallContainer(say));
          break;
        case "start-system":
        case "start-vm":
          say(await api.setupVm(false, say));
          break;
        case "build-image":
          say(await api.setupVm(true, say));
          break;
        case "selftest":
          selftest = await api.setupSelftest();
          say(selftest.passed ? "Isolation check: all passed." : "Isolation check: some checks failed. See the list.");
          break;
      }
      return true;
    } catch (e) {
      error = errText(e);
      say(`Stopped: ${error}`);
      return false;
    } finally {
      busy = null;
      await refresh();
      void refreshVm();
    }
  }

  /** Do every open step in order, then the isolation check. */
  async function fixAll(): Promise<void> {
    for (let i = 0; i < 6; i++) {
      const next = report?.steps.find((s) => s.action && s.action !== "selftest");
      if (!next?.action) break;
      if (!(await run(next.action))) return; // stopped, failed, or waiting for a yes
    }
    const test = report?.steps.find((s) => s.action === "selftest");
    if (test) await run("selftest");
  }

  onMount(() => {
    void refresh();
    api.setupLastSelftest().then((t) => (selftest = t)).catch(() => {});
    void api.setupSeen();
  });
</script>

<section class="setup">
  <header class="head">
    <h1>Setup</h1>
    <button class="btn-secondary" disabled={!!busy} onclick={() => (app.view = "chat")}>Back to chat</button>
  </header>

  <div class="body">
    <div class="intro">
      {#if allGood}
        <h2>You're all set</h2>
        <p>Everything Ruan's Harness needs on this Mac is installed, current and checked.</p>
      {:else}
        <h2>Let's get this Mac ready</h2>
        <p>
          The app checks each part it needs and installs or updates what's missing. Nothing is installed without your OK, and
          macOS asks for your password itself.
        </p>
      {/if}
      <div class="row">
        <button class="btn-primary" disabled={!!busy || !report || pending.length === 0} onclick={fixAll}>
          {pending.length ? (report?.needsAttention ? "Set up everything" : `Fix ${pending.length} ${pending.length === 1 ? "item" : "items"}`) : "Nothing to fix"}
        </button>
        <button class="btn-secondary" disabled={!!busy} onclick={refresh}>Check again</button>
        {#if busy}<span class="muted">{busy}…</span>{/if}
      </div>
    </div>

    <ol class="steps">
      {#each steps as s, i (s.id)}
        <li data-state={s.state}>
          <span class="num" aria-hidden="true">{i + 1}</span>
          <div class="main">
            <div class="row">
              <strong>{s.title}</strong>
              <span class="badge"><span class="dot" aria-hidden="true"></span>{LABEL[s.state]}</span>
              <span class="spacer"></span>
              {#if s.action}
                <button class={s.action === "selftest" ? "btn-secondary" : "btn-primary"} disabled={!!busy} onclick={() => s.action && run(s.action)}>
                  {BUTTON[s.action]}
                </button>
              {:else if s.id === "model" && s.state !== "ok"}
                <button class="btn-secondary" onclick={() => (app.view = "settings")}>Open Settings</button>
              {/if}
            </div>
            <p>{s.detail}</p>

            {#if confirm && (s.action === confirm)}
              <div class="confirm" role="dialog" aria-label="Confirm install">
                <p>
                  Download <strong>{report?.latestContainer?.name ?? "Apple container"}</strong>
                  ({Math.round((report?.latestContainer?.size ?? 0) / 1_000_000)} MB) from Apple's release on github.com/apple/container
                  and install it?
                </p>
                <ul>
                  <li>Before installing, the app checks the checksum, that Apple signed it (Containerization team) and that it's notarized.</li>
                  <li>macOS asks for your password. The installer puts the tool in /usr/local.</li>
                  {#if confirm === "upgrade-container"}<li>The sandbox VM stops during the upgrade and starts again on the next tool call.</li>{/if}
                </ul>
                <div class="row">
                  <button class="btn-primary" onclick={() => (confirm === "upgrade-container" ? run("upgrade-container") : run("install-container"))}>
                    {confirm === "upgrade-container" ? "Download and upgrade" : "Download and install"}
                  </button>
                  <button class="btn-secondary" onclick={() => (confirm = null)}>Not now</button>
                </div>
              </div>
            {/if}

            {#if s.id === "selftest" && selftest}
              <details open={!selftest.passed}>
                <summary>{selftest.checks.filter((c) => c.passed).length} of {selftest.checks.length} passed</summary>
                <ul class="checks">
                  {#each selftest.checks as c (c.name)}
                    <li data-pass={c.passed}><span aria-hidden="true">{c.passed ? "✓" : "✗"}</span> {c.name} <span class="muted">· {c.detail}</span></li>
                  {/each}
                </ul>
              </details>
            {/if}
          </div>
        </li>
      {/each}
    </ol>

    {#if log.length}
      <div class="logwrap">
        <div class="logtitle">{busy ? `${busy}…` : "Last output"}</div>
        <pre class="log" bind:this={logBox} aria-live="polite">{log.join("\n")}</pre>
      </div>
    {/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <p class="muted small">App version {report?.appVersion ?? "…"}. This screen opens by itself after an update when something needs attention. You can open it any time from the sidebar.</p>
  </div>
</section>

<style>
  .setup {
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
    max-width: 820px;
    width: 100%;
    margin: 0 auto;
    padding: var(--space-5);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .intro h2 {
    font-size: 20px;
    margin-bottom: var(--space-2);
  }

  .intro p {
    margin: 0 0 var(--space-3);
    color: var(--text-muted);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .spacer {
    flex: 1;
  }

  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .steps > li {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 4px solid var(--blue-25);
    border-radius: var(--radius);
  }

  .steps > li[data-state="ok"] {
    border-left-color: var(--ok);
  }
  .steps > li[data-state="missing"],
  .steps > li[data-state="unsupported"] {
    border-left-color: var(--danger);
  }
  .steps > li[data-state="outdated"],
  .steps > li[data-state="stopped"],
  .steps > li[data-state="warning"] {
    border-left-color: var(--warn);
  }

  .num {
    flex: none;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--mist);
    display: grid;
    place-items: center;
    font-size: 12px;
    font-weight: 700;
  }

  .main {
    flex: 1;
    min-width: 0;
  }

  .main p {
    margin: var(--space-1) 0 0;
    font-size: 13px;
    color: var(--text-muted);
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--text-muted);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-muted);
  }

  [data-state="ok"] .dot {
    background: var(--ok);
  }
  [data-state="missing"] .dot,
  [data-state="unsupported"] .dot {
    background: var(--danger);
  }
  [data-state="outdated"] .dot,
  [data-state="stopped"] .dot,
  [data-state="warning"] .dot {
    background: var(--warn);
  }

  .confirm {
    margin-top: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--primary);
    border-radius: var(--radius);
    background: var(--mist-25);
    font-size: 13px;
  }

  .confirm p {
    color: var(--text);
    margin: 0 0 var(--space-2);
  }

  .confirm ul {
    margin: 0 0 var(--space-3);
    padding-left: var(--space-5);
    color: var(--text-muted);
  }

  details {
    margin-top: var(--space-2);
    font-size: 13px;
  }

  summary {
    cursor: pointer;
    color: var(--text-muted);
  }

  .checks {
    list-style: none;
    margin: var(--space-2) 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .checks li[data-pass="true"] span:first-child {
    color: var(--ok);
    font-weight: 700;
  }

  .checks li[data-pass="false"] {
    color: var(--danger);
  }

  .muted {
    color: var(--text-muted);
    font-size: 13px;
  }

  .small {
    font-family: var(--font-micro);
    font-size: 12px;
    margin: 0;
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
    padding-left: var(--space-2);
    border-left: 3px solid var(--danger);
    font-size: 13px;
  }
</style>
