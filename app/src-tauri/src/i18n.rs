//! The few texts the Rust side shows itself: the tray menu, notifications and
//! a command error. The UI's own texts live in `src/lib/locales`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Tr,
}

#[derive(Debug, Clone, Copy)]
pub enum Text {
    TrayShow,
    TrayPauseAll,
    TrayQuit,
    DownloadComplete,
    DownloadFailed,
    UnknownError,
    NotFinished,
}

pub fn text(lang: Lang, text: Text) -> &'static str {
    use Text::*;
    match (lang, text) {
        (Lang::En, TrayShow) => "Show Ferry",
        (Lang::En, TrayPauseAll) => "Pause all",
        (Lang::En, TrayQuit) => "Quit",
        (Lang::En, DownloadComplete) => "Download complete",
        (Lang::En, DownloadFailed) => "Download failed",
        (Lang::En, UnknownError) => "unknown error",
        (Lang::En, NotFinished) => "The download is not finished yet.",
        (Lang::Tr, TrayShow) => "Ferry'yi göster",
        (Lang::Tr, TrayPauseAll) => "Tümünü duraklat",
        (Lang::Tr, TrayQuit) => "Çıkış",
        (Lang::Tr, DownloadComplete) => "İndirme tamamlandı",
        (Lang::Tr, DownloadFailed) => "İndirme başarısız",
        (Lang::Tr, UnknownError) => "bilinmeyen hata",
        (Lang::Tr, NotFinished) => "İndirme henüz bitmedi.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_languages_have_every_text() {
        use Text::*;
        for t in [
            TrayShow,
            TrayPauseAll,
            TrayQuit,
            DownloadComplete,
            DownloadFailed,
            UnknownError,
            NotFinished,
        ] {
            assert!(!text(Lang::En, t).is_empty());
            assert!(!text(Lang::Tr, t).is_empty());
            assert_ne!(text(Lang::En, t), text(Lang::Tr, t));
        }
    }
}
