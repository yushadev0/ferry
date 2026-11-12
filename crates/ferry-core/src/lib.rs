//! Ferry download engine: segmented, resumable HTTP downloads.
//!
//! The [`Engine`] owns a SQLite database with every download and its segment
//! progress, so downloads survive pauses and application restarts.
//!
//! ```no_run
//! # async fn demo() -> ferry_core::Result<()> {
//! use ferry_core::{Engine, EngineConfig, Request};
//!
//! let engine = Engine::open("ferry.db", EngineConfig::default())?;
//! let id = engine.add(Request::new("https://example.com/file.iso", "downloads")).await?;
//! engine.start(id)?;
//! # Ok(()) }
//! ```

mod download;
mod engine;
mod error;
pub mod filename;
mod limiter;
mod probe;
mod store;

pub use download::Segment;
pub use engine::{Engine, EngineConfig, Event, Request};
pub use error::{Error, Result};
pub use probe::{probe, ProbeInfo};
pub use store::{DownloadId, DownloadInfo, Status};

/// Engine version, taken from the crate manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
