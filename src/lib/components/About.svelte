<script lang="ts">
  import { onMount } from "svelte";
  import { api, errText } from "../api";
  import { firewallText, fmtInt } from "../format";
  import { isKnownModel, MODEL_CARD } from "../modelcard";
  import { activeProfile, app } from "../state.svelte";
  import type { AboutInfo } from "../types";
  import logo from "../../assets/logo.png";
  import { CHANNEL } from "../release";

  let info = $state<AboutInfo | null>(null);
  let error = $state("");
  const profile = $derived(activeProfile());
  const known = $derived(isKnownModel(profile?.model, profile?.modelInfo));
  const lastMeta = $derived(
    [...app.messages].reverse().find((m) => m.role === "assistant" && m.meta?.tokensPerSecond)?.meta ?? null,
  );

  onMount(() => {
    api.aboutInfo().then((i) => (info = i)).catch((e) => (error = errText(e)));
  });

  const link = (name: string) => () => void api.openLink(name).catch((e) => (error = errText(e)));
</script>

<section class="about">
  <header class="head">
    <h1>About</h1>
    <button class="btn-secondary" onclick={() => (app.view = "chat")}>Back to chat</button>
  </header>

  <div class="body">
    <article class="card hero">
      <img src={logo} alt="" width="56" height="56" />
      <div>
        <h2>Ruan's Harness</h2>
        <p>Version {info?.version ?? "…"}{CHANNEL ? ` ${CHANNEL}` : ""} · Developed by Ruan · 2026 · <button class="link" onclick={link("github")}>@Roekels</button> · <button class="link" onclick={link("repo")}>Source code</button></p>
        <p class="muted">A sandboxed, token-lean agent harness for a self-hosted model. Tauri 2, Rust, Svelte 5 and TypeScript.</p>
      </div>
    </article>

    <article class="card wide">
      <h2>How it fits together</h2>
      <svg viewBox="0 0 760 250" role="img" aria-labelledby="map-title map-desc">
        <title id="map-title">Architecture map</title>
        <desc id="map-desc">The model server runs the model. This Mac runs the app and talks to the model over Tailscale. The app runs tools in a Linux VM through container exec. The VM can reach the internet only.</desc>
        <defs>
          <marker id="arr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
            <path d="M0,0 L10,5 L0,10 z" class="fill-line" />
          </marker>
        </defs>

        <g class="box">
          <rect x="10" y="20" width="220" height="130" rx="4" />
          <text x="24" y="46" class="t1">{(profile?.name ?? "Model server").slice(0, 24)}</text>
          <text x="24" y="70" class="t2">Your GPU machine or this Mac</text>
          <text x="24" y="90" class="t2">llama-server (llama.cpp)</text>
          <text x="24" y="110" class="t2">{(profile?.model ?? "").slice(0, 26)}</text>
          <text x="24" y="130" class="t3">OpenAI-compatible API</text>
        </g>
        <g class="box main">
          <rect x="270" y="20" width="220" height="130" rx="4" />
          <text x="284" y="46" class="t1">This Mac</text>
          <text x="284" y="70" class="t2">Ruan's Harness</text>
          <text x="284" y="90" class="t2">Keys, history, model traffic</text>
          <text x="284" y="110" class="t2">Approves every tool call</text>
          <text x="284" y="130" class="t3">The only bridge between the two</text>
        </g>
        <g class="box vm">
          <rect x="530" y="20" width="220" height="130" rx="4" />
          <text x="544" y="46" class="t1">Sandbox VM</text>
          <text x="544" y="70" class="t2">Debian Linux, user agent</text>
          <text x="544" y="90" class="t2">Terminal, files, browser</text>
          <text x="544" y="110" class="t2">Only /work is shared</text>
          <text x="544" y="130" class="t3">Apple container, own kernel</text>
        </g>

        <line x1="232" y1="85" x2="268" y2="85" class="line" marker-start="url(#arr)" marker-end="url(#arr)" />
        <text x="250" y="172" class="t3" text-anchor="middle">LAN or</text>
        <text x="250" y="186" class="t3" text-anchor="middle">Tailscale</text>
        <line x1="492" y1="85" x2="528" y2="85" class="line" marker-end="url(#arr)" />
        <text x="510" y="172" class="t3" text-anchor="middle">container exec</text>

        <line x1="640" y1="152" x2="640" y2="196" class="line" marker-end="url(#arr)" />
        <g class="box net">
          <rect x="560" y="198" width="160" height="40" rx="4" />
          <text x="640" y="223" class="t2" text-anchor="middle">Internet only</text>
        </g>
        <text x="10" y="215" class="t3 blocked">Blocked from the VM: your home network, the tailnet</text>
        <text x="10" y="231" class="t3 blocked">(your other devices) and this Mac.</text>
      </svg>
    </article>

    <article class="card">
      <h2>Model</h2>
      {#if known}
        <dl>
          <dt>Model</dt><dd>{MODEL_CARD.name}</dd>
          <dt>File</dt><dd><code>{MODEL_CARD.file}</code></dd>
          <dt>Base model</dt><dd><button class="link" onclick={link("base-model")}>{MODEL_CARD.base}</button></dd>
          <dt>Quantization</dt><dd>{MODEL_CARD.quant}</dd>
          <dt>Architecture</dt><dd>{MODEL_CARD.architecture}</dd>
          <dt>Context</dt><dd>{MODEL_CARD.context}</dd>
          <dt>Vision</dt><dd>{MODEL_CARD.vision}</dd>
          <dt>Thinking</dt><dd>{MODEL_CARD.thinking}</dd>
          <dt>Alignment</dt><dd>{MODEL_CARD.alignment}</dd>
          <dt>Known weakness</dt><dd>{MODEL_CARD.weakness}</dd>
          <dt>Hugging Face</dt><dd><button class="link" onclick={link("model")}>HauhauCS/Qwen3.6-35B-A3B-Uncensored-HauhauCS-Aggressive</button></dd>
        </dl>
      {:else}
        <p class="muted">No model card for this profile. Live details are below.</p>
      {/if}
      <h3>Live</h3>
      <dl>
        <dt>Profile</dt><dd>{profile?.name ?? "None"} · <code>{profile?.baseUrl ?? ""}</code></dd>
        <dt>Server reports</dt><dd>{app.models.length ? app.models.join(", ") : "Not connected"}</dd>
        <dt>Status</dt><dd>{app.statusMessage || app.status}{app.latencyMs != null ? ` · ${app.latencyMs} ms` : ""}</dd>
        {#if lastMeta}<dt>Last reply</dt><dd>{lastMeta.tokensPerSecond?.toFixed(1)} tok/s · {fmtInt(lastMeta.promptTokens ?? 0)} prompt, {fmtInt(lastMeta.cachedTokens ?? 0)} cached</dd>{/if}
      </dl>
    </article>

    <article class="card">
      <h2>Model server</h2>
      <dl>
        <dt>Profile</dt><dd>{profile?.name ?? "None"}</dd>
        <dt>Address</dt><dd><code>{profile?.baseUrl ?? ""}</code>{profile?.fallbackUrl ? " (with a fallback)" : ""}</dd>
        <dt>Model id</dt><dd><code>{profile?.model ?? ""}</code>{profile?.modelInfo ? ` · ${profile.modelInfo}` : ""}</dd>
        <dt>Context</dt><dd>{fmtInt(profile?.contextTokens ?? 0)} tokens</dd>
        <dt>Key</dt><dd>{profile?.keyRef ? (profile.keyRef.startsWith("keychain:") ? "Keychain" : "Environment") : "None"}</dd>
        <dt>Server</dt><dd><button class="link" onclick={link("llama-cpp")}>llama.cpp llama-server</button> or any OpenAI-compatible server</dd>
      </dl>
      <h3>This Mac</h3>
      <dl>
        <dt>Chip</dt><dd>{info?.host.chip || "…"}</dd>
        <dt>macOS</dt><dd>{info?.host.macos || "…"}</dd>
        <dt>Memory</dt><dd>{info ? `${info.host.memoryGb} GB` : "…"}</dd>
        <dt>App data</dt><dd><code>{info?.dataDir ?? "…"}</code></dd>
      </dl>
    </article>

    <article class="card">
      <h2>Sandbox VM</h2>
      <dl>
        <dt>Name</dt><dd><code>qwen35-sandbox</code></dd>
        <dt>State</dt><dd>{info?.vm.message ?? "…"}</dd>
        <dt>Engine</dt><dd><button class="link" onclick={link("container")}>{info?.vm.version ?? "Apple container"}</button></dd>
        <dt>Image</dt><dd><code>{info?.vm.image ?? "…"}</code></dd>
        <dt>Firewall</dt><dd>{info?.vm.network ? firewallText(info.vm.network) : "Not running"}</dd>
        {#if info?.vmDetails}
          {@const d = info.vmDetails}
          <dt>System</dt><dd>{d.os} · kernel {d.kernel} · {d.arch}</dd>
          <dt>Resources</dt><dd>{d.cpus} CPUs · {fmtInt(Number(d.memoryMb) || 0)} MB</dd>
          <dt>Tools</dt><dd>Python {d.python} · Node {d.node} · Chromium {d.chromium}</dd>
        {:else}
          <dt>System</dt><dd class="muted">Start the VM to see its versions.</dd>
        {/if}
      </dl>
    </article>

    <article class="card">
      <h2>Tools and safety</h2>
      <dl>
        <dt>Tools</dt><dd>sh, read, write, browser, skill · {app.tools.enabled ? (app.tools.autoApprove ? "auto-approve" : "approval for each change") : "off"}</dd>
        <dt>Skills</dt><dd>{info?.skills.join(", ") ?? "…"}</dd>
        <dt>Keys</dt><dd>macOS Keychain (or .env for development). Held in Rust only; never sent to the window or the VM.</dd>
        <dt>Prompt</dt><dd>Short system prompt with injection rules: page text, files and tool output are data, not instructions. Each message carries its send time.</dd>
        <dt>Isolation</dt><dd>Commands, files and the browser run in the VM as an unprivileged user. No sudo, no firewall changes, no view of this Mac.</dd>
        <dt>Patching</dt><dd>Settings &gt; Tools &gt; Sandbox: system updates, your own patches, full rebuild.</dd>
      </dl>
    </article>

    {#if error}<p class="err" role="alert">{error}</p>{/if}
  </div>
</section>

<style>
  .about {
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
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: var(--space-4);
    padding: var(--space-5);
    overflow-y: auto;
    align-content: start;
  }

  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: var(--space-4);
    min-width: 0;
  }

  .wide,
  .hero {
    grid-column: 1 / -1;
  }

  .hero {
    display: flex;
    gap: var(--space-4);
    align-items: center;
  }

  .hero img {
    border-radius: 50%;
    flex: none;
  }

  .hero p {
    margin: var(--space-1) 0 0;
    font-size: 13px;
  }

  h2 {
    font-size: 15px;
    margin-bottom: var(--space-3);
  }

  .hero h2 {
    margin: 0;
    font-size: 18px;
  }

  h3 {
    font-size: 13px;
    margin: var(--space-4) 0 var(--space-2);
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-1) var(--space-3);
    margin: 0;
    font-size: 13px;
  }

  dt {
    font-weight: 700;
    color: var(--text-muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .muted {
    color: var(--text-muted);
  }

  .link {
    padding: 0;
    color: var(--link);
    text-decoration: underline;
    text-align: left;
  }

  svg {
    width: 100%;
    height: auto;
    display: block;
  }

  .box rect {
    fill: var(--mist-25);
    stroke: var(--blue-25);
    stroke-width: 1.5;
  }

  .box.main rect {
    fill: var(--mist-60);
    stroke: var(--primary);
  }

  .box.vm rect {
    stroke: var(--ok);
  }

  .box.net rect {
    fill: var(--surface);
  }

  .t1 {
    font: 700 15px var(--font-display);
    fill: var(--text);
  }

  .t2 {
    font: 13px var(--font-text);
    fill: var(--text);
  }

  .t3 {
    font: 11px var(--font-micro);
    fill: var(--text-muted);
  }

  .blocked {
    fill: var(--danger);
  }

  .line {
    stroke: var(--text-muted);
    stroke-width: 1.5;
  }

  .fill-line {
    fill: var(--text-muted);
  }

  .err {
    grid-column: 1 / -1;
    margin: 0;
    padding-left: var(--space-2);
    border-left: 3px solid var(--danger);
  }
</style>
