<script lang="ts">
  import { app } from "../state.svelte";
  import { checkForUpdates, installUpdate, restartNow, setAutoCheck, update } from "../updates.svelte";

  const mb = (n: number) => `${(n / 1_048_576).toFixed(1)} MB`;
  const pct = $derived(update.total ? Math.min(100, Math.round((update.received / update.total) * 100)) : null);
  const lastCheck = $derived(
    update.status?.lastCheck
      ? new Date(update.status.lastCheck).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })
      : "never",
  );
  const released = $derived(
    update.available?.date ? new Date(update.available.date).toLocaleDateString(undefined, { dateStyle: "medium" }) : "",
  );
</script>

<article class="card updates" aria-labelledby="updates-title">
  <h2 id="updates-title">Updates</h2>

  {#if !update.status}
    <p class="muted">Loading…</p>
  {:else if !update.status.supported}
    <p>Version {update.status.current}. {update.status.reason}</p>
  {:else}
    {#if update.phase === "installed"}
      <p><strong>Version {update.available?.version} is installed.</strong> Restart to start using it.</p>
      <div class="row">
        <button class="btn-primary" disabled={app.generating} onclick={() => void restartNow()}>Restart now</button>
        {#if app.generating}<span class="muted">Waiting for the reply to finish.</span>{/if}
      </div>
    {:else if update.available}
      <p>
        <strong>Version {update.available.version} is available.</strong>
        You have {update.status.current}.{released ? ` Released ${released}.` : ""}
      </p>
      {#if update.available.notes}
        <pre class="notes">{update.available.notes}</pre>
      {/if}
      {#if update.phase === "downloading"}
        <div class="bar" role="progressbar" aria-label="Download" aria-valuemin="0" aria-valuemax="100" aria-valuenow={pct ?? undefined}>
          <div class="fill" style:width="{pct ?? 5}%"></div>
        </div>
        <p class="muted">Downloading {mb(update.received)}{update.total ? ` of ${mb(update.total)}` : ""}…</p>
      {:else if update.phase === "installing"}
        <p class="muted">Signature checked. Installing… macOS may ask for your password.</p>
      {:else}
        <div class="row">
          <button class="btn-primary" onclick={() => void installUpdate()}>Install update</button>
          <span class="muted">Downloads, checks the signature, then replaces the app. Your sessions and settings stay.</span>
        </div>
      {/if}
    {:else}
      <p>
        Version {update.status.current}.
        {#if update.checking}Checking GitHub…{:else if update.upToDate}You're up to date.{/if}
      </p>
      <div class="row">
        <button class="btn-secondary" disabled={update.checking} onclick={() => void checkForUpdates()}>Check for updates</button>
        <span class="muted">Last checked: {lastCheck}</span>
      </div>
    {/if}

    <label class="auto">
      <input type="checkbox" checked={update.status.autoCheck} onchange={(e) => void setAutoCheck(e.currentTarget.checked)} />
      Check for updates automatically (at start and once a day). Nothing installs without your click.
    </label>
  {/if}

  {#if update.error}<p class="err" role="alert">{update.error}</p>{/if}
</article>

<style>
  .card {
    grid-column: 1 / -1;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: var(--space-4);
    min-width: 0;
  }

  h2 {
    font-size: 15px;
    margin-bottom: var(--space-2);
  }

  p {
    margin: 0 0 var(--space-2);
    font-size: 13px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
    margin-bottom: var(--space-2);
  }

  .muted {
    color: var(--text-muted);
    font-size: 12px;
  }

  .notes {
    max-height: 160px;
    overflow: auto;
    margin: 0 0 var(--space-3);
    padding: var(--space-2) var(--space-3);
    background: var(--surface-muted);
    border-radius: var(--radius);
    white-space: pre-wrap;
    font-family: var(--font-text);
    font-size: 13px;
  }

  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-muted);
    overflow: hidden;
    margin-bottom: var(--space-2);
  }

  .fill {
    height: 100%;
    background: var(--primary);
    transition: width 0.2s;
  }

  .auto {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    font-size: 12px;
    color: var(--text-muted);
  }

  .err {
    font-size: 12px;
    padding-left: var(--space-2);
    border-left: 3px solid var(--danger);
    overflow-wrap: anywhere;
  }
</style>
