// Keeps app.vm current for the top bar and Settings.
import { api } from "./api";
import { app } from "./state.svelte";

let inFlight = false;

export async function refreshVm(): Promise<void> {
  if (inFlight) return;
  inFlight = true;
  try {
    app.vm = await api.vmStatus();
  } catch {
    app.vm = null;
  } finally {
    inFlight = false;
  }
}

/** Poll every 20 s. Returns a stop function. */
export function watchVm(): () => void {
  void refreshVm();
  const t = setInterval(() => void refreshVm(), 20_000);
  return () => clearInterval(t);
}
