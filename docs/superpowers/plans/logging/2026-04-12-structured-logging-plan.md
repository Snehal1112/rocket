# Structured Logging Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `log` + `env_logger` with `tracing` + `tracing-subscriber` across the entire RocketAPI workspace, add structured spans at key operation boundaries, and pipe backend log events to the frontend Console Panel via a custom Tauri tracing layer.

**Architecture:** Phase 1 swaps the logging facade and subscriber in all 4 crates that use `log` today (`rocket-app`, `rocket-infra`, `rocket-git`, `src-tauri`). Phase 2 adds `#[instrument]` spans at 5 key boundaries (request execution, variable resolution, git operations, collection I/O, Bruno import). Phase 3 implements a custom `tracing::Layer` in `rocket-infra` that forwards structured log events to the frontend via Tauri's event bus, and wires the frontend to display them in the Console Panel.

**Tech Stack:** Rust (`tracing 0.1`, `tracing-subscriber 0.3`, `tracing-log 0.2`), Tauri v2, React 18, TypeScript, Zustand

---

## File Map

| File | Action | Phase | Purpose |
|---|---|---|---|
| `Cargo.toml` (workspace root) | Modify | 1 | Add `tracing`, `tracing-subscriber`, `tracing-log` to workspace deps; remove `env_logger` |
| `crates/rocket-app/Cargo.toml` | Modify | 1 | Replace `log` → `tracing` |
| `crates/rocket-infra/Cargo.toml` | Modify | 1 | Replace `log` → `tracing`; add `tracing-subscriber` (for Phase 3 Layer) |
| `crates/rocket-git/Cargo.toml` | Modify | 1 | Replace `log` → `tracing` |
| `crates/rocket-import/Cargo.toml` | Modify | 1 | Add `tracing` (currently has no logging dep) |
| `src-tauri/Cargo.toml` | Modify | 1 | Replace `log`/`env_logger` → `tracing`/`tracing-subscriber`/`tracing-log` |
| `src-tauri/src/lib.rs` | Modify | 1+3 | Replace `env_logger::init()` with tracing subscriber; add `TauriTracingLayer` in Phase 3 |
| All `.rs` files using `log::*` macros | Modify | 1 | Mechanical `log::info!` → `tracing::info!` etc. |
| `crates/rocket-app/src/execution_service.rs` | Modify | 2 | Add request execution span |
| `crates/rocket-git/src/git2_service.rs` | Modify | 2 | Add `#[instrument]` on public methods |
| `crates/rocket-infra/src/fs_collection_repo.rs` | Modify | 2 | Add `#[instrument]` on repo methods |
| `crates/rocket-import/src/importer.rs` | Modify | 2 | Add import operation span |
| `crates/rocket-infra/src/tauri_tracing_layer.rs` | Create | 3 | Custom `tracing::Layer` that emits to Tauri event bus |
| `crates/rocket-infra/src/lib.rs` | Modify | 3 | Export `TauriTracingLayer` |
| `frontend/src/hooks/useBackendLogs.ts` | Create | 3 | Tauri event listener for `backend-log` events |
| `frontend/src/stores/console-store.ts` | Modify | 3 | Extend `ConsoleEntry` to support backend log entries |
| `frontend/src/components/layout/ConsolePanel.tsx` | Modify | 3 | Render backend log entries alongside HTTP entries |

---

## Chunk 1: Foundation Swap (Phase 1)

### Task 1: Update workspace dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add tracing dependencies to workspace**

In the root `Cargo.toml`, find the `[workspace.dependencies]` section. Add these three new entries:

```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json", "fmt"] }
tracing-log = "0.2"
```

- [ ] **Step 2: Remove env_logger from workspace dependencies**

In the same `[workspace.dependencies]` section, delete:

```toml
env_logger = "0.11"
```

Keep `log = "0.4"` — it's still needed by transitive dependencies (`reqwest`, `notify`, `git2`). The `tracing-log` bridge will capture those.

- [ ] **Step 3: Verify workspace file parses**

```bash
cargo metadata --format-version 1 --no-deps 2>&1 | head -1
```

Expected: JSON output (no TOML parse error).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add tracing workspace deps, remove env_logger"
```

---

### Task 2: Migrate crate dependencies

**Files:**
- Modify: `crates/rocket-app/Cargo.toml`
- Modify: `crates/rocket-infra/Cargo.toml`
- Modify: `crates/rocket-git/Cargo.toml`
- Modify: `crates/rocket-import/Cargo.toml`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Update `crates/rocket-app/Cargo.toml`**

Replace:
```toml
log.workspace = true
```
With:
```toml
tracing.workspace = true
```

- [ ] **Step 2: Update `crates/rocket-infra/Cargo.toml`**

Replace:
```toml
log.workspace = true
```
With:
```toml
tracing.workspace = true
tracing-subscriber.workspace = true
```

(The `tracing-subscriber` dep is needed here for Phase 3's `TauriTracingLayer` which implements `tracing_subscriber::Layer`.)

- [ ] **Step 3: Update `crates/rocket-git/Cargo.toml`**

Replace:
```toml
log.workspace = true
```
With:
```toml
tracing.workspace = true
```

- [ ] **Step 4: Update `crates/rocket-import/Cargo.toml`**

Add under `[dependencies]` (this crate currently has no logging dep):
```toml
tracing.workspace = true
```

- [ ] **Step 5: Update `src-tauri/Cargo.toml`**

Replace:
```toml
log.workspace = true
env_logger.workspace = true
```
With:
```toml
tracing.workspace = true
tracing-subscriber.workspace = true
tracing-log.workspace = true
```

- [ ] **Step 6: Verify all crates resolve**

```bash
cargo check --workspace 2>&1 | grep "^error" | head -20
```

Expected: compile errors about `log::info!` etc. not found — that's expected, we'll fix those in the next task.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/Cargo.toml crates/rocket-infra/Cargo.toml crates/rocket-git/Cargo.toml crates/rocket-import/Cargo.toml src-tauri/Cargo.toml
git commit -m "chore: migrate crate deps from log/env_logger to tracing"
```

---

### Task 3: Replace log macros across all crates + set up subscriber

**Files:**
- Modify: all `.rs` files currently using `log::info!`, `log::error!`, `log::warn!`, `log::debug!`, `log::trace!`
- Modify: `src-tauri/src/lib.rs` (subscriber initialization)

- [ ] **Step 1: Find all log macro usages**

Run this from the project root to identify every file that needs updating:

```bash
grep -rn "log::\(info\|error\|warn\|debug\|trace\)!" crates/ src-tauri/src/ --include="*.rs"
```

Document the list. Typical hits will be in:
- `crates/rocket-app/src/` (service orchestration logs)
- `crates/rocket-infra/src/` (I/O operation logs)
- `crates/rocket-git/src/git2_service.rs` (git operation logs)
- `src-tauri/src/lib.rs` (the `log::info!("RocketAPI initialized...")` line)

- [ ] **Step 2: Replace all `log::*` macro calls with `tracing::*`**

For each file found in Step 1, do a direct replacement:
- `log::info!(...)` → `tracing::info!(...)`
- `log::error!(...)` → `tracing::error!(...)`
- `log::warn!(...)` → `tracing::warn!(...)`
- `log::debug!(...)` → `tracing::debug!(...)`
- `log::trace!(...)` → `tracing::trace!(...)`

Also replace any `use log;` or `use log::*;` imports with nothing (tracing macros are used without import via the `tracing` crate's `#[macro_use]` or direct `tracing::info!` syntax).

If any file has `extern crate log;`, remove it.

- [ ] **Step 3: Replace `env_logger::init()` with tracing subscriber in `src-tauri/src/lib.rs`**

Find:
```rust
env_logger::init();
```

Replace with:
```rust
    // ── Structured logging subscriber ──────────────────────────────────────
    {
        use tracing_subscriber::{fmt, prelude::*, EnvFilter};

        let env_filter = EnvFilter::try_from_env("ROCKET_LOG")
            .or_else(|_| EnvFilter::try_from_env("RUST_LOG"))
            .unwrap_or_else(|_| EnvFilter::new("info,git2=warn,reqwest=warn,hyper=warn"));

        if cfg!(debug_assertions) {
            // Dev: human-readable, colored, with span context
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_file(false)
                        .with_line_number(false)
                        .pretty(),
                )
                .init();
        } else {
            // Release: JSON structured output for machine ingestion
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .json()
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_span_list(true)
                        .flatten_event(true),
                )
                .init();
        }

        // Bridge log crate → tracing for transitive deps (reqwest, notify, git2)
        // This is a no-op when tracing-log's "log-always" feature isn't set,
        // because tracing_subscriber already installs a log compatibility layer.
        // We call it explicitly for clarity.
        let _ = tracing_log::LogTracer::init();
    }
```

Also update the existing `log::info!` at the end of `.setup()`:

Find:
```rust
log::info!("RocketAPI initialized at {:?}", data_dir);
```

Replace with:
```rust
tracing::info!(data_dir = %data_dir.display(), "RocketAPI initialized");
```

- [ ] **Step 4: Add tracing-related imports at the top of `src-tauri/src/lib.rs`**

No new `use` statements needed at module level — the subscriber setup uses a scoped block with its own imports, and `tracing::info!` is accessed via the crate path.

- [ ] **Step 5: Verify the full workspace compiles**

```bash
cargo check --workspace 2>&1 | grep "^error" | head -20
```

Expected: no errors. All `log::*` calls are now `tracing::*`, and the subscriber is wired.

- [ ] **Step 6: Run existing tests to ensure no regressions**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all existing tests pass. Logging changes don't affect test behavior.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: replace log/env_logger with tracing across workspace

- Migrate all log::info!/error!/warn!/debug! to tracing equivalents
- Set up tracing-subscriber with EnvFilter (ROCKET_LOG / RUST_LOG)
- Dev mode: pretty-printed colored output
- Release mode: JSON structured output
- Bridge transitive log crate users via tracing-log"
```

---

## Chunk 2: Meaningful Spans (Phase 2)

### Task 4: Add request execution span

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`

- [ ] **Step 1: Add tracing use**

At the top of `execution_service.rs`, ensure `tracing` is available:

```rust
use tracing::{info_span, Instrument};
```

If the execute method is `async`:
```rust
use tracing::instrument;
```

- [ ] **Step 2: Wrap the execute method in a span**

Find the main `execute` method on `RequestExecutionService`. Wrap the method body in a span. If the method is async, use `#[instrument]`:

```rust
#[tracing::instrument(
    name = "http_request",
    skip(self, input),
    fields(
        method = %input.method,
        url = %input.url,
        collection = input.collection.as_deref().unwrap_or("unknown"),
    )
)]
pub async fn execute(&self, input: ExecuteRequestInput) -> DomainResult<HttpResponse> {
    // ... existing body unchanged ...
}
```

If the method is synchronous, use a manual span:

```rust
pub fn execute(&self, input: ExecuteRequestInput) -> DomainResult<HttpResponse> {
    let _span = tracing::info_span!(
        "http_request",
        method = %input.method,
        url = %input.url,
        collection = input.collection.as_deref().unwrap_or("unknown"),
    )
    .entered();

    // ... existing body unchanged ...
}
```

- [ ] **Step 3: Add structured fields to existing log lines within execute**

Find any `tracing::info!` or `tracing::debug!` calls inside the execute body. Enhance them with structured fields. For example, after the HTTP response comes back:

```rust
tracing::info!(
    status = result.status,
    duration_ms = result.duration_ms,
    size_bytes = result.size_bytes,
    "request completed"
);
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check -p rocket-app
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat(app): add tracing span to request execution pipeline"
```

---

### Task 5: Add git operation spans

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

- [ ] **Step 1: Identify public methods to instrument**

The following public methods on `Git2Service` should get `#[instrument]`:
- `commit` — fields: `repo_path`, `message` (truncated)
- `push` — fields: `repo_path`, `remote`
- `pull` — fields: `repo_path`, `remote`
- `fetch` — fields: `repo_path`, `remote`
- `stash_save` — fields: `repo_path`
- `clone_repo` (or equivalent) — fields: `url`, `target_path`
- `status` — fields: `repo_path`

- [ ] **Step 2: Add `#[instrument]` to each method**

For each method listed above, add the attribute. Example for `commit`:

```rust
#[tracing::instrument(
    name = "git_commit",
    skip(self),
    fields(repo_path = %path, message = %message.get(..50).unwrap_or(message))
)]
pub fn commit(&self, path: &str, message: &str) -> DomainResult<CommitInfo> {
    // ... existing body unchanged ...
}
```

Example for `push`:

```rust
#[tracing::instrument(
    name = "git_push",
    skip(self, credentials),
    fields(repo_path = %path)
)]
pub fn push(&self, path: &str, remote: &str, branch: &str, credentials: &GitCredentials) -> DomainResult<()> {
    // ... existing body unchanged ...
}
```

Example for `status`:

```rust
#[tracing::instrument(
    name = "git_status",
    skip(self),
    fields(repo_path = %path)
)]
pub fn status(&self, path: &str) -> DomainResult<RepoStatus> {
    // ... existing body unchanged ...
}
```

Follow the same pattern for `pull`, `fetch`, `stash_save`, and `clone_repo`. Always `skip(self)` and `skip(credentials)` where applicable. Never log credential content.

- [ ] **Step 3: Verify it compiles and tests pass**

```bash
cargo check -p rocket-git
cargo test -p rocket-git
```

Expected: no errors, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "feat(git): add tracing #[instrument] spans to all public Git2Service methods"
```

---

### Task 6: Add collection I/O spans

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs`

- [ ] **Step 1: Add `#[instrument]` to key FsCollectionRepo methods**

Add to the following methods on `FsCollectionRepo`:
- `create` — field: `name`
- `get` — field: `name`
- `delete` — field: `name`
- `save_request` — fields: `collection`, `request_path`
- `rename` — fields: `old_name`, `new_name`

Example for `create`:

```rust
#[tracing::instrument(
    name = "collection_create",
    skip(self),
    fields(collection_name = %name)
)]
fn create(&self, name: &str) -> DomainResult<Collection> {
    // ... existing body unchanged ...
}
```

Example for `save_request`:

```rust
#[tracing::instrument(
    name = "collection_save_request",
    skip(self, request),
    fields(collection = %collection_name, path = %request_path)
)]
fn save_request(&self, collection_name: &str, request_path: &str, request: &Request) -> DomainResult<()> {
    // ... existing body unchanged ...
}
```

Follow the same pattern for `get`, `delete`, and `rename`. Always `skip(self)` and skip large data parameters like `request`.

- [ ] **Step 2: Verify it compiles and tests pass**

```bash
cargo check -p rocket-infra
cargo test -p rocket-infra
```

Expected: no errors, all tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/fs_collection_repo.rs
git commit -m "feat(infra): add tracing spans to FsCollectionRepo I/O methods"
```

---

### Task 7: Add Bruno import span

**Files:**
- Modify: `crates/rocket-import/src/importer.rs`

- [ ] **Step 1: Add tracing span to `import_auto`**

Find the `import_auto` method on `ImportService`. Add:

```rust
#[tracing::instrument(
    name = "bruno_import",
    skip(self),
    fields(
        source_path = %path.display(),
        workspace_id = %workspace_id,
    )
)]
pub fn import_auto(&self, path: &Path, workspace_id: &str) -> ImportResult<ImportReport> {
    // ... existing body unchanged ...
}
```

- [ ] **Step 2: Add tracing span to `import_auto_from_zip`**

```rust
#[tracing::instrument(
    name = "bruno_import_zip",
    skip(self),
    fields(
        zip_path = %zip_path.display(),
        workspace_id = %workspace_id,
    )
)]
pub fn import_auto_from_zip(&self, zip_path: &Path, workspace_id: &str) -> ImportResult<ImportReport> {
    // ... existing body unchanged ...
}
```

- [ ] **Step 3: Add structured log at the end of successful imports**

Inside both methods, after the import completes successfully but before returning the report, add:

```rust
tracing::info!(
    total_files = report.total_files,
    imported = report.imported,
    skipped = report.skipped.len(),
    "import complete"
);
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check -p rocket-import
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-import/src/importer.rs
git commit -m "feat(import): add tracing spans to Bruno import operations"
```

---

## Chunk 3: Frontend Console Pipeline (Phase 3)

### Task 8: Create TauriTracingLayer

**Files:**
- Create: `crates/rocket-infra/src/tauri_tracing_layer.rs`
- Modify: `crates/rocket-infra/src/lib.rs`

- [ ] **Step 1: Add tauri dependency to rocket-infra**

In `crates/rocket-infra/Cargo.toml`, add under `[dependencies]`:

```toml
tauri = { version = "2", features = [] }
```

This is needed because `TauriTracingLayer` holds an `AppHandle` and calls `emit()`.

- [ ] **Step 2: Create `crates/rocket-infra/src/tauri_tracing_layer.rs`**

```rust
use serde::Serialize;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// A structured log entry emitted to the frontend via Tauri events.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendLogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields: HashMap<String, String>,
    pub span_fields: HashMap<String, String>,
}

/// A `tracing` Layer that forwards log events (INFO and above) to the
/// Tauri frontend via the `"backend-log"` event channel.
pub struct TauriTracingLayer {
    app_handle: AppHandle,
}

impl TauriTracingLayer {
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }
}

/// Visitor that collects tracing fields into a HashMap.
struct FieldVisitor {
    fields: HashMap<String, String>,
    message: Option<String>,
}

impl FieldVisitor {
    fn new() -> Self {
        Self {
            fields: HashMap::new(),
            message: None,
        }
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        } else {
            self.fields
                .insert(field.name().to_string(), format!("{:?}", value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

impl<S> Layer<S> for TauriTracingLayer
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        // Only forward INFO and above to the frontend
        let metadata = event.metadata();
        if *metadata.level() > tracing::Level::INFO {
            return;
        }

        // Collect event fields
        let mut visitor = FieldVisitor::new();
        event.record(&mut visitor);

        // Collect span fields from the current span context
        let mut span_fields = HashMap::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let extensions = span.extensions();
                if let Some(fields) = extensions.get::<HashMap<String, String>>() {
                    span_fields.extend(fields.clone());
                }
            }
        }

        let entry = BackendLogEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: metadata.level().to_string(),
            target: metadata.target().to_string(),
            message: visitor.message.unwrap_or_default(),
            fields: visitor.fields,
            span_fields,
        };

        // Fire-and-forget emit to the frontend. If the window isn't ready
        // yet or the event fails, we silently drop it.
        let _ = self.app_handle.emit("backend-log", &entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_log_entry_serializes_camel_case() {
        let entry = BackendLogEntry {
            timestamp: "2026-04-12T10:00:00Z".to_string(),
            level: "INFO".to_string(),
            target: "rocket_app::execution_service".to_string(),
            message: "request completed".to_string(),
            fields: HashMap::from([("status".into(), "200".into())]),
            span_fields: HashMap::from([("method".into(), "GET".into())]),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"spanFields\""));
        assert!(json.contains("\"target\""));
        assert!(!json.contains("span_fields")); // must be camelCase
    }
}
```

- [ ] **Step 3: Export from `crates/rocket-infra/src/lib.rs`**

Add at the top of `lib.rs` alongside other module declarations:

```rust
mod tauri_tracing_layer;
pub use tauri_tracing_layer::{BackendLogEntry, TauriTracingLayer};
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check -p rocket-infra
```

Expected: no errors.

- [ ] **Step 5: Run the unit test**

```bash
cargo test -p rocket-infra -- tauri_tracing_layer::tests
```

Expected: 1 test passes.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/tauri_tracing_layer.rs crates/rocket-infra/src/lib.rs crates/rocket-infra/Cargo.toml
git commit -m "feat(infra): add TauriTracingLayer for forwarding structured logs to frontend"
```

---

### Task 9: Wire TauriTracingLayer into subscriber

**Files:**
- Modify: `src-tauri/src/lib.rs`

The challenge: the subscriber must be initialized early (before `.setup()`), but the `AppHandle` is only available inside `.setup()`. Solution: use `tracing_subscriber::reload` to start with a no-op layer and hot-swap in the `TauriTracingLayer` once the `AppHandle` is available.

- [ ] **Step 1: Add reload layer imports and restructure subscriber init**

Replace the entire subscriber initialization block from Task 3 with:

```rust
    // ── Structured logging subscriber ──────────────────────────────────────
    use std::sync::Arc as StdArc;
    use tracing_subscriber::{fmt, prelude::*, reload, EnvFilter, Registry};

    let env_filter = EnvFilter::try_from_env("ROCKET_LOG")
        .or_else(|_| EnvFilter::try_from_env("RUST_LOG"))
        .unwrap_or_else(|_| EnvFilter::new("info,git2=warn,reqwest=warn,hyper=warn"));

    // Create a reload layer for the TauriTracingLayer (initially a no-op None).
    let (tauri_layer, reload_handle) =
        reload::Layer::new(None::<rocket_infra::TauriTracingLayer>);

    if cfg!(debug_assertions) {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_file(false)
                    .with_line_number(false)
                    .pretty(),
            )
            .with(tauri_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .json()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_span_list(true)
                    .flatten_event(true),
            )
            .with(tauri_layer)
            .init();
    }

    let _ = tracing_log::LogTracer::init();

    // Store the reload handle so we can inject the AppHandle later in .setup()
    let reload_handle = StdArc::new(reload_handle);
```

- [ ] **Step 2: Activate the TauriTracingLayer inside `.setup()`**

Inside the `.setup(|app| { ... })` closure, after the line that creates `app_handle`, add:

```rust
            // Activate the Tauri tracing layer now that we have an AppHandle.
            let tauri_tracing = rocket_infra::TauriTracingLayer::new(app_handle.clone());
            if let Err(e) = reload_handle.modify(|layer| *layer = Some(tauri_tracing)) {
                eprintln!("Failed to activate TauriTracingLayer: {}", e);
            }
```

Note: `reload_handle` needs to be moved into the closure. Since it's an `Arc`, clone it before the builder:

Before the `tauri::Builder::default()` call, add:
```rust
    let reload_handle_clone = StdArc::clone(&reload_handle);
```

Then use `reload_handle_clone` inside `.setup()`.

- [ ] **Step 3: Verify it compiles**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: no errors. The `reload::Layer` requires the inner type to implement `Layer`, which `Option<TauriTracingLayer>` does (Option<L> implements Layer when L does, with None being a no-op).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tauri): wire TauriTracingLayer via reload handle in subscriber setup"
```

---

### Task 10: Frontend — listen for backend logs

**Files:**
- Create: `frontend/src/hooks/useBackendLogs.ts`
- Modify: `frontend/src/stores/console-store.ts`

- [ ] **Step 1: Extend ConsoleEntry type in console-store.ts**

In `frontend/src/stores/console-store.ts`, update the `ConsoleEntry` interface to support backend log entries. Find:

```ts
export interface ConsoleEntry {
  id: string;
  timestamp: string;
  method: string;
  url: string;
  status: number;
  statusText: string;
  durationMs: number;
  sizeBytes: number;
  requestHeaders: { key: string; value: string }[];
  requestBody: string;
  responseHeaders: { key: string; value: string }[];
  responseBody: string;
}
```

Replace with:

```ts
export type ConsoleEntryKind = 'http' | 'log';

export interface HttpConsoleEntry {
  kind: 'http';
  id: string;
  timestamp: string;
  method: string;
  url: string;
  status: number;
  statusText: string;
  durationMs: number;
  sizeBytes: number;
  requestHeaders: { key: string; value: string }[];
  requestBody: string;
  responseHeaders: { key: string; value: string }[];
  responseBody: string;
}

export interface LogConsoleEntry {
  kind: 'log';
  id: string;
  timestamp: string;
  level: string;
  target: string;
  message: string;
  fields: Record<string, string>;
  spanFields: Record<string, string>;
}

export type ConsoleEntry = HttpConsoleEntry | LogConsoleEntry;
```

- [ ] **Step 2: Update `addEntry` to handle both types**

Find the `addEntry` function body. Update it so existing HTTP callers still work:

```ts
  addEntry: (entry) => {
    const full: ConsoleEntry = {
      ...entry,
      kind: 'kind' in entry ? entry.kind : 'http',
      id: crypto.randomUUID(),
      timestamp: 'timestamp' in entry && entry.timestamp ? entry.timestamp : new Date().toISOString(),
    } as ConsoleEntry;
    set((state) => ({
      entries: [full, ...state.entries].slice(0, MAX_ENTRIES),
    }));
  },
```

Also add a dedicated `addLogEntry` method:

```ts
  addLogEntry: (entry: Omit<LogConsoleEntry, 'id' | 'kind'>) => {
    const full: LogConsoleEntry = {
      ...entry,
      kind: 'log',
      id: crypto.randomUUID(),
    };
    set((state) => ({
      entries: [full, ...state.entries].slice(0, MAX_ENTRIES),
    }));
  },
```

And add `addLogEntry` to the `ConsoleState` interface:

```ts
interface ConsoleState {
  entries: ConsoleEntry[];
  addEntry: (entry: Omit<HttpConsoleEntry, 'id' | 'timestamp' | 'kind'>) => void;
  addLogEntry: (entry: Omit<LogConsoleEntry, 'id' | 'kind'>) => void;
  clearEntries: () => void;
}
```

- [ ] **Step 3: Create `frontend/src/hooks/useBackendLogs.ts`**

```ts
import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useConsoleStore } from '@/stores/console-store';

interface BackendLogPayload {
  timestamp: string;
  level: string;
  target: string;
  message: string;
  fields: Record<string, string>;
  spanFields: Record<string, string>;
}

/**
 * Listens for structured backend log events emitted by TauriTracingLayer
 * and feeds them into the console store.
 */
export function useBackendLogs() {
  useEffect(() => {
    const unlisten = listen<BackendLogPayload>('backend-log', (event) => {
      useConsoleStore.getState().addLogEntry({
        timestamp: event.payload.timestamp,
        level: event.payload.level,
        target: event.payload.target,
        message: event.payload.message,
        fields: event.payload.fields,
        spanFields: event.payload.spanFields,
      });
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);
}
```

- [ ] **Step 4: Wire the hook into App.tsx**

In `frontend/src/App.tsx`, add the import:

```ts
import { useBackendLogs } from '@/hooks/useBackendLogs';
```

Inside the `App` component, call it at the top level (alongside other hooks):

```ts
useBackendLogs();
```

- [ ] **Step 5: Verify types**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -20
```

Expected: no type errors. The existing `addEntry` callers that pass HTTP data still work because the function signature accepts the same shape (minus the new `kind` field which defaults to `'http'`).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/hooks/useBackendLogs.ts frontend/src/stores/console-store.ts frontend/src/App.tsx
git commit -m "feat(frontend): listen for backend-log events and feed into console store"
```

---

### Task 11: Update ConsolePanel to render log entries

**Files:**
- Modify: `frontend/src/components/layout/ConsolePanel.tsx`

- [ ] **Step 1: Add log level color helper**

Near the existing `statusColor` function, add:

```ts
function levelColor(level: string): string {
  switch (level.toUpperCase()) {
    case 'ERROR':
      return 'text-red-500';
    case 'WARN':
      return 'text-yellow-500';
    case 'INFO':
      return 'text-blue-500';
    case 'DEBUG':
      return 'text-muted-foreground';
    default:
      return 'text-muted-foreground';
  }
}
```

- [ ] **Step 2: Add LogEntryDetail component**

Below the existing `EntryDetail` component, add:

```tsx
function LogEntryDetail({ entry }: { entry: LogConsoleEntry }) {
  const allFields = { ...entry.spanFields, ...entry.fields };
  const fieldEntries = Object.entries(allFields);

  if (fieldEntries.length === 0) {
    return (
      <div className="px-4 py-2 bg-muted/30 border-t text-xs text-muted-foreground">
        No additional fields
      </div>
    );
  }

  return (
    <div className="px-4 py-2 bg-muted/30 border-t text-xs">
      <div className="font-medium text-muted-foreground mb-1">Fields</div>
      <div className="font-mono bg-background/60 rounded p-1.5">
        {fieldEntries.map(([key, value]) => (
          <div key={key}>
            <span className="text-muted-foreground">{key}:</span>{' '}
            <span>{value}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Update the entry row rendering**

Find the section in `ConsolePanel` where entries are mapped to rows. Update it to handle both entry types. The existing row renders HTTP entries. Add a branch for log entries:

```tsx
{filtered.map((entry) => (
  <div key={entry.id}>
    <div
      className="flex items-center gap-2 px-3 py-1.5 hover:bg-muted/50 cursor-pointer text-xs font-mono"
      onClick={() => setExpandedId(expandedId === entry.id ? null : entry.id)}
    >
      {expandedId === entry.id ? (
        <ChevronDown className="h-3 w-3 shrink-0" />
      ) : (
        <ChevronRight className="h-3 w-3 shrink-0" />
      )}
      <span className="text-muted-foreground shrink-0">
        {formatTime(entry.timestamp)}
      </span>

      {entry.kind === 'http' ? (
        <>
          <span className="font-semibold shrink-0">{entry.method}</span>
          <span className="truncate">{entry.url}</span>
          <span className={cn('shrink-0', statusColor(entry.status))}>
            {entry.status}
          </span>
          <span className="text-muted-foreground shrink-0">
            {entry.durationMs}ms
          </span>
        </>
      ) : (
        <>
          <span
            className={cn(
              'shrink-0 font-semibold uppercase text-[10px] px-1 rounded',
              levelColor(entry.level)
            )}
          >
            {entry.level}
          </span>
          <span className="text-muted-foreground shrink-0 truncate max-w-[200px]">
            {entry.target}
          </span>
          <span className="truncate">{entry.message}</span>
        </>
      )}
    </div>

    {expandedId === entry.id &&
      (entry.kind === 'http' ? (
        <EntryDetail entry={entry} />
      ) : (
        <LogEntryDetail entry={entry} />
      ))}
  </div>
))}
```

- [ ] **Step 4: Update imports**

At the top of `ConsolePanel.tsx`, update the import from console-store to include the new types:

```ts
import { useConsoleStore, type ConsoleEntry, type HttpConsoleEntry, type LogConsoleEntry } from '@/stores/console-store';
```

- [ ] **Step 5: Update the search filter**

Find the filter logic for the `search` input. Update it to also search log entries:

```ts
const filtered = search
  ? entries.filter((e) => {
      const q = search.toLowerCase();
      if (e.kind === 'http') {
        return e.url.toLowerCase().includes(q) || e.method.toLowerCase().includes(q);
      }
      return (
        e.message.toLowerCase().includes(q) ||
        e.target.toLowerCase().includes(q) ||
        e.level.toLowerCase().includes(q)
      );
    })
  : entries;
```

- [ ] **Step 6: Verify types and build**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -10
cd frontend && yarn build 2>&1 | tail -10
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/layout/ConsolePanel.tsx
git commit -m "feat(frontend): render backend log entries in Console Panel alongside HTTP entries"
```

---

## Done

Structured logging is fully implemented across the stack:

**Phase 1 — Foundation:**
- `tracing` replaces `log` across all crates
- `tracing-subscriber` with `EnvFilter` (controlled via `ROCKET_LOG` / `RUST_LOG`)
- Dev mode: pretty-printed colored console output
- Release mode: JSON structured output for machine ingestion
- `tracing-log` bridge captures output from transitive `log` crate users

**Phase 2 — Spans:**
- `http_request` span on request execution (method, url, collection, status, duration)
- `git_*` spans on all public `Git2Service` methods (repo_path, branch, remote)
- `collection_*` spans on `FsCollectionRepo` CRUD (collection_name, path)
- `bruno_import` / `bruno_import_zip` spans on import operations (source_path, counts)

**Phase 3 — Frontend Pipeline:**
- `TauriTracingLayer` captures INFO+ events with structured fields and span context
- Hot-swapped into subscriber via `reload::Layer` once `AppHandle` is available
- Frontend `useBackendLogs` hook listens for `backend-log` Tauri events
- Console Panel displays both HTTP and log entries with expandable field details
- Search filter works across both entry types
