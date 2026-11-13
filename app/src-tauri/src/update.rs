//! Keeps downloads going across an app update. Installing an update closes
//! Ferry, so the downloads running at that moment are paused (their progress
//! saved) and remembered in a small file; the updated app starts them again.

use std::path::{Path, PathBuf};

use ferry_core::{DownloadId, Engine};

const FILE: &str = "resume-after-update.json";

pub fn resume_file(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// Pauses the running downloads and remembers them. Returns their ids.
pub async fn prepare(engine: &Engine, file: &Path) -> std::io::Result<Vec<DownloadId>> {
    let ids = engine.active_ids();
    std::fs::write(file, serde_json::to_vec(&ids)?)?;
    for id in &ids {
        let _ = engine.pause(*id).await;
    }
    Ok(ids)
}

/// Starts the remembered downloads again (after an update, or when the
/// update failed) and forgets them.
pub fn resume(engine: &Engine, file: &Path) {
    let Ok(raw) = std::fs::read(file) else { return };
    let _ = std::fs::remove_file(file);
    let ids: Vec<DownloadId> = serde_json::from_slice(&raw).unwrap_or_default();
    for id in ids {
        if let Err(e) = engine.start(id) {
            tracing::warn!(id, error = %e, "could not resume a download after the update");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferry_core::EngineConfig;

    #[tokio::test]
    async fn remembers_and_forgets_downloads() {
        let dir = std::env::temp_dir().join(format!("ferry-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = resume_file(&dir);
        let engine = Engine::in_memory(EngineConfig::default()).unwrap();

        assert!(prepare(&engine, &file).await.unwrap().is_empty());
        assert!(file.exists(), "the list is written even when empty");
        resume(&engine, &file);
        assert!(!file.exists(), "resuming forgets the list");
        // Nothing to resume is not an error.
        resume(&engine, &file);

        std::fs::write(&file, "[12345]").unwrap();
        resume(&engine, &file); // unknown ids are skipped
        assert!(!file.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
