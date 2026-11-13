"""Renders the store screenshots (docs/store/*.html) to 1280×800 PNGs.

    python docs/store/render.py            all slides
    python docs/store/render.py 1-capture  one slide

Needs Microsoft Edge or Google Chrome. Icon drawings are copied from
app/src/lib/Icon.svelte first, so the slides always match the app.
"""

import functools
import http.server
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import threading

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
OUT = HERE / "png"

BROWSERS = [
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    "msedge",
    "google-chrome",
    "chromium",
]


def find_browser():
    for b in BROWSERS:
        path = b if os.path.isfile(b) else shutil.which(b)
        if path:
            return path
    sys.exit("Microsoft Edge or Google Chrome is needed to render the slides")


def write_icons():
    src = (ROOT / "app/src/lib/Icon.svelte").read_text(encoding="utf-8")
    paths = dict(re.findall(r"^\s+(\w+):\s*\n?\s*'([^']*)'", src, re.M))
    (HERE / "icons.js").write_text(
        "// Generated from app/src/lib/Icon.svelte by docs/store/render.py; do not edit.\n"
        f"export const PATHS = {json.dumps(paths, indent=2)};\n",
        encoding="utf-8",
    )


def serve():
    handler = functools.partial(QuietHandler, directory=str(ROOT))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def main():
    write_icons()
    names = sys.argv[1:] or sorted(p.stem for p in HERE.glob("[0-9]*.html"))
    browser = find_browser()
    server = serve()
    OUT.mkdir(exist_ok=True)
    try:
        for name in names:
            url = f"http://127.0.0.1:{server.server_port}/docs/store/{name}.html"
            out = OUT / f"{name}.png"
            with tempfile.TemporaryDirectory() as profile:
                subprocess.run(
                    [
                        browser,
                        "--headless=new",
                        "--disable-gpu",
                        "--hide-scrollbars",
                        "--force-device-scale-factor=1",
                        "--window-size=1280,800",
                        "--virtual-time-budget=3000",
                        f"--user-data-dir={profile}",
                        f"--screenshot={out}",
                        url,
                    ],
                    check=True,
                    capture_output=True,
                )
            print(out.relative_to(ROOT).as_posix())
    finally:
        server.shutdown()


if __name__ == "__main__":
    main()
