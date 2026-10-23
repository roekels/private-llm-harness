import { mount } from "svelte";
import "./lib/theme.css";
import App from "./App.svelte";
import { app } from "./lib/state.svelte";

const target = document.getElementById("app");
if (!target) throw new Error("Missing #app element");

async function start(): Promise<void> {
  // Demo data for screenshots (src/demo.ts). Dev server only: removed from builds.
  const demo = import.meta.env.DEV && new URLSearchParams(location.search).has("demo");
  if (import.meta.env.DEV && demo) {
    const d = await import("./demo");
    d.installDemo();
    mount(App, { target: target! });
    await d.applyDemoView(app);
    return;
  }
  mount(App, { target: target! });
}

void start();
