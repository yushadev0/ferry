//! Browser integration: the local socket `ferry-host` talks to, and the
//! native messaging registration that lets Chrome and Edge start `ferry-host`.

use std::{io, path::Path};

use ferry_host::{socket_name, Capture, Message, Reply, EXTENSION_IDS, HOST_NAME};
use interprocess::local_socket::{
    tokio::{prelude::*, Stream},
    ListenerOptions,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::AppState;

/// Tells the UI that browser downloads are waiting in [`AppState`].
pub const CAPTURE_EVENT: &str = "ferry://capture";

/// Accepts connections from `ferry-host` for as long as the app runs.
pub fn listen(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let listener = match socket_name().and_then(|name| {
            ListenerOptions::new()
                .name(name)
                .reclaim_name(true)
                .try_overwrite(true)
                .create_tokio()
        }) {
            Ok(listener) => listener,
            Err(e) => {
                tracing::warn!(error = %e, "browser integration is off: could not open the socket");
                return;
            }
        };
        loop {
            match listener.accept().await {
                Ok(conn) => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = serve(&app, conn).await {
                            tracing::debug!(error = %e, "browser connection failed");
                        }
                    });
                }
                Err(e) => tracing::warn!(error = %e, "could not accept a browser connection"),
            }
        }
    });
}

async fn serve(app: &AppHandle, conn: Stream) -> io::Result<()> {
    let mut lines = BufReader::new(&conn).lines();
    let mut send = &conn;
    while let Some(line) = lines.next_line().await? {
        let reply = match serde_json::from_str::<Message>(&line) {
            Ok(Message::Ping) => Reply::ok(true),
            Ok(Message::Download(capture)) => receive(app, capture),
            Err(e) => Reply::error(format!("unknown message: {e}")),
        };
        let mut out = serde_json::to_vec(&reply)?;
        out.push(b'\n');
        send.write_all(&out).await?;
    }
    Ok(())
}

fn receive(app: &AppHandle, capture: Capture) -> Reply {
    let url = capture.url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ftp://")) {
        return Reply::error("Ferry can only download http, https and ftp links");
    }
    app.state::<AppState>().push_capture(capture);
    crate::show_main_window(app);
    let _ = app.emit(CAPTURE_EVENT, ());
    Reply::ok(true)
}

/// Registers `ferry-host` (next to the app's executable) with Chrome and Edge.
/// Run on every start, so the registration follows the app when it moves.
pub fn register_native_host(manifest_dir: &Path) {
    let host = match std::env::current_exe() {
        Ok(exe) => exe.with_file_name(if cfg!(windows) {
            "ferry-host.exe"
        } else {
            "ferry-host"
        }),
        Err(e) => {
            tracing::warn!(error = %e, "could not find the app's own path");
            return;
        }
    };
    if !host.is_file() {
        // In development: `cargo build -p ferry-host` puts it next to the app.
        tracing::warn!(path = %host.display(), "ferry-host not found; browser integration is off");
        return;
    }
    if let Err(e) = write_and_register(&host, manifest_dir) {
        tracing::warn!(error = %e, "could not register the browser integration");
    }
}

/// The extension in each browser's store, as `(registry path, ID, update URL)`.
/// See docs/store-listing.md.
#[cfg(all(windows, not(debug_assertions)))]
const STORE_EXTENSIONS: &[(&str, &str, &str)] = &[
    (
        r"Google\Chrome",
        "ofkibiokjdlboldiiplkllehifkjbihd",
        "https://clients2.google.com/service/update2/crx",
    ),
    // Edge Add-ons: added once the listing exists.
];

/// Asks Chrome and Edge to offer the store extension: on their next start they
/// show "Ferry was added" and the user turns it on (or doesn't, and they won't
/// ask again). Release builds only, so development keeps the unpacked one.
pub fn offer_store_extension() {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for (browser, id, update_url) in STORE_EXTENSIONS {
            let result = hkcu
                .create_subkey(format!(r"Software\{browser}\Extensions\{id}"))
                .and_then(|(key, _)| key.set_value("update_url", update_url));
            if let Err(e) = result {
                tracing::warn!(browser, error = %e, "could not offer the browser extension");
            }
        }
    }
}

fn manifest(host: &Path) -> serde_json::Value {
    serde_json::json!({
        "name": HOST_NAME,
        "description": "Ferry Download Center",
        "path": host,
        "type": "stdio",
        "allowed_origins": EXTENSION_IDS
            .iter()
            .map(|id| format!("chrome-extension://{id}/"))
            .collect::<Vec<_>>(),
    })
}

fn write_manifest(host: &Path, path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(&manifest(host)).map_err(io::Error::other)?;
    std::fs::write(path, json)
}

/// Windows: one manifest file, pointed to from each browser's registry key.
#[cfg(windows)]
fn write_and_register(host: &Path, manifest_dir: &Path) -> io::Result<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    let path = manifest_dir.join(format!("{HOST_NAME}.json"));
    write_manifest(host, &path)?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for browser in [r"Google\Chrome", r"Microsoft\Edge"] {
        let (key, _) = hkcu.create_subkey(format!(
            r"Software\{browser}\NativeMessagingHosts\{HOST_NAME}"
        ))?;
        key.set_value("", &path.as_os_str())?;
    }
    Ok(())
}

/// Linux and macOS: each browser reads manifests from a folder of its own.
#[cfg(not(windows))]
fn write_and_register(host: &Path, _manifest_dir: &Path) -> io::Result<()> {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return Err(io::Error::other("HOME is not set"));
    };
    let dirs: &[&str] = if cfg!(target_os = "macos") {
        &[
            "Library/Application Support/Google/Chrome/NativeMessagingHosts",
            "Library/Application Support/Microsoft Edge/NativeMessagingHosts",
        ]
    } else {
        &[
            ".config/google-chrome/NativeMessagingHosts",
            ".config/microsoft-edge/NativeMessagingHosts",
        ]
    };
    for dir in dirs {
        write_manifest(host, &home.join(dir).join(format!("{HOST_NAME}.json")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_allows_only_our_extension() {
        let m = manifest(Path::new("C:/Ferry/ferry-host.exe"));
        assert_eq!(m["name"], HOST_NAME);
        assert_eq!(m["type"], "stdio");
        assert_eq!(
            m["allowed_origins"][0],
            "chrome-extension://hieofjpdefhpcbpmbhigbnefpimhmlop/"
        );
    }

    #[test]
    fn manifest_is_written() {
        let dir = std::env::temp_dir().join(format!("ferry-manifest-{}", std::process::id()));
        let path = dir.join("nested").join("host.json");
        write_manifest(Path::new("/opt/ferry/ferry-host"), &path).unwrap();
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["path"], "/opt/ferry/ferry-host");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
