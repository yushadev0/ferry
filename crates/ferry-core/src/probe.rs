use reqwest::{
    header::{HeaderMap, CONTENT_DISPOSITION, CONTENT_RANGE, ETAG, LAST_MODIFIED, RANGE},
    Client, StatusCode,
};

use crate::{error::check_status, filename, Result};

/// What the server tells us about a file before downloading it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeInfo {
    /// Total size in bytes, when the server reports it.
    pub size: Option<u64>,
    /// Whether the server honours byte ranges (segmented download and resume).
    pub resumable: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// File name suggested by `Content-Disposition` or the final URL.
    pub filename: Option<String>,
}

/// Asks the server for the first byte of `url` to learn its size, range
/// support, validators and file name.
pub async fn probe(client: &Client, url: &str, headers: &HeaderMap) -> Result<ProbeInfo> {
    let resp = client
        .get(url)
        .headers(headers.clone())
        .header(RANGE, "bytes=0-0")
        .send()
        .await?;
    let status = resp.status();
    check_status(status)?;

    let h = resp.headers();
    let text = |name| h.get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);

    let (size, resumable) = if status == StatusCode::PARTIAL_CONTENT {
        match text(CONTENT_RANGE).as_deref().and_then(parse_content_range) {
            Some((_, _, Some(total))) => (Some(total), true),
            _ => (None, false),
        }
    } else {
        (resp.content_length(), false)
    };

    let filename = text(CONTENT_DISPOSITION)
        .as_deref()
        .and_then(filename::from_content_disposition)
        .or_else(|| filename::from_url(resp.url()));

    Ok(ProbeInfo {
        size,
        resumable,
        etag: text(ETAG),
        last_modified: text(LAST_MODIFIED),
        filename,
    })
}

/// Parses `bytes <start>-<end>/<total|*>`.
pub(crate) fn parse_content_range(value: &str) -> Option<(u64, u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes ")?;
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let total = match total.trim() {
        "*" => None,
        t => Some(t.parse().ok()?),
    };
    Some((start.trim().parse().ok()?, end.trim().parse().ok()?, total))
}

#[cfg(test)]
mod tests {
    use super::parse_content_range;

    #[test]
    fn content_range() {
        assert_eq!(
            parse_content_range("bytes 0-0/1234"),
            Some((0, 0, Some(1234)))
        );
        assert_eq!(parse_content_range("bytes 5-9/*"), Some((5, 9, None)));
        assert_eq!(parse_content_range("items 0-1/2"), None);
    }
}
