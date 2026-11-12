//! Ferry Download Center desktop app: wires `ferry-core` to a Tauri web UI.

mod commands;
mod settings;

use std::{path::PathBuf, sync::Mutex, time::Duration};

use chrono::Timelike;

use ferry_core::{Engine, EngineConfig, Event, Status};
use serde::Serialize;
use settings::Settings;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Notify;

/// Name of the event that carries engine events to the UI.
const ENGINE_EVENT: &str = "ferry://event";
/// Name of the event that carries [`QueueState`] changes to the UI.
const QUEUE_EVENT: &str = "ferry://queue";

/// Whether the queue runs, as shown in the status bar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueState {
    pub running: bool,
    /// When the schedule lets the queue run again ("HH:MM"), if it is held.
    pub resumes_at: Option<String>,
}

pub struct AppState {
    pub engine: Engine,
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    /// Wakes the scheduler when the settings change.
    schedule_changed: Notify,
}

impl AppState {
    pub fn settings(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn update_settings(&self, new: Settings) -> std::io::Result<Settings> {
        let new = new.normalized();
        new.save(&self.settings_path)?;
        self.engine.set_config(engine_config(&new));
        *self.settings.lock().unwrap_or_else(|e| e.into_inner()) = new.clone();
        self.schedule_changed.notify_one();
        Ok(new)
    }

    pub fn queue_state(&self) -> QueueState {
        let settings = self.settings();
        let running = self.engine.queue_running();
        QueueState {
            running,
            resumes_at: (!running && settings.schedule_enabled).then_some(settings.schedule_start),
        }
    }
}

fn engine_config(settings: &Settings) -> EngineConfig {
    EngineConfig {
        segments: settings.segments,
        max_active: settings.max_active,
        speed_limit: settings.speed_limit_kib * 1024,
        ..EngineConfig::default()
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Saves progress of running downloads, then exits.
fn quit(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        app.state::<AppState>().engine.shutdown().await;
        app.exit(0);
    });
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Ferry", true, None::<&str>)?;
    let pause_all = MenuItem::with_id(app, "pause_all", "Pause all", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &pause_all, &quit_item])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Ferry Download Center")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "pause_all" => {
                let engine = app.state::<AppState>().engine.clone();
                tauri::async_runtime::spawn(async move { engine.pause_all().await });
            }
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Runs the queue only inside the schedule window, checking every 15 seconds
/// and whenever the settings change.
fn run_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let now = chrono::Local::now();
            let wanted = state.settings().in_schedule(now.hour() * 60 + now.minute());
            if wanted != state.engine.queue_running() {
                tracing::info!(running = wanted, "schedule changed the queue");
            }
            // Also starts queued downloads after launch.
            state.engine.set_queue_running(wanted).await;
            let _ = app.emit(QUEUE_EVENT, state.queue_state());
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(15)) => {}
                _ = state.schedule_changed.notified() => {}
            }
        }
    });
}

/// Forwards engine events to the UI and shows notifications.
fn forward_events(app: AppHandle, engine: Engine) {
    let mut rx = engine.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            let event = match rx.recv().await {
                Ok(event) => event,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            let _ = app.emit(ENGINE_EVENT, &event);

            if let Event::Status { id, status, error } = &event {
                let state = app.state::<AppState>();
                if !state.settings().notify_on_complete {
                    continue;
                }
                let Ok(info) = engine.get(*id) else { continue };
                let (title, body) = match status {
                    Status::Completed => ("Download complete", info.file_name),
                    Status::Failed => (
                        "Download failed",
                        format!(
                            "{}: {}",
                            info.file_name,
                            error.as_deref().unwrap_or("unknown error")
                        ),
                    ),
                    _ => continue,
                };
                let _ = app.notification().builder().title(title).body(body).show();
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let paths = app.path();
            let data_dir = paths.app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let settings_path = paths.app_config_dir()?.join("settings.json");
            let default_dir = paths
                .download_dir()
                .unwrap_or_else(|_| data_dir.join("downloads"));
            let settings = Settings::load(&settings_path, default_dir);

            let engine = Engine::open(data_dir.join("ferry.db"), engine_config(&settings))
                .map_err(|e| format!("could not open the download database: {e}"))?;
            forward_events(app.handle().clone(), engine.clone());
            app.manage(AppState {
                engine,
                settings: Mutex::new(settings),
                settings_path,
                schedule_changed: Notify::new(),
            });
            run_scheduler(app.handle().clone());
            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if app.state::<AppState>().settings().close_to_tray {
                    let _ = window.hide();
                    api.prevent_close();
                } else {
                    api.prevent_close();
                    quit(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_downloads,
            commands::get_download,
            commands::inspect_url,
            commands::add_download,
            commands::start_downloads,
            commands::start_now,
            commands::queue_state,
            commands::pause_downloads,
            commands::remove_downloads,
            commands::open_file,
            commands::show_in_folder,
            commands::get_settings,
            commands::save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ferry");
}
