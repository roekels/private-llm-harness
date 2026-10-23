<script lang="ts">
  import { send, stop } from "../chat.svelte";
  import { MAX_IMAGES, toDataUri } from "../images";
  import { app } from "../state.svelte";

  let draft = $state("");
  let images = $state<string[]>([]);
  let note = $state("");
  let fileInput: HTMLInputElement | undefined = $state();

  const ready = $derived(app.status === "ready");
  const canSend = $derived(ready && !app.generating && (draft.trim().length > 0 || images.length > 0));

  async function submit(): Promise<void> {
    if (!canSend) return;
    const text = draft;
    const imgs = images;
    draft = "";
    images = [];
    const ok = await send(text, imgs);
    if (!ok && !draft && images.length === 0) {
      // Not sent: give the draft back.
      draft = text;
      images = imgs;
    }
  }

  async function addFiles(files: Iterable<File>): Promise<void> {
    note = "";
    for (const f of files) {
      if (!f.type.startsWith("image/")) continue;
      if (images.length >= MAX_IMAGES) {
        note = `At most ${MAX_IMAGES} images per message.`;
        break;
      }
      try {
        images.push(await toDataUri(f));
      } catch (e) {
        note = e instanceof Error ? e.message : String(e);
      }
    }
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void submit();
    } else if (e.key === "Escape" && app.generating) {
      stop();
    }
  }

  function onPaste(e: ClipboardEvent): void {
    const files = [...(e.clipboardData?.files ?? [])].filter((f) => f.type.startsWith("image/"));
    if (files.length) {
      e.preventDefault();
      void addFiles(files);
    }
  }

  function onDrop(e: DragEvent): void {
    e.preventDefault();
    if (e.dataTransfer?.files.length) void addFiles(e.dataTransfer.files);
  }
</script>

<form
  class="composer"
  onsubmit={(e) => {
    e.preventDefault();
    void submit();
  }}
  ondragover={(e) => e.preventDefault()}
  ondrop={onDrop}
>
  {#if images.length}
    <div class="thumbs">
      {#each images as src, i (i)}
        <div class="thumb">
          <img {src} alt="Attachment {i + 1}" />
          <button type="button" aria-label="Remove attachment {i + 1}" onclick={() => images.splice(i, 1)}>Remove</button>
        </div>
      {/each}
    </div>
  {/if}

  <label class="visually-hidden" for="draft">Message</label>
  <textarea
    id="draft"
    rows="3"
    placeholder={ready ? "Message the model" : "Waiting for the model. Check the status at the top."}
    bind:value={draft}
    onkeydown={onKey}
    onpaste={onPaste}
  ></textarea>

  <input
    bind:this={fileInput}
    type="file"
    accept="image/png,image/jpeg,image/webp,image/gif"
    multiple
    hidden
    onchange={(e) => {
      const input = e.currentTarget;
      void addFiles(input.files ?? []).then(() => (input.value = ""));
    }}
  />

  <div class="actions">
    <button type="button" class="btn-secondary" disabled={app.generating || images.length >= MAX_IMAGES} onclick={() => fileInput?.click()}
      >Attach image</button
    >
    <span class="hint">{note || "Enter sends. Shift+Enter adds a line. Paste or drop images."}</span>
    {#if app.generating}
      <button type="button" class="btn-secondary stop" onclick={stop}>Stop</button>
    {:else}
      <button type="submit" class="btn-primary" disabled={!canSend}>Send</button>
    {/if}
  </div>
</form>

<style>
  .composer {
    max-width: 868px;
    width: 100%;
    margin: 0 auto;
    padding: 0 var(--space-5) var(--space-3);
  }

  textarea {
    width: 100%;
    resize: vertical;
    min-height: 72px;
    max-height: 40vh;
    padding: var(--space-3);
    background: var(--surface);
    border: 1px solid var(--blue-25);
    border-radius: var(--radius);
    line-height: var(--line);
  }

  .thumbs {
    display: flex;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }

  .thumb {
    position: relative;
  }

  .thumb img {
    height: 64px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    display: block;
  }

  .thumb button {
    position: absolute;
    inset: auto 2px 2px auto;
    padding: 0 4px;
    font-size: 11px;
    background: var(--navy);
    color: var(--white);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-top: var(--space-2);
  }

  .hint {
    flex: 1;
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--text-muted);
  }

  .stop {
    border-color: var(--danger);
    color: var(--danger);
  }
</style>
