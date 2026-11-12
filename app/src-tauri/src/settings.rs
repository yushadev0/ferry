use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// User preferences, stored as JSON in the app config directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub download_dir: PathBuf,
    /// Parallel connections per download (1-16).
    pub segments: usize,
    pub notify_on_complete: bool,
    /// Hide to the tray instead of quitting when the window is closed.
    pub close_to_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: PathBuf::from("."),
            segments: 8,
            notify_on_complete: true,
            close_to_tray: true,
        }
    }
}

impl Settings {
    pub fn load(path: &Path, default_dir: PathBuf) -> Self {
        let mut settings: Settings = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| Settings {
                download_dir: default_dir.clone(),
                ..Settings::default()
            });
        if settings.download_dir.as_os_str().is_empty() || settings.download_dir == Path::new(".") {
            settings.download_dir = default_dir;
        }
        settings.segments = settings.segments.clamp(1, 16);
        settings
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }
}
