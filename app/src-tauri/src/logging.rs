//! Log files, so problems can be looked into after the fact. One file per day
//! in the `logs` folder, the last week kept. `FERRY_LOG` overrides the levels
//! (same syntax as `RUST_LOG`, e.g. `FERRY_LOG=debug`).

use std::path::{Path, PathBuf};

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, EnvFilter};

const DEFAULT_LEVELS: &str = "warn,ferry_core=info,ferry_app_lib=info";

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

/// Starts writing logs to `dir`. Logging is best effort: if the folder can't
/// be used, Ferry runs without it.
pub fn init(dir: &Path) {
    let appender = match RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("ferry")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir)
    {
        Ok(appender) => appender,
        Err(e) => {
            eprintln!("Ferry: logging is off: {e}");
            return;
        }
    };
    let filter =
        EnvFilter::try_from_env("FERRY_LOG").unwrap_or_else(|_| EnvFilter::new(DEFAULT_LEVELS));
    let installed = fmt()
        .with_env_filter(filter)
        .with_writer(appender)
        .with_ansi(false)
        .with_target(true)
        .try_init()
        .is_ok();
    if !installed {
        return;
    }

    // Crashes end up in the log too, then Rust's own report runs as usual.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("crashed: {info}");
        default_hook(info);
    }));
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Ferry started");
}
