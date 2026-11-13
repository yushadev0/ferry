// Updates from GitHub Releases (Tauri updater, signed). Ferry checks shortly
// after starting and twice a day when the setting is on, tells you once per
// new version, and installs only when you choose to in Settings.
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import * as api from "./api";
import { t } from "./i18n.svelte";

export type UpdateStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "upToDate" }
  | { kind: "available"; version: string }
  | { kind: "downloading"; version: string; progress: number | null }
  | { kind: "error"; during: "check" | "install"; message: string };

let status = $state<UpdateStatus>({ kind: "idle" });
let pending: Update | null = null;

export function updateStatus(): UpdateStatus {
  return status;
}

/** Asks GitHub for a newer version. Returns it, or null. */
export async function checkForUpdates(): Promise<string | null> {
  status = { kind: "checking" };
  try {
    pending = await check();
    status = pending ? { kind: "available", version: pending.version } : { kind: "upToDate" };
    return pending?.version ?? null;
  } catch (e) {
    status = { kind: "error", during: "check", message: api.message(e) };
    return null;
  }
}

/** Downloads the update, installs it and restarts Ferry. Running downloads continue afterwards. */
export async function installUpdate() {
  const update = pending;
  if (!update) return;
  let total = 0;
  let done = 0;
  status = { kind: "downloading", version: update.version, progress: null };
  try {
    await update.download((e) => {
      if (e.event === "Started") total = e.data.contentLength ?? 0;
      if (e.event === "Progress") {
        done += e.data.chunkLength;
        status = { kind: "downloading", version: update.version, progress: total ? done / total : null };
      }
    });
    // Only now, with the installer ready: pause downloads so their progress is saved.
    await api.prepareUpdate();
    await update.install();
    await relaunch();
  } catch (e) {
    await api.updateFailed().catch(() => {});
    status = { kind: "error", during: "install", message: api.message(e) };
  }
}

const FIRST_CHECK_MS = 30_000;
const CHECK_EVERY_MS = 12 * 60 * 60 * 1000;
const NOTIFIED_KEY = "ferry.update.notified";

function notifyOnce(version: string) {
  try {
    if (localStorage.getItem(NOTIFIED_KEY) === version) return;
    localStorage.setItem(NOTIFIED_KEY, version);
  } catch {
    // storage unavailable: notify anyway
  }
  api.notify(t("updates.notifyTitle", { version }), t("updates.notifyBody")).catch(() => {});
}

let started = false;

/** Starts the background checks; `enabled` is read each time, so the setting applies at once. */
export function startUpdateChecks(enabled: () => boolean) {
  if (started || import.meta.env.DEV) return;
  started = true;
  const run = async () => {
    if (!enabled()) return;
    const version = await checkForUpdates();
    if (version) notifyOnce(version);
  };
  setTimeout(run, FIRST_CHECK_MS);
  setInterval(run, CHECK_EVERY_MS);
}
