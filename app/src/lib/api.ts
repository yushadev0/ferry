import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Status = "queued" | "downloading" | "paused" | "completed" | "failed";

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
  /** Bytes per second; only known while downloading (UI-side). */
  speed?: number;
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
}

export type EngineEvent =
  | { type: "added"; id: number }
  | { type: "status"; id: number; status: Status; error: string | null }
  | { type: "progress"; id: number; downloaded: number; total: number | null; speed: number }
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
export const pauseDownloads = (ids: number[]) => invoke<void>("pause_downloads", { ids });
export const removeDownloads = (ids: number[], deleteFiles: boolean) =>
  invoke<void>("remove_downloads", { ids, deleteFiles });
export const openFile = (id: number) => invoke<void>("open_file", { id });
export const showInFolder = (id: number) => invoke<void>("show_in_folder", { id });
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<Settings>("save_settings", { settings });

export function onEngineEvent(handler: (event: EngineEvent) => void): Promise<UnlistenFn> {
  return listen<EngineEvent>("ferry://event", (e) => handler(e.payload));
}

/** Turns an invoke rejection into a readable message. */
export function message(err: unknown): string {
  return typeof err === "string" ? err : err instanceof Error ? err.message : String(err);
}
