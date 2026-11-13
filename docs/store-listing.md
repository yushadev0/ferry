# Store listing (Chrome Web Store and Edge Add-ons)

Text to paste into the developer dashboards. Upload `dist/ferry-extension-<version>.zip`
(`python scripts/pack_extension.py`, or the asset on the GitHub release).

## Name

Ferry Download Center

## Summary (132 characters max)

Send downloads to Ferry, the free open-source download manager: faster segmented downloads you can pause and resume.

## Description

Ferry Download Center is a free, open-source download manager for Windows. This
extension hands your browser's downloads to the Ferry app, which downloads them
over several connections at once, lets you pause and resume (even after a
restart), and runs them in a queue with a schedule and a speed limit.

- Catches downloads of the file types you choose (archives, installers, video,
  audio, PDFs…) above a minimum size, and opens them in Ferry.
- Right-click any link, video or audio and choose "Download with Ferry".
- Keeps your sign-in: the page's cookies and referrer go along, so downloads
  from sites you are logged in to still work.
- If Ferry isn't installed or can't be reached, the browser downloads the file
  as usual. Incognito/InPrivate downloads are never touched.

Requires the Ferry app: https://github.com/yushadev0/ferry/releases

Open source (MIT or Apache-2.0): https://github.com/yushadev0/ferry

## Category

Productivity (Chrome: "Tools" / Edge: "Productivity")

## Single purpose

Hands downloads from the browser to the Ferry Download Center app installed on
the same computer.

## Permission justifications

| Permission | Why |
|---|---|
| `downloads` | To notice downloads that match the user's file types and hand them to Ferry instead of saving them in the browser. |
| `nativeMessaging` | The only way the extension talks to the Ferry app on the same computer (through its `ferry-host` helper). |
| `contextMenus` | Adds "Download with Ferry" to the right-click menu of links, video and audio. |
| `storage` | Saves the extension's settings: capture on or off, file types, minimum size. |
| `cookies` | Reads the cookies for the download's address only, so Ferry can download files from sites the user is signed in to. |
| Host permission `<all_urls>` | Downloads can come from any site; the cookies permission needs host access to read the cookies of the download's address. |

## Remote code

No. All code is in the package.

## Data use disclosures

- Collects: website content? No. Authentication information? Cookies for the
  download's address are read and passed to the local Ferry app only.
- Not sold, not used for anything unrelated to the single purpose, not used
  for creditworthiness or lending.

## Privacy policy URL

https://github.com/yushadev0/ferry/blob/main/docs/privacy.md

## Screenshots (1280×800)

1. Ferry's "Download from browser" dialog over a web page.
2. The extension popup ("Connected to Ferry", file types).
3. Ferry's main window with a segmented download in progress.

## After publishing

Each store gives the extension its own ID. Add both to `EXTENSION_IDS` in
`crates/ferry-host/src/lib.rs` (so `ferry-host` accepts them) and to the store
list in `app/src-tauri/src/browser.rs` (so the installed app offers the
extension in Chrome and Edge), then release a new version.
