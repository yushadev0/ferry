# Privacy policy: Ferry Download Center and its browser extension

_Last updated: 13 November 2026_

Ferry is an open-source download manager. The browser extension hands
downloads from Chrome or Edge to the Ferry app on your own computer.

## What the extension reads

When a download is handed to Ferry (automatically, or through **Download with
Ferry** in the right-click menu), the extension reads:

- the download's address (URL), file name and size,
- the address of the page it came from (the referrer),
- the cookies your browser holds for that download's address,
- your browser's user agent string.

Ferry needs the cookies and referrer to download the file the same way your
browser would, for example from a site you are signed in to.

## Where it goes

Only to the Ferry app on the same computer, through Chrome's or Edge's
native messaging. Nothing is sent to us or to any third party, and the
extension makes no network requests of its own. Ferry then downloads the file
directly from the site it came from.

Ferry stores each download's address, file name and request headers (including
those cookies) in its database on your computer, so a paused download can
resume. Removing the download from Ferry deletes them.

## What is not collected

No browsing history, no analytics, no tracking, no accounts. Downloads in
incognito or InPrivate windows are never handed to Ferry.

## Settings

The extension's settings (whether capture is on, which file types, minimum
size) are stored with `chrome.storage.sync`, which your browser may sync to
your other devices if you use browser sync.

## Contact

Questions or problems: <https://github.com/yushadev0/ferry/issues>
