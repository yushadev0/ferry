import type { Download, Segment } from "./api";
import { lang, t } from "./i18n.svelte";

const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** A number with the current language's decimal separator ("3.72" or "3,72"). */
function decimal(value: number, digits: number): string {
  return value.toLocaleString(lang(), { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return t("fmt.unknown");
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  const digits = unit === 0 || value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${decimal(value, digits)} ${UNITS[unit]}`;
}

export function formatSpeed(bytesPerSecond: number | undefined): string {
  return bytesPerSecond ? `${formatBytes(bytesPerSecond)}/s` : "";
}

export function formatEta(remaining: number | null, speed: number | undefined): string {
  if (remaining == null || !speed) return "";
  const s = Math.ceil(remaining / speed);
  const two = (n: number) => String(n).padStart(2, "0");
  if (s < 60) return t("fmt.seconds", { s });
  if (s < 3600) return t("fmt.minutes", { m: Math.floor(s / 60), s: two(s % 60) });
  return t("fmt.hours", { h: Math.floor(s / 3600), m: two(Math.floor((s % 3600) / 60)) });
}

/** A whole percentage the way the language writes it: "61%" or "%61". */
export function formatPercent(value: number): string {
  return (Math.floor(value) / 100).toLocaleString(lang(), { style: "percent" });
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
export function formatStartTime(unixSeconds: number, now = new Date(), lower = false): string {
  const at = new Date(unixSeconds * 1000);
  // The clock follows Windows' regional format (24-hour like the time fields),
  // the words follow the interface language.
  const time = at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(at) - day(now)) / 86_400_000);
  if (days === 0) return time;
  if (days === 1) return t(lower ? "fmt.tomorrowLower" : "fmt.tomorrow", { time });
  return `${at.toLocaleDateString(lang(), { weekday: "short" })} ${time}`;
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
