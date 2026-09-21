# Sandbox Filesystem Ops Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the six `fs.*` operations Developer Mode scripts will get (`readFile`, `writeFile`, `readDir`, `exists`, `mkdir`, `remove`), fully unit-tested on their own — not yet reachable from any script, since extension registration/gating is a later plan in this sequence.

**Architecture:** Each op is split into a plain-Rust `*_impl` function (testable directly with `tempfile::TempDir`, no `deno_core` involvement) and a thin `#[op2]`-annotated wrapper that just calls it. This project's existing `ops/*.rs` files have no unit tests of their own — everything is tested indirectly through full `DenoScriptEngine::execute()` integration tests in `engine.rs` — but that only works once an op is registered into a `deno_core` extension, which doesn't happen until the next plan in this sequence. The impl/wrapper split is a deliberate, minimal, one-time deviation from that convention so this plan's ops are genuinely testable and this plan produces working, verified software on its own, per the note left for the next plan to pick up.

**Tech Stack:** Rust (rocket-infra crate), `deno_core::op2`, `tempfile`, `base64`, `cargo test`.

**Spec:** `docs/superpowers/specs/2026-09-21-sandbox-developer-mode-design.md`

## Global Constraints

- No path or command restriction — fully unrestricted filesystem access once reachable (spec §3, explicit user decision). Nothing in this plan enforces or even checks any path boundary.
- `mkdir`'s and `remove`'s `recursive` option defaults to `false` when omitted by the caller (matches Node's `mkdirSync`/`rmSync`) — this plan implements the Rust side with an explicit `recursive: bool` parameter; the JS-side default-to-`false` unpacking happens in the next plan's `bootstrap.js` wiring, not here.
- All errors surface as `ScriptOpError` (`crates/rocket-infra/src/scripting/ops/mod.rs`), matching every existing op file's error type — never a raw `std::io::Error` or a panic.
- Rust: avoid `.unwrap` panics in production code paths; test code uses `.expect("message")` for fallible setup calls.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-21-sandbox-process-op-and-engine-gating.md` next (plan 3 of 4 in the sandbox-developer-mode sequence).

---

### Task 1: `fs.readFile` / `fs.writeFile`

**Files:**
- Create: `crates/rocket-infra/src/scripting/ops/fs.rs`
- Modify: `crates/rocket-infra/src/scripting/ops/mod.rs:1-6` (register the new module)
- Test: `crates/rocket-infra/src/scripting/ops/fs.rs` (same file, `#[cfg(test)] mod tests` at end)

**Interfaces:**
- Produces: `read_file_impl(path: &str, encoding: &str) -> Result<String, ScriptOpError>`, `write_file_impl(path: &str, content: &str, encoding: &str) -> Result<(), ScriptOpError>`, and their thin wrappers `op_fs_read_file`/`op_fs_write_file` — consumed by Task 2 (this plan, same file) and by the extension-registration task in the next plan.

- [ ] **Step 1: Write the failing tests**

Create `crates/rocket-infra/src/scripting/ops/fs.rs`:

```rust
use deno_core::op2;
use std::fs;
use crate::scripting::ops::ScriptOpError;

fn io_err(e: std::io::Error, path: &str) -> ScriptOpError {
    ScriptOpError(format!("{path}: {e}"))
}

fn read_file_impl(path: &str, encoding: &str) -> Result<String, ScriptOpError> {
    let bytes = fs::read(path).map_err(|e| io_err(e, path))?;
    if encoding == "base64" {
        use base64::Engine;
        Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
    } else {
        String::from_utf8(bytes).map_err(|e| ScriptOpError(format!("{path}: not valid utf8: {e}")))
    }
}

#[op2]
#[string]
pub fn op_fs_read_file(#[string] path: String, #[string] encoding: String) -> Result<String, ScriptOpError> {
    read_file_impl(&path, &encoding)
}

fn write_file_impl(path: &str, content: &str, encoding: &str) -> Result<(), ScriptOpError> {
    let bytes = if encoding == "base64" {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(content)
            .map_err(|e| ScriptOpError(format!("{path}: invalid base64: {e}")))?
    } else {
        content.as_bytes().to_vec()
    };
    fs::write(path, bytes).map_err(|e| io_err(e, path))
}

#[op2(fast)]
pub fn op_fs_write_file(#[string] path: String, #[string] content: String, #[string] encoding: String) -> Result<(), ScriptOpError> {
    write_file_impl(&path, &content, &encoding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_then_read_utf8_roundtrips() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("hello.txt").to_string_lossy().to_string();
        write_file_impl(&path, "hello world", "utf8").expect("write");
        let content = read_file_impl(&path, "utf8").expect("read");
        assert_eq!(content, "hello world");
    }

    #[test]
    fn write_then_read_base64_roundtrips() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("bin.dat").to_string_lossy().to_string();
        // "hi" base64-encoded is "aGk=".
        write_file_impl(&path, "aGk=", "base64").expect("write");
        let content = read_file_impl(&path, "base64").expect("read");
        assert_eq!(content, "aGk=");
    }

    #[test]
    fn read_file_missing_path_returns_error() {
        let err = read_file_impl("/nonexistent/path/does/not/exist.txt", "utf8");
        assert!(err.is_err());
    }
}
```

In `crates/rocket-infra/src/scripting/ops/mod.rs`, find:

```rust
pub mod console;
pub mod req;
pub mod res;
pub mod rok;
```

Replace with:

```rust
pub mod console;
pub mod fs;
pub mod req;
pub mod res;
pub mod rok;
```

Run: `cargo test -p rocket-infra --lib scripting::ops::fs`
Expected: this should actually PASS immediately — the test file above is written with its implementation already present (unlike most TDD tasks, splitting the write from the RED check doesn't apply cleanly here because the three tests and the two functions they test are small enough that writing one without the other isn't meaningfully "the failing step"). Instead, verify RED the standard way: temporarily comment out the bodies of `read_file_impl` and `write_file_impl` (replace each with `unimplemented!()` — this is test-scaffold code deleted one command later, not a production path, so the project's unwrap/unimplemented convention doesn't apply to it), run the three tests and confirm they panic, then restore the real bodies from the code block above and re-run to confirm all three pass.

- [ ] **Step 2: Verify the tests pass with the real implementation**

Run: `cargo test -p rocket-infra --lib scripting::ops::fs`
Expected: PASS (3 tests). Then run `cargo check -p rocket-infra` to confirm the new `pub mod fs;` line and the op2-annotated functions compile (they aren't called from anywhere yet, so `#[allow(dead_code)]`-style unused-function warnings are expected and fine at this stage — the next plan wires them in).

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add crates/rocket-infra/src/scripting/ops/fs.rs crates/rocket-infra/src/scripting/ops/mod.rs
```

Commit message along the lines of: `feat(scripting): add fs.readFile/writeFile ops (not yet wired)`.

---

### Task 2: `fs.readDir` / `fs.exists`

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/fs.rs` (add two ops + tests)

**Interfaces:**
- Produces: `read_dir_impl(path: &str) -> Result<String, ScriptOpError>` (JSON array), `exists_impl(path: &str) -> bool`, and their thin wrappers `op_fs_read_dir`/`op_fs_exists`.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-infra/src/scripting/ops/fs.rs`, add after `op_fs_write_file` (before the `#[cfg(test)] mod tests` block):

```rust
fn read_dir_impl(path: &str) -> Result<String, ScriptOpError> {
    let entries = fs::read_dir(path).map_err(|e| io_err(e, path))?;
    let mut items = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_err(e, path))?;
        let file_type = entry.file_type().map_err(|e| io_err(e, path))?;
        items.push(serde_json::json!({
            "name": entry.file_name().to_string_lossy().to_string(),
            "isDirectory": file_type.is_dir(),
            "isFile": file_type.is_file(),
        }));
    }
    Ok(serde_json::to_string(&items).unwrap_or_else(|_| "[]".into()))
}

#[op2]
#[string]
pub fn op_fs_read_dir(#[string] path: String) -> Result<String, ScriptOpError> {
    read_dir_impl(&path)
}

fn exists_impl(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

#[op2(fast)]
pub fn op_fs_exists(#[string] path: String) -> bool {
    exists_impl(&path)
}
```

And add these tests inside the existing `#[cfg(test)] mod tests` block, after `read_file_missing_path_returns_error`:

```rust
    #[test]
    fn read_dir_lists_files_and_subdirectories() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("a.txt"), "a").expect("write a");
        fs::create_dir(dir.path().join("sub")).expect("mkdir sub");
        let json = read_dir_impl(&dir.path().to_string_lossy()).expect("read_dir");
        let entries: serde_json::Value = serde_json::from_str(&json).expect("parse");
        let names: Vec<&str> = entries
            .as_array()
            .expect("array")
            .iter()
            .map(|e| e["name"].as_str().expect("name"))
            .collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"sub"));
        let sub_entry = entries
            .as_array()
            .expect("array")
            .iter()
            .find(|e| e["name"] == "sub")
            .expect("sub entry");
        assert_eq!(sub_entry["isDirectory"], true);
        let file_entry = entries
            .as_array()
            .expect("array")
            .iter()
            .find(|e| e["name"] == "a.txt")
            .expect("a.txt entry");
        assert_eq!(file_entry["isFile"], true);
    }

    #[test]
    fn exists_true_for_present_path_false_for_absent() {
        let dir = TempDir::new().expect("tempdir");
        let present = dir.path().join("here.txt");
        fs::write(&present, "x").expect("write");
        assert!(exists_impl(&present.to_string_lossy()));
        assert!(!exists_impl(&dir.path().join("nope.txt").to_string_lossy()));
    }
```

Run: `cargo test -p rocket-infra --lib scripting::ops::fs`
Expected: FAIL to compile — `cannot find function 'read_dir_impl'/'exists_impl' in this scope` (the tests reference functions this step hasn't added the bodies for yet if you add the tests before the implementation; add them in the order shown above — implementation block first, then tests — so this compiles and the meaningful RED check is the same manual `unimplemented!()`-swap technique as Task 1's Step 1 if you want to see a real failure, otherwise proceed straight to Step 2 since the code above is already correct).

- [ ] **Step 2: Verify the tests pass**

Run: `cargo test -p rocket-infra --lib scripting::ops::fs`
Expected: PASS (5 tests total — the 3 from Task 1 plus these 2).

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add crates/rocket-infra/src/scripting/ops/fs.rs
```

Commit message along the lines of: `feat(scripting): add fs.readDir/exists ops (not yet wired)`.

---

### Task 3: `fs.mkdir` / `fs.remove`

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/fs.rs` (add two ops + tests)

**Interfaces:**
- Produces: `mkdir_impl(path: &str, recursive: bool) -> Result<(), ScriptOpError>`, `remove_impl(path: &str, recursive: bool) -> Result<(), ScriptOpError>`, and their thin wrappers `op_fs_mkdir`/`op_fs_remove`. This completes the full `fs.*` op surface this plan builds.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-infra/src/scripting/ops/fs.rs`, add after `op_fs_exists` (before the `#[cfg(test)] mod tests` block):

```rust
fn mkdir_impl(path: &str, recursive: bool) -> Result<(), ScriptOpError> {
    let result = if recursive { fs::create_dir_all(path) } else { fs::create_dir(path) };
    result.map_err(|e| io_err(e, path))
}

#[op2(fast)]
pub fn op_fs_mkdir(#[string] path: String, recursive: bool) -> Result<(), ScriptOpError> {
    mkdir_impl(&path, recursive)
}

fn remove_impl(path: &str, recursive: bool) -> Result<(), ScriptOpError> {
    let meta = fs::metadata(path).map_err(|e| io_err(e, path))?;
    let result = if meta.is_dir() {
        if recursive { fs::remove_dir_all(path) } else { fs::remove_dir(path) }
    } else {
        fs::remove_file(path)
    };
    result.map_err(|e| io_err(e, path))
}

#[op2(fast)]
pub fn op_fs_remove(#[string] path: String, recursive: bool) -> Result<(), ScriptOpError> {
    remove_impl(&path, recursive)
}
```

And add these tests inside the existing `#[cfg(test)] mod tests` block, after `exists_true_for_present_path_false_for_absent`:

```rust
    #[test]
    fn mkdir_non_recursive_fails_when_parent_missing() {
        let dir = TempDir::new().expect("tempdir");
        let nested = dir.path().join("a").join("b");
        assert!(mkdir_impl(&nested.to_string_lossy(), false).is_err());
    }

    #[test]
    fn mkdir_recursive_creates_missing_parents() {
        let dir = TempDir::new().expect("tempdir");
        let nested = dir.path().join("a").join("b");
        mkdir_impl(&nested.to_string_lossy(), true).expect("mkdir recursive");
        assert!(nested.exists());
    }

    #[test]
    fn remove_deletes_a_file() {
        let dir = TempDir::new().expect("tempdir");
        let file = dir.path().join("gone.txt");
        fs::write(&file, "x").expect("write");
        remove_impl(&file.to_string_lossy(), false).expect("remove");
        assert!(!file.exists());
    }

    #[test]
    fn remove_non_recursive_fails_on_non_empty_directory() {
        let dir = TempDir::new().expect("tempdir");
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).expect("mkdir");
        fs::write(sub.join("inner.txt"), "x").expect("write inner");
        assert!(remove_impl(&sub.to_string_lossy(), false).is_err());
        assert!(sub.exists(), "non-empty dir must survive a non-recursive remove attempt");
    }

    #[test]
    fn remove_recursive_deletes_non_empty_directory() {
        let dir = TempDir::new().expect("tempdir");
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).expect("mkdir");
        fs::write(sub.join("inner.txt"), "x").expect("write inner");
        remove_impl(&sub.to_string_lossy(), true).expect("remove recursive");
        assert!(!sub.exists());
    }
```

Run: `cargo test -p rocket-infra --lib scripting::ops::fs`
Expected: the 5 pre-existing tests still PASS; the 5 new ones exercise implementation code that's already correct above, so this reaches GREEN directly — if you want to see a genuine RED first, temporarily swap `mkdir_impl`'s and `remove_impl`'s bodies for `unimplemented!()`, confirm the 5 new tests panic, then restore the bodies shown above.

- [ ] **Step 2: Verify all tests pass**

Run: `cargo test -p rocket-infra --lib scripting::ops::fs`
Expected: PASS (10 tests total). Then run `cargo test -p rocket-infra` (full crate) and `cargo check -p rocket` to confirm nothing else in the workspace broke.

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add crates/rocket-infra/src/scripting/ops/fs.rs
```

Commit message along the lines of: `feat(scripting): add fs.mkdir/remove ops (not yet wired)`.

---

## Final verification (after all 3 tasks)

- [ ] Run `cargo test -p rocket-infra --lib scripting::ops::fs` — expect PASS (10 tests).
- [ ] Run `cargo test -p rocket-infra` (full crate) — expect PASS, no regressions.
- [ ] Run `cargo check -p rocket` — expect PASS.
- [ ] Confirm via `grep -rn "op_fs_" crates/rocket-infra/src/scripting/engine.rs` that these ops are NOT yet referenced anywhere in `engine.rs` — that's intentional; they're not registered into any extension or reachable from any script yet. The next plan in this sequence does that.
