import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Status = "queued" | "scheduled" | "downloading" | "paused" | "completed" | "failed";

/** A byte range of the file; `end` is inclusive, `null` means until the end. */
export interface Segment {
  start: number;
  end: number | null;
  pos: number;
}

export interface Download {
  id: number;
  url: string;
  dir: string;
  file_name: string;
  size: number | null;
  downloaded: number;
  resumable: boolean;
  status: Status;
  error: string | null;
  created_at: number;
  /** Unix seconds when a scheduled download joins the queue. */
  start_at: number | null;
  segments: Segment[];
  /** Bytes per second; only known while downloading (UI-side). */
  speed?: number;
  /** Open connections; only known while downloading (UI-side). */
  connections?: number;
}

export interface ProbeInfo {
  size: number | null;
  resumable: boolean;
  etag: string | null;
  last_modified: string | null;
  filename: string | null;
}

export interface Settings {
  downloadDir: string;
  segments: number;
  notifyOnComplete: boolean;
  closeToTray: boolean;
  maxActive: number;
  /** KiB/s; 0 = unlimited. */
  speedLimitKib: number;
  scheduleEnabled: boolean;
  /** "HH:MM" */
  scheduleStart: string;
  /** "HH:MM" */
  scheduleStop: string;
}

export interface QueueState {
  running: boolean;
  /** "HH:MM" when the schedule holds the queue. */
  resumesAt: string | null;
}

export type EngineEvent =
  | { type: "added"; id: number }
  | { type: "status"; id: number; status: Status; error: string | null }
  | {
      type: "progress";
      id: number;
      downloaded: number;
      total: number | null;
      speed: number;
      connections: number;
      segments: Segment[];
    }
  | { type: "removed"; id: number };

export interface AddRequest {
  url: string;
  dir?: string;
  fileName?: string;
  start?: boolean;
}

export const listDownloads = () => invoke<Download[]>("list_downloads");
export const getDownload = (id: number) => invoke<Download>("get_download", { id });
export const inspectUrl = (url: string) => invoke<ProbeInfo>("inspect_url", { url });
export const addDownload = (req: AddRequest) => invoke<number>("add_download", { req });
export const startDownloads = (ids: number[]) => invoke<void>("start_downloads", { ids });
export const startNow = (ids: number[]) => invoke<void>("start_now", { ids });
/** `at` is Unix seconds. */
export const scheduleDownloads = (ids: number[], at: number) =>
  invoke<void>("schedule_downloads", { ids, at });
export const queueState = () => invoke<QueueState>("queue_state");
export const pauseDownloads = (ids: number[]) => invoke<void>("pause_downloads", { ids });
export const removeDownloads = (ids: number[], deleteFiles: boolean) =>
  invoke<void>("remove_downloads", { ids, deleteFiles });
export const openFile = (id: number) => invoke<void>("open_file", { id });
export const showInFolder = (id: number) => invoke<void>("show_in_folder", { id });
export const fileIcon = (ext: string) => invoke<string | null>("file_icon", { ext });
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<Settings>("save_settings", { settings });

export function onEngineEvent(handler: (event: EngineEvent) => void): Promise<UnlistenFn> {
  return listen<EngineEvent>("ferry://event", (e) => handler(e.payload));
}

export function onQueueState(handler: (state: QueueState) => void): Promise<UnlistenFn> {
  return listen<QueueState>("ferry://queue", (e) => handler(e.payload));
}

/** Turns an invoke rejection into a readable message. */
export function message(err: unknown): string {
  return typeof err === "string" ? err : err instanceof Error ? err.message : String(err);
}
