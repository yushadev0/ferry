// Fills in icons on the store screenshots, using the app's own drawings.
//   <i data-icon="plus" data-size="16"></i>     Icon.svelte
//   <i data-file="disc" data-size="32"></i>     FileIcon.svelte fallback
//   <i data-logo data-size="22"></i>            AppLogo.svelte
import { PATHS } from "./icons.js";

const FILE_KINDS = {
  archive: `<rect x="10" y="12" width="13" height="14" rx="1.5" fill="#E8A317"/><path d="M16.5 12v3M15 14.5h3M16.5 16v2M15 18.5h3" stroke="#6B4A00" stroke-width="1.1"/><rect x="14.5" y="20" width="4" height="4" rx="1" fill="#6B4A00"/>`,
  disc: `<circle cx="16.5" cy="19.5" r="6.5" fill="#8C96A6"/><circle cx="16.5" cy="19.5" r="4.6" fill="#B9C2CF"/><circle cx="16.5" cy="19.5" r="1.8" fill="#FFFFFF"/>`,
  video: `<rect x="9.5" y="13" width="14" height="11" rx="2" fill="#7C4DDB"/><path d="M15 15.8v5.4l4.4-2.7z" fill="#FFFFFF"/>`,
  audio: `<circle cx="14" cy="22" r="3" fill="#D6336C"/><path d="M17 22v-9l5-1.5v8" fill="none" stroke="#D6336C" stroke-width="2"/><circle cx="20" cy="20" r="2.2" fill="#D6336C"/>`,
  pdf: `<rect x="7" y="13" width="15" height="7.5" rx="1.5" fill="#D93025"/><path d="M10 23h12M10 25.5h9" stroke="#C9CED6" stroke-width="1.2" stroke-linecap="round"/>`,
  app: `<rect x="9.5" y="13" width="14" height="12" rx="1.5" fill="#0F6CBD"/><rect x="9.5" y="13" width="14" height="3" rx="1.5" fill="#0A4F8C"/><rect x="11.5" y="18" width="6" height="5" rx=".8" fill="#FFFFFF"/>`,
};

const LOGO = `<circle cx="32" cy="32" r="22.5" fill="none" stroke="#FFFFFF" stroke-width="12"/><circle cx="32" cy="32" r="22.5" fill="none" stroke="#E2483D" stroke-width="12" stroke-dasharray="17.67 17.67" transform="rotate(-11.25 32 32)"/><circle cx="32" cy="32" r="28.5" fill="none" stroke="#1B3A57" stroke-width="1.6"/><circle cx="32" cy="32" r="16.5" fill="#EAF2FC" stroke="#1B3A57" stroke-width="1.6"/><path d="M32 23v15M25.5 32 32 38.5 38.5 32" fill="none" stroke="#1F6FD1" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>`;

for (const el of document.querySelectorAll("[data-icon]")) {
  const size = el.dataset.size ?? 16;
  const stroke = el.dataset.stroke ?? 1.8;
  el.outerHTML = `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${PATHS[el.dataset.icon]}</svg>`;
}
for (const el of document.querySelectorAll("[data-file]")) {
  const size = el.dataset.size ?? 32;
  el.outerHTML = `<svg class="file-icon" width="${size}" height="${size}" viewBox="0 0 32 32" aria-hidden="true"><path d="M8 2.5h11.5L26 9v19.5a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1v-25a1 1 0 0 1 1-1z" fill="#FFFFFF" stroke="#B8BFC9"/><path d="M19.5 2.5V9H26" fill="#EEF1F5" stroke="#B8BFC9" stroke-linejoin="round"/>${FILE_KINDS[el.dataset.file] ?? ""}</svg>`;
}
for (const el of document.querySelectorAll("[data-logo]")) {
  const size = el.dataset.size ?? 24;
  el.outerHTML = `<svg width="${size}" height="${size}" viewBox="0 0 64 64" aria-hidden="true">${LOGO}</svg>`;
}
document.documentElement.dataset.ready = "1";
