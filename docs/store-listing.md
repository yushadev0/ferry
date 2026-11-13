# Store listing (Chrome Web Store and Edge Add-ons)

Text to paste into the developer dashboards. Upload `dist/ferry-extension-<version>.zip`
(`python scripts/pack_extension.py`, or the asset on the GitHub release).

## Name

Ferry Download Center

## Summary (132 characters max)

English (112):

> Send your downloads to Ferry, the free open-source download manager: faster, split into parts, pause and resume.

Türkçe (132, exactly the limit):

> İndirmelerinizi ücretsiz ve açık kaynaklı indirme yöneticisi Ferry'ye gönderin: parçalara bölünerek daha hızlı, devam ettirilebilir.

## Description

Plain text for the "Description" field (the store doesn't render Markdown).

### English

```text
Ferry Download Center is a free, open-source download manager for Windows. This extension connects your browser to the Ferry app, so your downloads get Ferry's speed and control without changing how you browse.

What it does
• Catches downloads as you click them. Archives, installers, disk images, video, audio and PDFs above 1 MB open in Ferry instead of the browser. You choose the file types and the minimum size.
• Adds "Download with Ferry" to the right-click menu of links, video and audio.
• Keeps your sign-in. The page's cookies and referrer go along, so downloads from sites you're logged in to still work.
• Stays out of the way. If Ferry isn't running it starts it; if Ferry isn't installed, the browser downloads the file as usual. Incognito and InPrivate downloads are never touched.

What Ferry does with them
• Splits each file into parts and downloads them over several connections at once, splitting again as parts finish.
• Pauses and resumes, even after a restart or a dropped connection.
• Runs a queue: a few downloads at a time, only at the hours you choose, under a total speed limit.
• Fits in with Windows 11, in light or dark.

Privacy
The extension talks only to the Ferry app on your own computer. It sends nothing to us or to anyone else, and it has no analytics or tracking.

Requires the Ferry app for Windows (free): https://github.com/yushadev0/ferry/releases
Source code (MIT or Apache-2.0): https://github.com/yushadev0/ferry
```

### Türkçe

```text
Ferry Download Center, Windows için ücretsiz ve açık kaynaklı bir indirme yöneticisi. Bu eklenti tarayıcınızı Ferry uygulamasına bağlar; siz her zamanki gibi gezinirken indirmeleriniz Ferry'nin hızından ve kontrolünden yararlanır.

Neler yapar
• İndirmeleri tıkladığınız anda yakalar. 1 MB'tan büyük arşivler, kurulum dosyaları, disk kalıpları, video, ses ve PDF'ler tarayıcı yerine Ferry'de açılır. Dosya türlerini ve en küçük boyutu siz belirlersiniz.
• Linklerin, videoların ve seslerin sağ tık menüsüne "Download with Ferry" seçeneğini ekler.
• Oturumunuzu korur. Sayfanın çerezleri ve yönlendireni de gönderilir; giriş yaptığınız sitelerden indirmeler sorunsuz çalışır.
• Araya girmez. Ferry kapalıysa onu açar, kurulu değilse dosyayı tarayıcı her zamanki gibi indirir. Gizli pencere ve InPrivate indirmelerine hiç dokunmaz.

Ferry indirmelerle ne yapar
• Her dosyayı parçalara bölüp aynı anda birkaç bağlantıyla indirir, biten parçaların yerine kalanları yeniden böler.
• Duraklatıp devam ettirebilirsiniz; bilgisayarı yeniden başlatsanız ya da bağlantı kopsa bile.
• Bir kuyruk çalıştırır: aynı anda birkaç indirme, yalnızca seçtiğiniz saatlerde, toplam bir hız sınırının altında.
• Windows 11'e uyar; açık ya da koyu temada.

Gizlilik
Eklenti yalnızca kendi bilgisayarınızdaki Ferry uygulamasıyla konuşur. Bize ya da başka birine hiçbir şey göndermez; analiz ya da takip yoktur.

Windows için Ferry uygulaması gerekir (ücretsiz): https://github.com/yushadev0/ferry/releases
Kaynak kodu (MIT veya Apache-2.0): https://github.com/yushadev0/ferry
```

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

In `docs/store/png/`, made from the HTML next to them with
`python docs/store/render.py` (uses the app's own CSS and icons):

1. `1-capture.png`: a browser download opening in Ferry's "Download from browser" dialog.
2. `2-speed.png`: Ferry's main window, downloads split into parts, connection map.
3. `3-queue.png`: Settings in dark mode: queue, speed limit, schedule.
4. `4-menu.png`: "Download with Ferry" in the right-click menu and the extension popup.

## After publishing

Each store gives the extension its own ID. Add both to `EXTENSION_IDS` in
`crates/ferry-host/src/lib.rs` (so `ferry-host` accepts them) and to the store
list in `app/src-tauri/src/browser.rs` (so the installed app offers the
extension in Chrome and Edge), then release a new version.
