mod common;

use std::{sync::atomic::Ordering, time::Duration};

use ferry_core::{DownloadId, Engine, EngineConfig, Error, Event, Request, Status};
use tokio::sync::broadcast::{error::RecvError, Receiver};

fn config() -> EngineConfig {
    EngineConfig {
        segments: 4,
        min_split_size: 64 * 1024,
        retry_base_delay: Duration::from_millis(20),
        progress_interval: Duration::from_millis(20),
        save_interval: Duration::from_millis(20),
        busy_retry_delay: Duration::from_millis(100),
        // The local server is so fast that staggered workers would find the
        // file already downloaded; start them together.
        connection_stagger: Duration::ZERO,
        ..EngineConfig::default()
    }
}

async fn next_event(rx: &mut Receiver<Event>) -> Event {
    loop {
        match tokio::time::timeout(Duration::from_secs(30), rx.recv())
            .await
            .expect("timed out waiting for an engine event")
        {
            Ok(event) => return event,
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => panic!("engine event channel closed"),
        }
    }
}

/// Waits for a status event of `id` matching `want`.
async fn wait_status(rx: &mut Receiver<Event>, id: DownloadId, want: Status) {
    loop {
        if let Event::Status { id: i, status, .. } = next_event(rx).await {
            if i == id && status == want {
                return;
            }
        }
    }
}

/// Waits for the download to complete, fail or pause.
async fn wait_settled(rx: &mut Receiver<Event>, id: DownloadId) -> (Status, Option<String>) {
    loop {
        if let Event::Status {
            id: i,
            status,
            error,
        } = next_event(rx).await
        {
            if i == id && !matches!(status, Status::Downloading | Status::Queued) {
                return (status, error);
            }
        }
    }
}

/// Waits until at least `bytes` have been downloaded.
async fn wait_progress(rx: &mut Receiver<Event>, id: DownloadId, bytes: u64) {
    loop {
        if let Event::Progress {
            id: i, downloaded, ..
        } = next_event(rx).await
        {
            if i == id && downloaded >= bytes {
                return;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn segmented_download_matches_source() {
    let server = common::spawn(common::Options::default()).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(config()).unwrap();
    let mut rx = engine.subscribe();

    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    let info = engine.get(id).unwrap();
    assert_eq!(info.file_name, "test.bin");
    assert_eq!(info.size, Some(server.state.data.len() as u64));
    assert!(info.resumable);

    engine.start(id).unwrap();
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));

    assert_eq!(
        std::fs::read(dir.path().join("test.bin")).unwrap(),
        server.state.data
    );
    assert!(!dir.path().join("test.bin.ferrypart").exists());
    assert!(server.state.range_requests.load(Ordering::SeqCst) >= 4);
    let info = engine.get(id).unwrap();
    assert_eq!(info.downloaded, server.state.data.len() as u64);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn server_without_ranges_or_length() {
    let server = common::spawn(common::Options {
        ranges: false,
        known_length: false,
        ..common::Options::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(config()).unwrap();
    let mut rx = engine.subscribe();

    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    let info = engine.get(id).unwrap();
    assert!(!info.resumable);
    assert_eq!(info.size, None);

    engine.start(id).unwrap();
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));
    assert_eq!(
        std::fs::read(dir.path().join("test.bin")).unwrap(),
        server.state.data
    );
    assert_eq!(
        engine.get(id).unwrap().size,
        Some(server.state.data.len() as u64)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_and_resume_after_restart() {
    let server = common::spawn(common::Options::default()).await;
    *server.state.chunk_delay.lock().unwrap() = Some(Duration::from_millis(10));
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ferry.db");
    let downloads = dir.path().join("downloads");
    let len = server.state.data.len() as u64;

    let engine = Engine::open(&db, config()).unwrap();
    let mut rx = engine.subscribe();
    let id = engine
        .add(Request::new(&server.url, &downloads))
        .await
        .unwrap();
    engine.start(id).unwrap();
    wait_progress(&mut rx, id, len / 2).await;
    engine.pause(id).await.unwrap();

    let paused = engine.get(id).unwrap();
    assert_eq!(paused.status, Status::Paused);
    assert!(paused.downloaded > 0 && paused.downloaded < len);
    assert!(downloads.join("test.bin.ferrypart").exists());
    drop(rx);
    drop(engine);

    // A fresh engine on the same database picks up where we left off.
    *server.state.chunk_delay.lock().unwrap() = None;
    let served_before = server.state.bytes_served.load(Ordering::SeqCst);
    let engine = Engine::open(&db, config()).unwrap();
    let mut rx = engine.subscribe();
    engine.start(id).unwrap();
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));

    assert_eq!(
        std::fs::read(downloads.join("test.bin")).unwrap(),
        server.state.data
    );
    let served_after = server.state.bytes_served.load(Ordering::SeqCst) - served_before;
    // Splitting segments may fetch a little extra, but never the whole file again.
    assert!(
        served_after < len,
        "resume fetched {served_after} bytes of {len}; it should continue, not restart"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changed_file_restarts_from_scratch() {
    let server = common::spawn(common::Options::default()).await;
    *server.state.chunk_delay.lock().unwrap() = Some(Duration::from_millis(10));
    let dir = tempfile::tempdir().unwrap();
    let len = server.state.data.len() as u64;
    let engine = Engine::in_memory(config()).unwrap();
    let mut rx = engine.subscribe();

    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    engine.start(id).unwrap();
    wait_progress(&mut rx, id, len / 4).await;
    engine.pause(id).await.unwrap();

    *server.state.etag.lock().unwrap() = "\"v2\"".into();
    *server.state.chunk_delay.lock().unwrap() = None;
    let served_before = server.state.bytes_served.load(Ordering::SeqCst);
    let mut rx = engine.subscribe(); // skip the events of the first run
    engine.start(id).unwrap();
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));

    let served_after = server.state.bytes_served.load(Ordering::SeqCst) - served_before;
    assert!(served_after >= len, "expected a full re-download");
    assert_eq!(
        std::fs::read(dir.path().join("test.bin")).unwrap(),
        server.state.data
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retries_failed_and_cut_connections() {
    let server = common::spawn(common::Options::default()).await;
    server.state.fail_next.store(3, Ordering::SeqCst);
    server.state.truncate_next.store(3, Ordering::SeqCst);
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(config()).unwrap();
    let mut rx = engine.subscribe();

    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    engine.start(id).unwrap();
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));
    assert_eq!(
        std::fs::read(dir.path().join("test.bin")).unwrap(),
        server.state.data
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn expired_link_is_reported() {
    let server = common::spawn(common::Options::default()).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(config()).unwrap();

    *server.state.status_override.lock().unwrap() = Some(403);
    let err = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::LinkExpired(403)), "{err}");

    // Expiring after the download was added fails it with a clear error.
    *server.state.status_override.lock().unwrap() = None;
    let mut rx = engine.subscribe();
    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    *server.state.status_override.lock().unwrap() = Some(410);
    engine.start(id).unwrap();
    let (status, error) = wait_settled(&mut rx, id).await;
    assert_eq!(status, Status::Failed);
    assert!(error.unwrap().contains("410"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn duplicate_names_get_numbered_and_remove_cleans_up() {
    let server = common::spawn(common::Options {
        size: 200 * 1024,
        ..common::Options::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(config()).unwrap();
    let mut rx = engine.subscribe();

    let a = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    let b = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    assert_eq!(engine.get(b).unwrap().file_name, "test (1).bin");

    engine.start(a).unwrap();
    assert_eq!(wait_settled(&mut rx, a).await.0, Status::Completed);
    engine.remove(a, true).await.unwrap();
    assert!(!dir.path().join("test.bin").exists());
    assert!(matches!(engine.get(a), Err(Error::NotFound(_))));
    assert_eq!(engine.list().unwrap().len(), 1);
}

#[test]
fn start_outside_runtime_is_an_error_not_a_panic() {
    let engine = Engine::in_memory(config()).unwrap();
    let err = engine.start(1).unwrap_err();
    assert!(matches!(err, Error::InvalidState(_)), "{err}");
}

#[tokio::test]
async fn connection_errors_name_the_cause() {
    // Nothing listens on this port, so the connection is refused.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/file", listener.local_addr().unwrap());
    drop(listener);
    let engine = Engine::in_memory(config()).unwrap();
    let message = engine.probe(&url, &[]).await.unwrap_err().to_string();
    assert!(message.contains("could not connect"), "{message}");
    assert!(!message.contains("error sending request"), "{message}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn adapts_to_servers_that_limit_connections() {
    let server = common::spawn(common::Options::default()).await;
    *server.state.max_connections.lock().unwrap() = Some(2);
    // Slow the body down so connections overlap even on fast machines;
    // otherwise each could finish before the next one asks.
    *server.state.chunk_delay.lock().unwrap() = Some(Duration::from_millis(2));
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(EngineConfig {
        segments: 8,
        ..config()
    })
    .unwrap();
    let mut rx = engine.subscribe();

    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    engine.start(id).unwrap();
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));
    assert_eq!(
        std::fs::read(dir.path().join("test.bin")).unwrap(),
        server.state.data
    );
    assert!(server.state.refused.load(Ordering::SeqCst) > 0);
    assert!(server.state.peak_connections.load(Ordering::SeqCst) <= 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn single_connection_server_still_completes() {
    let server = common::spawn(common::Options::default()).await;
    *server.state.max_connections.lock().unwrap() = Some(1);
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(config()).unwrap();
    let mut rx = engine.subscribe();

    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    engine.start(id).unwrap();
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));
    assert_eq!(
        std::fs::read(dir.path().join("test.bin")).unwrap(),
        server.state.data
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn request_rate_limits_are_handled_with_few_requests() {
    let server = common::spawn(common::Options::default()).await;
    *server.state.min_request_interval.lock().unwrap() = Some(Duration::from_millis(250));
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(EngineConfig {
        segments: 8,
        ..config()
    })
    .unwrap();
    let mut rx = engine.subscribe();

    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    engine.start(id).unwrap();
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));
    assert_eq!(
        std::fs::read(dir.path().join("test.bin")).unwrap(),
        server.state.data
    );

    // Accepted connections carry on into neighbouring segments instead of
    // asking for each one separately.
    let accepted = server.state.range_requests.load(Ordering::SeqCst);
    assert!(accepted <= 3, "{accepted} range requests were needed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inspect_add_and_start_need_one_probe() {
    let server = common::spawn(common::Options::default()).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(config()).unwrap();
    let mut rx = engine.subscribe();

    let info = engine.probe(&server.url, &[]).await.unwrap();
    assert_eq!(info.filename.as_deref(), Some("test.bin"));
    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    engine.start(id).unwrap();
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));
    assert_eq!(server.state.probe_requests.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queue_runs_one_download_at_a_time() {
    let server = common::spawn(common::Options {
        size: 400 * 1024,
        ..common::Options::default()
    })
    .await;
    *server.state.chunk_delay.lock().unwrap() = Some(Duration::from_millis(5));
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(EngineConfig {
        max_active: 1,
        ..config()
    })
    .unwrap();
    let mut rx = engine.subscribe();

    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(
            engine
                .add(Request::new(&server.url, dir.path()))
                .await
                .unwrap(),
        );
    }
    for &id in &ids {
        engine.start(id).unwrap();
    }

    let mut running = std::collections::HashSet::new();
    let mut finished = Vec::new();
    while finished.len() < ids.len() {
        if let Event::Status { id, status, .. } = next_event(&mut rx).await {
            match status {
                Status::Downloading => {
                    running.insert(id);
                    assert_eq!(running.len(), 1, "more than one download ran at once");
                }
                Status::Completed => {
                    running.remove(&id);
                    finished.push(id);
                }
                Status::Failed | Status::Paused => panic!("unexpected {status:?}"),
                Status::Queued => {}
            }
        }
    }
    // First in, first out.
    assert_eq!(finished, ids);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn holding_the_queue_requeues_without_losing_progress() {
    let server = common::spawn(common::Options::default()).await;
    *server.state.chunk_delay.lock().unwrap() = Some(Duration::from_millis(10));
    let dir = tempfile::tempdir().unwrap();
    let len = server.state.data.len() as u64;
    let engine = Engine::in_memory(config()).unwrap();
    let mut rx = engine.subscribe();

    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    engine.start(id).unwrap();
    wait_progress(&mut rx, id, len / 2).await;

    engine.set_queue_running(false).await;
    let held = engine.get(id).unwrap();
    assert_eq!(held.status, Status::Queued);
    assert!(held.downloaded >= len / 2);

    // Starting it again while the queue is held only queues it.
    engine.start(id).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(engine.get(id).unwrap().status, Status::Queued);

    *server.state.chunk_delay.lock().unwrap() = None;
    let served_before = server.state.bytes_served.load(Ordering::SeqCst);
    let mut rx = engine.subscribe();
    engine.set_queue_running(true).await;
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));
    let served_after = server.state.bytes_served.load(Ordering::SeqCst) - served_before;
    assert!(served_after < len, "it restarted instead of continuing");
    assert_eq!(
        std::fs::read(dir.path().join("test.bin")).unwrap(),
        server.state.data
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_now_ignores_a_held_queue() {
    let server = common::spawn(common::Options::default()).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(config()).unwrap();
    let mut rx = engine.subscribe();
    engine.set_queue_running(false).await;

    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    engine.start_now(id).unwrap();
    wait_status(&mut rx, id, Status::Downloading).await;
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn speed_limit_caps_throughput() {
    let server = common::spawn(common::Options {
        size: 1024 * 1024,
        ..common::Options::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(EngineConfig {
        speed_limit: 512 * 1024,
        ..config()
    })
    .unwrap();
    let mut rx = engine.subscribe();

    let id = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    let started = std::time::Instant::now();
    engine.start(id).unwrap();
    assert_eq!(wait_settled(&mut rx, id).await, (Status::Completed, None));
    let elapsed = started.elapsed();
    // 1 MiB at 512 KiB/s takes about two seconds.
    assert!(
        elapsed >= Duration::from_millis(1600),
        "finished in {elapsed:?}"
    );
    assert_eq!(
        std::fs::read(dir.path().join("test.bin")).unwrap(),
        server.state.data
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_all_and_queued_downloads() {
    let server = common::spawn(common::Options::default()).await;
    *server.state.chunk_delay.lock().unwrap() = Some(Duration::from_millis(10));
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::in_memory(EngineConfig {
        max_active: 1,
        ..config()
    })
    .unwrap();
    let mut rx = engine.subscribe();
    let a = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    let b = engine
        .add(Request::new(&server.url, dir.path()))
        .await
        .unwrap();
    assert_eq!(engine.get(a).unwrap().status, Status::Paused);
    engine.start(a).unwrap();
    engine.start(b).unwrap();
    wait_status(&mut rx, a, Status::Downloading).await;
    assert_eq!(engine.get(b).unwrap().status, Status::Queued);

    engine.pause_all().await;
    assert_eq!(engine.get(a).unwrap().status, Status::Paused);
    assert_eq!(engine.get(b).unwrap().status, Status::Paused);
    assert!(engine.active_ids().is_empty());
    assert!(engine.queue_running());
}
