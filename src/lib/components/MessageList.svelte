<script lang="ts">
  import { tick } from "svelte";
  import { continueTurn } from "../chat.svelte";
  import { activeProfile, app } from "../state.svelte";
  import MessageView from "./MessageView.svelte";

  const profile = $derived(activeProfile());
  const problem = $derived(
    !["ready", "unknown", "checking"].includes(app.status) && app.statusMessage ? app.statusMessage : "",
  );
  // Tool results are shown inside the card of the call they answer.
  const results = $derived(
    new Map(app.messages.filter((m) => m.role === "tool" && m.toolCallId).map((m) => [m.toolCallId as string, m])),
  );
  // Index of the first message still sent to the model.
  const windowIndex = $derived(app.trimSeq > 0 ? app.messages.findIndex((m) => m.seq >= app.trimSeq) : -1);

  let list: HTMLElement | undefined = $state();
  let pinned = true; // follow new output while the user is at the bottom

  function onScroll(): void {
    if (!list) return;
    pinned = list.scrollHeight - list.scrollTop - list.clientHeight < 80;
  }

  $effect(() => {
    // Track the things that grow the list.
    void app.messages.length;
    void app.streaming?.content;
    void app.streaming?.reasoning;
    void app.chatError;
    void Object.keys(app.pendingTools).length;
    if (pinned) void tick().then(() => list?.scrollTo({ top: list.scrollHeight }));
  });

  $effect(() => {
    void app.activeSessionId;
    pinned = true;
  });
</script>

<section class="list" bind:this={list} onscroll={onScroll} aria-live="polite">
  {#each app.messages as m, i (m.id)}
    {#if i === windowIndex && i > 0}
      <div class="divider" role="note">
        {app.compactions
          ? "Compacted here: older messages are summarized in the session memory and no longer sent to the model."
          : "Older messages above are no longer sent to the model, to keep within its context."}
        {#if app.compactions}<button class="link" onclick={() => { app.memoryOpen = true; app.browserOpen = false; }}>Show memory</button>{/if}
      </div>
    {/if}
    {#if m.role !== "tool"}
      <MessageView
        role={m.role}
        content={m.content}
        images={m.images}
        reasoning={m.reasoning}
        meta={m.meta}
        toolCalls={m.toolCalls}
        {results}
        pending={app.pendingTools}
        faded={windowIndex > 0 && i < windowIndex}
      />
    {/if}
  {:else}
    {#if !app.streaming}
      <div class="empty">
        <h2>Start a conversation</h2>
        {#if profile}
          <p>
            Sessions stay on this Mac. The model runs on <strong>{profile.name}</strong> at
            <code>{profile.baseUrl}</code>.
          </p>
        {:else}
          <p>Add a profile in <button class="link" onclick={() => (app.view = "settings")}>Settings</button> to start.</p>
        {/if}
        {#if problem}
          <p class="problem" role="alert">
            {problem}
            <button class="link" onclick={() => (app.view = "settings")}>Open Settings</button>
          </p>
        {/if}
      </div>
    {/if}
  {/each}

  {#if app.compacting}
    <p class="compacting" role="status">Compacting the context: the model is writing the session memory…</p>
  {/if}

  {#if app.streaming}
    <MessageView role="assistant" content={app.streaming.content} reasoning={app.streaming.reasoning} streaming />
  {/if}

  {#if app.paused != null && !app.generating}
    <div class="paused" role="status">
      <span>Paused after {app.paused} tool {app.paused === 1 ? "round" : "rounds"}. The model can carry on where it stopped.</span>
      <button class="btn-primary small" onclick={() => void continueTurn()}>Continue</button>
      <button class="btn-secondary small" onclick={() => (app.paused = null)}>Stop here</button>
    </div>
  {/if}

  {#if app.chatError}
    <p class="error" role="alert">{app.chatError}</p>
  {/if}
</section>

<style>
  .list {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-5);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .divider {
    max-width: 820px;
    width: 100%;
    margin: var(--space-2) auto;
    padding-top: var(--space-2);
    border-top: 1px dashed var(--blue-25);
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--text-muted);
    text-align: center;
  }

  .paused {
    max-width: 820px;
    width: 100%;
    margin: 0 auto;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-left: 4px solid var(--warn);
    border-radius: var(--radius);
    background: var(--surface);
    font-size: 13px;
  }

  .paused span {
    flex: 1;
  }

  .small {
    padding: 2px var(--space-2);
    font-size: 12px;
  }

  .compacting {
    max-width: 820px;
    width: 100%;
    margin: 0 auto;
    padding: var(--space-2) var(--space-3);
    border: 1px dashed var(--warn);
    border-radius: var(--radius);
    font-size: 13px;
    color: var(--text-muted);
  }

  .empty {
    margin: auto;
    max-width: 480px;
    text-align: center;
  }

  .empty h2 {
    font-size: 20px;
    margin-bottom: var(--space-2);
  }

  .empty p {
    color: var(--text-muted);
    margin: 0;
  }

  .problem,
  .error {
    margin-top: var(--space-4);
    padding: var(--space-3) var(--space-4);
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 4px solid var(--danger);
    border-radius: var(--radius);
    color: var(--text);
    text-align: left;
  }

  .empty .problem {
    color: var(--text);
  }

  .error {
    max-width: 820px;
    width: 100%;
    margin: 0 auto;
  }

  .link {
    padding: 0;
    color: var(--link);
    text-decoration: underline;
  }
</style>
