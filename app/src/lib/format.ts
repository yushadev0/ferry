import type { Download, Segment } from "./api";

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return "Unknown";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  const digits = unit === 0 || value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${UNITS[unit]}`;
}

export function formatSpeed(bytesPerSecond: number | undefined): string {
  return bytesPerSecond ? `${formatBytes(bytesPerSecond)}/s` : "";
}

export function formatEta(remaining: number | null, speed: number | undefined): string {
  if (remaining == null || !speed) return "";
  const s = Math.ceil(remaining / speed);
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, "0")} s`;
  const h = Math.floor(s / 3600);
  return `${h} h ${String(Math.floor((s % 3600) / 60)).padStart(2, "0")} min`;
}

export function percent(downloaded: number, size: number | null): number | null {
  if (!size) return null;
  return Math.min(100, (downloaded / size) * 100);
}

export function extension(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** "14:05", or "Tomorrow 02:00" / "Mon 02:00" when not today. */
export function formatStartTime(unixSeconds: number, now = new Date()): string {
  const at = new Date(unixSeconds * 1000);
  const time = at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(at) - day(now)) / 86_400_000);
  if (days === 0) return time;
  if (days === 1) return `Tomorrow ${time}`;
  return `${at.toLocaleDateString([], { weekday: "short" })} ${time}`;
}

export function segmentLength(s: Segment, size: number | null): number {
  const end = s.end ?? (size != null ? size - 1 : s.pos);
  return Math.max(1, end - s.start + 1);
}

export function segmentDone(s: Segment, size: number | null): number {
  return Math.min(segmentLength(s, size), Math.max(0, s.pos - s.start));
}

/** Segments in file order, or one segment covering the file when none are known. */
export function partsOf(d: Download): Segment[] {
  if (d.segments.length) return [...d.segments].sort((a, b) => a.start - b.start);
  const end = d.size != null ? d.size - 1 : null;
  return [{ start: 0, end, pos: d.status === "completed" ? (d.size ?? d.downloaded) : d.downloaded }];
}
