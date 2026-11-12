use std::time::Duration;

use reqwest::{
    header::{HeaderMap, RETRY_AFTER},
    StatusCode,
};

/// Errors produced by the download engine.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {}", describe_http(.0))]
    Http(#[from] reqwest::Error),
    #[error("server returned HTTP {0}")]
    Status(u16),
    #[error("the link has expired or access was denied (HTTP {0})")]
    LinkExpired(u16),
    /// HTTP 429 or 503: the server wants fewer or slower connections.
    #[error("the server is limiting connections (HTTP {code})")]
    Busy {
        code: u16,
        retry_after: Option<Duration>,
    },
    #[error("server did not return the requested byte range")]
    BadRange,
    #[error("connection closed before the segment was finished")]
    Truncated,
    #[error("no data received for too long")]
    Timeout,
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("download {0} not found")]
    NotFound(i64),
    #[error("invalid state: {0}")]
    InvalidState(&'static str),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// reqwest's own message ("error sending request for url ...") hides the cause,
/// so name the most specific one (timeout, DNS, TLS...) first.
fn describe_http(err: &reqwest::Error) -> String {
    let mut cause = None;
    let mut source = std::error::Error::source(err);
    while let Some(inner) = source {
        cause = Some(inner.to_string());
        source = inner.source();
    }
    let kind = if err.is_timeout() {
        Some("timed out")
    } else if err.is_connect() {
        Some("could not connect to the server")
    } else {
        None
    };
    match (kind, cause) {
        (Some(kind), Some(cause)) => format!("{kind} ({cause})"),
        (Some(kind), None) => kind.to_string(),
        (None, Some(cause)) => format!("{err} ({cause})"),
        (None, None) => err.to_string(),
    }
}

impl Error {
    /// Whether retrying the same request may succeed.
    pub(crate) fn is_retryable(&self) -> bool {
        match self {
            Error::Http(_) | Error::Truncated | Error::Timeout | Error::Busy { .. } => true,
            Error::Status(code) => *code >= 500 || *code == 429 || *code == 408,
            _ => false,
        }
    }
}

/// Maps an HTTP status to an error, if it is not a success.
pub(crate) fn check_status(status: StatusCode, headers: &HeaderMap) -> Result<()> {
    match status.as_u16() {
        200..=299 => Ok(()),
        code @ (429 | 503) => Err(Error::Busy {
            code,
            retry_after: headers
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok())
                .map(Duration::from_secs),
        }),
        code @ (401 | 403 | 410) => Err(Error::LinkExpired(code)),
        code => Err(Error::Status(code)),
    }
}
