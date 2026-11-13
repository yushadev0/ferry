//! The segmented download loop.
//!
//! A download is split into byte-range segments, each fetched by a worker over
//! its own connection and written at its offset in a `.ferrypart` file. A worker
//! that runs out of segments splits the largest remaining one in half (dynamic
//! segmentation), so all connections stay busy until the end.

use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};

use reqwest::{
    header::{HeaderMap, CONTENT_RANGE, RANGE},
    StatusCode,
};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

use crate::{
    engine::{build_headers, EngineConfig, Event, Inner},
    error::check_status,
    filename,
    probe::{parse_content_range, probe},
    store::{DownloadId, Status},
    Error, Result,
};

/// A byte range of the file. `end` is inclusive; `None` means "until EOF".
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Segment {
    pub start: u64,
    pub end: Option<u64>,
    /// Next byte to write.
    pub pos: u64,
}

impl Segment {
    fn remaining(&self) -> Option<u64> {
        self.end.map(|e| (e + 1).saturating_sub(self.pos))
    }

    fn is_done(&self) -> bool {
        self.remaining() == Some(0)
    }

    /// Bytes of this segment already written.
    pub fn downloaded(&self) -> u64 {
        let pos = match self.end {
            Some(e) => self.pos.min(e + 1),
            None => self.pos,
        };
        pos.saturating_sub(self.start)
    }
}

pub(crate) fn part_name(file_name: &str) -> String {
    format!("{file_name}.ferrypart")
}

/// Initial segment layout for a file.
pub(crate) fn plan(size: Option<u64>, resumable: bool, cfg: &EngineConfig) -> Vec<Segment> {
    match size {
        Some(0) => Vec::new(),
        Some(size) if resumable => {
            let n = (size / cfg.min_split_size.max(1)).clamp(1, cfg.segments.max(1) as u64);
            let part = size / n;
            (0..n)
                .map(|i| {
                    let start = i * part;
                    let end = if i == n - 1 {
                        size - 1
                    } else {
                        start + part - 1
                    };
                    Segment {
                        start,
                        end: Some(end),
                        pos: start,
                    }
                })
                .collect()
        }
        _ => vec![Segment {
            start: 0,
            end: size.map(|s| s - 1),
            pos: 0,
        }],
    }
}

/// Segment table shared by the workers of one download.
struct State {
    segs: Vec<Segment>,
    in_use: Vec<bool>,
    complete: Vec<bool>,
    /// Workers still running. A worker turned away by the server (HTTP 429/503)
    /// retires while others remain, so the download settles on the number of
    /// connections the server accepts.
    workers: usize,
    /// The server has refused a connection; stop splitting to avoid more requests.
    throttled: bool,
}

impl State {
    fn new(segs: Vec<Segment>) -> Self {
        Self {
            in_use: vec![false; segs.len()],
            complete: segs.iter().map(Segment::is_done).collect(),
            segs,
            workers: 0,
            throttled: false,
        }
    }

    fn downloaded(&self) -> u64 {
        self.segs.iter().map(Segment::downloaded).sum()
    }

    /// When segment `*idx` is finished at `pos` and the segment starting at
    /// `pos` is untouched and idle, takes it over so the same connection keeps
    /// reading instead of opening a new one.
    fn chain(&mut self, idx: &mut usize, pos: u64) -> bool {
        let next = (0..self.segs.len()).find(|&j| {
            self.segs[j].start == pos
                && self.segs[j].pos == pos
                && !self.in_use[j]
                && !self.complete[j]
        });
        let Some(next) = next else { return false };
        self.complete[*idx] = true;
        self.in_use[*idx] = false;
        self.in_use[next] = true;
        *idx = next;
        true
    }

    /// Hands out an idle segment, or splits the largest busy one.
    fn claim(&mut self, allow_split: bool, min_split: u64) -> Option<usize> {
        if let Some(i) = (0..self.segs.len()).find(|&i| !self.in_use[i] && !self.complete[i]) {
            self.in_use[i] = true;
            return Some(i);
        }
        if !allow_split {
            return None;
        }
        let (i, remaining) = (0..self.segs.len())
            .filter(|&i| self.in_use[i] && !self.complete[i])
            .filter_map(|i| self.segs[i].remaining().map(|r| (i, r)))
            .max_by_key(|&(_, r)| r)?;
        if remaining < 2 * min_split.max(1) {
            return None;
        }
        let seg = self.segs[i];
        let mid = seg.pos + remaining / 2;
        self.segs[i].end = Some(mid - 1);
        self.segs.push(Segment {
            start: mid,
            end: seg.end,
            pos: mid,
        });
        self.in_use.push(true);
        self.complete.push(false);
        Some(self.segs.len() - 1)
    }
}

type Shared = Arc<Mutex<State>>;

fn lock(state: &Shared) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

/// Runs a download to completion, pause or failure and records the outcome.
pub(crate) async fn run(
    inner: Arc<Inner>,
    id: DownloadId,
    generation: u64,
    token: CancellationToken,
    requeue: Arc<AtomicBool>,
) {
    let (status, error) = match execute(&inner, id, &token).await {
        Ok(true) => (Status::Completed, None),
        Ok(false) if requeue.load(Ordering::SeqCst) => (Status::Queued, None),
        Ok(false) => (Status::Paused, None),
        Err(e) => {
            tracing::warn!(id, error = %e, "download failed");
            (Status::Failed, Some(e.to_string()))
        }
    };
    if let Err(e) = inner.store.set_status(id, status, error.as_deref()) {
        tracing::error!(id, error = %e, "could not save download status");
    }
    inner.emit(Event::Status { id, status, error });
    inner.task_finished(id, generation);
}

/// Returns `Ok(true)` when the file is complete, `Ok(false)` when paused.
async fn execute(inner: &Arc<Inner>, id: DownloadId, token: &CancellationToken) -> Result<bool> {
    let cfg = &inner.config();
    let rec = inner.store.get(id)?;
    let headers = build_headers(&rec.headers);

    let part = rec.dir.join(part_name(&rec.file_name));
    let saved = inner.store.load_segments(id)?;
    let fresh = inner.take_recent_probe(id).filter(|_| saved.is_empty());
    let info = match fresh {
        Some(info) => info,
        None => match probe_with_retry(inner, &rec.url, &headers, token).await? {
            Some(info) => info,
            None => return Ok(false),
        },
    };
    let validators_match = match (&rec.etag, &rec.last_modified) {
        (Some(etag), _) => info.etag.as_ref() == Some(etag),
        (None, Some(lm)) => info.last_modified.as_ref() == Some(lm),
        (None, None) => true,
    };
    let can_resume = !saved.is_empty()
        && rec.resumable
        && info.resumable
        && rec.size == info.size
        && validators_match
        && tokio::fs::try_exists(&part).await.unwrap_or(false);

    let segs = if can_resume {
        saved
    } else {
        if !saved.is_empty() {
            tracing::info!(
                id,
                "file changed on the server or cannot resume; restarting"
            );
        }
        let segs = plan(info.size, info.resumable, cfg);
        inner.store.update_meta(
            id,
            info.size,
            info.resumable,
            info.etag.as_deref(),
            info.last_modified.as_deref(),
        )?;
        tokio::fs::create_dir_all(&rec.dir).await?;
        let file = File::create(&part).await?;
        if let Some(size) = info.size {
            file.set_len(size).await?;
        }
        drop(file);
        inner.store.save_segments(id, &segs, 0)?;
        segs
    };

    let resumable = info.resumable && info.size.is_some();
    let state: Shared = Arc::new(Mutex::new(State::new(segs)));
    let workers_token = token.child_token();
    let mut workers = JoinSet::new();
    let worker_count = if resumable { cfg.segments.max(1) } else { 1 };
    lock(&state).workers = worker_count;
    for index in 0..worker_count {
        workers.spawn(worker(
            index,
            inner.clone(),
            rec.url.clone(),
            headers.clone(),
            part.clone(),
            state.clone(),
            resumable,
            workers_token.clone(),
        ));
    }

    let mut tick = tokio::time::interval(cfg.progress_interval);
    let mut last_sample = (Instant::now(), lock(&state).downloaded());
    let mut last_save = Instant::now();
    let mut first_error = None;
    loop {
        tokio::select! {
            joined = workers.join_next() => match joined {
                None => break,
                Some(Ok(Ok(()))) => {}
                Some(Ok(Err(e))) => {
                    if first_error.is_none() {
                        first_error = Some(e);
                        workers_token.cancel();
                    }
                }
                Some(Err(e)) => {
                    if first_error.is_none() {
                        first_error = Some(Error::Internal(e.to_string()));
                        workers_token.cancel();
                    }
                }
            },
            _ = tick.tick() => {
                let (downloaded, snapshot, connections) = {
                    let s = lock(&state);
                    (s.downloaded(), s.segs.clone(), s.in_use.iter().filter(|u| **u).count())
                };
                let elapsed = last_sample.0.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    (downloaded.saturating_sub(last_sample.1) as f64 / elapsed) as u64
                } else {
                    0
                };
                last_sample = (Instant::now(), downloaded);
                if last_save.elapsed() >= cfg.save_interval {
                    inner.store.save_segments(id, &snapshot, downloaded)?;
                    last_save = Instant::now();
                }
                inner.emit(Event::Progress {
                    id,
                    downloaded,
                    total: info.size,
                    speed,
                    connections,
                    segments: snapshot,
                });
            }
        }
    }

    let (downloaded, segs, all_complete) = {
        let s = lock(&state);
        (
            s.downloaded(),
            s.segs.clone(),
            s.complete.iter().all(|c| *c),
        )
    };
    inner.store.save_segments(id, &segs, downloaded)?;
    inner.emit(Event::Progress {
        id,
        downloaded,
        total: info.size,
        speed: 0,
        connections: 0,
        segments: segs,
    });

    if token.is_cancelled() {
        return Ok(false);
    }
    if let Some(e) = first_error {
        return Err(e);
    }
    if !all_complete {
        return Err(Error::Internal("workers stopped before finishing".into()));
    }

    let final_name = filename::unique_name(&rec.file_name, |n| rec.dir.join(n).exists());
    tokio::fs::rename(&part, rec.dir.join(&final_name)).await?;
    if final_name != rec.file_name {
        inner.store.set_file_name(id, &final_name)?;
    }
    inner.store.finish(id, info.size.unwrap_or(downloaded))?;
    Ok(true)
}

async fn probe_with_retry(
    inner: &Inner,
    url: &str,
    headers: &HeaderMap,
    token: &CancellationToken,
) -> Result<Option<crate::ProbeInfo>> {
    let cfg = inner.config();
    let mut attempt = 0;
    loop {
        let result = tokio::select! {
            _ = token.cancelled() => return Ok(None),
            r = probe(&inner.client, url, headers) => r,
        };
        match result {
            Ok(info) => return Ok(Some(info)),
            Err(e) if e.is_retryable() && attempt < retry_budget(&cfg, &e) => {
                attempt += 1;
                if !backoff(&cfg, attempt, &e, token).await {
                    return Ok(None);
                }
            }
            Err(e) => return Err(e),
        }
    }
}

/// How many retries an error gets. Rate limits are temporary and can last a
/// while, so they get more patience than other errors.
fn retry_budget(cfg: &EngineConfig, err: &Error) -> u32 {
    match err {
        Error::Busy { .. } => cfg.max_retries * 2,
        _ => cfg.max_retries,
    }
}

/// Sleeps before retry `attempt`, honouring `Retry-After`. Returns `false` if
/// cancelled meanwhile.
async fn backoff(cfg: &EngineConfig, attempt: u32, err: &Error, token: &CancellationToken) -> bool {
    let factor = 1u32 << (attempt.saturating_sub(1)).min(6);
    let mut delay = cfg
        .retry_base_delay
        .saturating_mul(factor)
        .min(Duration::from_secs(30));
    if let Error::Busy { retry_after, .. } = err {
        let wait = retry_after.unwrap_or(cfg.busy_retry_delay * attempt.min(10));
        delay = wait.max(cfg.retry_base_delay).min(Duration::from_secs(60));
    }
    tokio::select! {
        _ = token.cancelled() => false,
        _ = tokio::time::sleep(delay) => true,
    }
}

enum Exit {
    /// No more work, or cancelled.
    Done,
    /// Gave its segment back because the server refused another connection.
    Retired,
}

#[allow(clippy::too_many_arguments)]
async fn worker(
    index: usize,
    inner: Arc<Inner>,
    url: String,
    headers: HeaderMap,
    part: PathBuf,
    state: Shared,
    resumable: bool,
    token: CancellationToken,
) -> Result<()> {
    let stagger = inner.config().connection_stagger * index as u32;
    let result = tokio::select! {
        _ = token.cancelled() => Ok(Exit::Done),
        _ = tokio::time::sleep(stagger) => {
            worker_loop(&inner, &url, &headers, &part, &state, resumable, &token).await
        }
    };
    // A retired worker already left the count while holding the lock.
    if !matches!(result, Ok(Exit::Retired)) {
        lock(&state).workers -= 1;
    }
    result.map(|_| ())
}

#[allow(clippy::too_many_arguments)]
async fn worker_loop(
    inner: &Inner,
    url: &str,
    headers: &HeaderMap,
    part: &Path,
    state: &Shared,
    resumable: bool,
    token: &CancellationToken,
) -> Result<Exit> {
    let cfg = &inner.config();
    // Whether the server has let this connection in at least once.
    let mut accepted = false;
    // Refusals in a row of an accepted connection (see the Busy arm).
    let mut busy_attempts = 0;
    'claim: loop {
        let claimed = {
            let mut s = lock(state);
            // A throttled server costs a request per split, so only split for
            // connections the server has already accepted.
            let allow_split = resumable && (accepted || !s.throttled);
            s.claim(allow_split, cfg.min_split_size)
        };
        let Some(mut idx) = claimed else {
            return Ok(Exit::Done);
        };
        let mut attempt = 0;
        loop {
            if token.is_cancelled() {
                return Ok(Exit::Done);
            }
            let before = lock(state).segs[idx].pos;
            match fetch_segment(
                inner,
                url,
                headers,
                part,
                state,
                &mut idx,
                resumable,
                &mut accepted,
                token,
            )
            .await
            {
                Ok(()) => {
                    busy_attempts = 0;
                    break;
                }
                Err(e @ Error::Busy { .. }) => {
                    let wait_and_reclaim = {
                        let mut s = lock(state);
                        s.throttled = true;
                        if s.workers <= 1 {
                            false
                        } else if !accepted || busy_attempts >= retry_budget(cfg, &e) {
                            // A refused first request means the server wants
                            // fewer connections: hand the segment to the ones
                            // it accepted.
                            s.workers -= 1;
                            s.in_use[idx] = false;
                            tracing::debug!(
                                segment = idx,
                                workers = s.workers,
                                "server busy; dropping a connection"
                            );
                            return Ok(Exit::Retired);
                        } else {
                            // The server has accepted this connection, so the
                            // refusal only limits how often we ask (like nginx
                            // `limit_req`). Keep the slot and ask again later;
                            // meanwhile free the segment, so a connection that
                            // is still reading can carry on into it.
                            s.in_use[idx] = false;
                            true
                        }
                    };
                    if wait_and_reclaim {
                        busy_attempts += 1;
                        if !backoff(cfg, busy_attempts, &e, token).await {
                            return Ok(Exit::Done);
                        }
                        continue 'claim;
                    }
                    if attempt >= retry_budget(cfg, &e) {
                        return Err(e);
                    }
                    attempt += 1;
                    if !backoff(cfg, attempt, &e, token).await {
                        return Ok(Exit::Done);
                    }
                }
                Err(e) if e.is_retryable() && attempt < retry_budget(cfg, &e) => {
                    if lock(state).segs[idx].pos > before {
                        attempt = 0;
                    }
                    attempt += 1;
                    tracing::debug!(segment = idx, attempt, error = %e, "retrying segment");
                    if !backoff(cfg, attempt, &e, token).await {
                        return Ok(Exit::Done);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        if token.is_cancelled() {
            return Ok(Exit::Done);
        }
        let mut s = lock(state);
        s.complete[idx] = true;
        s.in_use[idx] = false;
    }
}

/// Downloads the rest of segment `*idx`, continuing into following idle
/// segments on the same connection (`*idx` is updated). Returns `Ok` when the
/// last one is finished or the download is cancelled. Sets `*accepted` once
/// the server answers the request with data.
#[allow(clippy::too_many_arguments)]
async fn fetch_segment(
    inner: &Inner,
    url: &str,
    headers: &HeaderMap,
    part: &Path,
    state: &Shared,
    idx: &mut usize,
    resumable: bool,
    accepted: &mut bool,
    token: &CancellationToken,
) -> Result<()> {
    let (mut pos, end) = {
        let mut s = lock(state);
        let seg = &mut s.segs[*idx];
        if !resumable {
            // Without range support a retry has to start over.
            seg.pos = seg.start;
        }
        (seg.pos, seg.end)
    };
    if end.is_some_and(|e| pos > e) {
        return Ok(());
    }

    let mut request = inner.client.get(url).headers(headers.clone());
    if resumable {
        // When the server limits requests (or only one connection is left),
        // ask for everything from `pos` so this connection can carry on into
        // the next segment. Otherwise ask for just this segment, so a closed
        // connection does not waste bandwidth on bytes another worker fetches.
        let open_ended = {
            let s = lock(state);
            s.throttled || s.workers <= 1
        };
        let range = match end {
            Some(end) if !open_ended => format!("bytes={pos}-{end}"),
            _ => format!("bytes={pos}-"),
        };
        request = request.header(RANGE, range);
    }
    let mut resp = tokio::select! {
        _ = token.cancelled() => return Ok(()),
        r = request.send() => r?,
    };
    check_status(resp.status(), resp.headers())?;
    *accepted = true;
    if resumable {
        let start = resp
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_range)
            .map(|(start, _, _)| start);
        if resp.status() != StatusCode::PARTIAL_CONTENT || start != Some(pos) {
            return Err(Error::BadRange);
        }
    }

    let read_timeout = inner.config().read_timeout;
    let mut file = OpenOptions::new().write(true).open(part).await?;
    file.seek(SeekFrom::Start(pos)).await?;

    loop {
        let chunk = tokio::select! {
            _ = token.cancelled() => break,
            c = tokio::time::timeout(read_timeout, resp.chunk()) => match c {
                Err(_) => return Err(Error::Timeout),
                Ok(r) => r?,
            },
        };
        let Some(chunk) = chunk else { break };
        tokio::select! {
            _ = token.cancelled() => break,
            _ = inner.limiter.acquire(chunk.len()) => {}
        }

        let mut offset = 0;
        let mut finished = false;
        while offset < chunk.len() {
            // The segment may have been shortened by a split, or be finished.
            let end_now = {
                let mut s = lock(state);
                match s.segs[*idx].end {
                    Some(e) if pos > e => {
                        if !s.chain(idx, pos) {
                            finished = true;
                            break;
                        }
                        s.segs[*idx].end
                    }
                    end => end,
                }
            };
            let take = match end_now {
                Some(e) => (chunk.len() - offset).min((e + 1 - pos) as usize),
                None => chunk.len() - offset,
            };
            file.write_all(&chunk[offset..offset + take]).await?;
            offset += take;
            pos += take as u64;
            lock(state).segs[*idx].pos = pos;
        }
        file.flush().await?;
        if finished {
            break;
        }
    }
    file.flush().await?;

    if token.is_cancelled() {
        return Ok(());
    }
    let seg = lock(state).segs[*idx];
    match seg.end {
        Some(e) if seg.pos <= e => Err(Error::Truncated),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(segments: usize, min_split: u64) -> EngineConfig {
        EngineConfig {
            segments,
            min_split_size: min_split,
            ..EngineConfig::default()
        }
    }

    #[test]
    fn plan_splits_evenly_and_covers_file() {
        let segs = plan(Some(1000), true, &cfg(4, 100));
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[0].start, 0);
        assert_eq!(segs[3].end, Some(999));
        for pair in segs.windows(2) {
            assert_eq!(pair[0].end.unwrap() + 1, pair[1].start);
        }
    }

    #[test]
    fn plan_respects_min_split_and_ranges() {
        assert_eq!(plan(Some(150), true, &cfg(8, 100)).len(), 1);
        assert_eq!(plan(Some(1000), false, &cfg(8, 1)).len(), 1);
        assert_eq!(plan(None, false, &cfg(8, 1))[0].end, None);
        assert!(plan(Some(0), true, &cfg(8, 1)).is_empty());
    }

    #[test]
    fn chain_takes_over_the_next_idle_segment() {
        let mut state = State::new(plan(Some(1000), true, &cfg(4, 100)));
        assert_eq!(state.claim(false, 100), Some(0));
        assert_eq!(state.claim(false, 100), Some(1));
        let mut idx = 0;
        // Segment 1 is busy, so segment 0 cannot continue into it.
        assert!(!state.chain(&mut idx, 250));
        state.in_use[1] = false;
        assert!(state.chain(&mut idx, 250));
        assert_eq!(idx, 1);
        assert!(state.complete[0] && state.in_use[1]);
        // A segment that already has progress is not taken over.
        state.segs[3].pos = 760;
        let mut idx = 2;
        state.in_use[2] = true;
        assert!(!state.chain(&mut idx, 750));
    }

    #[test]
    fn claim_splits_largest_busy_segment() {
        let mut state = State::new(plan(Some(1000), true, &cfg(1, 100)));
        assert_eq!(state.claim(true, 100), Some(0));
        state.segs[0].pos = 200;
        assert_eq!(state.claim(true, 100), Some(1));
        assert_eq!(state.segs[0].end, Some(599));
        assert_eq!(
            state.segs[1],
            Segment {
                start: 600,
                end: Some(999),
                pos: 600
            }
        );
        // Remaining 400 and 400 are below 2 * 300.
        assert_eq!(state.claim(true, 300), None);
        assert_eq!(state.downloaded(), 200);
    }
}
