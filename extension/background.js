// Hands browser downloads to Ferry through the native messaging host.

import { HOST, loadSettings, shouldCapture } from "./settings.js";

const MENU_ID = "ferry-download";

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: MENU_ID,
    title: "Download with Ferry",
    contexts: ["link", "video", "audio"],
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== MENU_ID) return;
  const url = info.linkUrl || info.srcUrl;
  if (!url) return;
  const reply = await send(await captureOf({ url, referrer: info.pageUrl || tab?.url }));
  if (!reply.ok) notifyFailure(reply.error);
});

// Runs before the browser picks a file name, so a download can be taken over
// before anything is written. Returning true lets us decide asynchronously.
chrome.downloads.onDeterminingFilename.addListener((item, suggest) => {
  takeOver(item, suggest);
  return true;
});

async function takeOver(item, suggest) {
  let handed = false;
  try {
    if (shouldCapture(item, await loadSettings())) {
      const reply = await send(
        await captureOf({
          url: item.finalUrl || item.url,
          referrer: item.referrer,
          fileName: item.filename,
          fileSize: item.totalBytes > 0 ? item.totalBytes : null,
        }),
      );
      handed = reply.ok;
    }
  } catch (e) {
    console.warn("Ferry: could not hand over the download", e);
  }
  if (!handed) {
    // Ferry is missing or said no: let the browser download it as usual.
    suggest();
    return;
  }
  await chrome.downloads.cancel(item.id).catch(() => {});
  await chrome.downloads.erase({ id: item.id }).catch(() => {});
}

async function captureOf({ url, referrer, fileName = null, fileSize = null }) {
  const cookies = await chrome.cookies.getAll({ url }).catch(() => []);
  return {
    type: "download",
    url,
    fileName,
    fileSize,
    referrer: referrer || null,
    cookies: cookies.length ? cookies.map((c) => `${c.name}=${c.value}`).join("; ") : null,
    userAgent: navigator.userAgent,
  };
}

/** Sends a message to ferry-host; never throws. */
async function send(message) {
  try {
    return await chrome.runtime.sendNativeMessage(HOST, message);
  } catch (e) {
    return { ok: false, running: false, error: e?.message ?? String(e) };
  }
}

function notifyFailure(error) {
  chrome.action.setBadgeBackgroundColor({ color: "#c42b1c" });
  chrome.action.setBadgeText({ text: "!" });
  chrome.action.setTitle({ title: `Ferry: ${error ?? "could not reach Ferry"}` });
}

// The popup clears the warning once it has shown it.
chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (message?.type === "ping") {
    send({ type: "ping" }).then(respond);
    return true;
  }
  if (message?.type === "clearBadge") {
    chrome.action.setBadgeText({ text: "" });
    chrome.action.setTitle({ title: "Ferry" });
  }
});
