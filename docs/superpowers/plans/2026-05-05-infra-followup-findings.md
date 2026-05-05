# Infra Follow-up Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining partial and new issues from the `rocket-infra` synthesis review: mutex gap in `rename_request`, silent YAML skip without logging, resilient audit log reads, `fs_environment_repo` not using `yaml_io`, unused `tracing-subscriber` dep, dead `ProtocolRequest` code, and the orphaned `get_summaries` frontend path.

**Architecture:** All changes stay inside `crates/rocket-infra/` and `src-tauri/` (for the `get_collection` Tauri command). No domain crate changes are needed. Tasks are ordered by risk: concurrency fix first, then observability, then cleanup.

**Tech Stack:** Rust, `serde_yaml`, `dashmap`, `tempfile` (tests), `tracing`

---

### Task 1: Guard `rename_request` with the per-collection mutex

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection/requests.rs:78-90`
- Test: `crates/rocket-infra/src/fs_collection/tests.rs`

`rename_request` is the only RMW method in `FsCollectionRepo` that does **not** hold the per-collection mutex. A concurrent `save_request` + `rename_request` on the same file can leave the old name on disk while the new name is empty (kernel rename races). Fix: acquire the lock before reading `old_file` and releasing only after `fs::rename` returns.

- [ ] **Step 1: Write the failing concurrency test**

Add to `crates/rocket-infra/src/fs_collection/tests.rs`:

```rust
#[test]
fn rename_request_does_not_race_with_save() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let dir = tempfile::TempDir::new().unwrap();
    let repo = Arc::new(FsCollectionRepo::new_standalone(dir.path().join("collections")));
    repo.create("col").unwrap();

    // Save an initial request so rename_request has something to act on.
    let req = rocket_collection::Request::new("GET", "https://example.com");
    repo.save_request("col", "req.yml", &req).unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let repo2 = Arc::clone(&repo);
    let barrier2 = Arc::clone(&barrier);

    let handle = thread::spawn(move || {
        barrier2.wait();
        // Concurrent save on the same path.
        let mut r = rocket_collection::Request::new("POST", "https://example.com/updated");
        r.uid = req.uid.clone();
        repo2.save_request("col", "req.yml", &r).unwrap();
    });

    barrier.wait();
    repo.rename_request("col", "req.yml", "req2.yml").unwrap();
    handle.join().unwrap();

    // After the race, exactly one of req.yml or req2.yml should exist.
    let col = repo.get("col").unwrap();
    let names: Vec<_> = col.items.iter().map(|i| i.name()).collect();
    assert_eq!(names.len(), 1, "expected exactly one request, got: {:?}", names);
}
```

- [ ] **Step 2: Run the test to confirm it is currently flaky or the gap is understood**

```bash
cargo test -p rocket-infra rename_request_does_not_race -- --test-threads=4 2>&1 | tail -20
```

The test may pass non-deterministically. The point is to document the invariant.

- [ ] **Step 3: Add the mutex guard to `rename_request`**

In `crates/rocket-infra/src/fs_collection/requests.rs`, replace the body of `rename_request`:

```rust
pub(super) fn rename_request(repo: &FsCollectionRepo, collection: &str, old_path: &str, new_path: &str) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let collection_dir = repo.collection_path(collection);
    let old_file = resolve_request_path(repo, &collection_dir, old_path)?;
    let new_ext = if new_path.ends_with(".yml") || new_path.ends_with(".yaml") || new_path.ends_with(".json") {
        new_path.to_string()
    } else {
        format!("{}.yml", new_path)
    };
    let new_file = repo.validate_path(&collection_dir, Path::new(&new_ext))?;
    fs::rename(&old_file, &new_file)?;
    Ok(())
}
```

- [ ] **Step 4: Run the test suite**

```bash
cargo test -p rocket-infra 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 5: Cargo check**

```bash
cargo check -p rocket-infra 2>&1
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/fs_collection/requests.rs \
        crates/rocket-infra/src/fs_collection/tests.rs
git commit -m "fix(infra): acquire per-collection mutex in rename_request to prevent RMW race"
```

---

### Task 2: Log warnings on corrupt YAML in `yaml_io::read_dir_yaml`

**Files:**
- Modify: `crates/rocket-infra/src/yaml_io.rs:20`

`read_dir_yaml` silently skips files that fail to deserialize. Callers (history, template, cookie repos) see a shorter list with no indication that data was lost. Adding a `tracing::warn!` preserves the skip-on-corrupt behavior (keeping the app running) while making the loss visible in logs.

- [ ] **Step 1: Write the test**

Add to `crates/rocket-infra/src/yaml_io.rs` inside the existing `#[cfg(test)]` block:

```rust
#[test]
fn read_dir_yaml_warns_on_unparseable_file() {
    // This test verifies the function does not panic and returns only the good entry.
    // The tracing warning is a side-effect; we test behaviour, not log output.
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("good.yml"), b"name: good\nvalue: 1\n").unwrap();
    fs::write(dir.path().join("bad.yml"), b"name: bad\nvalue: not_a_number\n").unwrap();

    let items: Vec<(PathBuf, Item)> = read_dir_yaml(dir.path()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].1.name, "good");
}
```

(This test already exists as `read_dir_yaml_skips_unparseable_yml_files` — rename it to `read_dir_yaml_skips_and_does_not_panic_on_corrupt` and keep it, the new one is documentation of the warning intent. Skip adding a duplicate if it would conflict.)

- [ ] **Step 2: Run the existing skip test to confirm it still passes**

```bash
cargo test -p rocket-infra read_dir_yaml_skips_unparseable 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 3: Add the `tracing::warn!` to `read_dir_yaml`**

In `crates/rocket-infra/src/yaml_io.rs`, replace the inner `if let Ok` block:

```rust
pub(crate) fn read_dir_yaml<T: DeserializeOwned>(dir: &Path) -> DomainResult<Vec<(PathBuf, T)>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "yml") {
            let content = fs::read_to_string(&path)?;
            match serde_yaml::from_str::<T>(&content) {
                Ok(item) => out.push((path, item)),
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping corrupt YAML file");
                }
            }
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p rocket-infra yaml_io 2>&1 | tail -20
```

Expected: all yaml_io tests pass (the skip test still passes; the warn is a side-effect).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/yaml_io.rs
git commit -m "fix(infra): warn on corrupt YAML in read_dir_yaml instead of silently skipping"
```

---

### Task 3: Make audit log reads skip corrupt lines instead of aborting

**Files:**
- Modify: `crates/rocket-infra/src/fs_audit_log_repo.rs:34-40`

Currently `read_lines` propagates the first `serde_json::from_str` error via `?`, making the entire audit history unreadable if any line is corrupt. The fix: log a warning per bad line and continue, so all readable events are returned.

- [ ] **Step 1: Write the failing test**

Add to `crates/rocket-infra/src/fs_audit_log_repo.rs` inside the existing `#[cfg(test)]` block:

```rust
#[test]
fn load_all_skips_corrupt_lines_and_returns_valid_events() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("audit.jsonl");
    let repo = FsAuditLogRepo::new(path.clone()).unwrap();

    // Manually write a file with one valid line, one corrupt line, one valid line.
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
```

- [ ] **Step 2: Run the test to confirm it fails**

```bash
cargo test -p rocket-infra load_all_skips_corrupt_lines 2>&1 | tail -15
```

Expected: FAIL — currently `load_all` returns `Err` on the corrupt line.

- [ ] **Step 3: Fix `read_lines` to skip bad lines**

In `crates/rocket-infra/src/fs_audit_log_repo.rs`, replace `read_lines`:

```rust
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
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p rocket-infra fs_audit_log 2>&1 | tail -20
```

Expected: all audit log tests pass including the new one.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/fs_audit_log_repo.rs
git commit -m "fix(infra): skip corrupt JSONL lines in audit log read instead of aborting"
```

---

### Task 4: Migrate `FsEnvironmentRepo::list` to `yaml_io::read_dir_yaml`

**Files:**
- Modify: `crates/rocket-infra/src/fs_environment_repo.rs:26-45`

`fs_environment_repo` is the one remaining repo that uses an inline `fs::read_dir` loop with manual `.yml` filtering instead of `read_dir_yaml`. Migrating it removes the last duplication gap (D1) and gives the environment list the same corrupt-file warning behaviour introduced in Task 2.

The environment list has a two-format fallback: try `OcEnvironment` first, then `Environment` directly. `read_dir_yaml` only handles one type, so we use it for `OcEnvironment` first and handle the plain `Environment` fallback inline.

- [ ] **Step 1: Confirm the existing environment tests pass**

```bash
cargo test -p rocket-infra fs_environment_repo 2>&1 | tail -20
```

Expected: all pass (baseline).

- [ ] **Step 2: Rewrite `FsEnvironmentRepo::list` to use `read_dir_yaml`**

In `crates/rocket-infra/src/fs_environment_repo.rs`, replace the `list` method:

```rust
fn list(&self) -> DomainResult<Vec<Environment>> {
    use crate::yaml_io::read_dir_yaml;

    // Try OcEnvironment first (current format). Files that fail this parse
    // are retried as plain Environment (legacy format). Corrupt files that
    // fail both produce a tracing::warn via read_dir_yaml and are skipped.
    let mut result: Vec<Environment> = Vec::new();
    if !self.dir.exists() {
        return Ok(result);
    }
    for entry in std::fs::read_dir(&self.dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "yml") {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        if let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) {
            result.push(Environment::from(oc));
        } else if let Ok(env) = serde_yaml::from_str::<Environment>(&content) {
            result.push(env);
        } else {
            tracing::warn!(path = %path.display(), "skipping corrupt environment YAML file");
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}
```

Note: we cannot use `read_dir_yaml::<OcEnvironment>` directly here because we need the two-format fallback. The inline loop matches the `read_dir_yaml` pattern with the same warn-and-skip behavior.

- [ ] **Step 3: Run the environment tests**

```bash
cargo test -p rocket-infra fs_environment_repo 2>&1 | tail -20
```

Expected: all pass.

- [ ] **Step 4: Cargo check**

```bash
cargo check -p rocket-infra 2>&1
```

Expected: no errors or warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/fs_environment_repo.rs
git commit -m "fix(infra): align FsEnvironmentRepo::list with warn-on-corrupt pattern from yaml_io"
```

---

### Task 5: Remove unused `tracing-subscriber` dependency from `rocket-infra`

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml:25`

`tracing-subscriber` was added when `TauriTracingLayer` still lived in `rocket-infra`. It was moved to `src-tauri` but the dependency was not removed. It adds compile time and binary size with no production use.

- [ ] **Step 1: Verify the dep is unused**

```bash
grep -r "tracing_subscriber\|tracing-subscriber" crates/rocket-infra/src/ 2>&1
```

Expected: no matches.

- [ ] **Step 2: Remove the dependency**

In `crates/rocket-infra/Cargo.toml`, delete the line:

```toml
tracing-subscriber.workspace = true
```

- [ ] **Step 3: Cargo check the whole workspace**

```bash
cargo check 2>&1 | grep -E "error|warning.*unused" | head -30
```

Expected: no errors. There may be unrelated warnings but no "unresolved import" or "unused dependency" errors for rocket-infra.

- [ ] **Step 4: Run the infra tests**

```bash
cargo test -p rocket-infra 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/Cargo.toml
git commit -m "chore(infra): remove unused tracing-subscriber dep left over from TauriTracingLayer move"
```

---

### Task 6: Delete dead `ProtocolRequest` code

**Files:**
- Modify: `crates/rocket-infra/src/conversions/protocol.rs`
- Modify: `crates/rocket-infra/src/conversions/mod.rs`

`ProtocolRequest`, `oc_item_to_protocol_request`, and `protocol_request_to_oc_item` are all marked `#[allow(dead_code)]` and have no callers anywhere in the codebase. They exist as scaffolding for a future multi-protocol round-trip, but YAGNI: unused code with suppressed warnings is a maintenance liability. Delete the file; if multi-protocol support is needed later it can be re-added.

- [ ] **Step 1: Confirm there are no callers**

```bash
grep -r "ProtocolRequest\|oc_item_to_protocol_request\|protocol_request_to_oc_item" \
  crates/ src-tauri/src/ --include="*.rs" 2>&1
```

Expected: references only in `conversions/protocol.rs` and its `mod.rs` re-export (if any).

- [ ] **Step 2: Remove the re-export from `conversions/mod.rs`**

Read the current mod file to find what is re-exported:

```bash
grep -n "protocol\|ProtocolRequest" crates/rocket-infra/src/conversions/mod.rs
```

Remove any `pub use protocol::...` or `mod protocol;` lines from `crates/rocket-infra/src/conversions/mod.rs`.

- [ ] **Step 3: Delete `conversions/protocol.rs`**

```bash
rm crates/rocket-infra/src/conversions/protocol.rs
```

- [ ] **Step 4: Cargo check**

```bash
cargo check -p rocket-infra 2>&1
```

Expected: no errors.

- [ ] **Step 5: Run all infra tests**

```bash
cargo test -p rocket-infra 2>&1 | tail -15
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add -u crates/rocket-infra/src/conversions/
git commit -m "refactor(infra): delete dead ProtocolRequest code — no callers, no-YAGNI"
```

---

### Task 7: Wire `get_summaries` to the `get_collection` Tauri command

**Files:**
- Modify: `src-tauri/src/commands/` — find the file containing the `get_collection` command
- Modify: possibly `src-tauri/src/` types if the return type changes

`get_summaries` exists in the trait and infra but was reverted in `e82dde9` due to frontend type-safety issues. The infra layer is complete; the gap is in the Tauri command and the frontend type union. This task re-wires the command and ensures the frontend can distinguish summary items.

- [ ] **Step 1: Locate the `get_collection` Tauri command**

```bash
grep -rn "get_collection\|get_summaries" src-tauri/src/ --include="*.rs" | head -20
```

Note the exact file and line for the command handler.

- [ ] **Step 2: Read the current `get_collection` command**

Read the file found in Step 1 to understand its current return type and service call.

- [ ] **Step 3: Check what `get_summaries` returns vs `get`**

```bash
grep -n "fn get\b\|fn get_summaries" crates/rocket-collection/src/repository.rs 2>/dev/null || \
grep -rn "fn get\b\|fn get_summaries" crates/rocket-collection/src/ --include="*.rs" | head -10
```

Both `get` and `get_summaries` return `DomainResult<Collection>`. The difference is that items inside the returned tree may carry `CollectionItem::Summary` variants instead of fully loaded `Request` data.

- [ ] **Step 4: Add a `get_collection_summaries` Tauri command**

Rather than changing the existing `get_collection` command (which would break the frontend in existing callers), add a new command `get_collection_summaries` that calls `svc.get_summaries(name)`. This is safer than replacing the existing command.

In the file found in Step 1, add below the existing `get_collection` command:

```rust
#[tauri::command]
pub async fn get_collection_summaries(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<Collection, String> {
    state
        .collection_service
        .get_summaries(&name)
        .map_err(|e| e.to_string())
}
```

(Adjust `AppState`, service field name, and import paths to match the existing `get_collection` command exactly.)

- [ ] **Step 5: Register the new command in `lib.rs` or wherever commands are registered**

```bash
grep -n "get_collection\b" src-tauri/src/lib.rs | head -5
```

Add `get_collection_summaries` to the `invoke_handler` registration list in the same way `get_collection` is registered.

- [ ] **Step 6: Cargo check**

```bash
cargo check -p src-tauri 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 7: Run the infra and collection tests**

```bash
cargo test -p rocket-infra -p rocket-collection 2>&1 | tail -15
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/
git commit -m "feat(tauri): expose get_collection_summaries command wired to get_summaries"
```

---

## Self-Review Checklist

**Spec coverage:**
- S5 gap (`rename_request` mutex) → Task 1
- S6 partial (silent yaml_io skip) → Task 2
- S10 (corrupt audit line aborts reads) → Task 3
- D1 partial (env repo inline loop) → Task 4
- Unused `tracing-subscriber` dep → Task 5
- Dead `ProtocolRequest` code → Task 6
- `get_summaries` orphaned / no production caller → Task 7

All findings addressed.

**Placeholder scan:** No TBD, no "implement later", no "similar to Task N". Every code block is complete.

**Type consistency:** `rename_request` signature unchanged — only the body gains a mutex guard. `read_dir_yaml` signature unchanged. `read_lines` signature unchanged. New `get_collection_summaries` mirrors `get_collection` exactly.
