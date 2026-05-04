# Bruno Import — Plan 01: Crate Scaffold

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the `rocket-import` crate with `ImportError`, `ImportReport`, and stub `ImportService`, wired into `rocket-app` as two placeholder Tauri commands.

**Architecture:** New Cargo workspace member `crates/rocket-import`. Depends on `rocket-shared`, `rocket-collection`, `rocket-environment`, `rocket-workspace`. `rocket-app` gains the two Tauri commands but they return stub `ImportReport` values until later plans fill in the real logic.

**Tech Stack:** Rust, Tauri v2, serde, serde_yaml

**Spec:** `docs/superpowers/specs/2026-04-04-bruno-import-design.md`

---

## Task 1: Crate scaffold — Cargo.toml, lib.rs, error.rs, report.rs

**Files:**
- Create: `crates/rocket-import/Cargo.toml`
- Create: `crates/rocket-import/src/lib.rs`
- Create: `crates/rocket-import/src/error.rs`
- Create: `crates/rocket-import/src/report.rs`
- Modify: `Cargo.toml` (workspace root — add `rocket-import` to `members`)

- [ ] **Step 1: Add `rocket-import` to workspace members**

In the root `Cargo.toml`, locate the `[workspace]` `members` array and add:
```toml
"crates/rocket-import",
```

- [ ] **Step 2: Create `crates/rocket-import/Cargo.toml`**

```toml
[package]
name = "rocket-import"
version = "0.1.0"
edition = "2021"

[dependencies]
rocket-shared = { path = "../rocket-shared" }
rocket-collection = { path = "../rocket-collection" }
rocket-environment = { path = "../rocket-environment" }
rocket-workspace = { path = "../rocket-workspace" }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
thiserror = "1"
```

- [ ] **Step 3: Create `crates/rocket-import/src/error.rs`**

```rust
use rocket_shared::error::DomainError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("not a Bruno directory (no bruno.json found): {0}")]
    NotABrunoDirectory(PathBuf),

    #[error("parse error in {path}: {message}")]
    ParseError { path: PathBuf, message: String },

    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("domain error: {0}")]
    DomainError(#[from] DomainError),
}

pub type ImportResult<T> = Result<T, ImportError>;
```

- [ ] **Step 4: Create `crates/rocket-import/src/report.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportReport {
    pub total_files: usize,
    pub imported: usize,
    pub skipped: Vec<SkippedItem>,
    pub created_workspace: Option<String>,
    pub created_collections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedItem {
    pub path: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "detail", rename_all = "camelCase")]
pub enum SkipReason {
    UnsupportedRequestType(String),
    UnsupportedAuthType(String),
    ParseError(String),
}
```

- [ ] **Step 5: Create `crates/rocket-import/src/lib.rs`**

```rust
pub mod error;
pub mod report;
pub(crate) mod bru;
pub(crate) mod converter;
mod importer;

pub use error::{ImportError, ImportResult};
pub use importer::ImportService;
pub use report::{ImportReport, SkippedItem, SkipReason};
```

- [ ] **Step 6: Verify the crate compiles**

```bash
cargo check -p rocket-import
```
Expected: no errors (missing modules `bru`, `converter`, `importer` will exist as stubs in next step).

---

## Task 2: Stub modules + ImportService skeleton

**Files:**
- Create: `crates/rocket-import/src/importer.rs`
- Create: `crates/rocket-import/src/bru/mod.rs`
- Create: `crates/rocket-import/src/converter/mod.rs`

- [ ] **Step 1: Create stub `bru/mod.rs`**

```rust
// Bruno .bru and .yml parser — implemented in plan-02 and plan-03.
pub(crate) mod ast;
pub(crate) mod lexer;
pub(crate) mod parser;
pub(crate) mod yml_adapter;
```

Create matching empty stub files:
- `crates/rocket-import/src/bru/ast.rs` — empty for now
- `crates/rocket-import/src/bru/lexer.rs` — empty for now
- `crates/rocket-import/src/bru/parser.rs` — empty for now
- `crates/rocket-import/src/bru/yml_adapter.rs` — empty for now

- [ ] **Step 2: Create stub `converter/mod.rs`**

```rust
// Converters: BruDocument → domain types — implemented in plan-04.
pub(crate) mod collection;
pub(crate) mod environment;
pub(crate) mod request;
```

Create matching empty stub files:
- `crates/rocket-import/src/converter/collection.rs` — empty for now
- `crates/rocket-import/src/converter/environment.rs` — empty for now
- `crates/rocket-import/src/converter/request.rs` — empty for now

- [ ] **Step 3: Create `crates/rocket-import/src/importer.rs`**

```rust
use std::path::Path;
use crate::error::ImportResult;
use crate::report::ImportReport;

/// Orchestrates the full Bruno import pipeline.
/// Real implementation added in plan-04.
pub struct ImportService;

impl ImportService {
    pub fn new() -> Self {
        Self
    }

    /// Import a single Bruno collection directory into the given workspace.
    pub fn import_collection(
        &self,
        _path: &Path,
        _workspace_id: &str,
    ) -> ImportResult<ImportReport> {
        Ok(ImportReport::default())
    }

    /// Import a Bruno workspace directory.
    /// If `create_new_workspace` is true, a new RocketAPI workspace is created.
    /// Otherwise collections are added to the workspace identified by `target_workspace_id`.
    pub fn import_workspace(
        &self,
        _path: &Path,
        _create_new_workspace: bool,
        _target_workspace_id: Option<&str>,
    ) -> ImportResult<ImportReport> {
        Ok(ImportReport::default())
    }
}
```

- [ ] **Step 4: Verify full crate compiles**

```bash
cargo check -p rocket-import
```
Expected: compiles cleanly.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-import/ Cargo.toml
git commit -m "feat(import): scaffold rocket-import crate with ImportError, ImportReport, ImportService stub"
```

---

## Task 3: Tauri commands in rocket-app

**Files:**
- Modify: `crates/rocket-app/Cargo.toml` (add `rocket-import` dependency)
- Create: `crates/rocket-app/src/import_commands.rs`
- Modify: `crates/rocket-app/src/main.rs` (register commands)

- [ ] **Step 1: Add `rocket-import` to `rocket-app` dependencies**

In `crates/rocket-app/Cargo.toml`:
```toml
rocket-import = { path = "../rocket-import" }
```

- [ ] **Step 2: Create `crates/rocket-app/src/import_commands.rs`**

```rust
use rocket_import::{ImportReport, ImportService};
use std::path::PathBuf;

/// Import a single Bruno collection directory into the active workspace.
#[tauri::command]
pub async fn import_bruno_collection(
    path: String,
    target_workspace_id: String,
) -> Result<ImportReport, String> {
    let service = ImportService::new();
    service
        .import_collection(&PathBuf::from(&path), &target_workspace_id)
        .map_err(|e| e.to_string())
}

/// Import a Bruno workspace directory.
/// `create_new_workspace`: true = create a new RocketAPI workspace;
/// false = add collections to the workspace identified by `target_workspace_id`.
#[tauri::command]
pub async fn import_bruno_workspace(
    path: String,
    create_new_workspace: bool,
    target_workspace_id: Option<String>,
) -> Result<ImportReport, String> {
    let service = ImportService::new();
    service
        .import_workspace(
            &PathBuf::from(&path),
            create_new_workspace,
            target_workspace_id.as_deref(),
        )
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Register commands in `main.rs`**

In `crates/rocket-app/src/main.rs`, find the `.invoke_handler(tauri::generate_handler![...])` call and add:
```rust
import_commands::import_bruno_collection,
import_commands::import_bruno_workspace,
```

Also add at the top of `main.rs`:
```rust
mod import_commands;
```

- [ ] **Step 4: Verify rocket-app compiles**

```bash
cargo check -p rocket-app
```
Expected: compiles cleanly.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/
git commit -m "feat(app): register import_bruno_collection and import_bruno_workspace Tauri commands"
```
