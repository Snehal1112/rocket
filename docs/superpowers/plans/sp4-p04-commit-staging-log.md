# SP4-P04: Git2 Commit, Staging, and Log

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `stage`, `unstage`, `discard`, `commit`, and `log` methods on `Git2Service`.

**Tech Stack:** Rust, git2

**Prerequisite:** SP4-P03 complete.

---

## Task 1: Implement stage, unstage, discard

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn stage_and_unstage_file() {
    let (dir, path) = setup_repo();
    fs::write(dir.path().join("test.bru"), "changed").unwrap();
    let svc = Git2Service::new();
    svc.stage(&path, &["test.bru"]).unwrap();
    let status = svc.status(&path).unwrap();
    assert!(status.files.iter().any(|f| f.path == "test.bru" && f.staged));
    svc.unstage(&path, &["test.bru"]).unwrap();
    let status2 = svc.status(&path).unwrap();
    assert!(status2.files.iter().any(|f| f.path == "test.bru" && !f.staged));
}

#[test]
fn discard_reverts_changes() {
    let (dir, path) = setup_repo();
    fs::write(dir.path().join("test.bru"), "changed").unwrap();
    let svc = Git2Service::new();
    svc.discard(&path, &["test.bru"]).unwrap();
    let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
    assert_eq!(content, "meta { name: Test }"); // original content
}
```

- [ ] **Step 2: Implement stage/unstage/discard**

- `stage`: `repo.index()?.add_path()` for each file, then `index.write()`
- `unstage`: `repo.reset_default(Some(head), paths)` to reset index to HEAD
- `discard`: `repo.checkout_head(Some(CheckoutBuilder::new().path(file).force()))` for each file

- [ ] **Step 3: Run tests, commit**

```bash
cargo test -p rocket-git -- git2_service::tests
git commit -am "feat(git): Git2 stage, unstage, discard"
```

---

## Task 2: Implement commit + log

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn commit_and_log() {
    let (dir, path) = setup_repo();
    let svc = Git2Service::new();
    fs::write(dir.path().join("new.bru"), "content").unwrap();
    svc.stage(&path, &["new.bru"]).unwrap();
    let info = svc.commit(&path, "add new request").unwrap();
    assert!(!info.id.is_empty());
    assert_eq!(info.message, "add new request");

    let log = svc.log(&path, 10).unwrap();
    assert!(log.len() >= 2);
    assert_eq!(log[0].message, "add new request");
}

#[test]
fn log_respects_limit() {
    let (dir, path) = setup_repo();
    let svc = Git2Service::new();
    for i in 0..5 {
        fs::write(dir.path().join(format!("f{}.bru", i)), format!("content {}", i)).unwrap();
        svc.stage(&path, &[&format!("f{}.bru", i)]).unwrap();
        svc.commit(&path, &format!("commit {}", i)).unwrap();
    }
    let log = svc.log(&path, 3).unwrap();
    assert_eq!(log.len(), 3);
}
```

- [ ] **Step 2: Implement commit**

```rust
fn commit(&self, path: &str, message: &str) -> DomainResult<CommitInfo> {
    let repo = Self::open_repo(path)?;
    let sig = repo.signature().or_else(|_|
        git2::Signature::now("RocketAPI User", "user@rocketapi.local")
    ).map_err(|e| DomainError::Internal(e.to_string()))?;

    let mut index = repo.index().map_err(|e| DomainError::Internal(e.to_string()))?;
    let tree_id = index.write_tree().map_err(|e| DomainError::Internal(e.to_string()))?;
    let tree = repo.find_tree(tree_id).map_err(|e| DomainError::Internal(e.to_string()))?;

    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();

    let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    Ok(CommitInfo {
        id: oid.to_string()[..7].to_string(),
        full_id: oid.to_string(),
        message: message.to_string(),
        author: sig.name().unwrap_or("").to_string(),
        author_email: sig.email().unwrap_or("").to_string(),
        timestamp: chrono::Utc::now(),
        files_changed: 0, // simplified
    })
}
```

- [ ] **Step 3: Implement log**

```rust
fn log(&self, path: &str, limit: usize) -> DomainResult<Vec<CommitInfo>> {
    let repo = Self::open_repo(path)?;
    let mut revwalk = repo.revwalk().map_err(|e| DomainError::Internal(e.to_string()))?;
    revwalk.push_head().map_err(|e| DomainError::Internal(e.to_string()))?;
    revwalk.set_sorting(git2::Sort::TIME).map_err(|e| DomainError::Internal(e.to_string()))?;

    let mut commits = Vec::new();
    for oid_result in revwalk.take(limit) {
        let oid = oid_result.map_err(|e| DomainError::Internal(e.to_string()))?;
        let commit = repo.find_commit(oid).map_err(|e| DomainError::Internal(e.to_string()))?;
        let time = commit.time();
        let timestamp = chrono::DateTime::from_timestamp(time.seconds(), 0)
            .unwrap_or_default()
            .with_timezone(&chrono::Utc);

        commits.push(CommitInfo {
            id: oid.to_string()[..7].to_string(),
            full_id: oid.to_string(),
            message: commit.message().unwrap_or("").to_string(),
            author: commit.author().name().unwrap_or("").to_string(),
            author_email: commit.author().email().unwrap_or("").to_string(),
            timestamp,
            files_changed: 0,
        });
    }
    Ok(commits)
}
```

- [ ] **Step 4: Run tests, commit**

```bash
cargo test -p rocket-git -- git2_service::tests
git commit -am "feat(git): Git2 commit + log"
```

---

## Milestone Checklist — P04

- [ ] `stage` adds files to index
- [ ] `unstage` resets index to HEAD
- [ ] `discard` reverts working tree to HEAD
- [ ] `commit` creates commit from staged changes
- [ ] `log` returns commit history with limit
- [ ] 4 new tests pass (stage/unstage, discard, commit+log, log limit)
