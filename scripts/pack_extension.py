"""Packs extension/ into dist/ferry-extension-<version>.zip for the Chrome
Web Store and Edge Add-ons.

The stores give the extension their own ID and refuse a manifest with a
`key`, so it is left out; tests and docs are left out too.

    python scripts/pack_extension.py
"""

import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "extension"
SKIP_SUFFIXES = {".md", ".mjs"}

manifest = json.loads((SRC / "manifest.json").read_text(encoding="utf-8"))
manifest.pop("key", None)

out = ROOT / "dist" / f"ferry-extension-{manifest['version']}.zip"
out.parent.mkdir(exist_ok=True)
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zf:
    zf.writestr("manifest.json", json.dumps(manifest, indent=2) + "\n")
    for path in sorted(SRC.rglob("*")):
        if path.is_dir() or path.name == "manifest.json" or path.suffix in SKIP_SUFFIXES:
            continue
        zf.write(path, path.relative_to(SRC).as_posix())

print(out.relative_to(ROOT).as_posix())
for name in zipfile.ZipFile(out).namelist():
    print("  " + name)
