use chrono::{DateTime, Utc};
use rocket_audit::{event::SecurityAuditEvent, repository::AuditLogRepository};
use rocket_shared::error::DomainResult;
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::Mutex,
};

pub struct FsAuditLogRepo {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl FsAuditLogRepo {
    pub fn new(path: PathBuf) -> DomainResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Self {
            path,
            write_lock: Mutex::new(()),
        })
    }

    fn read_lines(&self) -> DomainResult<Vec<SecurityAuditEvent>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut out = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<SecurityAuditEvent>(&line) {
                Ok(ev) => out.push(ev),
                Err(e) => {
                    tracing::warn!(error = %e, "skipping corrupt audit log line");
                }
            }
        }
        Ok(out)
    }
}

impl AuditLogRepository for FsAuditLogRepo {
    fn append(&self, event: &SecurityAuditEvent) -> DomainResult<()> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(event)?;
        // Write the JSON line and newline in one call to avoid a partial-line on crash.
        let mut record = line.into_bytes();
        record.push(b'\n');
        file.write_all(&record)?;
        file.sync_data()?;
        Ok(())
    }

    fn load_all(&self) -> DomainResult<Vec<SecurityAuditEvent>> {
        self.read_lines()
    }

    fn load_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DomainResult<Vec<SecurityAuditEvent>> {
        Ok(self
            .read_lines()?
            .into_iter()
            .filter(|e| e.occurred_at >= start && e.occurred_at <= end)
            .collect())
    }

    fn latest(&self) -> DomainResult<Option<SecurityAuditEvent>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let mut file = File::open(&self.path)?;
        let len = file.seek(SeekFrom::End(0))?;
        if len == 0 {
            return Ok(None);
        }

        // Scan backwards in 512-byte chunks to find the last newline.
        const CHUNK: u64 = 512;
        let mut search_end = len;
        // Skip a trailing newline at the very end of the file.
        if search_end > 0 {
            file.seek(SeekFrom::Start(search_end - 1))?;
            let mut tail = [0u8; 1];
            file.read_exact(&mut tail)?;
            if tail[0] == b'\n' {
                search_end -= 1;
            }
        }
        if search_end == 0 {
            return Ok(None);
        }

        let mut last_newline: Option<u64> = None;
        let mut pos = search_end;
        loop {
            let chunk_start = pos.saturating_sub(CHUNK);
            let chunk_len = (pos - chunk_start) as usize;
            file.seek(SeekFrom::Start(chunk_start))?;
            let mut buf = vec![0u8; chunk_len];
            file.read_exact(&mut buf)?;
            // Search backwards within the chunk.
            for i in (0..chunk_len).rev() {
                if buf[i] == b'\n' {
                    last_newline = Some(chunk_start + i as u64);
                    break;
                }
            }
            if last_newline.is_some() || chunk_start == 0 {
                break;
            }
            pos = chunk_start;
        }

        let line_start = last_newline.map(|p| p + 1).unwrap_or(0);
        let line_len = (search_end - line_start) as usize;
        file.seek(SeekFrom::Start(line_start))?;
        let mut line_buf = vec![0u8; line_len];
        file.read_exact(&mut line_buf)?;
        let line = String::from_utf8_lossy(&line_buf);
        let line = line.trim();
        if line.is_empty() {
            return Ok(None);
        }
        let ev: SecurityAuditEvent = serde_json::from_str(line)?;
        Ok(Some(ev))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_audit::{
        chain::{hash_event, verify_chain, ChainVerification},
        event::AuditEventKind,
    };
    use tempfile::TempDir;

    fn mk_event(prev: &str) -> SecurityAuditEvent {
        let mut ev = SecurityAuditEvent::new(
            "actor",
            None,
            AuditEventKind::CollectionDeleted {
                collection: "x".into(),
            },
            prev,
        );
        ev.hash = hash_event(&ev);
        ev
    }

    #[test]
    fn append_then_load_round_trip() {
        let dir = TempDir::new().unwrap();
        let repo = FsAuditLogRepo::new(dir.path().join("audit.jsonl")).unwrap();
        let a = mk_event("");
        let b = mk_event(&a.hash);
        repo.append(&a).unwrap();
        repo.append(&b).unwrap();
        let loaded = repo.load_all().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(verify_chain(&loaded), ChainVerification::Ok);
    }

    #[test]
    fn load_range_filters_by_occurred_at() {
        let dir = TempDir::new().unwrap();
        let repo = FsAuditLogRepo::new(dir.path().join("audit.jsonl")).unwrap();
        let a = mk_event("");
        repo.append(&a).unwrap();
        // Load a range that excludes the event.
        let range = repo
            .load_range(
                Utc::now() + chrono::Duration::hours(1),
                Utc::now() + chrono::Duration::hours(2),
            )
            .unwrap();
        assert!(range.is_empty());
    }

    #[test]
    fn latest_returns_most_recent() {
        let dir = TempDir::new().unwrap();
        let repo = FsAuditLogRepo::new(dir.path().join("audit.jsonl")).unwrap();
        let a = mk_event("");
        let b = mk_event(&a.hash);
        repo.append(&a).unwrap();
        repo.append(&b).unwrap();
        let latest = repo.latest().unwrap().unwrap();
        assert_eq!(latest.hash, b.hash);
    }

    #[test]
    fn load_all_skips_corrupt_lines_and_returns_valid_events() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("audit.jsonl");
        let repo = FsAuditLogRepo::new(path.clone()).unwrap();

        // Write a file with one valid line, one corrupt line, then another valid line.
        let a = mk_event("");
        let b = mk_event(&a.hash);
        let valid_a = serde_json::to_string(&a).unwrap();
        let valid_b = serde_json::to_string(&b).unwrap();
        std::fs::write(&path, format!("{valid_a}\nnot valid json\n{valid_b}\n")).unwrap();

        let events = repo.load_all().unwrap();
        assert_eq!(events.len(), 2, "expected 2 valid events, got {}", events.len());
        assert_eq!(events[0].hash, a.hash);
        assert_eq!(events[1].hash, b.hash);
    }

    #[test]
    fn append_is_callable_multiple_times() {
        let dir = TempDir::new().unwrap();
        let repo = FsAuditLogRepo::new(dir.path().join("audit.jsonl")).unwrap();
        for i in 0..3u64 {
            let ev = SecurityAuditEvent::new(
                "test_actor",
                None,
                AuditEventKind::CollectionDeleted {
                    collection: format!("coll-{i}"),
                },
                "",
            );
            repo.append(&ev).unwrap();
        }
        let events = repo.load_all().unwrap();
        assert_eq!(events.len(), 3);
    }
}
