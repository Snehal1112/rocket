# SP4-P01: Crate Scaffold + Status Value Objects

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the `rocket-git` crate in the Cargo workspace with `GitStatus`, `FileStatus`, and `RepoStatus` value objects.

**Architecture:** New DDD bounded context crate. Depends only on `rocket-shared`, `serde`, `chrono`.

**Tech Stack:** Rust, serde, rocket-shared

---

## Task 1: Create crate and add to workspace

**Files:**
- Create: `crates/rocket-git/Cargo.toml`
- Create: `crates/rocket-git/src/lib.rs` (empty stub with module declarations)
- Modify: `Cargo.toml` (workspace root — add member + dependency)

- [ ] **Step 1: Create `crates/rocket-git/Cargo.toml`**

```toml
[package]
name = "rocket-git"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared.workspace = true
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
git2 = { version = "0.19", features = ["ssh"] }
log.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Add to workspace root `Cargo.toml`**

Add `"crates/rocket-git"` to `[workspace] members`.

Add to `[workspace.dependencies]`:
```toml
rocket-git = { path = "crates/rocket-git" }
```

- [ ] **Step 3: Create stub `lib.rs`**

```rust
pub mod status;
```

- [ ] **Step 4: Verify workspace compiles (will fail on missing module — that's expected)**

```bash
cargo check --workspace 2>&1 | head -5
```

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/rocket-git/
git commit -m "feat: scaffold rocket-git crate in workspace"
```

---

## Task 2: Implement status value objects

**Files:**
- Create: `crates/rocket-git/src/status.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_status_short_labels() {
        assert_eq!(GitStatus::Modified.short_label(), "M");
        assert_eq!(GitStatus::Added.short_label(), "A");
        assert_eq!(GitStatus::Deleted.short_label(), "D");
        assert_eq!(GitStatus::Untracked.short_label(), "?");
        assert_eq!(GitStatus::Conflicted.short_label(), "C");
        assert_eq!(GitStatus::Renamed.short_label(), "R");
    }

    #[test]
    fn git_status_is_changed() {
        assert!(GitStatus::Modified.is_changed());
        assert!(GitStatus::Added.is_changed());
        assert!(!GitStatus::Unchanged.is_changed());
    }

    #[test]
    fn repo_status_changed_count() {
        let status = RepoStatus {
            branch: "main".into(),
            files: vec![
                FileStatus { path: "a.bru".into(), status: GitStatus::Modified, staged: false },
                FileStatus { path: "b.bru".into(), status: GitStatus::Added, staged: true },
                FileStatus { path: "c.bru".into(), status: GitStatus::Unchanged, staged: false },
            ],
            ahead: 1,
            behind: 0,
            is_clean: false,
        };
        assert_eq!(status.changed_count(), 2);
        assert_eq!(status.staged_count(), 1);
        assert_eq!(status.unstaged_count(), 1);
    }

    #[test]
    fn file_status_serialization_roundtrip() {
        let fs = FileStatus { path: "auth/login.bru".into(), status: GitStatus::Modified, staged: false };
        let json = serde_json::to_string(&fs).unwrap();
        assert!(json.contains("\"status\":\"modified\""));
        let deserialized: FileStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, GitStatus::Modified);
        assert!(!deserialized.staged);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p rocket-git -- status::tests
```

- [ ] **Step 3: Implement status types**

`crates/rocket-git/src/status.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
    Unchanged,
}

impl GitStatus {
    pub fn short_label(&self) -> &'static str {
        match self {
            GitStatus::Modified => "M",
            GitStatus::Added => "A",
            GitStatus::Deleted => "D",
            GitStatus::Renamed => "R",
            GitStatus::Untracked => "?",
            GitStatus::Conflicted => "C",
            GitStatus::Unchanged => "",
        }
    }

    pub fn is_changed(&self) -> bool {
        !matches!(self, GitStatus::Unchanged)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    pub status: GitStatus,
    pub staged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub branch: String,
    pub files: Vec<FileStatus>,
    pub ahead: usize,
    pub behind: usize,
    pub is_clean: bool,
}

impl RepoStatus {
    pub fn changed_count(&self) -> usize {
        self.files.iter().filter(|f| f.status.is_changed()).count()
    }

    pub fn staged_count(&self) -> usize {
        self.files.iter().filter(|f| f.staged && f.status.is_changed()).count()
    }

    pub fn unstaged_count(&self) -> usize {
        self.files.iter().filter(|f| !f.staged && f.status.is_changed()).count()
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p rocket-git -- status::tests
```
Expected: PASS — 4 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-git/src/status.rs
git commit -m "feat(git): GitStatus, FileStatus, RepoStatus value objects"
```

---

## Milestone Checklist — P01

- [ ] `rocket-git` crate exists in workspace and compiles
- [ ] `GitStatus` enum with `short_label()` and `is_changed()`
- [ ] `FileStatus` with path, status, staged flag
- [ ] `RepoStatus` with branch, files, ahead/behind, `changed_count()`, `staged_count()`, `unstaged_count()`
- [ ] Serde roundtrip test passes
- [ ] 4 tests pass
