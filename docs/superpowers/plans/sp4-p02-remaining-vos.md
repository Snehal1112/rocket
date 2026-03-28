# SP4-P02: Diff, Branch, Commit, Stash, Conflict, Credentials Value Objects

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create all remaining domain value objects for the `rocket-git` crate.

**Architecture:** Pure data types with serde. No I/O, no git2 dependency in these files.

**Tech Stack:** Rust, serde, chrono

**Prerequisite:** SP4-P01 complete.

---

## Task 1: Diff value objects

**Files:**
- Create: `crates/rocket-git/src/diff.rs`
- Modify: `crates/rocket-git/src/lib.rs` (add module)

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_additions_and_deletions() {
        let diff = FileDiff {
            path: "test.bru".into(),
            old_content: Some("old".into()),
            new_content: Some("new".into()),
            hunks: vec![DiffHunk {
                old_start: 1, old_lines: 1, new_start: 1, new_lines: 2,
                lines: vec![
                    DiffLine { content: "- old".into(), line_type: LineType::Remove },
                    DiffLine { content: "+ new".into(), line_type: LineType::Add },
                    DiffLine { content: "+ extra".into(), line_type: LineType::Add },
                ],
            }],
        };
        assert_eq!(diff.additions(), 2);
        assert_eq!(diff.deletions(), 1);
    }

    #[test]
    fn diff_serialization() {
        let line = DiffLine { content: "hello".into(), line_type: LineType::Add };
        let json = serde_json::to_string(&line).unwrap();
        assert!(json.contains("\"lineType\":\"add\""));
    }
}
```

- [ ] **Step 2: Implement diff.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub hunks: Vec<DiffHunk>,
}

impl FileDiff {
    pub fn additions(&self) -> usize {
        self.hunks.iter().flat_map(|h| &h.lines)
            .filter(|l| matches!(l.line_type, LineType::Add)).count()
    }
    pub fn deletions(&self) -> usize {
        self.hunks.iter().flat_map(|h| &h.lines)
            .filter(|l| matches!(l.line_type, LineType::Remove)).count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub old_start: u32, pub old_lines: u32,
    pub new_start: u32, pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub content: String,
    pub line_type: LineType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineType { Context, Add, Remove }
```

- [ ] **Step 3: Run tests, commit**

```bash
cargo test -p rocket-git -- diff::tests
git add crates/rocket-git/src/diff.rs
git commit -m "feat(git): FileDiff, DiffHunk, DiffLine value objects"
```

---

## Task 2: Branch, Commit, Stash value objects

**Files:**
- Create: `crates/rocket-git/src/branch.rs`
- Create: `crates/rocket-git/src/commit.rs`
- Create: `crates/rocket-git/src/stash.rs`

- [ ] **Step 1: Implement branch.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub is_head: bool,
    pub is_remote: bool,
    pub upstream: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchList {
    pub current: String,
    pub local: Vec<Branch>,
    pub remote: Vec<Branch>,
}
```

- [ ] **Step 2: Implement commit.rs**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub id: String,
    pub full_id: String,
    pub message: String,
    pub author: String,
    pub author_email: String,
    pub timestamp: DateTime<Utc>,
    pub files_changed: usize,
}
```

- [ ] **Step 3: Implement stash.rs**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub branch: String,
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-git/src/branch.rs crates/rocket-git/src/commit.rs crates/rocket-git/src/stash.rs
git commit -m "feat(git): Branch, CommitInfo, StashEntry value objects"
```

---

## Task 3: Conflict, Credentials VOs + wire lib.rs

**Files:**
- Create: `crates/rocket-git/src/conflict.rs`
- Create: `crates/rocket-git/src/credentials.rs`
- Modify: `crates/rocket-git/src/lib.rs`

- [ ] **Step 1: Implement conflict.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub path: String,
    pub ours: String,
    pub theirs: String,
    pub ancestor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "resolution", rename_all = "camelCase")]
pub enum ConflictResolution {
    Ours,
    Theirs,
    Custom { content: String },
}
```

- [ ] **Step 2: Implement credentials.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GitCredentials {
    #[serde(rename_all = "camelCase")]
    SshKey { private_key_path: String, passphrase: Option<String> },
    SshAgent,
    #[serde(rename_all = "camelCase")]
    UserPass { username: String, password: String },
    Token { token: String },
}
```

- [ ] **Step 3: Update lib.rs to export all modules**

```rust
pub mod status;
pub mod diff;
pub mod branch;
pub mod commit;
pub mod stash;
pub mod conflict;
pub mod credentials;

pub use status::*;
pub use diff::*;
pub use branch::*;
pub use commit::*;
pub use stash::*;
pub use conflict::*;
pub use credentials::*;
```

- [ ] **Step 4: Verify crate compiles**

```bash
cargo check -p rocket-git
cargo test -p rocket-git
```

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-git/src/
git commit -m "feat(git): ConflictFile, ConflictResolution, GitCredentials + wire lib.rs"
```

---

## Milestone Checklist — P02

- [ ] `FileDiff`, `DiffHunk`, `DiffLine`, `LineType` — 2 tests
- [ ] `Branch`, `BranchList`
- [ ] `CommitInfo`
- [ ] `StashEntry`
- [ ] `ConflictFile`, `ConflictResolution`
- [ ] `GitCredentials` enum (SshKey, SshAgent, UserPass, Token)
- [ ] All types exported from `lib.rs`
- [ ] `cargo check -p rocket-git` passes
