<script lang="ts">
  import { onMount } from "svelte";
  import { api, errText } from "./lib/api";
  import { loadSessions } from "./lib/chat.svelte";
  import { checkNow, watchFocus } from "./lib/connection.svelte";
  import { watchVm } from "./lib/vm.svelte";
  import { watchUpdates } from "./lib/updates.svelte";
  import { app } from "./lib/state.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import TopBar from "./lib/components/TopBar.svelte";
  import MessageList from "./lib/components/MessageList.svelte";
  import Composer from "./lib/components/Composer.svelte";
  import StatusBar from "./lib/components/StatusBar.svelte";
  import Settings from "./lib/components/Settings.svelte";
  import BrowserPane from "./lib/components/BrowserPane.svelte";
  import About from "./lib/components/About.svelte";
  import MemoryPane from "./lib/components/MemoryPane.svelte";
  import Setup from "./lib/components/Setup.svelte";
  import UpdateBanner from "./lib/components/UpdateBanner.svelte";

  onMount(() => {
    api
      .bootstrap()
      .then((b) => {
        app.profiles = b.profiles;
        app.activeId = b.activeId;
        app.envFile = b.envFile;
        app.keychainService = b.keychainService;
        app.tools = b.toolSettings;
        void checkNow();
        // First launch, or first launch after an update: open Setup if anything needs doing.
        void api.setupReport().then((r) => {
          app.setupNeeded = r.needsAttention;
          if (r.needsAttention && b.setupSeenVersion !== r.appVersion) app.view = "setup";
        });
        return loadSessions();
      })
      .catch((e) => {
        app.status = "error";
        app.statusMessage = `Could not start: ${errText(e)}`;
      });
    const stopVm = watchVm();
    const stopFocus = watchFocus();
    const stopUpdates = watchUpdates();
    return () => {
      stopVm();
      stopFocus();
      stopUpdates();
    };
  });
</script>

<div class="shell" class:with-browser={app.browserOpen && app.view === "chat"} class:with-memory={app.memoryOpen && !app.browserOpen && app.view === "chat"}>
  <Sidebar />
  <main>
    {#if app.view === "settings"}
      <Settings />
    {:else if app.view === "about"}
      <About />
    {:else if app.view === "setup"}
      <Setup />
    {:else}
      <TopBar />
      <UpdateBanner />
      <MessageList />
      <Composer />
      <StatusBar />
    {/if}
  </main>
  {#if app.view === "chat"}
    {#if app.browserOpen}
      <BrowserPane />
    {:else if app.memoryOpen}
      <MemoryPane />
    {/if}
  {/if}
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 248px 1fr;
    height: 100%;
  }

  .shell.with-browser {
    grid-template-columns: 248px minmax(420px, 1fr) minmax(380px, 44%);
  }

  .shell.with-memory {
    grid-template-columns: 248px minmax(480px, 1fr) minmax(300px, 30%);
  }

  main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
</style>
