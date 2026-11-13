use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant},
};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RANGE};
use serde::Serialize;
use tokio::{sync::broadcast, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::{
    download, filename,
    limiter::RateLimiter,
    probe::{probe, ProbeInfo},
    store::{DownloadId, DownloadInfo, NewDownload, Status, Store},
    Error, Result,
};

/// URL and extra headers of a probe.
type ProbeKey = (String, Vec<(String, String)>);

/// How long a probe answer is trusted instead of asking the server again.
const PROBE_REUSE: Duration = Duration::from_secs(30);

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
    /// Send a request again when the server has not answered it for this
    /// long, although it has answered another request of the download. Some
    /// servers (Google Drive) hold a few requests for half a minute but answer
    /// a fresh one at once.
    pub answer_patience: Duration,
    /// How often progress events are emitted.
    pub progress_interval: Duration,
    /// How often segment progress is written to the database.
    pub save_interval: Duration,
    /// Delay between opening the connections of one download, so servers
    /// that limit request rates are not hit by a burst.
    pub connection_stagger: Duration,
    /// First wait after the server answers 429/503 without `Retry-After`;
    /// grows linearly with each attempt (capped at 10x).
    pub busy_retry_delay: Duration,
    /// Downloads that may run at once when started through the queue
    /// ([`Engine::start`]); 0 means no limit.
    pub max_active: usize,
    /// Total download speed across all downloads, in bytes per second; 0 means
    /// no limit.
    pub speed_limit: u64,
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
            answer_patience: Duration::from_secs(5),
            progress_interval: Duration::from_millis(500),
            save_interval: Duration::from_secs(1),
            connection_stagger: Duration::from_millis(150),
            busy_retry_delay: Duration::from_secs(1),
            max_active: 0,
            speed_limit: 0,
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
        /// Connections currently downloading.
        connections: usize,
        /// Every segment and how far it got (a connection map).
        segments: Vec<download::Segment>,
    },
    Removed {
        id: DownloadId,
    },
}

struct Active {
    generation: u64,
    token: CancellationToken,
    handle: JoinHandle<()>,
    /// Started with [`Engine::start_now`]: ignores the queue limit and window.
    forced: bool,
    /// Set when the queue takes the download back; it ends `queued`, not `paused`.
    requeue: Arc<AtomicBool>,
}

pub(crate) struct Inner {
    pub client: reqwest::Client,
    pub store: Store,
    config: RwLock<EngineConfig>,
    events: broadcast::Sender<Event>,
    active: Mutex<HashMap<DownloadId, Active>>,
    generation: AtomicU64,
    /// Probe results from `add`, reused by an immediate `start` to save a request.
    recent_probes: Mutex<HashMap<DownloadId, (Instant, ProbeInfo)>>,
    /// Results of [`Engine::probe`] by URL and headers, reused by `add`.
    url_probes: Mutex<HashMap<ProbeKey, (Instant, ProbeInfo)>>,
    /// Whether queued downloads may start (false outside a schedule window).
    queue_running: AtomicBool,
    pub limiter: RateLimiter,
}

impl Inner {
    /// A snapshot of the current configuration.
    pub fn config(&self) -> EngineConfig {
        self.config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    fn active(&self) -> std::sync::MutexGuard<'_, HashMap<DownloadId, Active>> {
        self.active.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The probe made by `add`, if it is fresh enough to trust.
    pub fn take_recent_probe(&self, id: DownloadId) -> Option<ProbeInfo> {
        let (at, info) = self
            .recent_probes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id)?;
        (at.elapsed() < PROBE_REUSE).then_some(info)
    }

    /// Called by a finished download task to unregister itself; frees its
    /// queue slot for the next download.
    pub fn task_finished(self: &Arc<Self>, id: DownloadId, generation: u64) {
        {
            let mut active = self.active();
            if active.get(&id).is_some_and(|a| a.generation == generation) {
                active.remove(&id);
            }
        }
        self.pump();
    }

    /// Spawns the download task. The caller holds the `active` lock.
    fn spawn(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
        active: &mut HashMap<DownloadId, Active>,
        id: DownloadId,
        forced: bool,
    ) -> Result<()> {
        self.store.set_status(id, Status::Downloading, None)?;
        self.emit(Event::Status {
            id,
            status: Status::Downloading,
            error: None,
        });
        let generation = self.generation.fetch_add(1, Ordering::Relaxed);
        let token = CancellationToken::new();
        let requeue = Arc::new(AtomicBool::new(false));
        let handle = runtime.spawn(download::run(
            self.clone(),
            id,
            generation,
            token.clone(),
            requeue.clone(),
        ));
        active.insert(
            id,
            Active {
                generation,
                token,
                handle,
                forced,
                requeue,
            },
        );
        Ok(())
    }

    /// Starts queued downloads while there are free slots.
    pub fn pump(self: &Arc<Self>) {
        if !self.queue_running.load(Ordering::SeqCst) {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let max = match self.config().max_active {
            0 => usize::MAX,
            n => n,
        };
        let mut active = self.active();
        let queued = match self.store.queued_ids() {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!(error = %e, "could not read the queue");
                return;
            }
        };
        for id in queued {
            let running = active.values().filter(|a| !a.forced).count();
            if running >= max {
                break;
            }
            if active.contains_key(&id) {
                continue;
            }
            if let Err(e) = self.spawn(&runtime, &mut active, id, false) {
                tracing::error!(id, error = %e, "could not start a queued download");
            }
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
                events,
                active: Mutex::new(HashMap::new()),
                generation: AtomicU64::new(0),
                recent_probes: Mutex::new(HashMap::new()),
                url_probes: Mutex::new(HashMap::new()),
                queue_running: AtomicBool::new(true),
                limiter: RateLimiter::new(config.speed_limit),
                config: RwLock::new(config),
            }),
        })
    }

    /// Current configuration.
    pub fn config(&self) -> EngineConfig {
        self.inner.config()
    }

    /// Replaces the configuration. The speed limit and queue size apply at
    /// once; per-download settings apply to downloads started afterwards, and
    /// the user agent only changes when the engine is reopened.
    pub fn set_config(&self, config: EngineConfig) {
        self.inner.limiter.set_rate(config.speed_limit);
        *self.inner.config.write().unwrap_or_else(|e| e.into_inner()) = config;
        self.inner.pump();
    }

    /// Lets queued downloads start (true) or holds the queue (false). Holding
    /// it sends running queue downloads back to the queue, keeping their
    /// progress; downloads started with [`Engine::start_now`] keep running.
    pub async fn set_queue_running(&self, running: bool) {
        let was = self.inner.queue_running.swap(running, Ordering::SeqCst);
        if running {
            self.inner.pump();
            return;
        }
        if !was {
            return;
        }
        let stopped: Vec<Active> = {
            let mut active = self.inner.active();
            let ids: Vec<DownloadId> = active
                .iter()
                .filter(|(_, a)| !a.forced)
                .map(|(id, _)| *id)
                .collect();
            ids.iter().filter_map(|id| active.remove(id)).collect()
        };
        for a in &stopped {
            a.requeue.store(true, Ordering::SeqCst);
            a.token.cancel();
        }
        for a in stopped {
            let _ = a.handle.await;
        }
    }

    /// Whether queued downloads may currently start.
    pub fn queue_running(&self) -> bool {
        self.inner.queue_running.load(Ordering::SeqCst)
    }

    /// Asks the server about a URL without adding it (for an "add download"
    /// dialog). A following [`Engine::add`] of the same URL reuses the answer.
    pub async fn probe(&self, url: &str, headers: &[(String, String)]) -> Result<ProbeInfo> {
        let info = probe(&self.inner.client, url, &build_headers(headers)).await?;
        let mut cache = self
            .inner
            .url_probes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        cache.retain(|_, (at, _)| at.elapsed() < PROBE_REUSE);
        cache.insert(
            (url.to_string(), headers.to_vec()),
            (Instant::now(), info.clone()),
        );
        Ok(info)
    }

    /// Receives engine events. Slow receivers may miss progress events.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    /// Probes the URL and stores a new, `paused` download. Call
    /// [`Engine::start`] to queue it or [`Engine::start_now`] to begin at once.
    pub async fn add(&self, req: Request) -> Result<DownloadId> {
        let headers = build_headers(&req.headers);
        let cached = self
            .inner
            .url_probes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&(req.url.clone(), req.headers.clone()))
            .filter(|(at, _)| at.elapsed() < PROBE_REUSE)
            .map(|(_, info)| info);
        let info = match cached {
            Some(info) => info,
            None => self.probe_patiently(&req.url, &headers).await?,
        };

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
        self.inner
            .recent_probes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, (Instant::now(), info));
        self.inner.emit(Event::Added { id });
        Ok(id)
    }

    /// Probes, waiting out a couple of "too many requests" answers.
    async fn probe_patiently(&self, url: &str, headers: &HeaderMap) -> Result<ProbeInfo> {
        let mut attempt = 0;
        loop {
            match probe(&self.inner.client, url, headers).await {
                Err(Error::Busy { retry_after, .. }) if attempt < 3 => {
                    attempt += 1;
                    let wait = retry_after
                        .unwrap_or(self.inner.config().busy_retry_delay * attempt)
                        .min(Duration::from_secs(10));
                    tokio::time::sleep(wait).await;
                }
                result => return result,
            }
        }
    }

    /// Queues a download; it starts as soon as the queue has a free slot
    /// (immediately when there is no limit and the queue is running).
    ///
    /// Must be called from within a Tokio runtime.
    pub fn start(&self, id: DownloadId) -> Result<()> {
        runtime()?;
        let info = self.inner.store.get(id)?;
        if info.status == Status::Completed {
            return Err(Error::InvalidState("download is already completed"));
        }
        if self.inner.active().contains_key(&id) || info.status == Status::Queued {
            self.inner.pump();
            return Ok(());
        }
        self.inner.store.enqueue(id)?;
        self.inner.emit(Event::Status {
            id,
            status: Status::Queued,
            error: None,
        });
        self.inner.pump();
        Ok(())
    }

    /// Has a download join the queue at `at` (Unix seconds). A running
    /// download is paused first, keeping its progress. Call
    /// [`Engine::start_due`] regularly to act on schedules.
    pub async fn schedule(&self, id: DownloadId, at: i64) -> Result<()> {
        let info = self.inner.store.get(id)?;
        if info.status == Status::Completed {
            return Err(Error::InvalidState("download is already completed"));
        }
        self.pause(id).await?;
        self.inner.store.schedule(id, at)?;
        self.inner.emit(Event::Status {
            id,
            status: Status::Scheduled,
            error: None,
        });
        Ok(())
    }

    /// Queues every scheduled download whose time is at or before `now`
    /// (Unix seconds) and returns their ids.
    ///
    /// Must be called from within a Tokio runtime.
    pub fn start_due(&self, now: i64) -> Result<Vec<DownloadId>> {
        let due = self.inner.store.due_scheduled(now)?;
        for &id in &due {
            self.start(id)?;
        }
        Ok(due)
    }

    /// Starts a download right away, ignoring the queue limit and schedule.
    ///
    /// Must be called from within a Tokio runtime.
    pub fn start_now(&self, id: DownloadId) -> Result<()> {
        let runtime = runtime()?;
        let info = self.inner.store.get(id)?;
        if info.status == Status::Completed {
            return Err(Error::InvalidState("download is already completed"));
        }
        let mut active = self.inner.active();
        if let Some(a) = active.get_mut(&id) {
            a.forced = true;
            return Ok(());
        }
        self.inner.spawn(&runtime, &mut active, id, true)
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
        if matches!(info.status, Status::Queued | Status::Scheduled) {
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

    /// Pauses every running download. Queue downloads go back to the queue
    /// with their progress saved, so they continue on the next start.
    pub async fn pause_all(&self) {
        let queue_was_running = self.queue_running();
        self.set_queue_running(false).await;
        for id in self.active_ids() {
            let _ = self.pause(id).await;
        }
        for id in self.inner.store.queued_ids().unwrap_or_default() {
            let _ = self.pause(id).await;
        }
        if queue_was_running {
            self.inner.queue_running.store(true, Ordering::SeqCst);
        }
    }

    /// Saves progress of every running download. Call before exiting; queue
    /// downloads stay queued and continue when the engine starts again.
    pub async fn shutdown(&self) {
        self.set_queue_running(false).await;
        for id in self.active_ids() {
            let _ = self.pause(id).await;
        }
    }
}

fn runtime() -> Result<tokio::runtime::Handle> {
    tokio::runtime::Handle::try_current()
        .map_err(|_| Error::InvalidState("the engine must be used inside a Tokio runtime"))
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
