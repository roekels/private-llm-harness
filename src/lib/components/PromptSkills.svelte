<script lang="ts">
  import { onMount } from "svelte";
  import { api, errText } from "../api";
  import { app } from "../state.svelte";
  import type { PromptView, Skill } from "../types";

  const NAME = /^[a-z0-9](?:[a-z0-9-]{0,30}[a-z0-9])$/;
  const MAX_CHARS = 4000;

  let prompt = $state<PromptView | null>(null);
  let showSession = $state(false);
  let builtin = $state<Skill[]>([]);
  let custom = $state<Skill[]>([]);
  let draft = $state<Skill>({ name: "", body: "" });
  let editing = $state<string | null>(null);
  let editBody = $state("");
  let note = $state<{ kind: "ok" | "err"; text: string } | null>(null);
  let busy = $state(false);

  const nameProblem = $derived.by(() => {
    const n = draft.name.trim();
    if (!n) return "";
    if (!NAME.test(n)) return "Use 2 to 32 lowercase letters, digits and dashes.";
    if (builtin.some((b) => b.name === n)) return "That's a built-in skill.";
    if (custom.some((c) => c.name === n)) return "You already have a skill with this name.";
    return "";
  });
  const canAdd = $derived(!!draft.name.trim() && !!draft.body.trim() && !nameProblem && draft.body.length <= MAX_CHARS && !busy);

  async function load(): Promise<void> {
    try {
      const [p, s] = await Promise.all([api.promptView(app.activeSessionId), api.skills()]);
      prompt = p;
      builtin = s.builtin;
      custom = s.custom;
    } catch (e) {
      note = { kind: "err", text: errText(e) };
    }
  }

  async function save(next: Skill[], done: string): Promise<boolean> {
    busy = true;
    note = null;
    try {
      custom = await api.saveSkills(next);
      prompt = await api.promptView(app.activeSessionId);
      note = { kind: "ok", text: done };
      return true;
    } catch (e) {
      note = { kind: "err", text: errText(e) };
      return false;
    } finally {
      busy = false;
    }
  }

  async function add(): Promise<void> {
    const s = { name: draft.name.trim(), body: draft.body.trim() };
    if (await save([...custom, s], `Saved "${s.name}". New sessions list it; the model reads it when it calls skill.`)) {
      draft = { name: "", body: "" };
    }
  }

  async function update(name: string): Promise<void> {
    const next = custom.map((c) => (c.name === name ? { name, body: editBody.trim() } : c));
    if (await save(next, `Updated "${name}". The next time the model calls it, it gets the new text.`)) editing = null;
  }

  async function remove(name: string): Promise<void> {
    await save(
      custom.filter((c) => c.name !== name),
      `Deleted "${name}". Sessions that already list it get "No skill" if they call it.`,
    );
  }

  onMount(() => void load());
</script>

<section class="form" aria-labelledby="prompt-h">
  <h2 id="prompt-h">System prompt and skills</h2>

  <fieldset>
    <legend>System prompt (read-only)</legend>
    <p class="hint">
      Every session starts with this text, fixed when the session is created, so llama.cpp can reuse its cache. The time
      goes on each of your messages instead. After compaction, the session memory is added at the end.
    </p>
    {#if prompt}
      {#if prompt.session && prompt.session !== prompt.newSession}
        <div class="seg" role="radiogroup" aria-label="Which prompt">
          <label><input type="radio" name="which" checked={!showSession} onchange={() => (showSession = false)} /> New sessions</label>
          <label><input type="radio" name="which" checked={showSession} onchange={() => (showSession = true)} /> This session</label>
        </div>
      {/if}
      <pre class="prompt">{showSession && prompt.session ? prompt.session : prompt.newSession}</pre>
    {:else}
      <p class="hint">Loading…</p>
    {/if}
  </fieldset>

  <fieldset>
    <legend>Skills</legend>
    <p class="hint">
      Short notes the model fetches with its <code>skill</code> tool, only when it needs them. The system prompt lists your
      own skills by name, so a new skill reaches new sessions. Edits to a skill's text apply right away.
    </p>

    <ul class="skills">
      {#each builtin as s (s.name)}
        <li>
          <details>
            <summary><strong>{s.name}</strong> <span class="tag">Built-in</span></summary>
            <pre>{s.body}</pre>
          </details>
        </li>
      {/each}
      {#each custom as s (s.name)}
        <li>
          <details open={editing === s.name}>
            <summary><strong>{s.name}</strong> <span class="tag mine">Yours</span></summary>
            {#if editing === s.name}
              <label class="visually-hidden" for="edit-{s.name}">Text of {s.name}</label>
              <textarea id="edit-{s.name}" bind:value={editBody} rows="8" maxlength={MAX_CHARS} spellcheck="false"></textarea>
              <div class="row">
                <button class="btn-primary" disabled={busy || !editBody.trim()} onclick={() => void update(s.name)}>Save</button>
                <button class="btn-secondary" onclick={() => (editing = null)}>Cancel</button>
                <span class="hint">{editBody.length} / {MAX_CHARS}</span>
              </div>
            {:else}
              <pre>{s.body}</pre>
              <div class="row">
                <button class="btn-secondary" onclick={() => ((editing = s.name), (editBody = s.body))}>Edit</button>
                <button class="btn-secondary danger" disabled={busy} onclick={() => void remove(s.name)}>Delete</button>
              </div>
            {/if}
          </details>
        </li>
      {/each}
    </ul>

    <form
      class="add"
      onsubmit={(e) => {
        e.preventDefault();
        void add();
      }}
    >
      <h3>Add a skill</h3>
      <label>
        Name
        <input bind:value={draft.name} placeholder="git-commits" maxlength="32" spellcheck="false" autocomplete="off" />
        {#if nameProblem}<small class="bad">{nameProblem}</small>{:else}<small>Lowercase, digits and dashes. The model calls it by this name.</small>{/if}
      </label>
      <label>
        Notes for the model
        <textarea
          bind:value={draft.body}
          rows="7"
          maxlength={MAX_CHARS}
          placeholder={"For example:\n- Write commit messages in the imperative, under 72 characters.\n- Commit small, related changes together.\n- Run the tests before committing."}
          spellcheck="false"
        ></textarea>
        <small>{draft.body.length} / {MAX_CHARS}. Keep it short: the whole text is sent each time the model uses it.</small>
      </label>
      <div class="row">
        <button type="submit" class="btn-primary" disabled={!canAdd}>Save skill</button>
      </div>
    </form>

    {#if note}<p class="note" data-kind={note.kind} role={note.kind === "err" ? "alert" : "status"}>{note.text}</p>{/if}
  </fieldset>
</section>

<style>
  .form {
    grid-column: 1 / -1;
  }

  h2 {
    font-size: 14px;
    margin-bottom: var(--space-3);
  }

  h3 {
    font-size: 13px;
    margin: var(--space-3) 0 var(--space-2);
  }

  fieldset {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: var(--space-3) var(--space-4) var(--space-4);
    margin: 0 0 var(--space-4);
    background: var(--surface);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  legend {
    font-weight: 700;
    font-size: 13px;
    padding: 0 var(--space-1);
  }

  .hint,
  small {
    font-family: var(--font-micro);
    font-size: 12px;
    color: var(--text-muted);
    margin: 0;
    font-weight: 400;
  }

  .bad {
    color: var(--danger);
  }

  pre {
    margin: var(--space-2) 0 0;
    padding: var(--space-2) var(--space-3);
    background: var(--mist-25);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-size: 12px;
    max-height: 320px;
    overflow-y: auto;
  }

  .prompt {
    margin: 0;
  }

  .seg {
    display: flex;
    gap: var(--space-3);
    font-size: 13px;
  }

  .seg label {
    display: flex;
    gap: var(--space-1);
    align-items: center;
  }

  .skills {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .skills li {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: var(--space-2) var(--space-3);
  }

  summary {
    cursor: pointer;
    font-size: 13px;
  }

  .tag {
    margin-left: var(--space-2);
    padding: 0 6px;
    border-radius: 3px;
    background: var(--mist-60);
    font-family: var(--font-micro);
    font-size: 11px;
    color: var(--text-muted);
  }

  .tag.mine {
    background: var(--blue-25);
    color: var(--text);
  }

  .add label {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-weight: 700;
    font-size: 13px;
    margin-bottom: var(--space-3);
  }

  input,
  textarea {
    font-weight: 400;
    padding: var(--space-2);
    border: 1px solid var(--blue-25);
    border-radius: var(--radius);
    background: var(--surface);
  }

  input {
    max-width: 320px;
  }

  textarea {
    width: 100%;
    resize: vertical;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.45;
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .danger {
    color: var(--danger);
    border-color: var(--danger);
  }

  .note {
    margin: 0;
    font-size: 12px;
    padding-left: var(--space-2);
    border-left: 3px solid var(--ok);
  }

  .note[data-kind="err"] {
    border-left-color: var(--danger);
  }
</style>
