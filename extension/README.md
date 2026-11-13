# Ferry browser extension

Manifest V3 extension for Chrome and Edge. It hands downloads to Ferry Download
Center through the `ferry-host` native messaging host.

- **Automatic capture:** downloads whose file type is on the list (and that are
  at least the minimum size, when the size is known) are cancelled in the
  browser and opened in Ferry's Add dialog, with the page's cookies and referrer
  so links behind a login still work. Incognito downloads, `blob:` and `data:`
  links always stay in the browser. If Ferry can't be reached, the browser
  downloads the file as usual.
- **Right-click → Download with Ferry** on links, video and audio.
- **Popup:** shows whether Ferry is running, turns capture on or off and edits
  the file types and minimum size.

## How the pieces talk

```
extension ──native messaging──▶ ferry-host ──local socket──▶ Ferry app
```

Ferry registers `ferry-host` with Chrome and Edge every time it starts
(`HKCU\Software\{Google\Chrome,Microsoft\Edge}\NativeMessagingHosts\app.yusa.ferry`
on Windows). If a download arrives while Ferry is closed, `ferry-host` starts it.

## Trying it out

1. Build the host next to the app: `cargo build -p ferry-host`
2. Start Ferry once (`pnpm tauri dev` in `app/`) so it registers the host.
3. Open `chrome://extensions` (or `edge://extensions`), turn on Developer mode,
   choose **Load unpacked** and pick this folder.
4. The `key` in `manifest.json` fixes the extension ID to
   `hieofjpdefhpcbpmbhigbnefpimhmlop`, which the host's manifest allows.

Tests for the capture filter: `node --test extension/settings.test.mjs`
