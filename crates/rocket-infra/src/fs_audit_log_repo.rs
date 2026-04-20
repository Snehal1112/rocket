use chrono::{DateTime, Utc};
use rocket_audit::{event::SecurityAuditEvent, repository::AuditLogRepository};
use rocket_shared::error::DomainResult;
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
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
            let ev: SecurityAuditEvent = serde_json::from_str(&line)?;
            out.push(ev);
        }
        Ok(out)
    }
}

impl AuditLogRepository for FsAuditLogRepo {
    fn append(&self, event: &SecurityAuditEvent) -> DomainResult<()> {
        let _guard = self.write_lock.lock().expect("audit write-lock poisoned");
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
        Ok(self.read_lines()?.into_iter().last())
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
}
