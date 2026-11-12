//! Helpers for picking safe, unique file names.

use percent_encoding::percent_decode_str;
use reqwest::Url;

/// Extracts the file name from a `Content-Disposition` header value.
/// `filename*` (RFC 5987) wins over plain `filename`.
pub fn from_content_disposition(value: &str) -> Option<String> {
    let mut plain = None;
    for part in value.split(';') {
        let Some((key, val)) = part.trim().split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let val = val.trim();
        if key == "filename*" {
            let encoded = val.rsplit_once("''").map_or(val, |(_, rest)| rest);
            if let Ok(name) = percent_decode_str(encoded.trim_matches('"')).decode_utf8() {
                if !name.is_empty() {
                    return Some(name.into_owned());
                }
            }
        } else if key == "filename" {
            let name = val.trim_matches('"');
            if !name.is_empty() {
                plain = Some(name.to_string());
            }
        }
    }
    plain
}

/// Uses the last path segment of a URL as a file name.
pub fn from_url(url: &Url) -> Option<String> {
    let last = url.path_segments()?.next_back()?;
    let name = percent_decode_str(last).decode_utf8_lossy();
    (!name.is_empty()).then(|| name.into_owned())
}

/// Turns an arbitrary name into one that is valid on Windows, Linux and macOS.
pub fn sanitize(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned: String = cleaned
        .trim()
        .trim_end_matches(['.', ' '])
        .chars()
        .take(200)
        .collect();
    if cleaned.is_empty() {
        return "download".into();
    }
    let stem = cleaned
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        format!("_{cleaned}")
    } else {
        cleaned
    }
}

/// Returns `name`, or `name (1).ext`, `name (2).ext`... the first one not `taken`.
pub fn unique_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (1..)
        .map(|i| format!("{stem} ({i}){ext}"))
        .find(|candidate| !taken(candidate))
        .expect("an unused name always exists")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_disposition() {
        assert_eq!(
            from_content_disposition(r#"attachment; filename="report.pdf""#).as_deref(),
            Some("report.pdf")
        );
        assert_eq!(
            from_content_disposition(
                r#"attachment; filename="fallback.txt"; filename*=UTF-8''%C3%B6zet%20dosya.txt"#
            )
            .as_deref(),
            Some("özet dosya.txt")
        );
        assert_eq!(from_content_disposition("inline"), None);
    }

    #[test]
    fn url_names() {
        let url = Url::parse("https://example.com/files/my%20file.zip?x=1").unwrap();
        assert_eq!(from_url(&url).as_deref(), Some("my file.zip"));
        assert_eq!(from_url(&Url::parse("https://example.com/").unwrap()), None);
    }

    #[test]
    fn sanitizing() {
        assert_eq!(sanitize("../../etc/passwd"), "passwd");
        assert_eq!(sanitize(r"a:b*c?.txt"), "a_b_c_.txt");
        assert_eq!(sanitize("name. "), "name");
        assert_eq!(sanitize("CON.txt"), "_CON.txt");
        assert_eq!(sanitize("COM1"), "_COM1");
        assert_eq!(sanitize("  "), "download");
    }

    #[test]
    fn unique_names() {
        let taken = ["a.zip", "a (1).zip", "noext"];
        let is_taken = |n: &str| taken.contains(&n);
        assert_eq!(unique_name("b.zip", is_taken), "b.zip");
        assert_eq!(unique_name("a.zip", is_taken), "a (2).zip");
        assert_eq!(unique_name("noext", is_taken), "noext (1)");
    }
}
