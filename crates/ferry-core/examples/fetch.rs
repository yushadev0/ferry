//! Downloads a URL with the engine and prints events. Handy for trying real
//! servers by hand.
//!
//! ```text
//! cargo run -p ferry-core --example fetch -- <url> <dir> [segments] [pause-at-percent]
//! ```
//!
//! With `pause-at-percent`, the download is paused at that point, the engine is
//! closed and reopened from its database, and the download resumes, the same
//! path the app takes when it is restarted mid-download.

use std::time::Instant;

use ferry_core::{DownloadId, Engine, EngineConfig, Event, Request, Status};
use tokio::sync::broadcast::Receiver;

#[tokio::main]
async fn main() -> ferry_core::Result<()> {
    let mut args = std::env::args().skip(1);
    let url = args
        .next()
        .expect("usage: fetch <url> <dir> [segments] [pause-at-percent]");
    let dir = std::path::PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let segments = args.next().map_or(8, |s| s.parse().expect("segments"));
    let pause_at: Option<f64> = args.next().map(|s| s.parse().expect("pause-at-percent"));

    let db = dir.join("fetch-example.db");
    let config = EngineConfig {
        segments,
        ..EngineConfig::default()
    };
    let started = Instant::now();

    let engine = Engine::open(&db, config.clone())?;
    let mut rx = engine.subscribe();
    let id = engine.add(Request::new(url, &dir)).await?;
    let info = engine.get(id)?;
    println!(
        "file={} size={:?} resumable={}",
        info.file_name, info.size, info.resumable
    );
    engine.start(id)?;

    if let Some(percent) = pause_at {
        let size = info.size.expect("pausing needs a known size") as f64;
        follow(&mut rx, id, started, Some(size * percent / 100.0)).await;
        engine.pause(id).await?;
        let paused = engine.get(id)?;
        println!(
            "paused at {} bytes; reopening the engine",
            paused.downloaded
        );
        drop(rx);
        drop(engine);

        let engine = Engine::open(&db, config)?;
        let mut rx = engine.subscribe();
        engine.start(id)?;
        follow(&mut rx, id, started, None).await;
    } else {
        follow(&mut rx, id, started, None).await;
    }
    let _ = std::fs::remove_file(&db);
    Ok(())
}

/// Prints progress until the download settles, or until `stop_at` bytes.
async fn follow(rx: &mut Receiver<Event>, id: DownloadId, started: Instant, stop_at: Option<f64>) {
    loop {
        match rx.recv().await {
            Ok(Event::Progress {
                id: i,
                downloaded,
                total,
                speed,
            }) if i == id => {
                println!(
                    "{:6.1}s {:>10} / {:?}  {:>7.2} MB/s",
                    started.elapsed().as_secs_f64(),
                    downloaded,
                    total,
                    speed as f64 / 1e6
                );
                if stop_at.is_some_and(|at| downloaded as f64 >= at) {
                    return;
                }
            }
            Ok(Event::Status {
                id: i,
                status,
                error,
            }) if i == id && status != Status::Downloading => {
                println!(
                    "{status:?} {error:?} after {:.1}s",
                    started.elapsed().as_secs_f64()
                );
                return;
            }
            Ok(_) => {}
            Err(e) => println!("recv: {e}"),
        }
    }
}
