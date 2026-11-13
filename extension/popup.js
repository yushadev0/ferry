import { DEFAULTS, loadSettings, saveSettings } from "./settings.js";

const $ = (id) => document.getElementById(id);
const enabled = $("enabled");
const minSize = $("minSizeMB");
const extensions = $("extensions");

function show(settings) {
  enabled.checked = settings.enabled;
  minSize.value = settings.minSizeMB;
  extensions.value = settings.extensions;
  $("filters").disabled = !settings.enabled;
}

async function checkFerry() {
  const status = $("status");
  const reply = await chrome.runtime.sendMessage({ type: "ping" });
  if (reply?.ok && reply.running) {
    status.textContent = `Connected to Ferry ${reply.version}`;
    status.dataset.tone = "ok";
  } else if (reply?.ok) {
    status.textContent = "Ferry isn't running. It starts when a download is captured.";
    status.dataset.tone = "muted";
  } else {
    status.textContent = "Ferry isn't installed, or hasn't been opened once yet. Downloads stay in the browser.";
    status.dataset.tone = "warn";
    status.title = reply?.error ?? "";
  }
}

enabled.addEventListener("change", () => {
  saveSettings({ enabled: enabled.checked });
  $("filters").disabled = !enabled.checked;
});
minSize.addEventListener("change", () => {
  const value = Math.max(0, Number(minSize.value) || 0);
  minSize.value = value;
  saveSettings({ minSizeMB: value });
});
extensions.addEventListener("change", () => saveSettings({ extensions: extensions.value.trim() }));
$("reset").addEventListener("click", async () => {
  const { minSizeMB, extensions: exts } = DEFAULTS;
  await saveSettings({ minSizeMB, extensions: exts });
  show(await loadSettings());
});

show(await loadSettings());
chrome.runtime.sendMessage({ type: "clearBadge" });
checkFerry();
