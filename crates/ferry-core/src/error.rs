use reqwest::StatusCode;

/// Errors produced by the download engine.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned HTTP {0}")]
    Status(u16),
    #[error("the link has expired or access was denied (HTTP {0})")]
    LinkExpired(u16),
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

impl Error {
    /// Whether retrying the same request may succeed.
    pub(crate) fn is_retryable(&self) -> bool {
        match self {
            Error::Http(_) | Error::Truncated | Error::Timeout => true,
            Error::Status(code) => *code >= 500 || *code == 429 || *code == 408,
            _ => false,
        }
    }
}

/// Maps an HTTP status to an error, if it is not a success.
pub(crate) fn check_status(status: StatusCode) -> Result<()> {
    match status.as_u16() {
        200..=299 => Ok(()),
        code @ (401 | 403 | 410) => Err(Error::LinkExpired(code)),
        code => Err(Error::Status(code)),
    }
}
