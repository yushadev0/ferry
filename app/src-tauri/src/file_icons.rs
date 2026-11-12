//! The icon the system shows for a file type, so the list looks like Explorer.
//!
//! Windows and macOS hand out icons for an existing path, so an empty sample
//! file per extension is kept in the app cache and asked about instead.

use std::{collections::HashMap, path::PathBuf, sync::Mutex};

/// Extensions whose sample files the shell treats specially; use the fallback.
const SKIPPED: &[&str] = &["lnk", "url", "desktop", "library-ms"];

pub struct FileIcons {
    dir: PathBuf,
    cache: Mutex<HashMap<String, Option<String>>>,
}

impl FileIcons {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            dir: cache_dir.join("icon-samples"),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// A PNG data URL for files with extension `ext`, or `None` when the system
    /// has none to give (the UI then draws its own).
    pub fn get(&self, ext: &str) -> Option<String> {
        let ext = normalize(ext)?;
        if let Some(hit) = self.cache.lock().ok()?.get(&ext) {
            return hit.clone();
        }
        let icon = self.load(&ext);
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(ext, icon.clone());
        }
        icon
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn load(&self, ext: &str) -> Option<String> {
        std::fs::create_dir_all(&self.dir).ok()?;
        let sample = self.dir.join(format!("sample.{ext}"));
        if !sample.exists() {
            std::fs::write(&sample, []).ok()?;
        }
        let icon = file_icon_provider::get_file_icon(&sample, 64).ok()?;
        png_data_url(icon.width, icon.height, &icon.pixels)
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    fn load(&self, _ext: &str) -> Option<String> {
        // The Linux backend must run on the GTK main thread; the UI's own
        // icons are used there instead.
        let _ = &self.dir;
        None
    }
}

/// Lower-case letters and digits only, at most 12 of them.
fn normalize(ext: &str) -> Option<String> {
    let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    let valid =
        !ext.is_empty() && ext.len() <= 12 && ext.chars().all(|c| c.is_ascii_alphanumeric());
    (valid && !SKIPPED.contains(&ext.as_str())).then_some(ext)
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn png_data_url(width: u32, height: u32, rgba: &[u8]) -> Option<String> {
    use base64::Engine as _;
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_extensions() {
        assert_eq!(normalize(".ZIP").as_deref(), Some("zip"));
        assert_eq!(normalize("tar"), Some("tar".into()));
        assert_eq!(normalize("../x"), None);
        assert_eq!(normalize(""), None);
        assert_eq!(normalize("lnk"), None);
    }

    #[test]
    fn encodes_png_data_urls() {
        let url = png_data_url(1, 1, &[255, 0, 0, 255]).unwrap();
        assert!(url.starts_with("data:image/png;base64,iVBORw0KGgo"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_has_icons_for_common_types() {
        let dir = tempfile_dir();
        let icons = FileIcons::new(dir.clone());
        assert!(icons.get("txt").is_some());
        assert!(icons.get("zip").is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(windows)]
    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferry-icons-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
