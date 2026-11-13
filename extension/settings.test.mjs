// Run with `node --test extension/settings.test.mjs`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { DEFAULTS, extensionOf, parseExtensions, shouldCapture } from "./settings.js";

const item = (over) => ({
  incognito: false,
  url: "https://example.com/file",
  filename: "a.zip",
  totalBytes: 5 * 1024 * 1024,
  ...over,
});
const capture = (over, settings = DEFAULTS) => shouldCapture(item(over), settings);

test("captures matching files", () => {
  assert.equal(capture({}), true);
  assert.equal(capture({ filename: ["C:", "Users", "me", "Downloads", "setup.EXE"].join("\\") }), true);
  assert.equal(capture({ filename: "", url: "https://example.com/f/setup.msi" }), true);
});

test("leaves other downloads to the browser", () => {
  assert.equal(capture({ filename: "page.html" }), false);
  assert.equal(capture({ totalBytes: 1000 }), false);
  assert.equal(capture({ url: "blob:https://example.com/1" }), false);
  assert.equal(capture({ url: "data:application/zip;base64,AA" }), false);
  assert.equal(capture({ incognito: true }), false);
  assert.equal(capture({}, { ...DEFAULTS, enabled: false }), false);
});

test("unknown sizes are captured", () => {
  assert.equal(capture({ totalBytes: -1 }), true);
  assert.equal(capture({ totalBytes: 0 }), true);
});

test("file type lists are forgiving", () => {
  assert.deepEqual([...parseExtensions("ZIP, .rar  *.7z;iso")], ["zip", "rar", "7z", "iso"]);
  assert.equal(extensionOf("archive.tar.gz"), "gz");
  assert.equal(extensionOf(".hidden"), "");
});
