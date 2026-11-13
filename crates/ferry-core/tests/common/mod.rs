//! A local HTTP server with knobs for range support, failures and throttling.

use std::{
    io,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::Response,
    routing::get,
    Router,
};

pub struct Options {
    pub size: usize,
    /// Honour `Range` requests.
    pub ranges: bool,
    /// Send `Content-Length` on full (200) responses.
    pub known_length: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            size: 3 * 1024 * 1024 + 123,
            ranges: true,
            known_length: true,
        }
    }
}

pub struct ServerState {
    pub data: Vec<u8>,
    pub ranges: bool,
    pub known_length: bool,
    pub etag: Mutex<String>,
    /// Answer this many download requests with HTTP 500.
    pub fail_next: AtomicUsize,
    /// Cut this many download responses off half-way.
    pub truncate_next: AtomicUsize,
    /// Answer every request with this status.
    pub status_override: Mutex<Option<u16>>,
    pub chunk_delay: Mutex<Option<Duration>>,
    pub bytes_served: AtomicU64,
    /// Bytes sent while at least one other download was open.
    pub parallel_bytes: AtomicU64,
    pub range_requests: AtomicUsize,
    pub probe_requests: AtomicUsize,
    /// Refuse downloads beyond this many open at once with HTTP 429.
    pub max_connections: Mutex<Option<usize>>,
    pub open_connections: AtomicUsize,
    pub peak_connections: AtomicUsize,
    pub refused: AtomicUsize,
    /// Like nginx `limit_req`: refuse requests (with 429, no Retry-After)
    /// arriving sooner than this after the last accepted one.
    pub min_request_interval: Mutex<Option<Duration>>,
    /// Requests let through despite `min_request_interval`, like nginx's `burst`.
    pub request_burst: AtomicUsize,
    last_accepted: Mutex<Option<Instant>>,
}

/// Counts an open download response until its body is dropped.
struct OpenConnection(Arc<ServerState>);

impl Drop for OpenConnection {
    fn drop(&mut self) {
        self.0.open_connections.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct Server {
    pub url: String,
    pub state: Arc<ServerState>,
}

pub async fn spawn(opts: Options) -> Server {
    let mut seed: u32 = 0x1234_5678;
    let data = (0..opts.size)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 24) as u8
        })
        .collect();
    let state = Arc::new(ServerState {
        data,
        ranges: opts.ranges,
        known_length: opts.known_length,
        etag: Mutex::new("\"v1\"".into()),
        fail_next: AtomicUsize::new(0),
        truncate_next: AtomicUsize::new(0),
        status_override: Mutex::new(None),
        chunk_delay: Mutex::new(None),
        bytes_served: AtomicU64::new(0),
        parallel_bytes: AtomicU64::new(0),
        range_requests: AtomicUsize::new(0),
        probe_requests: AtomicUsize::new(0),
        max_connections: Mutex::new(None),
        open_connections: AtomicUsize::new(0),
        peak_connections: AtomicUsize::new(0),
        refused: AtomicUsize::new(0),
        min_request_interval: Mutex::new(None),
        request_burst: AtomicUsize::new(0),
        last_accepted: Mutex::new(None),
    });
    let app = Router::new()
        .route("/files/{name}", get(serve))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Server {
        url: format!("http://{addr}/files/test.bin"),
        state,
    }
}

fn parse_range(value: &str) -> Option<(u64, Option<u64>)> {
    let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
    let end = if end.is_empty() {
        None
    } else {
        Some(end.parse().ok()?)
    };
    Some((start.parse().ok()?, end))
}

fn take_one(counter: &AtomicUsize) -> bool {
    let mut current = counter.load(Ordering::SeqCst);
    while current > 0 {
        match counter.compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return true,
            Err(actual) => current = actual,
        }
    }
    false
}

async fn serve(State(st): State<Arc<ServerState>>, headers: HeaderMap) -> Response {
    if let Some(code) = *st.status_override.lock().unwrap() {
        return Response::builder()
            .status(code)
            .body(Body::empty())
            .unwrap();
    }
    if let Some(interval) = *st.min_request_interval.lock().unwrap() {
        let mut last = st.last_accepted.lock().unwrap();
        if last.is_some_and(|t| t.elapsed() < interval) && !take_one(&st.request_burst) {
            st.refused.fetch_add(1, Ordering::SeqCst);
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .body(Body::empty())
                .unwrap();
        }
        *last = Some(Instant::now());
    }
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let is_probe = range.as_deref() == Some("bytes=0-0");
    if is_probe {
        st.probe_requests.fetch_add(1, Ordering::SeqCst);
    }
    if !is_probe && take_one(&st.fail_next) {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap();
    }

    let connection = if is_probe {
        None
    } else {
        let open = st.open_connections.fetch_add(1, Ordering::SeqCst) + 1;
        let guard = OpenConnection(st.clone());
        if st
            .max_connections
            .lock()
            .unwrap()
            .is_some_and(|max| open > max)
        {
            st.refused.fetch_add(1, Ordering::SeqCst);
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header(header::RETRY_AFTER, "0")
                .body(Body::empty())
                .unwrap();
        }
        st.peak_connections.fetch_max(open, Ordering::SeqCst);
        Some(guard)
    };

    let len = st.data.len() as u64;
    let (status, start, end) = match (st.ranges, range.as_deref().and_then(parse_range)) {
        (true, Some((start, end))) => (
            StatusCode::PARTIAL_CONTENT,
            start,
            end.unwrap_or(len - 1).min(len - 1),
        ),
        _ => (StatusCode::OK, 0, len - 1),
    };
    if status == StatusCode::PARTIAL_CONTENT && !is_probe {
        st.range_requests.fetch_add(1, Ordering::SeqCst);
    }
    let truncate = !is_probe && take_one(&st.truncate_next);
    let delay = *st.chunk_delay.lock().unwrap();
    let cut_at = start + (end - start).div_ceil(2);

    let body_state = st.clone();
    let stream = futures_util::stream::unfold(start, move |pos| {
        let st = body_state.clone();
        let _open = &connection;
        async move {
            if pos > end {
                return None;
            }
            if truncate && pos >= cut_at {
                return Some((Err(io::Error::other("connection cut")), end + 1));
            }
            if let Some(d) = delay {
                tokio::time::sleep(d).await;
            }
            let mut next = (pos + 16 * 1024).min(end + 1);
            if truncate && pos < cut_at {
                next = next.min(cut_at);
            }
            st.bytes_served.fetch_add(next - pos, Ordering::SeqCst);
            if st.open_connections.load(Ordering::SeqCst) >= 2 {
                st.parallel_bytes.fetch_add(next - pos, Ordering::SeqCst);
            }
            let bytes = Bytes::copy_from_slice(&st.data[pos as usize..next as usize]);
            Some((Ok::<_, io::Error>(bytes), next))
        }
    });

    let mut resp = Response::builder()
        .status(status)
        .header(header::ETAG, st.etag.lock().unwrap().clone())
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"test.bin\"",
        );
    if st.ranges {
        resp = resp.header(header::ACCEPT_RANGES, "bytes");
    }
    if status == StatusCode::PARTIAL_CONTENT {
        resp = resp.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    if st.known_length || status == StatusCode::PARTIAL_CONTENT {
        resp = resp.header(header::CONTENT_LENGTH, end - start + 1);
    }
    resp.body(Body::from_stream(stream)).unwrap()
}
