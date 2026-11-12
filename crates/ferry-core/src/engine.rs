use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RANGE};
use serde::Serialize;
use tokio::{sync::broadcast, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::{
    download, filename,
    probe::probe,
    store::{DownloadId, DownloadInfo, NewDownload, Status, Store},
    Error, Result,
};

/// Tunables for the engine.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Maximum parallel connections per download.
    pub segments: usize,
    /// Segments are never split below this size (bytes).
    pub min_split_size: u64,
    /// Retries per segment before the download fails.
    pub max_retries: u32,
    /// First retry delay; doubles on each attempt (capped at 30 s).
    pub retry_base_delay: Duration,
    /// Fail a connection that sends no data for this long.
    pub read_timeout: Duration,
    /// How often progress events are emitted.
    pub progress_interval: Duration,
    /// How often segment progress is written to the database.
    pub save_interval: Duration,
    pub user_agent: String,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            segments: 8,
            min_split_size: 1024 * 1024,
            max_retries: 5,
            retry_base_delay: Duration::from_millis(500),
            read_timeout: Duration::from_secs(30),
            progress_interval: Duration::from_millis(500),
            save_interval: Duration::from_secs(1),
            user_agent: format!("Ferry/{}", crate::VERSION),
        }
    }
}

/// A new download to add.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub url: String,
    /// Directory the file is saved to (created if missing).
    pub dir: PathBuf,
    /// Overrides the name suggested by the server.
    pub file_name: Option<String>,
    /// Extra headers such as `Cookie`, `Referer` or `User-Agent` (from the browser).
    pub headers: Vec<(String, String)>,
}

impl Request {
    pub fn new(url: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            dir: dir.into(),
            ..Self::default()
        }
    }
}

/// Engine notifications, for the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    Added {
        id: DownloadId,
    },
    Status {
        id: DownloadId,
        status: Status,
        error: Option<String>,
    },
    Progress {
        id: DownloadId,
        downloaded: u64,
        total: Option<u64>,
        /// Bytes per second.
        speed: u64,
    },
    Removed {
        id: DownloadId,
    },
}

struct Active {
    generation: u64,
    token: CancellationToken,
    handle: JoinHandle<()>,
}

pub(crate) struct Inner {
    pub client: reqwest::Client,
    pub store: Store,
    pub config: EngineConfig,
    events: broadcast::Sender<Event>,
    active: Mutex<HashMap<DownloadId, Active>>,
    generation: AtomicU64,
}

impl Inner {
    pub fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    fn active(&self) -> std::sync::MutexGuard<'_, HashMap<DownloadId, Active>> {
        self.active.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Called by a finished download task to unregister itself.
    pub fn task_finished(&self, id: DownloadId, generation: u64) {
        let mut active = self.active();
        if active.get(&id).is_some_and(|a| a.generation == generation) {
            active.remove(&id);
        }
    }
}

/// The download engine. Cheap to clone; all clones share state.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    /// Opens (or creates) the engine database at `db_path`.
    pub fn open(db_path: impl AsRef<Path>, config: EngineConfig) -> Result<Self> {
        Self::with_store(Store::open(db_path.as_ref())?, config)
    }

    /// An engine whose state lives only in memory (tests, previews).
    pub fn in_memory(config: EngineConfig) -> Result<Self> {
        Self::with_store(Store::in_memory()?, config)
    }

    fn with_store(store: Store, config: EngineConfig) -> Result<Self> {
        store.reset_interrupted()?;
        let client = reqwest::Client::builder()
            .user_agent(config.user_agent.clone())
            .connect_timeout(Duration::from_secs(15))
            .build()?;
        let (events, _) = broadcast::channel(1024);
        Ok(Self {
            inner: Arc::new(Inner {
                client,
                store,
                config,
                events,
                active: Mutex::new(HashMap::new()),
                generation: AtomicU64::new(0),
            }),
        })
    }

    /// Receives engine events. Slow receivers may miss progress events.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    /// Probes the URL and stores a new `queued` download. Call [`Engine::start`] to begin.
    pub async fn add(&self, req: Request) -> Result<DownloadId> {
        let headers = build_headers(&req.headers);
        let info = probe(&self.inner.client, &req.url, &headers).await?;

        tokio::fs::create_dir_all(&req.dir).await?;
        let suggested = req
            .file_name
            .or(info.filename.clone())
            .unwrap_or_else(|| "download".into());
        let name = filename::unique_name(&filename::sanitize(&suggested), |n| {
            req.dir.join(n).exists()
                || req.dir.join(download::part_name(n)).exists()
                || self.inner.store.name_taken(&req.dir, n).unwrap_or(true)
        });

        let id = self.inner.store.insert(&NewDownload {
            url: &req.url,
            dir: &req.dir,
            file_name: &name,
            size: info.size,
            resumable: info.resumable,
            etag: info.etag.as_deref(),
            last_modified: info.last_modified.as_deref(),
            headers: &req.headers,
        })?;
        self.inner.emit(Event::Added { id });
        Ok(id)
    }

    /// Starts or resumes a download in the background.
    pub fn start(&self, id: DownloadId) -> Result<()> {
        let info = self.inner.store.get(id)?;
        if info.status == Status::Completed {
            return Err(Error::InvalidState("download is already completed"));
        }
        let mut active = self.inner.active();
        if active.contains_key(&id) {
            return Ok(());
        }
        self.inner.store.set_status(id, Status::Downloading, None)?;
        self.inner.emit(Event::Status {
            id,
            status: Status::Downloading,
            error: None,
        });

        let generation = self.inner.generation.fetch_add(1, Ordering::Relaxed);
        let token = CancellationToken::new();
        let handle = tokio::spawn(download::run(
            self.inner.clone(),
            id,
            generation,
            token.clone(),
        ));
        active.insert(
            id,
            Active {
                generation,
                token,
                handle,
            },
        );
        Ok(())
    }

    /// Pauses a download and waits until its progress is saved.
    pub async fn pause(&self, id: DownloadId) -> Result<()> {
        let active = self.inner.active().remove(&id);
        if let Some(active) = active {
            active.token.cancel();
            let _ = active.handle.await;
            return Ok(());
        }
        let info = self.inner.store.get(id)?;
        if info.status == Status::Queued {
            self.inner.store.set_status(id, Status::Paused, None)?;
            self.inner.emit(Event::Status {
                id,
                status: Status::Paused,
                error: None,
            });
        }
        Ok(())
    }

    /// Removes a download from the list, optionally deleting its file.
    pub async fn remove(&self, id: DownloadId, delete_file: bool) -> Result<()> {
        self.pause(id).await?;
        let info = self.inner.store.get(id)?;
        let part = info.dir.join(download::part_name(&info.file_name));
        let _ = tokio::fs::remove_file(&part).await;
        if delete_file && info.status == Status::Completed {
            let _ = tokio::fs::remove_file(info.dir.join(&info.file_name)).await;
        }
        self.inner.store.delete(id)?;
        self.inner.emit(Event::Removed { id });
        Ok(())
    }

    pub fn get(&self, id: DownloadId) -> Result<DownloadInfo> {
        self.inner.store.get(id)
    }

    pub fn list(&self) -> Result<Vec<DownloadInfo>> {
        self.inner.store.list()
    }

    /// Ids of downloads currently running.
    pub fn active_ids(&self) -> Vec<DownloadId> {
        self.inner.active().keys().copied().collect()
    }

    /// Pauses every running download, saving progress. Call before exiting.
    pub async fn shutdown(&self) {
        for id in self.active_ids() {
            let _ = self.pause(id).await;
        }
    }
}

/// Converts user-supplied header pairs, dropping invalid ones and `Range`.
pub(crate) fn build_headers(pairs: &[(String, String)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (k, v) in pairs {
        if let (Ok(k), Ok(v)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            if k != RANGE {
                map.append(k, v);
            }
        }
    }
    map
}
