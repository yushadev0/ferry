//! What the browser extension, `ferry-host` and the Ferry app say to each other.
//!
//! The extension talks to `ferry-host` through Chrome's native messaging
//! (stdin/stdout, each message a 32-bit length then JSON). `ferry-host` passes
//! each [`Message`] on to the running app over a local socket as one line of
//! JSON and relays the app's one-line [`Reply`].

use std::io;

use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, Name, NameType, ToFsName, ToNsName,
};
use serde::{Deserialize, Serialize};

/// Native messaging host name the extension connects to.
pub const HOST_NAME: &str = "app.yusa.ferry";

/// Extensions allowed to use the host. The first is the unpacked build in
/// `extension/`, whose ID is fixed by the `key` in its manifest.
pub const EXTENSION_IDS: &[&str] = &["hieofjpdefhpcbpmbhigbnefpimhmlop"];

/// A request from the extension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Message {
    /// Is Ferry installed and running? Never starts the app.
    Ping,
    /// Hand a download to Ferry, starting the app if needed.
    Download(Capture),
}

/// A download caught in the browser, with what it needs to fetch it the same way.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Capture {
    pub url: String,
    /// Name the browser picked, if any.
    pub file_name: Option<String>,
    /// Size the browser saw, in bytes.
    pub file_size: Option<u64>,
    pub referrer: Option<String>,
    /// `Cookie` header value for the URL.
    pub cookies: Option<String>,
    pub user_agent: Option<String>,
}

impl Capture {
    /// HTTP headers that make Ferry's requests look like the browser's.
    pub fn headers(&self) -> Vec<(String, String)> {
        [
            ("Referer", &self.referrer),
            ("Cookie", &self.cookies),
            ("User-Agent", &self.user_agent),
        ]
        .into_iter()
        .filter_map(|(name, value)| {
            let value = value.as_deref()?.trim();
            (!value.is_empty()).then(|| (name.to_string(), value.to_string()))
        })
        .collect()
    }
}

/// The answer to a [`Message`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Reply {
    pub ok: bool,
    /// Whether the app is running (answers to [`Message::Ping`]).
    pub running: bool,
    /// Version of the app, or of the host when the app isn't running.
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Reply {
    pub fn ok(running: bool) -> Self {
        Self {
            ok: true,
            running,
            version: env!("CARGO_PKG_VERSION").into(),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            ..Self::ok(false)
        }
    }
}

/// The app's local socket, one per user so two people on one PC don't meet.
pub fn socket_name() -> io::Result<Name<'static>> {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default();
    let user: String = user
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let base = format!("{HOST_NAME}.{user}.sock");
    if GenericNamespaced::is_supported() {
        Ok(base.to_ns_name::<GenericNamespaced>()?.into_owned())
    } else {
        // macOS has no socket namespace; use a file in the per-user temp dir.
        Ok(std::env::temp_dir()
            .join(base)
            .to_fs_name::<GenericFilePath>()?
            .into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_match_the_extension() {
        let ping: Message = serde_json::from_str(r#"{"type":"ping"}"#).unwrap();
        assert_eq!(ping, Message::Ping);

        let json = r#"{"type":"download","url":"https://example.com/a.zip",
            "fileName":"a.zip","fileSize":42,"referrer":"https://example.com/",
            "cookies":"s=1","userAgent":"UA","unknown":true}"#;
        let Message::Download(capture) = serde_json::from_str(json).unwrap() else {
            panic!("not a download");
        };
        assert_eq!(capture.file_size, Some(42));
        assert_eq!(
            capture.headers(),
            vec![
                ("Referer".into(), "https://example.com/".into()),
                ("Cookie".into(), "s=1".into()),
                ("User-Agent".into(), "UA".into()),
            ]
        );
    }

    #[test]
    fn empty_headers_are_left_out() {
        let capture = Capture {
            url: "https://example.com/".into(),
            cookies: Some("  ".into()),
            ..Capture::default()
        };
        assert!(capture.headers().is_empty());
    }

    #[test]
    fn errors_say_why() {
        let json = serde_json::to_string(&Reply::error("nope")).unwrap();
        assert!(json.contains(r#""ok":false"#) && json.contains(r#""error":"nope""#));
        assert!(!serde_json::to_string(&Reply::ok(true))
            .unwrap()
            .contains("error"));
    }
}
