//! Commands the web UI calls through `invoke`.

use std::path::PathBuf;

use ferry_core::{DownloadId, DownloadInfo, ProbeInfo, Request, Status};
use serde::Deserialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::{settings::Settings, AppState, QueueState};

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn list_downloads(state: State<'_, AppState>) -> CmdResult<Vec<DownloadInfo>> {
    state.engine.list().map_err(err)
}

#[tauri::command]
pub fn get_download(state: State<'_, AppState>, id: DownloadId) -> CmdResult<DownloadInfo> {
    state.engine.get(id).map_err(err)
}

#[tauri::command]
pub async fn inspect_url(
    state: State<'_, AppState>,
    url: String,
    headers: Option<Vec<(String, String)>>,
) -> CmdResult<ProbeInfo> {
    state
        .engine
        .probe(url.trim(), &headers.unwrap_or_default())
        .await
        .map_err(err)
}

/// Downloads caught in the browser since the last call, oldest first.
#[tauri::command]
pub fn take_captures(state: State<'_, AppState>) -> Vec<ferry_host::Capture> {
    state.take_captures()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDownload {
    url: String,
    dir: Option<PathBuf>,
    file_name: Option<String>,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default = "yes")]
    start: bool,
}

fn yes() -> bool {
    true
}

#[tauri::command]
pub async fn add_download(state: State<'_, AppState>, req: AddDownload) -> CmdResult<DownloadId> {
    let dir = match req.dir {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => state.settings().download_dir,
    };
    let id = state
        .engine
        .add(Request {
            url: req.url.trim().to_string(),
            dir,
            file_name: req.file_name.filter(|n| !n.trim().is_empty()),
            headers: req.headers,
        })
        .await
        .map_err(err)?;
    if req.start {
        // Through the queue, which starts it at once when there is room.
        state.engine.start(id).map_err(err)?;
    }
    Ok(id)
}

// Async so it runs on the Tokio runtime that download tasks are spawned on.
#[tauri::command]
pub async fn start_downloads(state: State<'_, AppState>, ids: Vec<DownloadId>) -> CmdResult<()> {
    for id in ids {
        let info = state.engine.get(id).map_err(err)?;
        if info.status != Status::Completed {
            state.engine.start(id).map_err(err)?;
        }
    }
    Ok(())
}

/// Starts downloads right away, ignoring the queue limit and schedule.
#[tauri::command]
pub async fn start_now(state: State<'_, AppState>, ids: Vec<DownloadId>) -> CmdResult<()> {
    for id in ids {
        let info = state.engine.get(id).map_err(err)?;
        if info.status != Status::Completed {
            state.engine.start_now(id).map_err(err)?;
        }
    }
    Ok(())
}

/// Starts downloads at `at` (Unix seconds) through the queue.
#[tauri::command]
pub async fn schedule_downloads(
    state: State<'_, AppState>,
    ids: Vec<DownloadId>,
    at: i64,
) -> CmdResult<()> {
    for id in ids {
        state.engine.schedule(id, at).await.map_err(err)?;
    }
    state.wake_scheduler();
    Ok(())
}

/// The system icon for a file extension, as a PNG data URL.
#[tauri::command]
pub fn file_icon(state: State<'_, AppState>, ext: String) -> Option<String> {
    state.file_icons.get(&ext)
}

#[tauri::command]
pub fn queue_state(state: State<'_, AppState>) -> QueueState {
    state.queue_state()
}

#[tauri::command]
pub async fn pause_downloads(state: State<'_, AppState>, ids: Vec<DownloadId>) -> CmdResult<()> {
    for id in ids {
        state.engine.pause(id).await.map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn remove_downloads(
    state: State<'_, AppState>,
    ids: Vec<DownloadId>,
    delete_files: bool,
) -> CmdResult<()> {
    for id in ids {
        state.engine.remove(id, delete_files).await.map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_file(app: AppHandle, state: State<'_, AppState>, id: DownloadId) -> CmdResult<()> {
    let info = state.engine.get(id).map_err(err)?;
    if info.status != Status::Completed {
        let lang = state.settings().language.resolved();
        return Err(crate::i18n::text(lang, crate::i18n::Text::NotFinished).into());
    }
    let path = info.dir.join(&info.file_name);
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(err)
}

#[tauri::command]
pub fn show_in_folder(app: AppHandle, state: State<'_, AppState>, id: DownloadId) -> CmdResult<()> {
    let info = state.engine.get(id).map_err(err)?;
    let file = info.dir.join(&info.file_name);
    if file.exists() {
        app.opener().reveal_item_in_dir(file).map_err(err)
    } else {
        app.opener()
            .open_path(info.dir.to_string_lossy(), None::<&str>)
            .map_err(err)
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<Settings> {
    let saved = state.update_settings(settings).map_err(err)?;
    crate::apply_theme(&app, &saved);
    crate::apply_language(&app, &saved);
    Ok(saved)
}
