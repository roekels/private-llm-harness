<script lang="ts">
  import { app } from "../state.svelte";
  import { update } from "../updates.svelte";

  const show = $derived(!!update.available && update.dismissed !== update.available.version && update.phase === "idle");
</script>

{#if show && update.available}
  <div class="banner" role="status">
    <span>Version {update.available.version} of Ruan's Harness is available.</span>
    <button class="btn-secondary small" onclick={() => (app.view = "about")}>See what's new</button>
    <button class="close small" aria-label="Hide until the next version" onclick={() => (update.dismissed = update.available?.version ?? "")}>Not now</button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-5);
    background: var(--surface-muted);
    border-bottom: 1px solid var(--border);
    font-size: 13px;
  }

  .banner span {
    flex: 1;
  }

  .small {
    padding: 2px var(--space-2);
    font-size: 12px;
  }

  .close {
    color: var(--text-muted);
  }
</style>
