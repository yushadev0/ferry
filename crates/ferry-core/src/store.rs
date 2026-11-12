use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::{download::Segment, Error, Result};

pub type DownloadId = i64;

/// Lifecycle state of a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    /// Waits for its start time, then joins the queue.
    Scheduled,
    Downloading,
    Paused,
    Completed,
    Failed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Queued => "queued",
            Status::Scheduled => "scheduled",
            Status::Downloading => "downloading",
            Status::Paused => "paused",
            Status::Completed => "completed",
            Status::Failed => "failed",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "queued" => Status::Queued,
            "scheduled" => Status::Scheduled,
            "downloading" => Status::Downloading,
            "paused" => Status::Paused,
            "completed" => Status::Completed,
            _ => Status::Failed,
        }
    }
}

/// A download as stored in the database.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadInfo {
    pub id: DownloadId,
    pub url: String,
    pub dir: PathBuf,
    pub file_name: String,
    pub size: Option<u64>,
    pub downloaded: u64,
    pub resumable: bool,
    pub status: Status,
    pub error: Option<String>,
    /// Unix timestamp (seconds).
    pub created_at: i64,
    /// When a `scheduled` download joins the queue (Unix seconds).
    pub start_at: Option<i64>,
    /// Byte ranges and how far each got; empty once finished.
    pub segments: Vec<Segment>,
    #[serde(skip)]
    pub etag: Option<String>,
    #[serde(skip)]
    pub last_modified: Option<String>,
    /// Extra request headers (cookies, referer...). Never serialized.
    #[serde(skip)]
    pub headers: Vec<(String, String)>,
}

pub(crate) struct NewDownload<'a> {
    pub url: &'a str,
    pub dir: &'a Path,
    pub file_name: &'a str,
    pub size: Option<u64>,
    pub resumable: bool,
    pub etag: Option<&'a str>,
    pub last_modified: Option<&'a str>,
    pub headers: &'a [(String, String)],
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS downloads (
    id            INTEGER PRIMARY KEY,
    url           TEXT    NOT NULL,
    dir           TEXT    NOT NULL,
    file_name     TEXT    NOT NULL,
    size          INTEGER,
    downloaded    INTEGER NOT NULL DEFAULT 0,
    resumable     INTEGER NOT NULL,
    etag          TEXT,
    last_modified TEXT,
    headers       TEXT    NOT NULL,
    status        TEXT    NOT NULL,
    error         TEXT,
    created_at    INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS segments (
    download_id INTEGER NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    start       INTEGER NOT NULL,
    end         INTEGER,
    pos         INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS segments_download ON segments(download_id);
";

const COLUMNS: &str = "id, url, dir, file_name, size, downloaded, resumable, status, error, \
                       created_at, etag, last_modified, headers, start_at";

pub(crate) struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn insert(&self, d: &NewDownload<'_>) -> Result<DownloadId> {
        let headers =
            serde_json::to_string(d.headers).map_err(|e| Error::Internal(e.to_string()))?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |t| t.as_secs() as i64);
        let conn = self.conn();
        conn.execute(
            "INSERT INTO downloads (url, dir, file_name, size, resumable, etag, last_modified, \
             headers, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                d.url,
                d.dir.to_string_lossy(),
                d.file_name,
                d.size.map(|s| s as i64),
                d.resumable,
                d.etag,
                d.last_modified,
                headers,
                Status::Paused.as_str(),
                now
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get(&self, id: DownloadId) -> Result<DownloadInfo> {
        let mut info = self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM downloads WHERE id = ?1"),
                [id],
                row_to_info,
            )
            .optional()?
            .ok_or(Error::NotFound(id))?;
        info.segments = self.load_segments(id)?;
        Ok(info)
    }

    pub fn list(&self) -> Result<Vec<DownloadInfo>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM downloads ORDER BY id"))?;
        let rows: Vec<DownloadInfo> = stmt
            .query_map([], row_to_info)?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        drop(conn);
        rows.into_iter()
            .map(|mut info| {
                info.segments = self.load_segments(info.id)?;
                Ok(info)
            })
            .collect()
    }

    pub fn name_taken(&self, dir: &Path, name: &str) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM downloads WHERE dir = ?1 AND file_name = ?2)",
            params![dir.to_string_lossy(), name],
            |r| r.get(0),
        )?)
    }

    pub fn set_status(&self, id: DownloadId, status: Status, error: Option<&str>) -> Result<()> {
        // Only `scheduled` keeps a start time.
        self.conn().execute(
            "UPDATE downloads SET status = ?2, error = ?3, \
             start_at = CASE WHEN ?2 = 'scheduled' THEN start_at END WHERE id = ?1",
            params![id, status.as_str(), error],
        )?;
        Ok(())
    }

    pub fn update_meta(
        &self,
        id: DownloadId,
        size: Option<u64>,
        resumable: bool,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE downloads SET size = ?2, resumable = ?3, etag = ?4, last_modified = ?5 \
             WHERE id = ?1",
            params![id, size.map(|s| s as i64), resumable, etag, last_modified],
        )?;
        Ok(())
    }

    pub fn set_file_name(&self, id: DownloadId, name: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE downloads SET file_name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    pub fn save_segments(&self, id: DownloadId, segs: &[Segment], downloaded: u64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM segments WHERE download_id = ?1", [id])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO segments (download_id, start, end, pos) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for s in segs {
                stmt.execute(params![
                    id,
                    s.start as i64,
                    s.end.map(|e| e as i64),
                    s.pos as i64
                ])?;
            }
        }
        tx.execute(
            "UPDATE downloads SET downloaded = ?2 WHERE id = ?1",
            params![id, downloaded as i64],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn load_segments(&self, id: DownloadId) -> Result<Vec<Segment>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT start, end, pos FROM segments WHERE download_id = ?1 ORDER BY start",
        )?;
        let rows = stmt.query_map([id], |r| {
            Ok(Segment {
                start: r.get::<_, i64>(0)? as u64,
                end: r.get::<_, Option<i64>>(1)?.map(|e| e as u64),
                pos: r.get::<_, i64>(2)? as u64,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Marks a download as fully written: records its size and drops segments.
    pub fn finish(&self, id: DownloadId, size: u64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM segments WHERE download_id = ?1", [id])?;
        tx.execute(
            "UPDATE downloads SET size = ?2, downloaded = ?2 WHERE id = ?1",
            params![id, size as i64],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete(&self, id: DownloadId) -> Result<()> {
        self.conn()
            .execute("DELETE FROM downloads WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Marks a download to join the queue at `at` (Unix seconds).
    pub fn schedule(&self, id: DownloadId, at: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE downloads SET status = 'scheduled', error = NULL, start_at = ?2 WHERE id = ?1",
            params![id, at],
        )?;
        Ok(())
    }

    /// Scheduled downloads whose time has come.
    pub fn due_scheduled(&self, now: i64) -> Result<Vec<DownloadId>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id FROM downloads WHERE status = 'scheduled' AND start_at <= ?1 \
             ORDER BY start_at, id",
        )?;
        let rows = stmt.query_map([now], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Puts a download at the back of the queue.
    pub fn enqueue(&self, id: DownloadId) -> Result<()> {
        self.conn().execute(
            "UPDATE downloads SET status = 'queued', error = NULL, start_at = NULL, \
             queue_pos = (SELECT COALESCE(MAX(queue_pos), 0) + 1 FROM downloads) \
             WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    /// Queued downloads, first in line first.
    pub fn queued_ids(&self) -> Result<Vec<DownloadId>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT id FROM downloads WHERE status = 'queued' ORDER BY queue_pos, id")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Downloads left in `downloading` by a crash become `paused`.
    pub fn reset_interrupted(&self) -> Result<()> {
        self.conn().execute(
            "UPDATE downloads SET status = 'paused' WHERE status = 'downloading'",
            [],
        )?;
        Ok(())
    }
}

fn row_to_info(r: &Row<'_>) -> rusqlite::Result<DownloadInfo> {
    let headers: String = r.get(12)?;
    Ok(DownloadInfo {
        id: r.get(0)?,
        url: r.get(1)?,
        dir: PathBuf::from(r.get::<_, String>(2)?),
        file_name: r.get(3)?,
        size: r.get::<_, Option<i64>>(4)?.map(|s| s as u64),
        downloaded: r.get::<_, i64>(5)? as u64,
        resumable: r.get(6)?,
        status: Status::parse(&r.get::<_, String>(7)?),
        error: r.get(8)?,
        created_at: r.get(9)?,
        etag: r.get(10)?,
        last_modified: r.get(11)?,
        headers: serde_json::from_str(&headers).unwrap_or_default(),
        start_at: r.get(13)?,
        segments: Vec::new(),
    })
}

/// Schema changes after the first release, keyed by `PRAGMA user_version`.
fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        let has_queue_pos = conn
            .prepare("SELECT 1 FROM pragma_table_info('downloads') WHERE name = 'queue_pos'")?
            .exists([])?;
        if !has_queue_pos {
            conn.execute_batch("ALTER TABLE downloads ADD COLUMN queue_pos INTEGER")?;
        }
        conn.pragma_update(None, "user_version", 1)?;
    }
    if version < 2 {
        add_column(conn, "start_at", "INTEGER")?;
        conn.pragma_update(None, "user_version", 2)?;
    }
    Ok(())
}

fn add_column(conn: &Connection, name: &str, kind: &str) -> Result<()> {
    let exists = conn
        .prepare("SELECT 1 FROM pragma_table_info('downloads') WHERE name = ?1")?
        .exists([name])?;
    if !exists {
        conn.execute_batch(&format!("ALTER TABLE downloads ADD COLUMN {name} {kind}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_databases_from_before_the_queue() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            // The first release's schema had no queue position.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute(
                "INSERT INTO downloads (url, dir, file_name, resumable, headers, status, created_at) \
                 VALUES ('http://x/a', '.', 'a', 1, '[]', 'paused', 0)",
                [],
            )
            .unwrap();
        }
        let store = Store::open(&path).unwrap();
        store.enqueue(1).unwrap();
        assert_eq!(store.queued_ids().unwrap(), vec![1]);
        assert_eq!(store.get(1).unwrap().status, Status::Queued);
        // Opening again is a no-op.
        drop(store);
        Store::open(&path).unwrap();
    }
}
