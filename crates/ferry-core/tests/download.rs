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

/// Waits for the download to complete, fail or pause.
async fn wait_settled(rx: &mut Receiver<Event>, id: DownloadId) -> (Status, Option<String>) {
    loop {
        if let Event::Status {
            id: i,
            status,
            error,
        } = next_event(rx).await
        {
            if i == id && status != Status::Downloading {
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
