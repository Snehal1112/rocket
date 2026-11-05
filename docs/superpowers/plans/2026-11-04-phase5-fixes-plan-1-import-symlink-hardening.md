# Phase 5 fixes, Plan 1: import symlink hardening (S-1, S-2, C-1, C-5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Importing a cloned Bruno folder never follows a symlink out of the source folder and never copies a symlinked file into the workspace. The `is_dir()`, `exists()` and `canonicalize()` calls left in `rocket-import` go through the `ImportFiles` port. This finishes D-51.

**Architecture:** The port already has `kind()` (does not follow symlinks) and `list_dir()` (reports `EntryKind::Symlink`). Every walk switches from `Path::is_dir()` to the `entry.kind` it already has. A symlink is skipped and, where the walk has a report, recorded as a `SkippedItem`. No new port method is needed.

**Tech Stack:** Rust, `rocket-import` (tests use `FakeFiles` and `StdFiles` in `importer.rs`), `rocket-infra` (`FsImportFiles`).

**Spec:** `.claude/reviews/2026-11-04-ddd-phase5-audit.md` (S-1, S-2, C-1, C-5) and `docs/roadmaps/ddd-roadmap.md` item D-51.

## Global Constraints

- `rocket-import` is a domain crate: no `std::fs`, no `Path::is_dir/is_file/exists/canonicalize` in production code. Tests may use `std::fs`.
- A skipped symlink is reported as `SkipReason::ParseError(String)` with the text `a symlinked <what> is not copied` (the wording `copy_support_file` already uses), never an abort.
- No `unwrap` in production code. `cargo` commands take `-j4`; never `--workspace`.
- Persistence format and the import report shape are unchanged.
- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (the crate reads `.yml` collections).

## Review Focus

- A folder symlink `up -> ../..` inside a legacy Bruno collection is not walked and nothing outside the root is imported.
- A `leak.yml -> /etc/passwd` symlink in a Bruno 3 folder is not copied and is reported.
- A `*.yml` symlink that points at a folder no longer aborts the whole import.
- A symlinked collection folder inside a Bruno workspace is skipped, not imported.
- A `.proto` path that resolves outside the collection root through a symlink is still refused by `rebase_proto_path`.

---

### Task 1: Walks stop following folder symlinks (S-2)

**Files:**
- Modify: `crates/rocket-import/src/importer.rs` (`walk_requests` ~339, `collect_yml_files` ~1168, `import_workspace_with_name` ~223, the `env_dir` check ~173 and the `collections` check ~215)
- Test: same file, `mod tests` (uses `StdFiles` plus `tempfile`, `#[cfg(unix)]`)

**Interfaces:**
- Consumes: `ImportFiles::list_dir -> Vec<DirEntryInfo { path, kind }>`, `ImportFiles::kind(&Path) -> io::Result<EntryKind>`.
- Produces: private helper `fn is_real_dir(files: &dyn ImportFiles, path: &Path) -> bool` (true only for `EntryKind::Dir`, false on a symlink or an error). Task 2 and Task 3 use it.

- [ ] **Step 1: Write the failing tests.** `legacy_walk_does_not_follow_a_folder_symlink` (build a legacy collection with `bruno.json` and `a.bru`, plus `up -> ../` pointing at a sibling folder that holds `outside.bru`; assert the report imports only `a.bru` and no request named `outside` exists). `workspace_import_skips_a_symlinked_collection_folder` (a workspace with one real collection and one symlink to a collection elsewhere; assert one created collection).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-import folder_symlink` — Expected: FAIL (the symlinked folder is walked).
- [ ] **Step 3: Implement** `is_real_dir`, and use `entry.kind == EntryKind::Dir` in `walk_requests`, `collect_yml_files` and the workspace scan. A symlinked directory in `walk_requests` and the workspace scan pushes a `SkippedItem` (`a symlinked folder is not copied`); in `collect_yml_files` (no report) it is skipped. The `env_dir` and `collections` checks use `is_real_dir`.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-import` — Expected: PASS.
- [ ] **Step 5: Commit** with the `dev-workflow-skills:1-git-commit` skill, pathspec `crates/rocket-import/src/importer.rs`.

### Task 2: The Bruno 3 copy skips symlinks (S-1, C-5)

**Files:**
- Modify: `crates/rocket-import/src/importer.rs` (`copy_collection_files` ~1085-1170, `copy_support_file` ~1240)
- Test: same file (`FakeFiles` for the report, `StdFiles` for one end-to-end case)

**Interfaces:**
- Consumes: `is_real_dir` from Task 1, `copy_support_file(files, src, dest, rel, label, report)`.
- Produces: `copy_support_file` also serves `.yml` and `.yaml` copies (label `".yml"`). A new report line `a symlinked folder is not copied` for a symlinked directory.

- [ ] **Step 1: Write the failing tests.** `a_symlinked_yml_is_reported_not_copied` (`FakeFiles` with `kinds: [("s/leak.yml", Symlink)]`, `fail_copy: true`; run `copy_collection_files` on a one-entry listing and assert `reasons(&report) == ["a symlinked .yml file is not copied"]` and `report.imported == 0`). `a_yml_symlink_to_a_folder_does_not_abort_the_import` (`StdFiles`, `link.yml -> some_dir`; the import returns `Ok` and reports one skipped item). `a_symlinked_folder_is_reported` (kind `Symlink` on a directory entry).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-import symlinked` — Expected: FAIL.
- [ ] **Step 3: Implement.** In `copy_collection_files`, an entry of kind `Symlink` is skipped with the report line before the extension match (a symlink to a folder gets the `folder` wording, any other symlink the `<ext> file` wording, decided by extension). The `.yml` copy goes through `copy_support_file` so a failed copy is reported and a non-`File` kind is never copied. `report.imported` still counts only copied request files.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-import` and `cargo test -j4 -p rocket-infra fs_import_files` — Expected: PASS.
- [ ] **Step 5: Commit** (pathspec `crates/rocket-import/src/importer.rs`).

### Task 3: Close the predicates and the direct canonicalize (C-1, D-51)

**Files:**
- Modify: `crates/rocket-import/src/importer.rs` (`detect_collection` ~36, `detect_workspace` ~25, `rebase_proto_path` ~1281, `import_environments` and the other `.exists()` or `is_dir()` calls from `grep -n "is_dir()\|is_file()\|exists()\|canonicalize" importer.rs` outside `mod tests`)
- Modify: callers of `detect_collection` and `detect_workspace` (grep the workspace, including `src-tauri`)
- Modify: `crates/rocket-import/CLAUDE.md` (the stale `canonicalize` line, C-8 part), `docs/roadmaps/ddd-roadmap.md` (D-51 row: remove "predicates left", status `done`)
- Test: `importer.rs` `mod tests`

**Interfaces:**
- Consumes: `is_real_dir` (Task 1), `ImportFiles::kind`, `ImportFiles::canonicalize`.
- Produces: `pub fn detect_collection(files: &dyn ImportFiles, path: &Path) -> Option<...>` and `pub fn detect_workspace(files: &dyn ImportFiles, path: &Path) -> Option<...>` (same return types as today), and `fn rebase_proto_path(files: &dyn ImportFiles, root: &Path, request_dir: &Path, raw: &str) -> Option<String>`. `exists()` becomes `files.kind(p).is_ok()`; `is_file()` becomes `kind == EntryKind::File` after `canonicalize`.

- [ ] **Step 1: Write the failing tests.** `rebase_proto_path_uses_the_port` (`FakeFiles` whose `canonicalize` maps `root` to `r` and `r/a.proto` to `r/a.proto` and whose `kind` says `File`; assert `Some("a.proto")` without touching the disk). `rebase_proto_path_refuses_a_path_outside_the_root` (canonicalize returns `/etc/x`; assert `None`). `detect_collection_reads_through_the_port` (a `FakeFiles` where `kind("c/opencollection.yml")` is `File`; assert it is detected).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-import rebase_proto detect_collection` — Expected: compile error or FAIL.
- [ ] **Step 3: Implement** the signatures above and update every caller. Then `grep -n "is_dir()\|is_file()\|\.exists()\|canonicalize()" crates/rocket-import/src --include=*.rs` outside `mod tests` — Expected: no output.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-import`, `cargo check -j4`, `cargo clippy -j4 -p rocket-import -- -D warnings` — Expected: all pass.
- [ ] **Step 5: Commit** (pathspec: the touched `.rs` files, the crate `CLAUDE.md`, the roadmap).
