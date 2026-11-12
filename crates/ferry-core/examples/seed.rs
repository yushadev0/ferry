//! Fills a database with a few real downloads in different states, for
//! looking at the UI. Usage:
//!
//! ```text
//! cargo run -p ferry-core --example seed -- <data-dir>
//! FERRY_DATA_DIR=<data-dir> pnpm tauri dev
//! ```

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ferry_core::{Engine, EngineConfig, Event, Request, Status};

const BASE: &str = "https://fsn1-speed.hetzner.com";

#[tokio::main]
async fn main() -> ferry_core::Result<()> {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: seed <data-dir>"));
    let downloads = dir.join("downloads");
    let engine = Engine::open(dir.join("ferry.db"), EngineConfig::default())?;
    let mut rx = engine.subscribe();
    let add = |name: &str| Request::new(format!("{BASE}/{name}"), &downloads);

    // Finished.
    let done = engine.add(add("100MB.bin")).await?;
    engine.start(done)?;
    wait(&mut rx, done, |s| s == Status::Completed).await;
    println!("finished 100MB.bin");

    // Paused part-way, with its parts on disk.
    let paused = engine.add(add("1GB.bin")).await?;
    engine.start(paused)?;
    loop {
        if let Ok(Event::Progress { id, downloaded, .. }) = rx.recv().await {
            if id == paused && downloaded > 350 * 1024 * 1024 {
                break;
            }
        }
    }
    engine.pause(paused).await?;
    println!("paused 1GB.bin");

    // Scheduled for this time tomorrow.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let later = engine.add(add("10GB.bin")).await?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    engine.schedule(later, now + 86_400).await?;
    println!("scheduled 10GB.bin");

    // Failed: the server no longer has the file.
    tokio::time::sleep(Duration::from_secs(3)).await;
    match engine.add(add("missing-file.bin")).await {
        Ok(_) => println!("unexpectedly added missing-file.bin"),
        Err(e) => println!("missing-file.bin was refused as expected: {e}"),
    }
    Ok(())
}

async fn wait(
    rx: &mut tokio::sync::broadcast::Receiver<Event>,
    id: i64,
    done: impl Fn(Status) -> bool,
) {
    loop {
        if let Ok(Event::Status { id: i, status, .. }) = rx.recv().await {
            if i == id && done(status) {
                return;
            }
        }
    }
}
