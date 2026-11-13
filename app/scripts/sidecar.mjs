// Builds ferry-host and puts it where Tauri's `externalBin` expects it:
// src-tauri/binaries/ferry-host-<target triple>[.exe]. The installer then
// ships it next to the app, where the app registers it with the browsers.
//
//   node scripts/sidecar.mjs            debug build (pnpm tauri dev)
//   node scripts/sidecar.mjs --release  release build (pnpm tauri build)
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const release = process.argv.includes("--release");
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const triple = execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/^host: (\S+)/m)[1];
const exe = triple.includes("windows") ? ".exe" : "";

execFileSync("cargo", ["build", "-p", "ferry-host", ...(release ? ["--release"] : [])], {
  cwd: root,
  stdio: "inherit",
});

const targetDir = process.env.CARGO_TARGET_DIR ? resolve(process.env.CARGO_TARGET_DIR) : join(root, "target");
const from = join(targetDir, release ? "release" : "debug", `ferry-host${exe}`);
const dir = join(root, "app", "src-tauri", "binaries");
mkdirSync(dir, { recursive: true });
copyFileSync(from, join(dir, `ferry-host-${triple}${exe}`));
console.log(`ferry-host -> src-tauri/binaries/ferry-host-${triple}${exe}`);
