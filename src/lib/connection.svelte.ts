// Polls the active profile and keeps app.status current (docs/architecture.md).
import { api, errText } from "./api";
import { activeProfile, app } from "./state.svelte";
import type { Status } from "./types";

// Next check, by state. Missing = wait for the user (fix key, change settings).
const NEXT_MS: Partial<Record<Status, number>> = {
  ready: 30_000,
  loading: 5_000,
  starting: 5_000,
  unreachable: 15_000,
  "dns-failed": 15_000,
  error: 15_000,
};

let timer: ReturnType<typeof setTimeout> | undefined;
let generation = 0;

export async function checkNow(): Promise<void> {
  clearTimeout(timer);
  const mine = ++generation;
  const profile = activeProfile();
  if (!profile) {
    app.status = "unknown";
    app.statusMessage = "Add a profile in Settings.";
    return;
  }
  if (app.status !== "ready") app.status = "checking";

  try {
    const r = await api.probe($state.snapshot(profile));
    if (mine !== generation) return; // a newer check started
    app.status = r.status;
    app.statusMessage = r.message;
    app.models = r.models;
    app.modelFound = r.modelFound;
    app.latencyMs = r.latencyMs;
  } catch (e) {
    if (mine !== generation) return;
    app.status = "error";
    app.statusMessage = errText(e);
  }

  const ms = NEXT_MS[app.status];
  if (ms) timer = setTimeout(() => void checkNow(), ms);
}

/** Check again when the window comes back, unless already ready. */
export function watchFocus(): () => void {
  const onFocus = () => {
    if (app.status !== "ready" && app.status !== "checking") void checkNow();
  };
  window.addEventListener("focus", onFocus);
  return () => window.removeEventListener("focus", onFocus);
}
