// Capture settings, shared by the background worker and the popup.

export const HOST = "app.yusa.ferry";

export const DEFAULT_EXTENSIONS = [
  // Archives and disk images
  "zip rar 7z tar gz tgz xz bz2 zst iso img dmg",
  // Programs and packages
  "exe msi msix appx apk deb rpm appimage bin",
  // Video and audio
  "mp4 mkv avi mov webm m4v wmv flv mp3 flac wav m4a ogg opus aac",
  // Documents
  "pdf",
].join(" ");

export const DEFAULTS = {
  /** Hand matching downloads to Ferry. */
  enabled: true,
  /** File extensions to capture, separated by spaces. */
  extensions: DEFAULT_EXTENSIONS,
  /** Leave smaller downloads to the browser (when the size is known). */
  minSizeMB: 1,
};

export async function loadSettings() {
  return { ...DEFAULTS, ...(await chrome.storage.sync.get(Object.keys(DEFAULTS))) };
}

export function saveSettings(changes) {
  return chrome.storage.sync.set(changes);
}

/** "zip, .rar  7z" -> Set {"zip", "rar", "7z"} */
export function parseExtensions(text) {
  return new Set(
    text
      .toLowerCase()
      .split(/[\s,;]+/)
      .map((e) => e.replace(/^\*?\./, ""))
      .filter(Boolean),
  );
}

export function extensionOf(fileName) {
  const base = fileName.split(/[\\/]/).pop() ?? "";
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
}

/** Whether a browser download should go to Ferry. */
export function shouldCapture(item, settings) {
  if (!settings.enabled || item.incognito) return false;
  const url = item.finalUrl || item.url;
  if (!/^(https?|ftp):/i.test(url)) return false; // blob:, data: and the like stay in the browser
  if (!parseExtensions(settings.extensions).has(extensionOf(item.filename || url))) return false;
  const minBytes = Number(settings.minSizeMB) * 1024 * 1024;
  return !(item.totalBytes > 0 && item.totalBytes < minBytes);
}
