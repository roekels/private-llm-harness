// Self-update state and actions (docs/updates.md). Rust does the work.
import { api, errText } from "./api";
import type { UpdateAvailable, UpdateStatus } from "./types";

const DAY_MS = 24 * 60 * 60 * 1000;

export const update = $state({
  status: null as UpdateStatus | null,
  available: null as UpdateAvailable | null,
  checking: false,
  /** Last check found nothing newer. */
  upToDate: false,
  phase: "idle" as "idle" | "downloading" | "installing" | "installed",
  received: 0,
  total: null as number | null,
  error: "",
  /** The banner was closed for this version. */
  dismissed: "",
});

export async function loadUpdateStatus(): Promise<void> {
  try {
    update.status = await api.updateStatus();
  } catch (e) {
    update.error = errText(e);
  }
}

/** `quiet`: an automatic check, which shows no error when offline. */
export async function checkForUpdates(quiet = false): Promise<void> {
  if (update.checking || update.phase !== "idle" || !update.status?.supported) return;
  update.checking = true;
  if (!quiet) update.error = "";
  try {
    update.available = await api.updateCheck();
    update.upToDate = !update.available;
    update.error = "";
    await loadUpdateStatus();
  } catch (e) {
    if (!quiet) update.error = errText(e);
  } finally {
    update.checking = false;
  }
}

export async function installUpdate(): Promise<void> {
  if (!update.available || update.phase !== "idle") return;
  update.error = "";
  update.phase = "downloading";
  update.received = 0;
  update.total = null;
  try {
    await api.updateInstall((e) => {
      if (e.type === "downloading") {
        update.received = e.received;
        update.total = e.total ?? null;
      } else if (e.type === "installing") {
        update.phase = "installing";
      } else {
        update.phase = "installed";
      }
    });
    update.phase = "installed";
  } catch (e) {
    update.phase = "idle";
    update.error = errText(e);
  }
}

export async function restartNow(): Promise<void> {
  update.error = "";
  try {
    await api.updateRestart();
  } catch (e) {
    update.error = errText(e);
  }
}

export async function setAutoCheck(on: boolean): Promise<void> {
  try {
    await api.updateSettingsSet(on);
    await loadUpdateStatus();
    if (on) void checkForUpdates(true);
  } catch (e) {
    update.error = errText(e);
  }
}

/** Check at start and once a day while automatic checks are on. Returns a stop function. */
export function watchUpdates(): () => void {
  const auto = () => !!update.status?.supported && !!update.status.autoCheck;
  void loadUpdateStatus().then(() => {
    if (auto()) void checkForUpdates(true);
  });
  // Look every hour; check when the last check is a day old.
  const t = setInterval(() => {
    const last = update.status?.lastCheck ?? 0;
    if (auto() && Date.now() - last >= DAY_MS) void checkForUpdates(true);
  }, 60 * 60 * 1000);
  return () => clearInterval(t);
}
