# Bruno Import v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ZIP support, Bruno 3.0+ auto-detection (workspace.yml / opencollection.yml), unified auto-detect entry point, and a redesigned import dialog.

**Architecture:** Two new `ImportService` entry points (`import_auto` / `import_auto_from_zip`) replace the four-arg old ones. Detection via `detect_workspace`/`detect_collection` helpers replaces three hardcoded `bruno.json` checks. Bruno 3.0+ collections are copied directly (no parse/convert). Tauri commands collapse to `import_bruno` / `import_bruno_zip`. Dialog drops RadioGroup in favour of a drop zone with inline file links.

**Tech Stack:** Rust (`zip = "2"`, `tempfile`), Tauri 2, React + TypeScript, Biome.

**Spec:** `docs/superpowers/specs/2026-04-05-bruno-import-v2-design.md`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `crates/rocket-import/Cargo.toml` | Modify | Add `zip` dependency |
| `crates/rocket-import/src/error.rs` | Modify | Add `ZipExtractionFailed` and `EmptyZip` error variants |
| `crates/rocket-import/src/bru/zip_extractor.rs` | Create | `extract_to_temp()` — opens ZIP, extracts to `TempDir`, returns inner path |
| `crates/rocket-import/src/bru/mod.rs` | Modify | Re-export `zip_extractor` module |
| `crates/rocket-import/src/report.rs` | Modify | Add `detected_type: String` to `ImportReport` |
| `crates/rocket-import/src/importer.rs` | Modify | Add `BrunoFormat`, `detect_workspace`, `detect_collection`, `import_modern_collection`, update `import_collection`/`import_workspace`, add `import_auto`/`import_auto_from_zip` |
| `crates/rocket-import/src/lib.rs` | No change | Public API already exports `ImportService` |
| `src-tauri/src/commands/import.rs` | Modify | Replace old two commands with `import_bruno` and `import_bruno_zip` |
| `src-tauri/src/lib.rs` | Modify | Swap registered command names |
| `src/lib/tauri-api.ts` | Modify | Add `detectedType` to `ImportReport`; replace `importBrunoCollection`/`importBrunoWorkspace` with `importBruno`/`importBrunoZip` |
| `src/components/import/ImportBrunoDialog.tsx` | Modify | Remove RadioGroup/mode; add drop zone + inline links + ZIP picker; show `detectedType` in done state |
| `crates/rocket-import/tests/integration_test.rs` | Modify | Add v2 integration tests |

---

## Task 1: ZIP crate + error variants

**Files:**
- Modify: `crates/rocket-import/Cargo.toml`
- Modify: `crates/rocket-import/src/error.rs`

- [ ] **Step 1: Add `zip` to Cargo.toml**

In `crates/rocket-import/Cargo.toml`, add under `[dependencies]`:

```toml
[dependencies]
# ... existing deps ...
zip = "2"
```

- [ ] **Step 2: Add error variants to `error.rs`**

Replace the entire file content (keep existing variants, add two new ones):

```rust
use rocket_shared::error::DomainError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("not a Bruno directory (no marker file found): {0}")]
    NotABrunoDirectory(PathBuf),

    #[error("parse error in {path}: {message}")]
    ParseError { path: PathBuf, message: String },

    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("domain error: {0}")]
    DomainError(#[from] DomainError),

    #[error("zip extraction failed: {0}")]
    ZipExtractionFailed(String),

    #[error("zip is empty or contains no top-level directory")]
    EmptyZip,
}

pub type ImportResult<T> = Result<T, ImportError>;
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check -p rocket-import
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-import/Cargo.toml crates/rocket-import/src/error.rs
git commit -m "feat(import): add zip crate dep and ZipExtractionFailed/EmptyZip error variants"
```

---

## Task 2: ZIP extractor module

**Files:**
- Create: `crates/rocket-import/src/bru/zip_extractor.rs`
- Modify: `crates/rocket-import/src/bru/mod.rs`

- [ ] **Step 1: Write failing tests in a new file**

Create `crates/rocket-import/src/bru/zip_extractor.rs` with tests first:

```rust
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

use crate::error::ImportResult;

/// Extract a Bruno ZIP to a temp directory.
///
/// Bruno ZIPs always contain exactly one top-level directory (e.g. `my-workspace/`).
/// Returns `(TempDir, inner_path)`. The caller must keep `TempDir` alive for the
/// duration of the import — dropping it deletes the extracted files.
pub(crate) fn extract_to_temp(zip_path: &Path) -> ImportResult<(TempDir, std::path::PathBuf)> {
    use crate::error::ImportError;
    use std::fs;

    let file = fs::File::open(zip_path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| ImportError::ZipExtractionFailed(e.to_string()))?;

    let temp_dir = TempDir::new()?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ImportError::ZipExtractionFailed(e.to_string()))?;

        let out_path = match entry.enclosed_name() {
            Some(p) => temp_dir.path().join(p),
            None => continue,
        };

        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out_file = fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }
    }

    // Bruno ZIPs always extract to a single top-level folder.
    let inner = fs::read_dir(temp_dir.path())?
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .map(|e| e.path())
        .ok_or(ImportError::EmptyZip)?;

    Ok((temp_dir, inner))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal ZIP in memory with a single top-level folder and one file inside.
    fn make_test_zip(content: &[(&str, &str)]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("test.zip");
        let file = fs::File::create(&zip_path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        for (name, data) in content {
            if name.ends_with('/') {
                w.add_directory(*name, opts).unwrap();
            } else {
                w.start_file(*name, opts).unwrap();
                w.write_all(data.as_bytes()).unwrap();
            }
        }
        w.finish().unwrap();
        tmp
    }

    #[test]
    fn extracts_zip_and_returns_inner_dir() {
        let tmp = make_test_zip(&[
            ("my-workspace/", ""),
            ("my-workspace/workspace.yml", "opencollection: 1.0.0\n"),
        ]);
        let zip_path = tmp.path().join("test.zip");

        let (_dir, inner) = extract_to_temp(&zip_path).unwrap();
        assert!(inner.is_dir());
        assert_eq!(inner.file_name().unwrap(), "my-workspace");
        assert!(inner.join("workspace.yml").exists());
    }

    #[test]
    fn returns_empty_zip_error_when_no_top_level_dir() {
        let tmp = make_test_zip(&[("readme.txt", "hello")]);
        let zip_path = tmp.path().join("test.zip");

        let result = extract_to_temp(&zip_path);
        assert!(
            matches!(result, Err(crate::error::ImportError::EmptyZip)),
            "expected EmptyZip, got: {:?}",
            result
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p rocket-import zip_extractor
```

Expected: compile error — module not wired yet.

- [ ] **Step 3: Wire into `bru/mod.rs`**

Open `crates/rocket-import/src/bru/mod.rs` and add:

```rust
pub(crate) mod zip_extractor;
```

alongside the existing module declarations.

- [ ] **Step 4: Run tests — expect them to pass**

```bash
cargo test -p rocket-import zip_extractor
```

Expected: `extracts_zip_and_returns_inner_dir` PASS, `returns_empty_zip_error_when_no_top_level_dir` PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-import/src/bru/zip_extractor.rs crates/rocket-import/src/bru/mod.rs
git commit -m "feat(import): add zip_extractor module with extract_to_temp()"
```

---

## Task 3: Detection logic — `BrunoFormat` + helpers

**Files:**
- Modify: `crates/rocket-import/src/importer.rs`

- [ ] **Step 1: Write unit tests for detection**

Add a `#[cfg(test)]` block at the bottom of `importer.rs`:

```rust
#[cfg(test)]
mod detection_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detect_workspace_modern() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("workspace.yml"), "").unwrap();
        assert!(matches!(detect_workspace(d.path()), Some(BrunoFormat::Modern)));
    }

    #[test]
    fn detect_workspace_legacy() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("bruno.json"), "{}").unwrap();
        assert!(matches!(detect_workspace(d.path()), Some(BrunoFormat::Legacy)));
    }

    #[test]
    fn detect_workspace_none() {
        let d = TempDir::new().unwrap();
        assert!(detect_workspace(d.path()).is_none());
    }

    #[test]
    fn detect_collection_modern() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("opencollection.yml"), "").unwrap();
        assert!(matches!(detect_collection(d.path()), Some(BrunoFormat::Modern)));
    }

    #[test]
    fn detect_collection_legacy() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("bruno.json"), "{}").unwrap();
        assert!(matches!(detect_collection(d.path()), Some(BrunoFormat::Legacy)));
    }

    #[test]
    fn detect_collection_none() {
        let d = TempDir::new().unwrap();
        assert!(detect_collection(d.path()).is_none());
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p rocket-import detection_tests
```

Expected: compile error — `BrunoFormat`, `detect_workspace`, `detect_collection` not defined yet.

- [ ] **Step 3: Add `BrunoFormat` enum and detection helpers to `importer.rs`**

Add these definitions at the top of `importer.rs`, after the `use` statements:

```rust
/// Which generation of Bruno format a directory uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrunoFormat {
    /// Bruno 3.0+ — uses workspace.yml / opencollection.yml (OpenCollection-compatible).
    Modern,
    /// Bruno 2.x — uses bruno.json markers everywhere.
    Legacy,
}

/// Returns the Bruno format if `path` is a workspace root, or `None`.
pub(crate) fn detect_workspace(path: &Path) -> Option<BrunoFormat> {
    if path.join("workspace.yml").exists() {
        Some(BrunoFormat::Modern)
    } else if path.join("bruno.json").exists() {
        Some(BrunoFormat::Legacy)
    } else {
        None
    }
}

/// Returns the Bruno format if `path` is a collection root, or `None`.
pub(crate) fn detect_collection(path: &Path) -> Option<BrunoFormat> {
    if path.join("opencollection.yml").exists() {
        Some(BrunoFormat::Modern)
    } else if path.join("bruno.json").exists() {
        Some(BrunoFormat::Legacy)
    } else {
        None
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rocket-import detection_tests
```

Expected: all 6 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-import/src/importer.rs
git commit -m "feat(import): add BrunoFormat enum and detect_workspace/detect_collection helpers"
```

---

## Task 4: Add `detected_type` to `ImportReport`

**Files:**
- Modify: `crates/rocket-import/src/report.rs`

- [ ] **Step 1: Add the field**

Edit `report.rs` — add `detected_type` to `ImportReport`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub total_files: usize,
    pub imported: usize,
    pub skipped: Vec<SkippedItem>,
    pub created_workspace: Option<String>,
    pub created_collections: Vec<String>,
    /// "collection" or "workspace" — set by import_auto / import_collection / import_workspace.
    pub detected_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

Note: `#[derive(Default)]` on `ImportReport` means `detected_type` defaults to `""`. Each import method sets it to `"collection"` or `"workspace"` before returning.

- [ ] **Step 2: Verify it compiles (existing tests should still pass)**

```bash
cargo test -p rocket-import
```

Expected: all existing tests still pass. The new field defaults to `""` — existing assertions don't check it.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-import/src/report.rs
git commit -m "feat(import): add detected_type field to ImportReport"
```

---

## Task 5: Modern collection import path

**Files:**
- Modify: `crates/rocket-import/src/importer.rs`

This task adds `import_modern_collection` and its file-walking helper `copy_collection_files`. These are private methods on `ImportService`.

- [ ] **Step 1: Write a failing test**

Add to the `#[cfg(test)]` block in `importer.rs` (or a new `mod modern_tests`):

```rust
#[cfg(test)]
mod modern_tests {
    use super::*;
    use tempfile::TempDir;

    fn make_modern_collection(src: &std::path::Path) {
        // Minimal Bruno 3.0+ collection.
        std::fs::write(
            src.join("opencollection.yml"),
            "opencollection: \"1.0.0\"\ninfo:\n  name: my-col\n",
        )
        .unwrap();
        std::fs::write(
            src.join("get-users.yml"),
            "name: Get Users\nmethod: GET\nurl: https://api.example.com/users\n",
        )
        .unwrap();
        let env_dir = src.join("environments");
        std::fs::create_dir_all(&env_dir).unwrap();
        std::fs::write(env_dir.join("local.yml"), "name: local\nvars: []\n").unwrap();
    }

    #[test]
    fn modern_collection_copies_files_without_parsing() {
        let src_dir = TempDir::new().unwrap();
        let col_src = src_dir.path().join("my-col");
        std::fs::create_dir_all(&col_src).unwrap();
        make_modern_collection(&col_src);

        let ws_dir = TempDir::new().unwrap();
        let service = ImportService::new_with_workspace_path(ws_dir.path());

        let report = service
            .import_modern_collection(&col_src, "default")
            .expect("modern import should succeed");

        assert_eq!(report.detected_type, "collection");
        assert!(report.created_collections.contains(&"my-col".to_string()));
        // opencollection.yml is created by repo.create, not copied.
        assert!(ws_dir.path().join("collections/my-col/opencollection.yml").exists());
        // Request file should be copied.
        assert!(ws_dir.path().join("collections/my-col/get-users.yml").exists());
        // Environment copied verbatim.
        assert!(ws_dir.path().join("collections/my-col/environments/local.yml").exists());
        // Count: get-users.yml is 1 request.
        assert_eq!(report.imported, 1);
    }

    #[test]
    fn modern_collection_skips_root_opencollection_yml() {
        let src_dir = TempDir::new().unwrap();
        let col_src = src_dir.path().join("col");
        std::fs::create_dir_all(&col_src).unwrap();
        make_modern_collection(&col_src);

        let ws_dir = TempDir::new().unwrap();
        let service = ImportService::new_with_workspace_path(ws_dir.path());
        service.import_modern_collection(&col_src, "default").unwrap();

        // opencollection.yml must exist (from repo.create) but must NOT be a raw copy
        // of the source — we just verify it exists and was not a second creation.
        let oc_path = ws_dir.path().join("collections/col/opencollection.yml");
        assert!(oc_path.exists());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p rocket-import modern_tests
```

Expected: compile error — `import_modern_collection` does not exist.

- [ ] **Step 3: Implement `import_modern_collection` and `copy_collection_files`**

Add these two private methods inside `impl ImportService` in `importer.rs`:

```rust
/// Import a Bruno 3.0+ (OpenCollection-compatible) collection by direct file copy.
///
/// Skips parsing — modern Bruno files are already OpenCollection YAML.
pub(crate) fn import_modern_collection(
    &self,
    src: &Path,
    _workspace_id: &str,
) -> ImportResult<ImportReport> {
    let mut report = ImportReport::default();
    report.detected_type = "collection".to_string();

    let col_name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "imported".into());

    let resolved_name = self.resolve_collection_name(&col_name);
    let repo = self.make_collection_repo();

    repo.create(&resolved_name).map_err(ImportError::DomainError)?;
    report.created_collections.push(resolved_name.clone());

    let dest_root = self.workspace_path.join("collections").join(&resolved_name);
    self.copy_collection_files(src, src, &dest_root, &mut report)?;

    Ok(report)
}

/// Recursively copy `.yml` files from `src_dir` into `dest_root`, preserving structure.
///
/// Skips:
///   - `opencollection.yml` at the collection root (written by `repo.create`).
///   - `workspace.yml` anywhere (workspace marker, not a request).
///   - `_order.yml` (Bruno internal ordering file).
/// Files inside `environments/` are counted separately and not added to `report.imported`.
fn copy_collection_files(
    &self,
    src_root: &Path,
    src_dir: &Path,
    dest_root: &Path,
    report: &mut ImportReport,
) -> ImportResult<()> {
    for entry in std::fs::read_dir(src_dir)? {
        let entry = entry?;
        let src_path = entry.path();
        let rel = src_path.strip_prefix(src_root).unwrap_or(&src_path);
        let dest_path = dest_root.join(rel);

        if src_path.is_dir() {
            std::fs::create_dir_all(&dest_path)?;
            self.copy_collection_files(src_root, &src_path, dest_root, report)?;
            continue;
        }

        let ext = src_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "yml" | "yaml") {
            continue;
        }

        let name = src_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Root opencollection.yml is already written by repo.create().
        if src_path == src_root.join("opencollection.yml") {
            continue;
        }
        if name == "workspace.yml" || name == "_order.yml" {
            continue;
        }

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src_path, &dest_path)?;

        // Count only request files (not environment entries).
        let in_environments = rel.components().any(|c| {
            c.as_os_str() == "environments"
        });
        if !in_environments {
            report.imported += 1;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rocket-import modern_tests
```

Expected: all tests PASS.

- [ ] **Step 5: Run all import tests to make sure nothing regressed**

```bash
cargo test -p rocket-import
```

Expected: all existing tests plus new modern tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-import/src/importer.rs
git commit -m "feat(import): add modern collection import path (direct file copy, no parsing)"
```

---

## Task 6: Update `import_collection` and `import_workspace` to use detection

**Files:**
- Modify: `crates/rocket-import/src/importer.rs`

This task replaces all three hardcoded `bruno.json` checks with `detect_workspace`/`detect_collection` calls and branches on `BrunoFormat`.

- [ ] **Step 1: Update `import_collection`**

Replace the current `import_collection` method body. The new version:
- Uses `detect_collection` instead of `path.join("bruno.json").exists()`
- Sets `report.detected_type = "collection"`
- Routes modern → `import_modern_collection`, legacy → existing pipeline

```rust
pub fn import_collection(
    &self,
    path: &Path,
    workspace_id: &str,
) -> ImportResult<ImportReport> {
    match detect_collection(path) {
        None => Err(ImportError::NotABrunoDirectory(path.to_path_buf())),
        Some(BrunoFormat::Modern) => self.import_modern_collection(path, workspace_id),
        Some(BrunoFormat::Legacy) => self.import_legacy_collection(path, workspace_id),
    }
}
```

Rename the current `import_collection` body (excluding the `bruno.json` check) to `import_legacy_collection`:

```rust
fn import_legacy_collection(
    &self,
    path: &Path,
    _workspace_id: &str,
) -> ImportResult<ImportReport> {
    let mut report = ImportReport::default();
    report.detected_type = "collection".to_string();

    let col_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "imported".into());

    let resolved_name = self.resolve_collection_name(&col_name);
    let repo = self.make_collection_repo();

    repo.create(&resolved_name).map_err(ImportError::DomainError)?;
    report.created_collections.push(resolved_name.clone());

    self.walk_requests(path, path, &resolved_name, &repo, &mut report)?;

    let env_dir = path.join("environments");
    if env_dir.is_dir() {
        self.import_environments(&env_dir, &resolved_name, &mut report)?;
    }

    Ok(report)
}
```

- [ ] **Step 2: Update `import_workspace`**

Replace the `bruno.json` workspace check and the per-subdirectory `bruno.json` probe.
`import_workspace` should:
1. Use `detect_workspace(path)` instead of `path.join("bruno.json").exists()`
2. Use `detect_collection(&p)` for each subdirectory instead of `p.join("bruno.json").exists()`
3. Set `combined.detected_type = "workspace"`

Full updated method:

```rust
pub fn import_workspace(
    &self,
    path: &Path,
    create_new_workspace: bool,
    target_workspace_id: Option<&str>,
) -> ImportResult<ImportReport> {
    if detect_workspace(path).is_none() {
        return Err(ImportError::NotABrunoDirectory(path.to_path_buf()));
    }

    let mut combined = ImportReport::default();
    combined.detected_type = "workspace".to_string();

    if create_new_workspace {
        let ws_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "imported-workspace".into());
        combined.created_workspace = Some(ws_name);
    }

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() && detect_collection(&p).is_some() {
            let id = target_workspace_id.unwrap_or("default");
            match self.import_collection(&p, id) {
                Ok(r) => {
                    combined.total_files += r.total_files;
                    combined.imported += r.imported;
                    combined.skipped.extend(r.skipped);
                    combined.created_collections.extend(r.created_collections);
                }
                Err(e) => {
                    combined.skipped.push(SkippedItem {
                        path: p.to_string_lossy().to_string(),
                        reason: SkipReason::ParseError(e.to_string()),
                    });
                }
            }
        }
    }

    Ok(combined)
}
```

- [ ] **Step 3: Run all tests to verify existing tests still pass**

```bash
cargo test -p rocket-import
```

Expected: all tests PASS. `import_workspace_imports_all_sub_collections` now passes because the temp workspace has `bruno.json` at both levels, so `detect_workspace`/`detect_collection` return `Some(Legacy)`.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-import/src/importer.rs
git commit -m "feat(import): update import_collection/import_workspace to use BrunoFormat detection"
```

---

## Task 7: Add `import_auto` and `import_auto_from_zip`

**Files:**
- Modify: `crates/rocket-import/src/importer.rs`

- [ ] **Step 1: Add `import_auto` method**

Add this public method to `impl ImportService`:

```rust
/// Auto-detect whether `path` is a workspace or collection and import accordingly.
///
/// Tries workspace detection first, then collection. Returns `NotABrunoDirectory`
/// if neither marker is found.
pub fn import_auto(
    &self,
    path: &Path,
    workspace_id: &str,
) -> ImportResult<ImportReport> {
    if detect_workspace(path).is_some() {
        self.import_workspace(path, false, Some(workspace_id))
    } else if detect_collection(path).is_some() {
        self.import_collection(path, workspace_id)
    } else {
        Err(ImportError::NotABrunoDirectory(path.to_path_buf()))
    }
}
```

- [ ] **Step 2: Add `import_auto_from_zip` method**

Add this public method to `impl ImportService`:

```rust
/// Extract a Bruno ZIP to a temp directory and call `import_auto` on the inner path.
///
/// The `TempDir` is held for the duration of the import and cleaned up automatically
/// when this method returns.
pub fn import_auto_from_zip(
    &self,
    zip_path: &Path,
    workspace_id: &str,
) -> ImportResult<ImportReport> {
    let (_temp, inner) = crate::bru::zip_extractor::extract_to_temp(zip_path)?;
    self.import_auto(&inner, workspace_id)
}
```

- [ ] **Step 3: Write unit tests for `import_auto`**

Add to `importer.rs` test module:

```rust
#[cfg(test)]
mod auto_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn import_auto_detects_legacy_collection() {
        let src = TempDir::new().unwrap();
        std::fs::write(src.path().join("bruno.json"), "{}").unwrap();
        std::fs::write(
            src.path().join("req.bru"),
            "meta {\n  name: R\n  type: http\n  seq: 1\n}\nget {\n  url: https://ex.com\n}\n",
        )
        .unwrap();

        let ws = TempDir::new().unwrap();
        let service = ImportService::new_with_workspace_path(ws.path());
        let report = service.import_auto(src.path(), "default").unwrap();

        assert_eq!(report.detected_type, "collection");
        assert_eq!(report.imported, 1);
    }

    #[test]
    fn import_auto_returns_error_for_invalid_dir() {
        let dir = TempDir::new().unwrap();
        let ws = TempDir::new().unwrap();
        let service = ImportService::new_with_workspace_path(ws.path());
        let result = service.import_auto(dir.path(), "default");
        assert!(
            matches!(result, Err(ImportError::NotABrunoDirectory(_))),
            "expected NotABrunoDirectory"
        );
    }
}
```

- [ ] **Step 4: Run all tests**

```bash
cargo test -p rocket-import
```

Expected: all tests PASS, including the two new `auto_tests`.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-import/src/importer.rs
git commit -m "feat(import): add import_auto and import_auto_from_zip entry points"
```

---

## Task 8: Replace Tauri commands

**Files:**
- Modify: `src-tauri/src/commands/import.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Replace commands in `import.rs`**

Replace the entire file:

```rust
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_import::{ImportReport, ImportService};
use tauri::State;

/// Import a Bruno collection or workspace directory. Type is auto-detected from content.
#[tauri::command]
pub async fn import_bruno(
    path: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path.lock().unwrap().clone();
    let service = ImportService::new_with_workspace_path(&base);
    service
        .import_auto(&PathBuf::from(&path), &target_workspace_id)
        .map_err(|e| e.to_string())
}

/// Extract a Bruno ZIP and import the contained collection or workspace.
#[tauri::command]
pub async fn import_bruno_zip(
    zip_path: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path.lock().unwrap().clone();
    let service = ImportService::new_with_workspace_path(&base);
    service
        .import_auto_from_zip(&PathBuf::from(&zip_path), &target_workspace_id)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Update command registration in `lib.rs`**

Find the two old import command lines in `src-tauri/src/lib.rs`:

```rust
            commands::import::import_bruno_collection,
            commands::import::import_bruno_workspace,
```

Replace them with:

```rust
            commands::import::import_bruno,
            commands::import::import_bruno_zip,
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check -p rocket-app
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/import.rs src-tauri/src/lib.rs
git commit -m "feat(import): replace import_bruno_collection/workspace commands with import_bruno/zip"
```

---

## Task 9: TypeScript API update

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Update the Bruno import section**

Locate the Bruno import section (around line 741) in `tauri-api.ts` and replace:

```typescript
// ============================================================
// Bruno import
// ============================================================

export type SkipReason =
  | { type: 'unsupportedRequestType'; detail: string }
  | { type: 'unsupportedAuthType'; detail: string }
  | { type: 'parseError'; detail: string };

export interface SkippedItem {
  path: string;
  reason: SkipReason;
}

export interface ImportReport {
  totalFiles: number;
  imported: number;
  skipped: SkippedItem[];
  createdWorkspace: string | null;
  createdCollections: string[];
  detectedType: 'collection' | 'workspace';
}

export const importBruno = (path: string, targetWorkspaceId: string) =>
  invoke<ImportReport>('import_bruno', { path, targetWorkspaceId });

export const importBrunoZip = (zipPath: string, targetWorkspaceId: string) =>
  invoke<ImportReport>('import_bruno_zip', { zipPath, targetWorkspaceId });
```

Remove the old `importBrunoCollection` and `importBrunoWorkspace` function exports entirely.

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: errors because `ImportBrunoDialog.tsx` still references the old functions. That's OK — we fix it in Task 10.

- [ ] **Step 3: Commit this partial change**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(import): update TypeScript ImportReport type and API functions for v2"
```

---

## Task 10: Redesign `ImportBrunoDialog`

**Files:**
- Modify: `src/components/import/ImportBrunoDialog.tsx`

The redesign removes the `mode` state and `RadioGroup`, adds a drop zone with inline "choose folder" / "choose ZIP" links, tracks whether the selected path is a directory or ZIP, and shows `detectedType` in the done state.

- [ ] **Step 1: Replace `ImportBrunoDialog.tsx` with the redesigned component**

```tsx
import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
import { ChevronDown, ChevronRight, Loader2, Upload } from 'lucide-react';
import { useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { type ImportReport, importBruno, importBrunoZip } from '@/lib/tauri-api';

interface ImportBrunoDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workspaceId: string;
  onImportComplete?: () => void;
}

type SourceKind = 'folder' | 'zip';
type DialogState = 'picking' | 'importing' | 'done';

interface SelectedSource {
  path: string;
  kind: SourceKind;
  name: string;
}

export function ImportBrunoDialog({
  open,
  onOpenChange,
  workspaceId,
  onImportComplete,
}: ImportBrunoDialogProps) {
  const [source, setSource] = useState<SelectedSource | null>(null);
  const [dialogState, setDialogState] = useState<DialogState>('picking');
  const [report, setReport] = useState<ImportReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [skippedOpen, setSkippedOpen] = useState(false);

  function handleClose() {
    onOpenChange(false);
    // Reset state after dialog animates out.
    setTimeout(() => {
      setSource(null);
      setDialogState('picking');
      setReport(null);
      setError(null);
      setSkippedOpen(false);
    }, 200);
  }

  function clearSource() {
    setSource(null);
    setError(null);
  }

  async function handleChooseFolder() {
    const path = await openFilePicker({ directory: true, multiple: false });
    if (typeof path === 'string') {
      const name = path.split('/').pop() ?? path;
      setSource({ path, kind: 'folder', name });
      setError(null);
    }
  }

  async function handleChooseZip() {
    const path = await openFilePicker({
      directory: false,
      multiple: false,
      filters: [{ name: 'ZIP Archives', extensions: ['zip'] }],
    });
    if (typeof path === 'string') {
      const name = path.split('/').pop() ?? path;
      setSource({ path, kind: 'zip', name });
      setError(null);
    }
  }

  async function handleImport() {
    if (!source) return;
    setDialogState('importing');
    setError(null);
    try {
      const result =
        source.kind === 'zip'
          ? await importBrunoZip(source.path, workspaceId)
          : await importBruno(source.path, workspaceId);
      setReport(result);
      setDialogState('done');
      onImportComplete?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setDialogState('picking');
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (!o) handleClose();
      }}
    >
      <DialogContent className='sm:max-w-md'>
        {dialogState === 'picking' && (
          <>
            <DialogHeader>
              <DialogTitle>Import from Bruno</DialogTitle>
              <DialogDescription>
                Supports Bruno 2.x and 3.x formats. Collection or workspace is detected
                automatically.
              </DialogDescription>
            </DialogHeader>

            <div className='space-y-3 py-2'>
              <p className='text-xs font-semibold uppercase tracking-wide text-muted-foreground'>
                Source
              </p>

              {/* Drop zone */}
              <div
                className={[
                  'rounded-lg border-[1.5px] border-dashed px-5 py-6 text-center transition-colors',
                  source
                    ? 'border-primary bg-primary/5'
                    : 'border-border hover:border-muted-foreground/40',
                ].join(' ')}
              >
                {source ? (
                  <>
                    <div className='mb-2 flex h-9 w-9 items-center justify-center rounded-lg border bg-primary/10 mx-auto text-lg'>
                      {source.kind === 'zip' ? '🗜️' : '📁'}
                    </div>
                    <p className='text-sm font-semibold text-foreground'>{source.name}</p>
                    <p className='mt-0.5 text-xs text-muted-foreground'>
                      {source.kind === 'zip' ? 'ZIP archive' : 'Folder'}
                    </p>
                  </>
                ) : (
                  <>
                    <div className='mb-2 flex h-9 w-9 items-center justify-center rounded-lg border bg-muted mx-auto'>
                      <Upload className='h-4 w-4 text-muted-foreground' />
                    </div>
                    <p className='text-sm font-medium text-muted-foreground'>
                      Drop a folder or ZIP here
                    </p>
                    <p className='mt-0.5 text-xs text-muted-foreground'>
                      Bruno export or extracted directory
                    </p>
                  </>
                )}

                <div className='mt-3 flex items-center justify-center gap-1 text-xs text-muted-foreground'>
                  {source ? (
                    <>
                      change:
                      <button
                        type='button'
                        className='underline underline-offset-2 text-primary hover:text-primary/80 transition-colors'
                        onClick={() => void handleChooseFolder()}
                      >
                        folder
                      </button>
                      <span className='text-border'>·</span>
                      <button
                        type='button'
                        className='underline underline-offset-2 text-primary hover:text-primary/80 transition-colors'
                        onClick={() => void handleChooseZip()}
                      >
                        ZIP
                      </button>
                    </>
                  ) : (
                    <>
                      or browse:
                      <button
                        type='button'
                        className='underline underline-offset-2 text-primary hover:text-primary/80 transition-colors'
                        onClick={() => void handleChooseFolder()}
                      >
                        choose folder
                      </button>
                      <span className='text-border'>·</span>
                      <button
                        type='button'
                        className='underline underline-offset-2 text-primary hover:text-primary/80 transition-colors'
                        onClick={() => void handleChooseZip()}
                      >
                        choose ZIP
                      </button>
                    </>
                  )}
                </div>
              </div>

              {/* Selected path row */}
              {source && (
                <div className='flex items-center gap-2 rounded-md border bg-muted/40 px-3 py-1.5'>
                  <span className='text-xs'>{source.kind === 'zip' ? '🗜️' : '📁'}</span>
                  <span className='flex-1 truncate font-mono text-[10px] text-muted-foreground'>
                    {source.path}
                  </span>
                  <button
                    type='button'
                    className='shrink-0 text-muted-foreground hover:text-foreground transition-colors'
                    onClick={clearSource}
                    aria-label='Clear selection'
                  >
                    ✕
                  </button>
                </div>
              )}

              {error && <p className='text-xs text-destructive'>{error}</p>}
            </div>

            <DialogFooter>
              <Button variant='ghost' onClick={handleClose}>
                Cancel
              </Button>
              <Button onClick={() => void handleImport()} disabled={!source}>
                Import
              </Button>
            </DialogFooter>
          </>
        )}

        {dialogState === 'importing' && (
          <>
            <DialogHeader>
              <DialogTitle>Importing...</DialogTitle>
              <DialogDescription>
                Please wait while your collection is being imported.
              </DialogDescription>
            </DialogHeader>
            <div className='flex items-center justify-center py-8'>
              <Loader2 className='h-8 w-8 animate-spin text-muted-foreground' />
            </div>
          </>
        )}

        {dialogState === 'done' && report && (
          <>
            <DialogHeader>
              <DialogTitle>Import complete</DialogTitle>
              <DialogDescription>
                {report.imported} of {report.totalFiles} request
                {report.totalFiles !== 1 ? 's' : ''} imported successfully.
              </DialogDescription>
            </DialogHeader>

            <div className='space-y-3 py-2'>
              {/* Detected type badge */}
              {report.detectedType && (
                <div className='flex items-center gap-2'>
                  <span className='text-xs text-muted-foreground'>Imported as</span>
                  <Badge
                    variant='secondary'
                    className={
                      report.detectedType === 'workspace'
                        ? 'bg-emerald-500/15 text-emerald-400 border border-emerald-500/20'
                        : 'bg-blue-500/15 text-blue-400 border border-blue-500/20'
                    }
                  >
                    {report.detectedType === 'workspace' ? 'Workspace' : 'Collection'}
                  </Badge>
                </div>
              )}

              {report.createdCollections.length > 0 && (
                <div className='space-y-1.5'>
                  <p className='text-xs font-medium text-muted-foreground'>Created collections</p>
                  <div className='flex flex-wrap gap-1.5'>
                    {report.createdCollections.map((name) => (
                      <Badge key={name} variant='secondary'>
                        {name}
                      </Badge>
                    ))}
                  </div>
                </div>
              )}

              {report.skipped.length > 0 && (
                <Collapsible open={skippedOpen} onOpenChange={setSkippedOpen}>
                  <CollapsibleTrigger asChild>
                    <Button
                      variant='ghost'
                      size='sm'
                      className='h-auto p-0 gap-1 text-xs text-muted-foreground hover:text-foreground hover:bg-transparent'
                    >
                      {skippedOpen ? (
                        <ChevronDown className='h-3 w-3' />
                      ) : (
                        <ChevronRight className='h-3 w-3' />
                      )}
                      {report.skipped.length} item{report.skipped.length !== 1 ? 's' : ''} skipped
                    </Button>
                  </CollapsibleTrigger>
                  <CollapsibleContent>
                    <ul className='mt-1.5 space-y-1 max-h-40 overflow-y-auto'>
                      {report.skipped.map((item, i) => (
                        // biome-ignore lint/suspicious/noArrayIndexKey: stable list order after import
                        <li key={i} className='text-xs text-muted-foreground'>
                          <span className='font-mono'>{item.path}</span>
                          {' — '}
                          <span className='text-amber-500'>
                            {item.reason.type}
                            {item.reason.detail ? `: ${item.reason.detail}` : ''}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </CollapsibleContent>
                </Collapsible>
              )}
            </div>

            <DialogFooter>
              <Button onClick={handleClose}>Done</Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Run Biome lint and format**

```bash
yarn check
```

Expected: all checks pass. If lint auto-fix is needed: `yarn lint`.

- [ ] **Step 4: Commit**

```bash
git add src/components/import/ImportBrunoDialog.tsx
git commit -m "feat(import): redesign ImportBrunoDialog — drop zone, inline links, auto-detected type badge"
```

---

## Task 11: Integration tests for v2 scenarios

**Files:**
- Modify: `crates/rocket-import/tests/integration_test.rs`

- [ ] **Step 1: Write all new tests**

Add these tests to the existing integration test file. They go after the last existing test (`workspace_fixture_dir_setup`):

```rust
// ──────────────────────────────────────────────────────────
// Bruno Import v2 — modern format and ZIP tests
// ──────────────────────────────────────────────────────────

fn make_modern_collection_dir(col_dir: &std::path::Path, name: &str, req_count: usize) {
    std::fs::create_dir_all(col_dir).unwrap();
    std::fs::write(
        col_dir.join("opencollection.yml"),
        format!("opencollection: \"1.0.0\"\ninfo:\n  name: {name}\n"),
    )
    .unwrap();
    for i in 0..req_count {
        std::fs::write(
            col_dir.join(format!("req-{i}.yml")),
            format!("name: Req {i}\nmethod: GET\nurl: https://api.example.com/{i}\n"),
        )
        .unwrap();
    }
    let env_dir = col_dir.join("environments");
    std::fs::create_dir_all(&env_dir).unwrap();
    std::fs::write(env_dir.join("local.yml"), "name: local\nvars: []\n").unwrap();
}

#[test]
fn import_auto_modern_collection_directory() {
    let src = TempDir::new().unwrap();
    let col_src = src.path().join("my-col");
    make_modern_collection_dir(&col_src, "my-col", 3);

    let ws = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(ws.path());
    let report = service.import_auto(&col_src, "default").unwrap();

    assert_eq!(report.detected_type, "collection");
    assert_eq!(report.imported, 3);
    assert!(report.created_collections.contains(&"my-col".to_string()));
    assert!(ws.path().join("collections/my-col/opencollection.yml").exists());
    assert!(ws.path().join("collections/my-col/req-0.yml").exists());
    assert!(ws.path().join("collections/my-col/environments/local.yml").exists());
}

#[test]
fn import_auto_modern_workspace_directory() {
    let src = TempDir::new().unwrap();
    let ws_src = src.path().join("my-workspace");
    std::fs::create_dir_all(&ws_src).unwrap();
    std::fs::write(ws_src.join("workspace.yml"), "name: my-workspace\n").unwrap();

    let col_a = ws_src.join("col-a");
    make_modern_collection_dir(&col_a, "col-a", 2);
    let col_b = ws_src.join("col-b");
    make_modern_collection_dir(&col_b, "col-b", 1);

    let ws_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(ws_dir.path());
    let report = service.import_auto(&ws_src, "default").unwrap();

    assert_eq!(report.detected_type, "workspace");
    assert_eq!(report.imported, 3, "2 from col-a + 1 from col-b");
    assert_eq!(report.created_collections.len(), 2);
    assert!(ws_dir.path().join("collections/col-a").exists());
    assert!(ws_dir.path().join("collections/col-b").exists());
}

#[test]
fn import_auto_legacy_collection_still_works() {
    let workspace_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(workspace_dir.path());

    let report = service
        .import_auto(&fixture_path(), "default")
        .expect("legacy collection import via import_auto should succeed");

    assert_eq!(report.detected_type, "collection");
    assert!(report.imported >= 3);
    assert!(report.created_collections.contains(&"my-api".to_string()));
}

#[test]
fn import_auto_returns_error_for_non_bruno_dir() {
    let dir = TempDir::new().unwrap();
    let ws = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(ws.path());
    let result = service.import_auto(dir.path(), "default");
    assert!(result.is_err(), "expected error for non-Bruno directory");
}

#[test]
fn import_auto_from_zip_modern_collection() {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    // Build a ZIP containing a modern collection folder.
    let src = TempDir::new().unwrap();
    let zip_path = src.path().join("my-col.zip");
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut w = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default();

    w.add_directory("my-col/", opts).unwrap();
    w.start_file("my-col/opencollection.yml", opts).unwrap();
    w.write_all(b"opencollection: \"1.0.0\"\ninfo:\n  name: my-col\n").unwrap();
    w.start_file("my-col/get-users.yml", opts).unwrap();
    w.write_all(b"name: Get Users\nmethod: GET\nurl: https://api.example.com/users\n").unwrap();
    w.finish().unwrap();

    let ws_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(ws_dir.path());
    let report = service.import_auto_from_zip(&zip_path, "default").unwrap();

    assert_eq!(report.detected_type, "collection");
    assert_eq!(report.imported, 1);
    assert!(ws_dir.path().join("collections/my-col/get-users.yml").exists());
}

#[test]
fn import_workspace_mixed_modern_and_legacy_collections() {
    let src = TempDir::new().unwrap();
    let ws_src = src.path();

    // Workspace root with workspace.yml (modern marker).
    std::fs::write(ws_src.join("workspace.yml"), "name: mixed-ws\n").unwrap();

    // Modern sub-collection.
    let modern_col = ws_src.join("modern-col");
    make_modern_collection_dir(&modern_col, "modern-col", 2);

    // Legacy sub-collection.
    let legacy_col = ws_src.join("legacy-col");
    std::fs::create_dir_all(&legacy_col).unwrap();
    std::fs::write(legacy_col.join("bruno.json"), r#"{"name":"legacy-col","version":"1","type":"collection"}"#).unwrap();
    std::fs::write(
        legacy_col.join("req.bru"),
        "meta {\n  name: Req\n  type: http\n  seq: 1\n}\nget {\n  url: https://example.com\n}\n",
    )
    .unwrap();

    let ws_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(ws_dir.path());
    let report = service.import_workspace(ws_src, false, Some("default")).unwrap();

    assert_eq!(report.detected_type, "workspace");
    assert_eq!(report.imported, 3, "2 modern + 1 legacy");
    assert_eq!(report.created_collections.len(), 2);
    assert!(ws_dir.path().join("collections/modern-col").exists());
    assert!(ws_dir.path().join("collections/legacy-col").exists());
}
```

- [ ] **Step 2: Run all integration tests**

```bash
cargo test -p rocket-import
```

Expected: all tests PASS, including the 6 new ones.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-import/tests/integration_test.rs
git commit -m "test(import): add v2 integration tests for modern format, ZIP import, and mixed workspace"
```

---

## Task 12: Final verification

- [ ] **Step 1: Run all Rust tests**

```bash
cargo test -p rocket-import
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all tests pass, no compile errors.

- [ ] **Step 2: TypeScript and Biome checks**

```bash
yarn tsc --noEmit
yarn check
yarn build
```

Expected: no errors.

- [ ] **Step 3: Run full Rust test suite**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 4: Final commit if any stray fixes were needed**

```bash
git add -p
git commit -m "fix(import): final cleanup after v2 integration verification"
```

---

## Self-Review Checklist

### Spec coverage

| Spec requirement | Task |
|---|---|
| ZIP extraction to TempDir, inner path returned | Task 2 |
| `ZipExtractionFailed` / `EmptyZip` error variants | Task 1 |
| `detect_workspace` — Modern for workspace.yml, Legacy for bruno.json | Task 3 |
| `detect_collection` — Modern for opencollection.yml, Legacy for bruno.json | Task 3 |
| `import_auto` tries workspace then collection | Task 7 |
| `import_auto_from_zip` extracts then delegates | Task 7 |
| Modern path: file copy, no parse/convert | Task 5 |
| Legacy path: existing pipeline unchanged | Task 6 |
| `import_workspace` uses detection for root + per-subdir | Task 6 |
| `ImportReport.detected_type` field | Task 4 |
| Tauri commands: `import_bruno` + `import_bruno_zip` | Task 8 |
| Old commands removed, `lib.rs` updated | Task 8 |
| TypeScript `ImportReport.detectedType` | Task 9 |
| `importBruno` / `importBrunoZip` replace old functions | Task 9 |
| Dialog: drop zone, inline links, no RadioGroup | Task 10 |
| Dialog: `detectedType` badge in done state only | Task 10 |
| Tests: detect_workspace / detect_collection unit | Task 3 |
| Tests: extract_to_temp, EmptyZip | Task 2 |
| Tests: modern collection directory | Tasks 5, 11 |
| Tests: modern workspace directory | Task 11 |
| Tests: legacy collection still works | Task 11 |
| Tests: ZIP import | Task 11 |
| Tests: mixed modern+legacy workspace | Task 11 |
| Tests: import_auto on invalid dir | Tasks 7, 11 |

All requirements covered.
