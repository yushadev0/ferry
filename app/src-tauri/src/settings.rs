use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Light or dark appearance; `System` follows the Windows setting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    /// The window theme to request; `None` lets the OS decide.
    pub fn window_theme(self) -> Option<tauri::Theme> {
        match self {
            Theme::System => None,
            Theme::Light => Some(tauri::Theme::Light),
            Theme::Dark => Some(tauri::Theme::Dark),
        }
    }
}

/// Interface language; `System` follows Windows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    System,
    En,
    Tr,
}

impl Language {
    /// The language to show: English or Turkish, resolving `System`.
    pub fn resolved(self) -> crate::i18n::Lang {
        use crate::i18n::Lang;
        match self {
            Language::En => Lang::En,
            Language::Tr => Lang::Tr,
            Language::System => sys_locale::get_locale()
                .filter(|l| l.to_lowercase().starts_with("tr"))
                .map_or(Lang::En, |_| Lang::Tr),
        }
    }
}

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
    /// Downloads the queue runs at the same time (1-10).
    pub max_active: usize,
    /// Total speed limit in KiB/s; 0 means unlimited.
    pub speed_limit_kib: u64,
    /// Only run the queue between `schedule_start` and `schedule_stop`.
    pub schedule_enabled: bool,
    /// Local time, "HH:MM".
    pub schedule_start: String,
    /// Local time, "HH:MM". Earlier than the start means the window runs overnight.
    pub schedule_stop: String,
    pub theme: Theme,
    pub language: Language,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: PathBuf::from("."),
            segments: 8,
            notify_on_complete: true,
            close_to_tray: true,
            max_active: 3,
            speed_limit_kib: 0,
            schedule_enabled: false,
            schedule_start: "02:00".into(),
            schedule_stop: "08:00".into(),
            theme: Theme::System,
            language: Language::System,
        }
    }
}

impl Settings {
    pub fn load(path: &Path, default_dir: PathBuf) -> Self {
        let settings: Settings = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let mut settings = settings.normalized();
        if settings.download_dir.as_os_str().is_empty() || settings.download_dir == Path::new(".") {
            settings.download_dir = default_dir;
        }
        settings
    }

    /// Clamps values into their valid ranges.
    pub fn normalized(mut self) -> Self {
        let defaults = Settings::default();
        self.segments = self.segments.clamp(1, 16);
        self.max_active = self.max_active.clamp(1, 10);
        if parse_time(&self.schedule_start).is_none() {
            self.schedule_start = defaults.schedule_start;
        }
        if parse_time(&self.schedule_stop).is_none() {
            self.schedule_stop = defaults.schedule_stop;
        }
        self
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Whether the queue may run at `minute_of_day` (0-1439, local time).
    pub fn in_schedule(&self, minute_of_day: u32) -> bool {
        if !self.schedule_enabled {
            return true;
        }
        let (Some(start), Some(stop)) = (
            parse_time(&self.schedule_start),
            parse_time(&self.schedule_stop),
        ) else {
            return true;
        };
        match start.cmp(&stop) {
            std::cmp::Ordering::Equal => true,
            std::cmp::Ordering::Less => (start..stop).contains(&minute_of_day),
            std::cmp::Ordering::Greater => minute_of_day >= start || minute_of_day < stop,
        }
    }
}

/// Parses "HH:MM" into minutes after midnight.
pub fn parse_time(value: &str) -> Option<u32> {
    let (h, m) = value.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule(start: &str, stop: &str) -> Settings {
        Settings {
            schedule_enabled: true,
            schedule_start: start.into(),
            schedule_stop: stop.into(),
            ..Settings::default()
        }
    }

    #[test]
    fn parses_times() {
        assert_eq!(parse_time("02:30"), Some(150));
        assert_eq!(parse_time("23:59"), Some(1439));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_time("7"), None);
    }

    #[test]
    fn daytime_window() {
        let s = schedule("09:00", "17:00");
        assert!(!s.in_schedule(8 * 60 + 59));
        assert!(s.in_schedule(9 * 60));
        assert!(s.in_schedule(16 * 60 + 59));
        assert!(!s.in_schedule(17 * 60));
    }

    #[test]
    fn overnight_window() {
        let s = schedule("23:00", "06:00");
        assert!(s.in_schedule(23 * 60 + 30));
        assert!(s.in_schedule(60));
        assert!(!s.in_schedule(6 * 60));
        assert!(!s.in_schedule(12 * 60));
    }

    #[test]
    fn disabled_or_equal_times_always_run() {
        assert!(Settings::default().in_schedule(12 * 60));
        assert!(schedule("05:00", "05:00").in_schedule(12 * 60));
    }

    #[test]
    fn bad_values_are_fixed() {
        let s = Settings {
            max_active: 0,
            segments: 99,
            schedule_start: "nonsense".into(),
            ..Settings::default()
        }
        .normalized();
        assert_eq!((s.max_active, s.segments), (1, 16));
        assert_eq!(s.schedule_start, "02:00");
    }
}
