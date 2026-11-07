# Team network send: Plan 2, import side

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 2 of 5.** Previous plan: `docs/superpowers/plans/team-network-send/2026-11-06-send-plan-01-protocol.md` (must be done and green; it adds `MAX_ZIP_ENTRIES` and `MAX_UNZIPPED_BYTES`, `transfer::TransferId` and the other limits to `rocket-network`).
**Next plan to run: `docs/superpowers/plans/team-network-send/2026-11-06-send-plan-03-service.md`**
**Recommended model: Sonnet.**

**Goal:** A staged ZIP that came from another node can be checked, named and imported safely. `rocket-import` has a strict ZIP validator. The staging port, the import port and the display-name rule exist for the service of Plan 3. `rocket-infra` stages chunks in a file and hashes them. `src-tauri` has the import adapter, and a probe test pins that the importer names the collection from the ZIP file stem.

**Architecture:** `rocket_import::transfer_zip` reads the staged file with the `zip` crate. It counts the unzipped size by really decompressing every entry into a counting sink, so a ZIP whose declared sizes lie still fails. A second function in the same module checks that the ZIP holds exactly one collection (and no workspace). The three staging types sit in `rocket-network` (`staging.rs`) and are re-exported from `rocket-app` (`network/transfer_ports.rs`), because `rocket-infra` cannot depend on `rocket-app`. `rocket-app` also holds `ImportedCollection`, `CollectionImportPort` and the pure `receiver_display_name`. `FsTransferStaging` in `rocket-infra` writes `<dir>/<transfer_id>.part`, hashes while writing and renames to `<transfer_id>.zip` on finish. `ImporterAdapter` in `src-tauri/src/import_wiring.rs` implements `CollectionImportPort` on top of `ImportServiceFactory`. It copies the staged file into its own private per-process folder (mode 0700, random name, under `<network dir>/import-tmp/`, holding a locked `lock` file) as `<display_name>.zip`, validates that COPY, imports the copy with `import_auto_from_zip`, serialises imports with a mutex, and after a failed import removes exactly the collection folders the importer reports through the new `ImportService::created_roots()`. `purge_temp` at app start removes only folders whose owner process is gone.

**Tech Stack:** Rust 1.89 or later (`std::fs::File::try_lock`; the toolchain is 1.99), `zip` 2 (already a dependency of `rocket-import`, new dev-dependency of `src-tauri`, same locked version 2.4.2), `sha2` (already in `rocket-infra`), `tempfile` (new normal dependency of `src-tauri` for the private temp folder, already a dev-dependency there).

**Spec:** `docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md` sections 2 (decisions 3 and 6), 8 (ZIP safety, deletion of temp files), 9 (`rocket-import`, `rocket-infra`, `src-tauri` components) and 12 (first open item, the stem probe). Index: `docs/superpowers/plans/team-network-send/00-plan-index.md` (binding names).

## Rulings (recorded here, because they differ from or extend the index)

1. **The staging traits live in `rocket-network`, re-exported by `rocket-app`.** The index puts `StagedFile`, `StagedZip` and `TransferStaging` in `rocket-app`, and says `rocket-infra` implements them. `rocket-infra` has no dependency on `rocket-app` (the layering rule in `crates/rocket-infra/CLAUDE.md`), and a dev-dependency cycle would link two copies of `rocket-app` in tests. So the three types are defined in `crates/rocket-network/src/staging.rs` and `rocket-app/src/network/transfer_ports.rs` does `pub use rocket_network::staging::{StagedFile, StagedZip, TransferStaging};`. Every path the index uses (`rocket_app::network::transfer_ports::TransferStaging`) still resolves. Plans 3 and 4 need no change.
2. **`validate(files: &dyn ImportFiles, path)` reads through the `ImportFiles` port.** `crates/rocket-import/CLAUDE.md` says all file access in `rocket-import` production code goes through the `ImportFiles` port, so the validator takes `files` and opens the ZIP with `files.open_read(path)` (a `Box<dyn ReadSeek>`, which gives the `zip` crate the `Read + Seek` it needs). The index signature `validate(path)` is changed to match (index and Plan 4 are updated). The validator code uses no `std::fs` and no `Path::exists`. Tests pass `rocket_infra::FsImportFiles` (`rocket-import` already has `rocket-infra` as a dev-dependency, see `crates/rocket-import/Cargo.toml`).
3. **One extra function, `transfer_zip::require_collection_root(files, path)`.** `import_auto_from_zip` also accepts a Bruno workspace ZIP, which would create several collections and a "created collections" list with more than one name. A pushed ZIP must be exactly one collection. The adapter calls `validate` and then `require_collection_root`.
4. **The name clash check is case-insensitive even though the importer is case-sensitive.** `resolve_collection_name` compares exact strings, but a Mac or Windows disk folds case, so `orders` and `Orders` would collide on disk. The stricter check only adds the `(from <sender>)` suffix a little more often.
5. **`UNSAFE_ZIP_MESSAGE` is a public const in `transfer_ports.rs`, returned ONLY for validator refusals.** The adapter returns `DomainError::InvalidInput(UNSAFE_ZIP_MESSAGE)` when `transfer_zip::validate` or `require_collection_root` refuses the ZIP (`UnsafeTransferZip`, `EmptyZip`, `ZipExtractionFailed`), and for nothing else. Every other failure uses another variant or another text: a bad display name is `InvalidInput("The collection name is not valid.")`, a staged file that cannot be opened or copied is `DomainError::Io`, and a failed import is `DomainError::Internal("The collection could not be imported: <importer message>")`. Plan 3 maps to `Rejected` by comparing the EXACT `UNSAFE_ZIP_MESSAGE` text (not any `InvalidInput`) and passes the text of every other error to `Finished.message` (index ruling 4). Tests: `the_unsafe_message_is_used_only_for_validator_refusals`.
6. **Workspace id passed to the importer: `"default"`.** The frontend passes the active workspace id as `targetWorkspaceId` to `import_bruno_zip`, but `import_auto_from_zip` ignores it for a collection ZIP (`import_modern_collection(_workspace_id)` and `import_legacy_collection(_workspace_id)`). The target workspace is the folder inside `ImportServiceFactory::workspace_path()`, which follows the shared `active_workspace_path` mutex at call time. So the adapter needs no workspace id lookup. `"default"` is the importer's own fallback value.
7. **`import_zip` is blocking.** Plan 3 must call it inside `tokio::task::spawn_blocking`.
8. **`ImporterAdapter::new(active_workspace_path: Arc<Mutex<PathBuf>>, temp_root: PathBuf)` is the pinned constructor** (index ruling 7 as amended by second-review R5). It builds its own `ImportServiceFactory` from the first argument. Plan 4 passes `Arc::clone(&active_workspace_path)` and `network_dir.join("import-tmp")` in `lib.rs`; tests pass a folder inside their temp workspace. There is no second constructor and no default in the system temp dir.
9. **The cleanup after a failed import removes exactly what the importer created (second-review R3).** The importer writes straight into the workspace (it has no dry-run), so a failed import can leave a half-written collection. `ImportService` now records every collection root it creates, right after `CollectionRepository::create` succeeded and before any request file is written, in `created_roots()`. `create` refuses an existing folder (`AlreadyExists`), so a recorded root is always one this run made. On failure the adapter removes only those roots, and only when the root sits directly inside `<workspace>/collections`. There is no snapshot and no diff of the collection list, so a collection that anything else creates during the import (the user, another import command, a git pull) survives. Test: `a_collection_created_during_a_failed_import_survives`.
10. **The adapter's temp ZIP copy lives in a private per-process folder (second-review R5).** The first import creates `<temp_root>/p<pid>-<random>/` with `tempfile::Builder::tempdir_in` (random name, mode 0700 on unix, refuses an existing path) and a `lock` file inside it, locked with `std::fs::File::try_lock` and held for the adapter's lifetime. `temp_root` itself is created with mode 0700. Each import copies the staged file to `<process dir>/<n>/<display_name>.zip`, runs `validate` and `require_collection_root` on that copy, and imports the same copy; a drop guard removes `<n>` on every exit path. `purge_temp()` (Plan 4 calls it at app start next to `TransferStaging::purge`) skips its own folder and removes another folder only when its `lock` can be locked (the owner exited) or, if it has no `lock` file, when it is older than this adapter's start. A second running instance (dev and release builds) never deletes the first one's live copy.
11. **The display name is capped by BYTES** (index ruling 9): the whole name, suffix included, is at most 120 bytes (file systems cap a name at 255 bytes). The sender part is cut to 40 bytes, then the offered part is cut to `120 - suffix bytes`, both on a character boundary, then trimmed again.
12. **Names and entries (second-review R15).** `transfer_zip` is the single home of two rules: `is_reserved_device_name` (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, any case, with or without an extension) and `is_hidden_format_char` (the bidi and zero-width list of spec section 6). The ZIP entry check refuses `:`, Cc and hidden format characters, a segment ending in a dot or a space, a reserved name and two entries whose names match case-folded. `receiver_display_name` replaces `:` and the hidden format characters with `_`, trims trailing dots and spaces, and maps a reserved name by putting `_` after the stem (`CON_`, `nul_.v2`). The adapter's `is_plain_folder_name` refuses all of these as well, because it does not trust its caller. NFC normalisation is not done (index ruling R15).
13. **The two ZIP limits stay copied (second-review R16).** `rocket-import` does not depend on `rocket-network`, so `transfer_zip` keeps `MAX_ZIP_ENTRIES` and `MAX_UNZIPPED_BYTES`, and `the_zip_limits_equal_the_protocol_limits` in `rocket-app` pins them equal. Spec section 7 says this.
14. **Scripts and trust (second-review R1).** No script gate is built. A received collection has no trust record, so `effective_capabilities` gives it no Developer mode, no extra script folders, no agent runs, no host env and no vault access until the user allows them. Its scripts DO run in the Safe sandbox on the first Send. The adapter test proves the effective mode is `Safe` for a ZIP whose `opencollection.yml` asks for `extensions.rocketapi.sandboxMode: developer` (the real key, `crates/rocket-infra/src/fs_collection/settings.rs`), after first proving that the stored setting reads back as `Developer`, so the `Safe` assertion can fail.
15. **The keychain path is a manual check (second-review R12).** A pushed ZIP with `environments/*.yml` goes through `FsEnvFactory`, which uses `crate::env_secret_store()` (the OS keychain). No automated test here drives it, because it would touch the real keychain. Plan 5 manual check 13 covers it.

## Global Constraints

- `-j4` on every cargo invocation. Never `cargo test --workspace` or `--all`. One crate at a time: `cargo test -j4 -p <crate> <filter>`. The `src-tauri` package is named `rocket` (`src-tauri/Cargo.toml`), so its tests run as `cargo test -j4 -p rocket <filter>`.
- Use only the names from the index. Where a ruling above changes a name or a place, the ruling wins.
- No `unwrap` in production code. Tests use `expect("reason")`. Never write a bare unwrap call anywhere, tests included (a repo hook rejects Rust text that contains one).
- Code comments are short, full sentences that end with a period.
- Nothing about the payload, the entry names, the collection name or the sender's paths goes into an error text at info level or above. Error texts for a rejected ZIP are fixed generic sentences.
- No commit during this plan. Every task that touches import, collections or the trust gate starts with the OpenCollection read step.
- Line numbers drift. Edits are anchored on quoted text.
- No test in `src-tauri` may put an environment file in a test ZIP: `FsEnvFactory` builds `FsEnvironmentRepo::with_secret_store(.., crate::env_secret_store())`, which is the OS keychain. That path is Plan 5 manual check 13 (Ruling 15).
- `cargo fmt` runs per crate with `-p`, never `--all`.

## Review Focus

Each line names the test that pins it.

- **A name with path characters (Task 2, Task 3).** `receiver_display_name_replaces_characters_unsafe_in_a_folder_name` (`../../etc/passwd`, `a:b*c`) in Task 2. In Task 3, `import_zip_refuses_names_that_are_not_a_plain_folder_name` (`a/b`, `..`, `.hidden`, empty, a NUL) proves the adapter does not trust the caller.
- **A long name in a multi-byte script (Task 2).** `receiver_display_name_caps_cjk_names_by_bytes` (100 CJK characters, 300 bytes, comes back at 120 bytes or less), `receiver_display_name_caps_the_whole_name_with_the_suffix_by_bytes` and `receiver_display_name_cuts_on_a_character_boundary`.
- **A name with a trailing dot or spaces (Task 2, Task 3).** `receiver_display_name_trims_spaces_and_dots_at_both_ends` and `receiver_display_name_never_returns_an_empty_name`. In Task 3 the same refusal test covers `Orders.` and `Orders `. Windows drops them from a folder name, so the collection would not match its own name.
- **A Windows reserved name, a colon, a hidden format character, a duplicate entry (Task 1, Task 2, Task 3).** `validate_refuses_a_colon_anywhere_in_an_entry_name`, `validate_refuses_windows_reserved_names_in_any_segment`, `validate_refuses_a_trailing_dot_or_space_in_a_segment`, `validate_refuses_control_and_hidden_format_characters`, `validate_refuses_duplicate_entry_names_compared_case_folded`, `the_shared_name_helpers_match_the_spec_lists` (Task 1); `receiver_display_name_maps_windows_reserved_names`, `receiver_display_name_replaces_hidden_format_characters` (Task 2); `import_zip_refuses_names_that_are_not_a_plain_folder_name` with `CON`, `nul.txt`, `a\u{202E}b` (Task 3).
- **Two imports of the same name racing (Task 3).** `two_imports_of_the_same_name_make_two_collections` runs two threads that import different ZIPs under the display name `Race`. Without the adapter mutex one of them fails with "already exists". With it the result is `Race` and `Race-1`.
- **A failed import leaves nothing behind and deletes nothing else (Task 3).** `created_roots_is_filled_when_the_import_fails_after_the_folder_was_made` (in `rocket-import`) proves the importer reports its folder even on `Err`. `remove_created_roots_removes_only_the_listed_folders_inside_collections` pins the rule, `a_collection_created_during_a_failed_import_survives` makes another collection appear in the middle of a failing import and checks it stays while the import's own folder goes, and `a_failed_import_returns_an_import_error_and_leaves_no_new_collection` runs a real failing import.
- **Only validator refusals say "unsafe" (Task 3).** `the_unsafe_message_is_used_only_for_validator_refusals`, `a_missing_staged_file_is_an_io_error_not_the_unsafe_message`.
- **The adapter's temp copy is private, validated and purgeable without hurting a live instance (Task 3).** `the_validated_copy_is_the_file_that_is_imported`, `the_private_temp_folders_are_mode_0700` (unix), `the_temp_copy_is_removed_on_success_and_on_failure`, `purge_temp_keeps_the_folder_of_a_live_adapter`, `purge_temp_removes_a_folder_whose_lock_is_free`, `purge_temp_removes_an_old_folder_without_a_lock_and_keeps_a_young_one`, `purge_temp_is_fine_with_a_missing_root`.
- **A ZIP with a single file and no root folder (Task 1, Task 3).** `validate_accepts_a_zip_with_one_root_file` and `require_collection_root_accepts_a_flat_root_marker`, `require_collection_root_refuses_a_single_file_without_a_marker` in Task 1. In Task 3, `import_zip_imports_a_flat_root_zip` and `import_zip_refuses_a_single_file_zip_without_a_marker` (nothing created).
- **A collection that exports with a non-ASCII name (Task 1, Task 2, Task 3).** `validate_accepts_non_ascii_entry_names` (Task 1), `receiver_display_name_keeps_non_ascii_letters` and `receiver_display_name_cuts_on_a_character_boundary` (Task 2), and `import_zip_keeps_a_non_ascii_name` plus the probe test `a_non_ascii_zip_stem_is_the_collection_name` (Task 3).
- **A received collection gets no trust record and no elevated access; its scripts still run in Safe mode (Task 3).** `an_imported_collection_has_no_trust_record_and_runs_only_in_safe_mode` (the fixture asks for `extensions.rocketapi.sandboxMode: developer` and the stored setting is checked first).
- **The stem probe (Task 3).** `the_zip_stem_names_the_collection_not_opencollection_yml`. If it fails, use the fallback in Task 3 Step 10.

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/rocket-import/src/error.rs` | modify | New variant `ImportError::UnsafeTransferZip(String)` |
| `crates/rocket-import/src/transfer_zip.rs` | create | `MAX_ZIP_ENTRIES`, `MAX_UNZIPPED_BYTES`, `validate`, `require_collection_root`, tests |
| `crates/rocket-import/src/lib.rs` | modify | `pub mod transfer_zip;` |
| `crates/rocket-import/CLAUDE.md` | modify | One module-map row |
| `crates/rocket-network/src/staging.rs` | create | `StagedFile`, `StagedZip`, `TransferStaging` with `create`, `purge` and `release` (ruling 1) |
| `crates/rocket-network/src/lib.rs` | modify | `pub mod staging;` and re-export |
| `crates/rocket-app/src/network/transfer_ports.rs` | create | Re-exports, `ImportedCollection`, `CollectionImportPort`, `UNSAFE_ZIP_MESSAGE`, `receiver_display_name`, tests |
| `crates/rocket-app/src/network/mod.rs` | modify | `pub mod transfer_ports;` and re-exports |
| `crates/rocket-infra/src/network/staging.rs` | create | `FsTransferStaging` (with `release`) and `FsStagedZip`, tests |
| `crates/rocket-infra/src/network/mod.rs` | modify | `pub mod staging;` and re-export |
| `crates/rocket-infra/src/lib.rs` | modify | Add `FsTransferStaging` to the `pub use network::{..}` list |
| `crates/rocket-import/tests/transfer_name_probe_test.rs` | create | The stem probe and the naming edge cases |
| `crates/rocket-import/src/importer.rs` | modify | `created_roots` field, `create_collection_root` helper at the five `collection_repo.create` sites, `pub fn created_roots()` |
| `crates/rocket-import/tests/created_roots_test.rs` | create | `created_roots` on success, on a failure after the folder was made, and on a refused create |
| `src-tauri/Cargo.toml` | modify | `tempfile = "3"` in `[dependencies]`, `zip = "2"` in `[dev-dependencies]` |
| `src-tauri/src/import_wiring.rs` | modify | `ImporterAdapter` (`new`, `purge_temp`, private process folder), `remove_created_roots`, helpers, tests |

---

### Task 1: Strict transfer ZIP validator

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-import/src/error.rs`, `crates/rocket-import/src/lib.rs`, `crates/rocket-import/CLAUDE.md`
- Create: `crates/rocket-import/src/transfer_zip.rs`
- Test: inline `#[cfg(test)] mod tests` in `transfer_zip.rs`

**Interfaces:**
- Consumes: `zip::ZipArchive`, `crate::error::{ImportError, ImportResult}`. Existing variants reused: `ZipExtractionFailed(String)` for a file that is not a readable ZIP, `EmptyZip` for a ZIP with no entries, `IoError` (through `From<io::Error>`) for a missing file.
- Produces (used by Task 3 and by the check in Task 2):

```rust
// error.rs
#[error("unsafe transfer zip: {0}")]
UnsafeTransferZip(String),

// transfer_zip.rs
pub const MAX_ZIP_ENTRIES: usize = 5000;
pub const MAX_UNZIPPED_BYTES: u64 = 50 * 1024 * 1024;
pub fn validate(files: &dyn ImportFiles, path: &std::path::Path) -> ImportResult<()>;
pub fn require_collection_root(files: &dyn ImportFiles, path: &std::path::Path) -> ImportResult<()>;
/// `CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, any case, with or without an extension.
pub fn is_reserved_device_name(segment: &str) -> bool;
/// The bidi and zero-width format characters of spec section 6 (U+00AD, U+061C, U+180E,
/// U+200B-U+200F, U+202A-U+202E, U+2060-U+2064, U+2066-U+2069, U+FEFF).
pub fn is_hidden_format_char(c: char) -> bool;
```

`is_reserved_device_name` and `is_hidden_format_char` are the single home of these two rules: Task 2 (`receiver_display_name`) and Task 3 (`is_plain_folder_name`) call them instead of keeping their own lists.

The two consts equal the `rocket-network` values. `rocket-import` must not depend on `rocket-network`, so Task 2 pins the equality in a `rocket-app` test.

- [ ] **Step 1: Read the pieces you extend**

Read `crates/rocket-import/src/error.rs` (the `ImportError` enum has no variant for an unsafe archive: `ZipExtractionFailed` is a generic read failure and `EmptyZip` is only about an empty tree, so a new variant is justified) and `crates/rocket-import/src/bru/zip_extractor.rs` (the second line of defence, with limits of 20000 entries and 512 MiB, which stays untouched).

- [ ] **Step 2: Add the error variant**

In `crates/rocket-import/src/error.rs`, after the `EmptyZip` variant:

```rust
    #[error("unsafe transfer zip: {0}")]
    UnsafeTransferZip(String),
```

- [ ] **Step 3: Write the module with stubs and the full test set (RED)**

Create `crates/rocket-import/src/transfer_zip.rs`. The two `pub fn` bodies are stubs that return `Ok(())`, so the good cases pass and every refusal test fails. The test module is the final one.

```rust
//! Strict check of a ZIP that came from another node, run before the importer sees it.
//!
//! The importer's own extractor skips unsafe entry names silently and allows 20000 entries
//! and 512 MiB, so it stays as a second line of defence and is not the check.

use std::path::Path;

use crate::error::ImportResult;
use crate::files::ImportFiles;

/// The most entries a pushed ZIP may hold. Equals `rocket_network::limits::MAX_ZIP_ENTRIES`.
pub const MAX_ZIP_ENTRIES: usize = 5000;
/// The most bytes a pushed ZIP may expand to. Equals `rocket_network::limits::MAX_UNZIPPED_BYTES`.
pub const MAX_UNZIPPED_BYTES: u64 = 50 * 1024 * 1024;

/// Stub, replaced in the next step.
pub fn validate(_files: &dyn ImportFiles, _path: &Path) -> ImportResult<()> {
    Ok(())
}

/// Stub, replaced in the next step.
pub fn require_collection_root(_files: &dyn ImportFiles, _path: &Path) -> ImportResult<()> {
    Ok(())
}

/// Stub, replaced in the next step.
pub fn is_reserved_device_name(_segment: &str) -> bool {
    false
}

/// Stub, replaced in the next step.
pub fn is_hidden_format_char(_c: char) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use rocket_infra::FsImportFiles;

    use super::*;
    use crate::error::ImportError;

    const MIB: u64 = 1024 * 1024;

    /// The real filesystem port, as `import_wiring.rs` uses it.
    fn files() -> FsImportFiles {
        FsImportFiles
    }

    fn opts() -> SimpleFileOptions {
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
    }

    /// Builds a ZIP on disk. The returned `TempDir` must stay alive while the path is used.
    fn build(fill: impl FnOnce(&mut ZipWriter<std::fs::File>)) -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("in.zip");
        let file = std::fs::File::create(&path).expect("create zip");
        let mut writer = ZipWriter::new(file);
        fill(&mut writer);
        writer.finish().expect("finish zip");
        (dir, path)
    }

    fn add_text(w: &mut ZipWriter<std::fs::File>, name: &str, text: &str) {
        w.start_file(name, opts()).expect("start file");
        w.write_all(text.as_bytes()).expect("write file");
    }

    /// Writes `total` zero bytes into one entry. Zeros deflate to almost nothing, so the
    /// test stays fast.
    fn add_zeros(w: &mut ZipWriter<std::fs::File>, name: &str, total: u64) {
        w.start_file(name, opts()).expect("start file");
        let chunk = vec![0u8; MIB as usize];
        let mut left = total;
        while left > 0 {
            let n = left.min(MIB) as usize;
            w.write_all(&chunk[..n]).expect("write zeros");
            left -= n as u64;
        }
    }

    fn is_unsafe(result: &ImportResult<()>) -> bool {
        matches!(result, Err(ImportError::UnsafeTransferZip(_)))
    }

    fn good_collection(w: &mut ZipWriter<std::fs::File>) {
        add_text(
            w,
            "orders/opencollection.yml",
            "opencollection: \"1.0.0\"\ninfo:\n  name: orders\n",
        );
        add_text(w, "orders/get.yml", "info:\n  name: get\n");
    }

    // ---- validate ----

    #[test]
    fn validate_accepts_a_good_collection_zip() {
        let (_dir, path) = build(good_collection);
        validate(&files(), &path).expect("a good zip passes");
    }

    #[test]
    fn validate_accepts_a_zip_with_one_root_file() {
        let (_dir, path) = build(|w| add_text(w, "opencollection.yml", "info:\n  name: a\n"));
        validate(&files(), &path).expect("a single root file is fine for the safety check");
    }

    #[test]
    fn validate_accepts_non_ascii_entry_names() {
        let (_dir, path) = build(|w| {
            add_text(w, "Café – Ünï API/opencollection.yml", "info:\n  name: x\n");
            add_text(w, "Café – Ünï API/日本語.yml", "info:\n  name: y\n");
        });
        validate(&files(), &path).expect("non-ascii names pass");
    }

    #[test]
    fn validate_refuses_a_zip_slip_name() {
        let (_dir, path) = build(|w| add_text(w, "../evil.txt", "x"));
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_a_dot_dot_segment_in_the_middle() {
        // `enclosed_name()` allows `a/../b`, because the depth never goes below zero.
        let (_dir, path) = build(|w| add_text(w, "a/../b.txt", "x"));
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_a_backslash_traversal() {
        let (_dir, path) = build(|w| add_text(w, "..\\evil.txt", "x"));
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_an_absolute_path() {
        let (_dir, path) = build(|w| add_text(w, "/etc/passwd", "x"));
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_a_windows_drive_path() {
        let (_dir, path) = build(|w| add_text(w, "C:/evil.txt", "x"));
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_a_symlink_entry() {
        let (_dir, path) = build(|w| {
            good_collection(w);
            w.add_symlink("orders/link", "/etc/passwd", opts())
                .expect("add symlink");
        });
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_accepts_exactly_the_entry_limit_and_refuses_one_more() {
        let (_a, at_limit) = build(|w| {
            for i in 0..MAX_ZIP_ENTRIES {
                add_text(w, &format!("c/f{i}.txt"), "");
            }
        });
        validate(&files(), &at_limit).expect("5000 entries pass");

        let (_b, over) = build(|w| {
            for i in 0..=MAX_ZIP_ENTRIES {
                add_text(w, &format!("c/f{i}.txt"), "");
            }
        });
        assert!(is_unsafe(&validate(&files(), &over)), "5001 entries must fail");
    }

    #[test]
    fn validate_accepts_exactly_the_unzipped_limit_and_refuses_one_more_byte() {
        let (_a, at_limit) = build(|w| add_zeros(w, "c/big.bin", MAX_UNZIPPED_BYTES));
        validate(&files(), &at_limit).expect("exactly 50 MiB passes");

        let (_b, over) = build(|w| add_zeros(w, "c/big.bin", MAX_UNZIPPED_BYTES + 1));
        assert!(is_unsafe(&validate(&files(), &over)), "50 MiB + 1 byte must fail");
    }

    #[test]
    fn validate_counts_all_entries_together() {
        let (_dir, path) = build(|w| {
            for i in 0..3 {
                add_zeros(w, &format!("c/part{i}.bin"), 20 * MIB);
            }
        });
        assert!(is_unsafe(&validate(&files(), &path)), "3 x 20 MiB is over the limit");
    }

    /// Patches the declared uncompressed size of the only entry, in the local header and in
    /// the central directory.
    fn lie_about_size(path: &Path, declared: u32) {
        let mut bytes = std::fs::read(path).expect("read zip");
        let central = bytes
            .windows(4)
            .rposition(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .expect("central directory header");
        bytes[central + 24..central + 28].copy_from_slice(&declared.to_le_bytes());
        bytes[22..26].copy_from_slice(&declared.to_le_bytes());
        std::fs::write(path, bytes).expect("write zip");
    }

    #[test]
    fn validate_refuses_a_zip_whose_declared_size_lies() {
        // 60 MiB of data that claims to be 1 KiB. Either the counter sees 60 MiB or the
        // zip reader fails its size or CRC check. Both must end in an error.
        let (_dir, path) = build(|w| add_zeros(w, "c/big.bin", 60 * MIB));
        lie_about_size(&path, 1024);
        assert!(validate(&files(), &path).is_err());
    }

    #[test]
    fn validate_refuses_a_file_that_is_not_a_zip() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("not.zip");
        std::fs::write(&path, b"this is not a zip file").expect("write");
        assert!(matches!(
            validate(&files(), &path),
            Err(ImportError::ZipExtractionFailed(_))
        ));
    }

    #[test]
    fn validate_refuses_an_empty_zip() {
        let (_dir, path) = build(|_| {});
        assert!(matches!(validate(&files(), &path), Err(ImportError::EmptyZip)));
    }

    #[test]
    fn validate_reports_a_missing_file_as_an_io_error() {
        let dir = TempDir::new().expect("tempdir");
        let result = validate(&files(), &dir.path().join("missing.zip"));
        assert!(matches!(result, Err(ImportError::IoError(_))));
    }

    #[test]
    fn the_refusal_text_does_not_echo_the_entry_name() {
        let (_dir, path) = build(|w| add_text(w, "../secret-name-9f2.txt", "x"));
        let text = validate(&files(), &path).expect_err("refused").to_string();
        assert!(!text.contains("secret-name-9f2"), "text was: {text}");
    }

    #[test]
    fn validate_refuses_a_colon_anywhere_in_an_entry_name() {
        // A colon names an NTFS alternate data stream on Windows.
        let (_dir, path) = build(|w| {
            good_collection(w);
            add_text(w, "orders/get.yml:hidden", "x");
        });
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_windows_reserved_names_in_any_segment() {
        for name in ["orders/con.yml", "orders/NUL", "Aux/get.yml", "orders/com1.txt", "orders/LPT9.yml"] {
            let (_dir, path) = build(|w| add_text(w, name, "x"));
            assert!(is_unsafe(&validate(&files(), &path)), "{name} must fail");
        }
        // Longer names that only start with a reserved word are fine.
        let (_dir, path) = build(|w| {
            add_text(w, "orders/CONSOLE.yml", "x");
            add_text(w, "orders/com10.yml", "x");
        });
        validate(&files(), &path).expect("CONSOLE and COM10 are not reserved");
    }

    #[test]
    fn validate_refuses_a_trailing_dot_or_space_in_a_segment() {
        for name in ["orders./get.yml", "orders/get.yml.", "orders /get.yml", "orders/get.yml "] {
            let (_dir, path) = build(|w| add_text(w, name, "x"));
            assert!(is_unsafe(&validate(&files(), &path)), "{name:?} must fail");
        }
    }

    #[test]
    fn validate_refuses_control_and_hidden_format_characters() {
        for name in ["orders/a\u{7}b.yml", "orders/\u{202E}lmy.exe", "orders/a\u{200B}b.yml", "orders/\u{FEFF}x.yml"] {
            let (_dir, path) = build(|w| add_text(w, name, "x"));
            assert!(is_unsafe(&validate(&files(), &path)), "{name:?} must fail");
        }
    }

    #[test]
    fn validate_refuses_duplicate_entry_names_compared_case_folded() {
        // `ZipWriter` refuses an exact duplicate, so write `gex.yml` and patch the name bytes
        // (local and central headers) to `get.yml`. The CRC covers the data, not the name.
        let (_a, exact) = build(|w| {
            add_text(w, "orders/get.yml", "a");
            add_text(w, "orders/gex.yml", "b");
        });
        let bytes = std::fs::read(&exact).expect("read zip");
        let patched = String::from_utf8_lossy(&bytes).contains("gex.yml");
        assert!(patched, "the fixture name is in the file");
        let mut out = bytes.clone();
        for i in 0..out.len().saturating_sub(7) {
            if &out[i..i + 7] == b"gex.yml" {
                out[i + 2] = b't';
            }
        }
        std::fs::write(&exact, out).expect("write zip");
        // The zip reader itself may refuse the duplicate (`ZipExtractionFailed`); either refusal is fine.
        assert!(validate(&files(), &exact).is_err(), "an exact duplicate must fail");

        let (_b, folded) = build(|w| {
            add_text(w, "orders/Get.yml", "a");
            add_text(w, "Orders/get.YML", "b");
        });
        assert!(is_unsafe(&validate(&files(), &folded)), "a case-folded duplicate must fail");
    }

    #[test]
    fn the_shared_name_helpers_match_the_spec_lists() {
        for name in ["CON", "con.txt", "Prn", "aux.yml", "NUL", "com1", "COM9.json", "lpt1", "LPT9"] {
            assert!(is_reserved_device_name(name), "{name}");
        }
        for name in ["CONSOLE", "com10", "lpt0", "nul_", "orders"] {
            assert!(!is_reserved_device_name(name), "{name}");
        }
        for c in ['\u{00AD}', '\u{061C}', '\u{180E}', '\u{200B}', '\u{200F}', '\u{202A}', '\u{202E}', '\u{2060}', '\u{2064}', '\u{2066}', '\u{2069}', '\u{FEFF}'] {
            assert!(is_hidden_format_char(c), "{:04X}", c as u32);
        }
        for c in ['a', 'é', '日', ' ', '\u{2065}'] {
            assert!(!is_hidden_format_char(c), "{:04X}", c as u32);
        }
    }

    // ---- require_collection_root ----

    #[test]
    fn require_collection_root_accepts_a_wrapped_modern_collection() {
        let (_dir, path) = build(good_collection);
        require_collection_root(&files(), &path).expect("one wrapped collection");
    }

    #[test]
    fn require_collection_root_accepts_a_wrapped_legacy_collection() {
        let (_dir, path) = build(|w| {
            add_text(w, "Old API/bruno.json", "{\"name\":\"Old API\"}");
            add_text(w, "Old API/get.bru", "meta {\n  name: get\n}\n");
        });
        require_collection_root(&files(), &path).expect("one wrapped legacy collection");
    }

    #[test]
    fn require_collection_root_accepts_a_flat_root_marker() {
        let (_dir, path) = build(|w| add_text(w, "opencollection.yml", "info:\n  name: a\n"));
        require_collection_root(&files(), &path).expect("a single root marker file is a collection");
    }

    #[test]
    fn require_collection_root_accepts_non_ascii_wrapper_names() {
        let (_dir, path) = build(|w| {
            add_text(w, "Café – Ünï API/opencollection.yml", "info:\n  name: x\n");
        });
        require_collection_root(&files(), &path).expect("non-ascii wrapper");
    }

    #[test]
    fn require_collection_root_refuses_a_single_file_without_a_marker() {
        let (_dir, path) = build(|w| add_text(w, "notes.txt", "hello"));
        assert!(is_unsafe(&require_collection_root(&files(), &path)));
    }

    #[test]
    fn require_collection_root_refuses_a_workspace() {
        let (_dir, path) = build(|w| {
            add_text(w, "workspace.yml", "opencollection: \"1.0.0\"\n");
            add_text(w, "opencollection.yml", "info:\n  name: a\n");
        });
        assert!(is_unsafe(&require_collection_root(&files(), &path)));

        let (_dir2, wrapped) = build(|w| {
            add_text(w, "ws/workspace.yml", "opencollection: \"1.0.0\"\n");
            add_text(w, "ws/opencollection.yml", "info:\n  name: a\n");
        });
        assert!(is_unsafe(&require_collection_root(&files(), &wrapped)));
    }

    #[test]
    fn require_collection_root_refuses_two_top_level_folders() {
        let (_dir, path) = build(|w| {
            add_text(w, "one/opencollection.yml", "info:\n  name: a\n");
            add_text(w, "two/opencollection.yml", "info:\n  name: b\n");
        });
        assert!(is_unsafe(&require_collection_root(&files(), &path)));
    }

    #[test]
    fn require_collection_root_refuses_loose_files_next_to_the_wrapper() {
        let (_dir, path) = build(|w| {
            good_collection(w);
            add_text(w, "readme.txt", "hello");
        });
        assert!(is_unsafe(&require_collection_root(&files(), &path)));
    }

    #[test]
    fn require_collection_root_refuses_a_file_that_is_not_a_zip() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("not.zip");
        std::fs::write(&path, b"nope").expect("write");
        assert!(require_collection_root(&files(), &path).is_err());
    }
}
```

- [ ] **Step 4: Export the module**

In `crates/rocket-import/src/lib.rs`, add `pub mod transfer_zip;` after the line `pub mod report;`:

```rust
pub mod report;
pub mod transfer_zip;
pub(crate) mod wsdl;
```

- [ ] **Step 5: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-import transfer_zip`
Expected: compiles; the refusal tests FAIL (`validate_refuses_*`, including the new colon, reserved-name, trailing dot or space, control or format character and case-folded duplicate tests, `validate_accepts_exactly_the_entry_limit_and_refuses_one_more`, `validate_accepts_exactly_the_unzipped_limit_and_refuses_one_more_byte`, `validate_counts_all_entries_together`, `require_collection_root_refuses_*`, `the_refusal_text_does_not_echo_the_entry_name`) and `the_shared_name_helpers_match_the_spec_lists` FAILS (the stubs return `false`); the `*_accepts_*` tests pass.

- [ ] **Step 6: Implement the validator (GREEN)**

In `transfer_zip.rs`, replace everything from `use std::path::Path;` down to the end of the `require_collection_root` stub (keep the module doc comment and the `#[cfg(test)]` module) with:

```rust
use std::collections::{BTreeSet, HashSet};
use std::io::{self, Read, Seek, Write};
use std::path::Path;

use crate::error::{ImportError, ImportResult};
use crate::files::ImportFiles;

/// The most entries a pushed ZIP may hold. Equals `rocket_network::limits::MAX_ZIP_ENTRIES`.
pub const MAX_ZIP_ENTRIES: usize = 5000;
/// The most bytes a pushed ZIP may expand to. Equals `rocket_network::limits::MAX_UNZIPPED_BYTES`.
pub const MAX_UNZIPPED_BYTES: u64 = 50 * 1024 * 1024;

const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;
const COLLECTION_MARKERS: [&str; 2] = ["opencollection.yml", "bruno.json"];

fn unsafe_zip(why: &str) -> ImportError {
    ImportError::UnsafeTransferZip(why.to_string())
}

fn unreadable(e: impl std::fmt::Display) -> ImportError {
    ImportError::ZipExtractionFailed(e.to_string())
}

/// A writer that only counts the bytes it is given.
struct CountingSink;

impl Write for CountingSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const RESERVED_DEVICE_NAMES: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

/// `CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, any case, with or without an
/// extension. Windows cannot create a file or folder with such a name.
pub fn is_reserved_device_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or_default().to_ascii_uppercase();
    if RESERVED_DEVICE_NAMES.contains(&stem.as_str()) {
        return true;
    }
    let bytes = stem.as_bytes();
    bytes.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && (b'1'..=b'9').contains(&bytes[3])
}

/// The bidi and zero-width format characters that can spoof a name (spec section 6).
pub fn is_hidden_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
    )
}

/// True for a name that stays inside the extraction folder and can be created on every
/// platform: not absolute, no `:` (drive letters and NTFS streams), no control or hidden
/// format character, no `..` segment, no segment ending in a dot or a space and no Windows
/// reserved device name (with either kind of separator).
fn is_plain_name(name: &str) -> bool {
    if name.starts_with('/') || name.starts_with('\\') || name.contains(':') {
        return false;
    }
    if name.chars().any(|c| c.is_control() || is_hidden_format_char(c)) {
        return false;
    }
    name.split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .all(|part| {
            part != ".."
                && !part.ends_with('.')
                && !part.ends_with(' ')
                && !is_reserved_device_name(part)
        })
}

/// The key two entry names collide on, on a case-folding disk.
fn folded(name: &str) -> String {
    name.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn is_symlink(unix_mode: Option<u32>) -> bool {
    unix_mode.is_some_and(|mode| mode & S_IFMT == S_IFLNK)
}

/// Checks a staged ZIP before the importer sees it. It refuses a name that `is_plain_name`
/// refuses, two entries whose names match case-folded, a symlink entry, more than `MAX_ZIP_ENTRIES` entries and more than
/// `MAX_UNZIPPED_BYTES` unzipped. The size is counted by decompressing every entry into a
/// counting sink, so a lying declared size cannot pass.
///
/// The file is opened through the `ImportFiles` port, as the crate rule requires. A missing
/// or unreadable file is an I/O error from `open_read`. The texts are fixed sentences and
/// never carry an entry name.
pub fn validate(files: &dyn ImportFiles, path: &Path) -> ImportResult<()> {
    let file = files.open_read(path)?;
    validate_archive(file)
}

fn validate_archive<R: Read + Seek>(reader: R) -> ImportResult<()> {
    let mut archive = zip::ZipArchive::new(reader).map_err(unreadable)?;
    if archive.len() == 0 {
        return Err(ImportError::EmptyZip);
    }
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(unsafe_zip("the zip has too many entries"));
    }

    let mut total: u64 = 0;
    let mut seen: HashSet<String> = HashSet::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(unreadable)?;
        if !is_plain_name(entry.name()) || entry.enclosed_name().is_none() {
            return Err(unsafe_zip("an entry has an unsafe path"));
        }
        if !seen.insert(folded(entry.name())) {
            return Err(unsafe_zip("the zip holds two entries with the same name"));
        }
        if is_symlink(entry.unix_mode()) {
            return Err(unsafe_zip("the zip holds a symbolic link"));
        }
        if entry.is_dir() {
            continue;
        }
        // Read one byte past what is left, so the limit is exact and a bomb stops early.
        let allowed = MAX_UNZIPPED_BYTES - total;
        let copied = io::copy(&mut (&mut entry).take(allowed + 1), &mut CountingSink)
            .map_err(unreadable)?;
        total += copied;
        if total > MAX_UNZIPPED_BYTES {
            return Err(unsafe_zip("the zip expands to too many bytes"));
        }
    }
    Ok(())
}

/// Requires that the ZIP holds exactly one collection: an `opencollection.yml` or
/// `bruno.json` at the root, or one wrapper folder that holds it and everything else. A
/// Bruno workspace (`workspace.yml` at the root or in the wrapper) is refused, because the
/// importer would create several collections from it.
pub fn require_collection_root(files: &dyn ImportFiles, path: &Path) -> ImportResult<()> {
    let file = files.open_read(path)?;
    let archive = zip::ZipArchive::new(file).map_err(unreadable)?;
    check_layout(archive.file_names())
}

fn check_layout<'a>(names: impl Iterator<Item = &'a str>) -> ImportResult<()> {
    let mut tops: BTreeSet<String> = BTreeSet::new();
    let mut loose_root_files = false;
    let mut root_marker = false;
    let mut nested_marker = false;
    let mut workspace = false;

    for name in names {
        let normal = name.replace('\\', "/");
        let is_dir = normal.ends_with('/');
        let parts: Vec<&str> = normal
            .split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();
        let Some(first) = parts.first().copied() else {
            continue;
        };
        if parts.len() == 1 && !is_dir {
            loose_root_files = true;
            root_marker |= COLLECTION_MARKERS.contains(&first);
            workspace |= first == "workspace.yml";
            continue;
        }
        tops.insert(first.to_string());
        if parts.len() == 2 && !is_dir {
            nested_marker |= COLLECTION_MARKERS.contains(&parts[1]);
            workspace |= parts[1] == "workspace.yml";
        }
    }

    if workspace {
        return Err(unsafe_zip("the zip holds a workspace, not one collection"));
    }
    if root_marker || (nested_marker && tops.len() == 1 && !loose_root_files) {
        return Ok(());
    }
    Err(unsafe_zip("the zip does not hold exactly one collection"))
}
```

- [ ] **Step 7: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-import transfer_zip`
Expected: all tests in `transfer_zip::tests` PASS (32 tests). The two 50 MiB tests take a few seconds in a debug build. If `validate_refuses_a_zip_whose_declared_size_lies` fails because the patch offsets are wrong for this zip version, print `bytes[central..central + 46]` and fix the offsets (central header: uncompressed size at 24, local header: at 22); do not weaken the assertion.

- [ ] **Step 8: Document the module**

In `crates/rocket-import/CLAUDE.md`, add one row to the Module Map table, after the `importer.rs` row:

```markdown
| `transfer_zip.rs` | `validate(files, path)` (strict check of a ZIP pushed by another node: no absolute or `..` names, no `:`, control or bidi/zero-width characters, no trailing dot or space, no Windows reserved names, no case-folded duplicates, no symlinks, at most 5000 entries and 50 MiB unzipped, counted by really decompressing) and `require_collection_root(files, path)` (exactly one collection, no workspace), plus the shared name helpers `is_reserved_device_name` and `is_hidden_format_char`. Used by the team network import adapter; the consts equal the `rocket-network` limits and a `rocket-app` test pins that. |
```

- [ ] **Step 9: Check and lint the crate**

Run: `cargo check -j4 -p rocket-import`
Expected: `Finished` with no errors.

Run: `cargo clippy -j4 -p rocket-import --all-targets`
Expected: no warnings from `transfer_zip.rs`.

---

### Task 2: Ports, display name and file staging

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-network/src/staging.rs`, `crates/rocket-app/src/network/transfer_ports.rs`, `crates/rocket-infra/src/network/staging.rs`
- Modify: `crates/rocket-network/src/lib.rs`, `crates/rocket-app/src/network/mod.rs`, `crates/rocket-infra/src/network/mod.rs`, `crates/rocket-infra/src/lib.rs`
- Test: inline test modules in the three new files.

**Interfaces:**
- Consumes (Plan 1): `rocket_network::transfer::TransferId::parse(&str) -> DomainResult<TransferId>` (32 lowercase hex chars) and `TransferId::random()`, `as_str()`. `rocket_network::limits::{MAX_ZIP_ENTRIES, MAX_UNZIPPED_BYTES}`. From Task 1: `rocket_import::transfer_zip::{MAX_ZIP_ENTRIES, MAX_UNZIPPED_BYTES}`.
- Produces (used by Task 3 and Plans 3 and 4), exactly as the index, with the staging types in `rocket-network` (ruling 1):

```rust
// rocket-network/src/staging.rs
pub struct StagedFile { pub path: PathBuf, pub len: u64, pub sha256: String }   // sha256 lowercase hex
pub trait StagedZip: Send { fn append(&mut self, bytes: &[u8]) -> DomainResult<()>; fn finish(self: Box<Self>) -> DomainResult<StagedFile>; fn discard(self: Box<Self>); }
pub trait TransferStaging: Send + Sync { fn create(&self, transfer_id: &str) -> DomainResult<Box<dyn StagedZip>>; fn purge(&self) -> DomainResult<()>; fn release(&self, file: &StagedFile) -> DomainResult<()>; }   // release: deletes the staged <transfer_id>.zip after the import or an abort; a missing file is Ok; a path outside the staging dir is InvalidInput

// rocket-app/src/network/transfer_ports.rs (re-exports the three above)
pub struct ImportedCollection { pub name: String }
pub trait CollectionImportPort: Send + Sync {
    fn existing_names(&self) -> DomainResult<Vec<String>>;
    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection>;   // copies to <private dir>/<display_name>.zip, validates the copy, imports it; on failure removes only created_roots()
}
pub const UNSAFE_ZIP_MESSAGE: &str = "The collection was rejected because it is not safe to import.";
pub fn receiver_display_name(offered: &str, sender: &str, existing: &[String]) -> String;   // the whole name is at most 120 bytes (ruling 11)

// rocket-infra
pub struct FsTransferStaging { /* dir */ }
impl FsTransferStaging { pub fn new(dir: PathBuf) -> Self; }   // implements TransferStaging
```

- [ ] **Step 1: Read what the name rule must match**

Read `Collection::validate_name` in `crates/rocket-collection/src/collection.rs` (rejects empty, `/`, `\` and a leading dot) and `safe_file_stem` in `crates/rocket-app/src/collection_export_service.rs` (replaces `/ \ : * ? " < > |`, NUL and control characters with `_`). `resolve_collection_name` in `crates/rocket-import/src/importer.rs` compares names exactly (case-sensitive) and appends `-1`, `-2`. The display-name rule below uses the export's replacement set, adds trimming of spaces and dots at both ends (Windows drops trailing ones and `Collection::validate_name` forbids a leading dot), compares case-insensitively (ruling 4), replaces the hidden bidi and zero-width characters and maps Windows reserved names with `_` (ruling 13). Both character rules come from `rocket_import::transfer_zip` (Task 1), so there is one list.

- [ ] **Step 2: Add the staging types to `rocket-network`**

Create `crates/rocket-network/src/staging.rs`:

```rust
//! Ports for the temporary file that holds a received collection ZIP.

use std::path::PathBuf;

use rocket_shared::error::DomainResult;

/// A finished staged file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    pub path: PathBuf,
    pub len: u64,
    /// Lowercase hex SHA-256 of the whole file.
    pub sha256: String,
}

/// One file being written chunk by chunk. Dropping it without `finish` removes the file.
pub trait StagedZip: Send {
    fn append(&mut self, bytes: &[u8]) -> DomainResult<()>;
    /// Closes the file and returns its path, length and hash.
    fn finish(self: Box<Self>) -> DomainResult<StagedFile>;
    /// Deletes the file.
    fn discard(self: Box<Self>);
}

/// Where received ZIPs are staged. Implemented by `rocket-infra`.
pub trait TransferStaging: Send + Sync {
    /// Starts a staged file for a transfer id. The id must be 32 lowercase hex characters.
    fn create(&self, transfer_id: &str) -> DomainResult<Box<dyn StagedZip>>;
    /// Removes everything in the staging folder. A missing folder is fine.
    fn purge(&self) -> DomainResult<()>;
    /// Deletes the staged `<transfer_id>.zip` after the import or an abort. A missing file is
    /// fine. A path outside the staging folder is refused with `InvalidInput`.
    fn release(&self, file: &StagedFile) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_traits_are_object_safe() {
        fn takes(_: Option<&dyn TransferStaging>, _: Option<Box<dyn StagedZip>>) {}
        takes(None, None);
    }
}
```

In `crates/rocket-network/src/lib.rs`, add `pub mod staging;` after `pub mod rate;` and `pub mod records;` ordering (keep alphabetical: between `records` and `wire`), and after the `pub use records::{..};` block add:

```rust
pub use staging::{StagedFile, StagedZip, TransferStaging};
```

- [ ] **Step 3: Check the crate and its test**

Run: `cargo test -j4 -p rocket-network staging`
Expected: `the_traits_are_object_safe` PASS.

- [ ] **Step 4: Write the ports file with stubs and the full test set (RED)**

Create `crates/rocket-app/src/network/transfer_ports.rs`. `receiver_display_name` is a stub that returns the offered name unchanged, so the tests fail.

```rust
//! Ports of the receive side of a collection transfer: where the ZIP is staged, how it is
//! imported, and which name the new collection gets.

use std::path::Path;

use rocket_import::transfer_zip::{is_hidden_format_char, is_reserved_device_name};
use rocket_shared::error::DomainResult;

pub use rocket_network::staging::{StagedFile, StagedZip, TransferStaging};

/// The collection an import created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedCollection {
    pub name: String,
}

/// Imports a staged ZIP as a new collection of the active workspace.
///
/// `import_zip` copies the ZIP to a private temp folder as `<display_name>.zip`, validates
/// that copy, imports the same copy and never overwrites an existing collection. After a
/// failed import it removes only the folders the importer reports it created. It blocks, so
/// callers run it in `spawn_blocking`.
pub trait CollectionImportPort: Send + Sync {
    /// Names of the collections of the active workspace.
    fn existing_names(&self) -> DomainResult<Vec<String>>;
    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection>;
}

/// The text `import_zip` returns (as `DomainError::InvalidInput`) for a ZIP that failed the
/// safety checks. The service compares against it to tell "not safe" from "import failed".
pub const UNSAFE_ZIP_MESSAGE: &str = "The collection was rejected because it is not safe to import.";

/// Stub, replaced in the next step.
pub fn receiver_display_name(offered: &str, _sender: &str, _existing: &[String]) -> String {
    offered.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_free_name_is_kept() {
        let got = receiver_display_name("Orders", "Ana", &names(&["Billing"]));
        assert_eq!(got, "Orders");
    }

    #[test]
    fn a_clash_adds_the_sender() {
        let got = receiver_display_name("Orders", "Ana", &names(&["Orders"]));
        assert_eq!(got, "Orders (from Ana)");
    }

    #[test]
    fn a_clash_on_the_suffixed_name_keeps_it_as_is() {
        // The importer adds -1, -2 on its own.
        let got = receiver_display_name("Orders", "Ana", &names(&["Orders", "Orders (from Ana)"]));
        assert_eq!(got, "Orders (from Ana)");
    }

    #[test]
    fn the_clash_check_ignores_letter_case() {
        let got = receiver_display_name("orders", "Ana", &names(&["Orders"]));
        assert_eq!(got, "orders (from Ana)");
    }

    #[test]
    fn the_sender_name_is_trimmed() {
        let got = receiver_display_name("Orders", "  Ana  ", &names(&["Orders"]));
        assert_eq!(got, "Orders (from Ana)");
    }

    #[test]
    fn an_empty_sender_falls_back_to_a_teammate() {
        for blank in ["", "   ", "\t"] {
            let got = receiver_display_name("Orders", blank, &names(&["Orders"]));
            assert_eq!(got, "Orders (from a teammate)", "sender was {blank:?}");
        }
    }

    #[test]
    fn receiver_display_name_replaces_characters_unsafe_in_a_folder_name() {
        let got = receiver_display_name("../../etc/passwd", "Ana", &[]);
        assert_eq!(got, "_.._etc_passwd");
        assert!(!got.contains('/') && !got.contains('\\'));

        let got = receiver_display_name("a:b*c?d\"e<f>g|h", "Ana", &[]);
        assert_eq!(got, "a_b_c_d_e_f_g_h");

        let got = receiver_display_name("tab\there\nnewline\0nul", "Ana", &[]);
        assert_eq!(got, "tab_here_newline_nul");
    }

    #[test]
    fn receiver_display_name_replaces_hidden_format_characters() {
        // A right-to-left override or a zero-width space could spoof the name in the sidebar.
        let got = receiver_display_name("a\u{202E}b\u{200B}c\u{FEFF}", "Ana", &[]);
        assert_eq!(got, "a_b_c_");
    }

    #[test]
    fn receiver_display_name_maps_windows_reserved_names() {
        assert_eq!(receiver_display_name("CON", "Ana", &[]), "CON_");
        assert_eq!(receiver_display_name("nul.v2", "Ana", &[]), "nul_.v2");
        assert_eq!(receiver_display_name("com1", "Ana", &[]), "com1_");
        assert_eq!(receiver_display_name("CONSOLE", "Ana", &[]), "CONSOLE");
        // With the suffix and no dot the name is no longer reserved.
        assert_eq!(receiver_display_name("Aux", "Ana", &names(&["aux"])), "Aux (from Ana)");
        // Windows also reserves `CON.<anything>`, so a dotted clash is mapped too.
        assert_eq!(
            receiver_display_name("con.v2", "Ana", &names(&["con_.v2", "con.v2"])),
            "con_.v2 (from Ana)"
        );
    }

    #[test]
    fn the_sender_name_is_cleaned_the_same_way() {
        let got = receiver_display_name("Orders", "../Ana/", &names(&["Orders"]));
        assert_eq!(got, "Orders (from _Ana_)");
    }

    #[test]
    fn receiver_display_name_trims_spaces_and_dots_at_both_ends() {
        assert_eq!(receiver_display_name("Orders.", "Ana", &[]), "Orders");
        assert_eq!(receiver_display_name("Orders ", "Ana", &[]), "Orders");
        assert_eq!(receiver_display_name(" . Orders . ", "Ana", &[]), "Orders");
        assert_eq!(receiver_display_name(".hidden", "Ana", &[]), "hidden");
        assert_eq!(receiver_display_name("v1.2 API", "Ana", &[]), "v1.2 API");
    }

    #[test]
    fn receiver_display_name_never_returns_an_empty_name() {
        for odd in ["", "   ", "...", " . ", "\0"] {
            let got = receiver_display_name(odd, "Ana", &[]);
            assert!(!got.is_empty(), "offered was {odd:?}");
            assert!(!got.starts_with('.') && !got.ends_with('.') && !got.ends_with(' '));
        }
        assert_eq!(receiver_display_name("", "Ana", &[]), "Received collection");
    }

    #[test]
    fn receiver_display_name_keeps_non_ascii_letters() {
        let got = receiver_display_name("Café – Ünï 日本語", "Zoë", &names(&["Café – Ünï 日本語"]));
        assert_eq!(got, "Café – Ünï 日本語 (from Zoë)");
    }

    #[test]
    fn receiver_display_name_cuts_on_a_character_boundary() {
        // 150 two-byte characters = 300 bytes. A byte cut would panic or split a character.
        let long = "é".repeat(150);
        let got = receiver_display_name(&long, "Ana", &[]);
        assert_eq!(got.len(), 120);
        assert_eq!(got.chars().count(), 60);
        assert!(got.chars().all(|c| c == 'é'));

        // A three-byte character that does not divide 120 evenly after the suffix.
        let long_sender = "ü".repeat(80);
        let got = receiver_display_name("Orders", &long_sender, &names(&["Orders"]));
        assert_eq!(got, format!("Orders (from {})", "ü".repeat(20)));
    }

    #[test]
    fn receiver_display_name_caps_cjk_names_by_bytes() {
        // 100 CJK characters are 300 bytes. Counting characters would let all of them through.
        let cjk = "日".repeat(100);
        let got = receiver_display_name(&cjk, "Ana", &[]);
        assert_eq!(got, "日".repeat(40));
        assert_eq!(got.len(), 120);

        // With the suffix " (from Ana)" (11 bytes) the offered part gets 109 bytes: 36 characters.
        let taken = "日".repeat(40);
        let got = receiver_display_name(&cjk, "Ana", &names(&[taken.as_str()]));
        assert_eq!(got, format!("{} (from Ana)", "日".repeat(36)));
        assert_eq!(got.len(), 119);
    }

    #[test]
    fn receiver_display_name_caps_the_whole_name_with_the_suffix_by_bytes() {
        let cjk = "日".repeat(100);
        let cjk_sender = "本".repeat(60);
        for (offered, sender) in [
            ("Orders".to_string(), "Ana".to_string()),
            (cjk.clone(), "Ana".to_string()),
            (cjk.clone(), cjk_sender.clone()),
            ("a".repeat(300), cjk_sender.clone()),
            ("é".repeat(200), "ü".repeat(200)),
        ] {
            // Make the offered name clash so that the suffix is added.
            let clash = receiver_display_name(&offered, &sender, &[]);
            let got = receiver_display_name(&offered, &sender, &[clash]);
            assert!(got.len() <= 120, "{} bytes for {offered:.10}", got.len());
            assert!(!got.is_empty() && !got.ends_with(' ') && !got.ends_with('.'));
        }
        // The sender part alone is cut to 40 bytes: 13 characters of 3 bytes.
        let got = receiver_display_name("Orders", &cjk_sender, &names(&["Orders"]));
        assert_eq!(got, format!("Orders (from {})", "本".repeat(13)));
    }

    #[test]
    fn a_cut_that_ends_in_a_space_is_trimmed() {
        let offered = format!("{} tail", "a".repeat(119));
        let got = receiver_display_name(&offered, "Ana", &[]);
        assert_eq!(got, "a".repeat(119));
    }

    #[test]
    fn the_zip_limits_equal_the_protocol_limits() {
        // `rocket-import` cannot depend on `rocket-network`, so the values are duplicated and
        // this test is what keeps them equal.
        assert_eq!(
            rocket_import::transfer_zip::MAX_ZIP_ENTRIES,
            rocket_network::limits::MAX_ZIP_ENTRIES
        );
        assert_eq!(
            rocket_import::transfer_zip::MAX_UNZIPPED_BYTES,
            rocket_network::limits::MAX_UNZIPPED_BYTES
        );
    }
}
```

In `crates/rocket-app/src/network/mod.rs`, add after `mod status;`:

```rust
pub mod transfer_ports;
```

and after the `pub use status::{NetworkState, NetworkStatus};` line:

```rust
pub use transfer_ports::{
    receiver_display_name, CollectionImportPort, ImportedCollection, StagedFile, StagedZip,
    TransferStaging, UNSAFE_ZIP_MESSAGE,
};
```

- [ ] **Step 5: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-app transfer_ports`
Expected: compiles; `a_clash_adds_the_sender`, `a_clash_on_the_suffixed_name_keeps_it_as_is`, `the_clash_check_ignores_letter_case`, `the_sender_name_is_trimmed`, `an_empty_sender_falls_back_to_a_teammate`, the cleaning, hidden-format, reserved-name, trimming, empty-name, non-ASCII, boundary and the two byte-cap tests (`receiver_display_name_caps_cjk_names_by_bytes`, `receiver_display_name_caps_the_whole_name_with_the_suffix_by_bytes`) FAIL; `a_free_name_is_kept` and `the_zip_limits_equal_the_protocol_limits` PASS.

- [ ] **Step 6: Implement `receiver_display_name` (GREEN)**

Replace the stub function in `transfer_ports.rs` with:

```rust
/// The whole display name, suffix included, in bytes. File systems cap a name at 255 bytes.
const MAX_NAME_BYTES: usize = 120;
/// The sender part of the suffix, in bytes.
const MAX_SENDER_BYTES: usize = 40;
const FALLBACK_NAME: &str = "Received collection";
const FALLBACK_SENDER: &str = "a teammate";

/// Cuts `text` to at most `max_bytes` bytes, never inside a character.
fn cap_bytes(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Makes text safe to use as a folder name on every platform: the characters the export
/// also replaces, plus the hidden bidi and zero-width format characters, become `_`, the text is cut to `max_bytes` bytes (never inside a
/// character), and spaces and dots at both ends go. A trailing dot or space is dropped by
/// Windows, and a leading dot is refused by `Collection::validate_name`.
fn clean_part(raw: &str, max_bytes: usize) -> String {
    let replaced: String = raw
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() || is_hidden_format_char(c) => '_',
            c => c,
        })
        .collect();
    cap_bytes(&replaced, max_bytes)
        .trim_matches(|c: char| c.is_whitespace() || c == '.')
        .to_string()
}

/// Windows cannot create a folder named `CON`, `NUL`, `COM1` and so on, with or without an
/// extension, so `_` goes right after the reserved stem (`CON_`, `nul_.v2`). The stem is at
/// most 4 bytes, so the cap that follows never cuts the `_` away.
fn unreserve(name: String) -> String {
    if !is_reserved_device_name(&name) {
        return name;
    }
    let mapped = match name.split_once('.') {
        Some((stem, rest)) => format!("{stem}_.{rest}"),
        None => format!("{name}_"),
    };
    cap_bytes(&mapped, MAX_NAME_BYTES)
        .trim_matches(|c: char| c.is_whitespace() || c == '.')
        .to_string()
}

/// The name the receiver gives the new collection: the offered name when it is free,
/// otherwise `<offered> (from <sender>)`. The importer still adds `-1`, `-2` when that name
/// is taken too. The clash check ignores letter case, because a Mac or Windows disk does.
/// The result is at most `MAX_NAME_BYTES` bytes in both cases.
pub fn receiver_display_name(offered: &str, sender: &str, existing: &[String]) -> String {
    let mut base = clean_part(offered, MAX_NAME_BYTES);
    if base.is_empty() {
        base = FALLBACK_NAME.to_string();
    }
    let wanted = base.to_lowercase();
    if !existing.iter().any(|name| name.to_lowercase() == wanted) {
        return unreserve(base);
    }
    let mut who = clean_part(sender, MAX_SENDER_BYTES);
    if who.is_empty() {
        who = FALLBACK_SENDER.to_string();
    }
    let suffix = format!(" (from {who})");
    // The suffix is at most 48 bytes, so the offered part keeps at least 72 bytes. Cutting can
    // leave a trailing space or dot, so the part is trimmed again.
    let room = MAX_NAME_BYTES - suffix.len();
    let kept = cap_bytes(&base, room).trim_matches(|c: char| c.is_whitespace() || c == '.');
    unreserve(format!("{kept}{suffix}"))
}
```

- [ ] **Step 7: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-app transfer_ports`
Expected: all 18 tests PASS. Check by hand if one fails: `../../etc/passwd` becomes `.._.._etc_passwd` after replacement, and trimming the two leading dots gives `_.._etc_passwd`, the asserted value. In the same way `../Ana/` becomes `.._Ana_` and then `_Ana_`.

- [ ] **Step 8: Write the infra staging file with its full tests (RED)**

Create `crates/rocket-infra/src/network/staging.rs`. `create` is a stub that returns an error, so every test fails.

```rust
//! Stages a received collection ZIP in a file, chunk by chunk, and hashes it on the way.

use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;

use rocket_network::staging::{StagedFile, StagedZip, TransferStaging};
use rocket_network::transfer::TransferId;
use rocket_shared::error::{DomainError, DomainResult};
use sha2::{Digest, Sha256};

/// Staging folder of the receive side: `<dir>/<transfer_id>.part` while chunks arrive and
/// `<dir>/<transfer_id>.zip` once finished.
pub struct FsTransferStaging {
    dir: PathBuf,
}

impl FsTransferStaging {
    /// Does not touch the disk. The folder is created by the first `create`.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl TransferStaging for FsTransferStaging {
    fn create(&self, _transfer_id: &str) -> DomainResult<Box<dyn StagedZip>> {
        Err(DomainError::Internal("not implemented".into()))
    }

    fn purge(&self) -> DomainResult<()> {
        Err(DomainError::Internal("not implemented".into()))
    }

    fn release(&self, _file: &StagedFile) -> DomainResult<()> {
        Err(DomainError::Internal("not implemented".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef0123456789abcdef";
    const OTHER_ID: &str = "fedcba9876543210fedcba9876543210";
    const SHA_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const SHA_EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn staging() -> (tempfile::TempDir, FsTransferStaging) {
        let dir = tempfile::tempdir().expect("tempdir");
        let staging = FsTransferStaging::new(dir.path().join("transfers"));
        (dir, staging)
    }

    fn listing(dir: &std::path::Path) -> Vec<String> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn new_does_not_create_the_folder() {
        let (dir, _staging) = staging();
        assert!(!dir.path().join("transfers").exists());
    }

    #[test]
    fn a_staged_file_is_finished_with_its_length_and_hash() {
        let (dir, staging) = staging();
        let mut staged = staging.create(ID).expect("create");
        staged.append(b"a").expect("append");
        staged.append(b"bc").expect("append");
        let done = staged.finish().expect("finish");

        assert_eq!(done.len, 3);
        assert_eq!(done.sha256, SHA_ABC);
        assert_eq!(done.path, dir.path().join("transfers").join(format!("{ID}.zip")));
        assert_eq!(fs::read(&done.path).expect("read"), b"abc");
        assert_eq!(listing(&dir.path().join("transfers")), vec![format!("{ID}.zip")]);
    }

    #[test]
    fn an_empty_file_has_the_empty_hash() {
        let (_dir, staging) = staging();
        let done = staging.create(ID).expect("create").finish().expect("finish");
        assert_eq!(done.len, 0);
        assert_eq!(done.sha256, SHA_EMPTY);
    }

    #[test]
    fn the_hash_covers_every_chunk() {
        let (_dir, staging) = staging();
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let mut staged = staging.create(ID).expect("create");
        for chunk in data.chunks(32 * 1024) {
            staged.append(chunk).expect("append");
        }
        let done = staged.finish().expect("finish");

        assert_eq!(done.len, data.len() as u64);
        assert_eq!(done.sha256, format!("{:x}", Sha256::digest(&data)));
        assert_eq!(fs::read(&done.path).expect("read"), data);
    }

    #[test]
    fn the_part_file_exists_while_chunks_arrive() {
        let (dir, staging) = staging();
        let mut staged = staging.create(ID).expect("create");
        staged.append(b"x").expect("append");
        assert_eq!(listing(&dir.path().join("transfers")), vec![format!("{ID}.part")]);
        staged.discard();
    }

    #[test]
    fn discard_deletes_the_file() {
        let (dir, staging) = staging();
        let mut staged = staging.create(ID).expect("create");
        staged.append(b"data").expect("append");
        staged.discard();
        assert!(listing(&dir.path().join("transfers")).is_empty());
    }

    #[test]
    fn dropping_a_staged_file_deletes_it() {
        let (dir, staging) = staging();
        {
            let mut staged = staging.create(ID).expect("create");
            staged.append(b"data").expect("append");
        }
        assert!(listing(&dir.path().join("transfers")).is_empty());
    }

    #[test]
    fn a_finished_file_survives_dropping_the_staging_handle() {
        let (dir, staging) = staging();
        let done = staging.create(ID).expect("create").finish().expect("finish");
        assert!(done.path.exists());
        drop(staging);
        assert!(done.path.exists());
        assert_eq!(listing(&dir.path().join("transfers")).len(), 1);
    }

    #[test]
    fn purge_removes_everything_in_the_folder() {
        let (dir, staging) = staging();
        let root = dir.path().join("transfers");
        let _finished = staging.create(ID).expect("create").finish().expect("finish");
        let mut open = staging.create(OTHER_ID).expect("create");
        open.append(b"half").expect("append");
        fs::create_dir_all(root.join("stray-dir/inner")).expect("stray dir");
        fs::write(root.join("stray-dir/inner/file"), b"x").expect("stray file");
        fs::write(root.join("note.txt"), b"x").expect("note");

        staging.purge().expect("purge");

        assert!(listing(&root).is_empty());
        assert!(root.exists(), "the folder itself stays");
        // The open file lost its path; dropping it must not fail or panic.
        drop(open);
    }

    #[test]
    fn purge_on_a_missing_folder_is_fine() {
        let (_dir, staging) = staging();
        staging.purge().expect("a missing folder is not an error");
    }

    #[test]
    fn an_id_that_is_not_32_lowercase_hex_chars_is_refused() {
        let (dir, staging) = staging();
        let bad = [
            "",
            "abc",
            "../evil",
            "../../etc/passwd",
            "0123456789abcdef0123456789abcde",
            "0123456789abcdef0123456789abcdef0",
            "0123456789ABCDEF0123456789ABCDEF",
            "0123456789abcdef0123456789abcde/",
            "0123456789abcdef0123456789abcd\\e",
            "0123456789abcdef0123456789abcd\0e",
            "0123456789abcdef0123456789abcdeg",
        ];
        for id in bad {
            assert!(staging.create(id).is_err(), "id {id:?} must be refused");
        }
        assert!(!dir.path().join("transfers").exists(), "nothing was created");
        assert!(!dir.path().join("evil.part").exists());
    }

    #[test]
    fn a_second_create_for_the_same_id_is_refused_and_keeps_the_first() {
        let (dir, staging) = staging();
        let mut first = staging.create(ID).expect("create");
        first.append(b"first").expect("append");
        assert!(staging.create(ID).is_err());
        let done = first.finish().expect("finish");
        assert_eq!(fs::read(&done.path).expect("read"), b"first");
        assert_eq!(listing(&dir.path().join("transfers")).len(), 1);
    }

    #[test]
    fn two_transfers_stage_independently() {
        let (_dir, staging) = staging();
        let mut a = staging.create(ID).expect("a");
        let mut b = staging.create(OTHER_ID).expect("b");
        a.append(b"aaa").expect("append a");
        b.append(b"bb").expect("append b");
        let done_a = a.finish().expect("finish a");
        let done_b = b.finish().expect("finish b");
        assert_eq!((done_a.len, done_b.len), (3, 2));
        assert_ne!(done_a.sha256, done_b.sha256);
    }

    #[test]
    fn release_deletes_the_staged_file() {
        let (dir, staging) = staging();
        let done = staging.create(ID).expect("create").finish().expect("finish");
        assert!(done.path.exists());
        staging.release(&done).expect("release");
        assert!(!done.path.exists());
        assert!(listing(&dir.path().join("transfers")).is_empty());
    }

    #[test]
    fn releasing_twice_is_ok() {
        let (_dir, staging) = staging();
        let done = staging.create(ID).expect("create").finish().expect("finish");
        staging.release(&done).expect("first release");
        staging.release(&done).expect("a missing file is not an error");
    }

    #[test]
    fn release_refuses_a_path_outside_the_staging_folder() {
        let (dir, staging) = staging();
        let outside = dir.path().join("outside.zip");
        fs::write(&outside, b"keep me").expect("outside file");
        let forged = StagedFile {
            path: outside.clone(),
            len: 7,
            sha256: SHA_ABC.to_string(),
        };
        let err = staging.release(&forged).expect_err("must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)), "got {err:?}");
        assert_eq!(fs::read(&outside).expect("read"), b"keep me");

        let climbing = StagedFile {
            path: dir.path().join("transfers").join("..").join("outside.zip"),
            len: 7,
            sha256: SHA_ABC.to_string(),
        };
        assert!(staging.release(&climbing).is_err());
        assert!(outside.exists(), "the outside file survives");
    }

    #[test]
    fn release_does_not_touch_another_transfers_files() {
        let (dir, staging) = staging();
        let first = staging.create(ID).expect("create").finish().expect("finish");
        let second = staging.create(OTHER_ID).expect("create").finish().expect("finish");
        let mut open = staging.create("00000000000000000000000000000001").expect("create");
        open.append(b"half").expect("append");

        staging.release(&first).expect("release");

        assert!(!first.path.exists());
        assert!(second.path.exists());
        assert_eq!(
            listing(&dir.path().join("transfers")),
            vec![
                format!("{OTHER_ID}.zip"),
                "00000000000000000000000000000001.part".to_string()
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_staged_file_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, staging) = staging();
        let _open = staging.create(ID).expect("create");
        let path = dir.path().join("transfers").join(format!("{ID}.part"));
        let mode = fs::metadata(path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o077, 0, "mode was {mode:o}");
    }
}
```

In `crates/rocket-infra/src/network/mod.rs`, add `pub mod staging;` after `pub mod server;` (keeping the list ordered: after `server`, before `socket` is `pub(crate) mod socket;`; place `pub mod staging;` between them) and add the re-export after the `pub use server::RustlsPeerServer;` line:

```rust
pub use staging::FsTransferStaging;
```

In `crates/rocket-infra/src/lib.rs`, in the `pub use network::{` list (near line 115) add `FsTransferStaging` to the list. Read the exact list first and keep the existing order style.

- [ ] **Step 9: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-infra network::staging`
Expected: compiles; every test that calls `create` or `purge` FAILS with `not implemented` (the `new_does_not_create_the_folder` test passes); `release_deletes_the_staged_file`, `releasing_twice_is_ok` and `release_does_not_touch_another_transfers_files` FAIL, and `release_refuses_a_path_outside_the_staging_folder` fails at its `InvalidInput` match (the stub returns `Internal`). Unused-import warnings for `File`, `ErrorKind`, `Write` are expected until the next step.

- [ ] **Step 10: Implement the staging (GREEN)**

In `crates/rocket-infra/src/network/staging.rs`, replace the stub `impl TransferStaging for FsTransferStaging { .. }` block with the following (keep the struct, `new` and the test module):

```rust
/// The file being written. `file` is `None` once closed.
struct FsStagedZip {
    file: Option<File>,
    part: PathBuf,
    zip: PathBuf,
    hasher: Sha256,
    len: u64,
    finished: bool,
}

impl StagedZip for FsStagedZip {
    fn append(&mut self, bytes: &[u8]) -> DomainResult<()> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| DomainError::Internal("The staged file is closed.".into()))?;
        file.write_all(bytes)?;
        self.hasher.update(bytes);
        self.len += bytes.len() as u64;
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> DomainResult<StagedFile> {
        let file = self
            .file
            .take()
            .ok_or_else(|| DomainError::Internal("The staged file is closed.".into()))?;
        file.sync_all()?;
        drop(file);
        // A rename onto an existing file fails on Windows, so a stale one goes first.
        let _ = fs::remove_file(&self.zip);
        fs::rename(&self.part, &self.zip)?;
        self.finished = true;
        Ok(StagedFile {
            path: self.zip.clone(),
            len: self.len,
            sha256: format!("{:x}", self.hasher.clone().finalize()),
        })
    }

    fn discard(self: Box<Self>) {
        // Dropping removes the part file.
        drop(self);
    }
}

impl Drop for FsStagedZip {
    fn drop(&mut self) {
        if !self.finished {
            self.file.take();
            let _ = fs::remove_file(&self.part);
        }
    }
}

impl TransferStaging for FsTransferStaging {
    fn create(&self, transfer_id: &str) -> DomainResult<Box<dyn StagedZip>> {
        // The id becomes part of a file name, so it must be a plain 32-char hex id.
        let id = TransferId::parse(transfer_id)?;
        fs::create_dir_all(&self.dir)?;
        let part = self.dir.join(format!("{}.part", id.as_str()));
        let zip = self.dir.join(format!("{}.zip", id.as_str()));

        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&part).map_err(|e| {
            if e.kind() == ErrorKind::AlreadyExists {
                DomainError::AlreadyExists("That transfer is already being received.".into())
            } else {
                DomainError::from(e)
            }
        })?;
        Ok(Box::new(FsStagedZip {
            file: Some(file),
            part,
            zip,
            hasher: Sha256::new(),
            len: 0,
            finished: false,
        }))
    }

    fn release(&self, file: &StagedFile) -> DomainResult<()> {
        // Only a plain `<dir>/<name>.zip` is deleted, so a forged path cannot remove other files.
        let inside = file.path.parent() == Some(self.dir.as_path())
            && file.path.extension().is_some_and(|ext| ext == "zip")
            && file
                .path
                .components()
                .all(|c| !matches!(c, std::path::Component::ParentDir));
        if !inside {
            return Err(DomainError::InvalidInput(
                "The staged file is not in the staging folder.".into(),
            ));
        }
        match fs::remove_file(&file.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn purge(&self) -> DomainResult<()> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            // `file_type` does not follow a symlink, so a link is removed, not its target.
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
        Ok(())
    }
}
```

`DomainError` has a `From<std::io::Error>` impl (the infra collection code uses `?` on `fs` calls in functions that return `DomainResult`); if the compiler says otherwise, map with `DomainError::Io(e.to_string())`.

- [ ] **Step 11: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-infra network::staging`
Expected: all 18 tests PASS (17 on Windows, where the permission test is `cfg(unix)`).

- [ ] **Step 12: Check and lint the three crates**

Run: `cargo check -j4 -p rocket-network`
Expected: `Finished`, no errors.

Run: `cargo check -j4 -p rocket-app`
Expected: `Finished`, no errors.

Run: `cargo check -j4 -p rocket-infra`
Expected: `Finished`, no errors.

Run: `cargo clippy -j4 -p rocket-infra --all-targets`
Expected: no warnings from `network/staging.rs`.

---

### Task 3: The import adapter in `src-tauri`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-import/src/importer.rs`, `src-tauri/Cargo.toml`, `src-tauri/src/import_wiring.rs`
- Create: `crates/rocket-import/tests/transfer_name_probe_test.rs`, `crates/rocket-import/tests/created_roots_test.rs`
- Test: the two new integration test files, and inline tests in `import_wiring.rs`.

**Interfaces:**
- Consumes: Task 1 `transfer_zip::{validate, require_collection_root, is_reserved_device_name, is_hidden_format_char}`; Task 2 `rocket_app::network::{CollectionImportPort, ImportedCollection, UNSAFE_ZIP_MESSAGE}`; existing `ImportServiceFactory::{new, workspace_path, service}`, `ImportService::import_auto_from_zip`, `ImportReport.created_collections`, `rocket_infra::{FsCollectionRepo, FsImportFiles, FsCollectionTrustStore}`, `rocket_app::effective_capabilities`.
- Produces (used by Plan 4 wiring in `lib.rs`):

```rust
// rocket-import (importer.rs)
impl ImportService {
    /// The collection roots (`<workspace>/collections/<name>`) this service created, in order.
    /// Filled right after each successful `CollectionRepository::create`, so an `Err` from an
    /// import still lists the folder it made.
    pub fn created_roots(&self) -> Vec<PathBuf>;
}

// src-tauri (import_wiring.rs)
pub struct ImporterAdapter { /* factory, import_lock, temp_root, process_dir, started */ }
impl ImporterAdapter {
    /// Pinned signature (index ruling 7, second-review R5). Plan 4 calls
    /// `ImporterAdapter::new(Arc::clone(&active_workspace_path), network_dir.join("import-tmp"))`.
    /// It builds its own `ImportServiceFactory` and touches no disk.
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>, temp_root: PathBuf) -> Self;
    /// Removes the per-process folders of instances that are gone. Never its own folder or a
    /// live instance's. A missing root is fine. Plan 4 calls it at app start.
    pub fn purge_temp(&self) -> DomainResult<()>;
}
impl CollectionImportPort for ImporterAdapter { /* existing_names, import_zip */ }
```

`import_zip` errors (rulings 5 and 9): `InvalidInput(UNSAFE_ZIP_MESSAGE)` only when the validator refuses the copy; `InvalidInput("The collection name is not valid.")` for a bad name; `Io("The staged collection file could not be read.")` for a staged file that cannot be copied, and `Io` for a private folder that cannot be made; `Internal("The collection could not be imported: <importer message>")` for a failed import, after the roots in `created_roots()` have been removed. Plan 3 compares the exact `UNSAFE_ZIP_MESSAGE` text and passes the text of the other errors to `Finished.message`.

**Ruling (where the adapter lives and how it is tested):** the adapter stays in `src-tauri/src/import_wiring.rs` as the spec says, because `ImportServiceFactory` and `FsEnvFactory` (which needs `crate::env_secret_store()`) are there. It is testable in place: the existing tests in that file already build `ImportServiceFactory` over a temp folder, and an import of a collection ZIP without environment files never touches the keychain. An import WITH environment files does reach the keychain through `FsEnvFactory`; that path is covered by Plan 5 manual check 13, not by a test here (ruling 15). So no extra trait split is needed. The adapter's import body is a private `import_with(&self, service: &ImportService, copy: &Path)` so a test can hand it a service over a failing `ImportFiles` and check the cleanup. The name behaviour of the importer itself is pinned one level lower, in `rocket-import`, where the test harness is lighter.

**Facts this task relies on (verified in the code):**
- `import_auto_from_zip` takes a ZIP path and uses the ZIP file stem as the name hint. For a modern collection (`opencollection.yml`) `import_modern_collection` takes `col_name = name_hint` and creates the folder through `resolve_collection_name(col_name)`, which returns the name when free, else `name-1`, `name-2`. The legacy path does the same. So the folder name comes from the stem, not from `info.name` of `opencollection.yml`. Step 2 proves it.
- `Collection::validate_name` (called by `FsCollectionRepo::create`) refuses empty names, `/`, `\` and a leading dot. `create` returns `AlreadyExists` when the folder exists, so a race fails and never overwrites, and a successful `create` always made a new folder.
- The importer creates a collection root in exactly five places, each `self.collection_repo.create(&<name>).map_err(ImportError::DomainError)?;` followed by `report.created_collections.push(..)`: the legacy collection import (around line 154), the modern collection import (around line 579), Postman (around line 662), WSDL (around line 799) and OpenAPI (around line 935). A workspace import calls these per collection. `ImportServiceFactory::service_for` builds a fresh `ImportService` per call with the repo rooted at `<base>/collections` and `workspace_path = base`, so `created_roots` is per import and `workspace_path.join("collections").join(name)` is the folder `create` made.
- Trust: `FsCollectionRepo::create` does not write a trust record. Only `CollectionService::create` does. An import goes through the repository, so no record exists, and `effective_capabilities` returns no vault access, no host environment and an effective sandbox of `Safe` whatever the file asks for. The stored request lives in `opencollection.yml` under `extensions.rocketapi.sandboxMode` (`crates/rocket-infra/src/fs_collection/settings.rs`).

- [ ] **Step 1: Write the probe and name edge-case tests in `rocket-import`**

Create `crates/rocket-import/tests/transfer_name_probe_test.rs`. This pins how the importer names a collection from a ZIP file name (spec section 12, first open item). The tests are expected to pass at once; a failure means the importer behaves differently and Step 10 applies.

```rust
//! The team network import names the new collection from the ZIP file stem. These tests pin
//! that, so the adapter in `src-tauri` can rely on it. A modern-format ZIP is built by hand
//! with an `opencollection.yml` whose `info.name` differs from the file name.

use std::io::Write;
use std::path::{Path, PathBuf};

use rocket_collection::CollectionRepository;
use rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_import::ImportService;
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

struct FsEnvFactory(PathBuf);

impl EnvironmentRepositoryFactory for FsEnvFactory {
    fn for_collection(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::new(
            self.0
                .join("collections")
                .join(collection_name)
                .join("environments"),
        ))
    }
}

fn service(workspace: &Path) -> ImportService {
    let base = workspace.to_path_buf();
    ImportService::new(
        base.clone(),
        Box::new(FsCollectionRepo::new_standalone(base.join("collections"))),
        Box::new(FsEnvFactory(base)),
        std::sync::Arc::new(rocket_infra::FsImportFiles),
    )
}

fn repo(workspace: &Path) -> FsCollectionRepo {
    FsCollectionRepo::new_standalone(workspace.join("collections"))
}

fn write_zip(path: &Path, entries: &[(&str, &str)]) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    for (name, text) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .expect("start file");
        writer.write_all(text.as_bytes()).expect("write entry");
    }
    writer.finish().expect("finish zip");
}

const COLLECTION: [(&str, &str); 2] = [
    (
        "Inner Name/opencollection.yml",
        "opencollection: \"1.0.0\"\ninfo:\n  name: Inner Name\n",
    ),
    (
        "Inner Name/get.yml",
        "info:\n  name: get\nhttp:\n  method: GET\n  url: https://example.test\n",
    ),
];

/// Imports `COLLECTION` stored under `<stem>.zip` and returns the created names.
fn import_as(workspace: &Path, stem: &str) -> Vec<String> {
    let zips = TempDir::new().expect("tempdir");
    let zip_path = zips.path().join(format!("{stem}.zip"));
    write_zip(&zip_path, &COLLECTION);
    service(workspace)
        .import_auto_from_zip(&zip_path, "default", false)
        .expect("import")
        .created_collections
}

fn collection_names(workspace: &Path) -> Vec<String> {
    let mut names: Vec<String> = repo(workspace)
        .list()
        .expect("list")
        .into_iter()
        .map(|c| c.name)
        .collect();
    names.sort();
    names
}

#[test]
fn the_zip_stem_names_the_collection_not_opencollection_yml() {
    let ws = TempDir::new().expect("tempdir");
    let created = import_as(ws.path(), "Renamed Copy");
    assert_eq!(created, vec!["Renamed Copy".to_string()]);
    assert_eq!(collection_names(ws.path()), vec!["Renamed Copy".to_string()]);
    assert!(
        !ws.path().join("collections/Inner Name").exists(),
        "the wrapper folder and info.name must not name the collection"
    );
}

#[test]
fn a_second_import_of_the_same_stem_gets_a_numeric_suffix_and_overwrites_nothing() {
    let ws = TempDir::new().expect("tempdir");
    assert_eq!(import_as(ws.path(), "Orders"), vec!["Orders".to_string()]);
    let marker = ws.path().join("collections/Orders/keep.txt");
    std::fs::write(&marker, "mine").expect("marker");

    assert_eq!(import_as(ws.path(), "Orders"), vec!["Orders-1".to_string()]);
    assert_eq!(std::fs::read_to_string(&marker).expect("marker"), "mine");
    assert_eq!(
        collection_names(ws.path()),
        vec!["Orders".to_string(), "Orders-1".to_string()]
    );
}

#[test]
fn a_stem_with_a_suffix_like_text_is_kept_as_written() {
    let ws = TempDir::new().expect("tempdir");
    let created = import_as(ws.path(), "Orders (from Ana)");
    assert_eq!(created, vec!["Orders (from Ana)".to_string()]);
}

#[test]
fn a_non_ascii_zip_stem_is_the_collection_name() {
    let ws = TempDir::new().expect("tempdir");
    let created = import_as(ws.path(), "Café – Ünï 日本語");
    assert_eq!(created, vec!["Café – Ünï 日本語".to_string()]);
    assert!(ws.path().join("collections/Café – Ünï 日本語/opencollection.yml").exists());
}

#[test]
fn a_zip_with_one_root_file_and_no_wrapper_folder_imports() {
    let ws = TempDir::new().expect("tempdir");
    let zips = TempDir::new().expect("tempdir");
    let zip_path = zips.path().join("Flat.zip");
    write_zip(
        &zip_path,
        &[("opencollection.yml", "opencollection: \"1.0.0\"\ninfo:\n  name: x\n")],
    );
    let report = service(ws.path())
        .import_auto_from_zip(&zip_path, "default", false)
        .expect("import");
    assert_eq!(report.created_collections, vec!["Flat".to_string()]);
}

#[test]
fn a_zip_with_one_file_and_no_marker_creates_nothing() {
    let ws = TempDir::new().expect("tempdir");
    let zips = TempDir::new().expect("tempdir");
    let zip_path = zips.path().join("Notes.zip");
    write_zip(&zip_path, &[("notes.txt", "hello")]);
    let result = service(ws.path()).import_auto_from_zip(&zip_path, "default", false);
    assert!(result.is_err());
    assert!(collection_names(ws.path()).is_empty());
}
```

- [ ] **Step 2: Run the probe**

Run: `cargo test -j4 -p rocket-import --test transfer_name_probe_test`
Expected: 6 tests PASS. If `the_zip_stem_names_the_collection_not_opencollection_yml` FAILS (the created name is `Inner Name`), go to Step 10 and use the fallback; the other failing tests then show which of the later expectations also change.

- [ ] **Step 3: Write the `created_roots` tests with a stub (RED)**

In `crates/rocket-import/src/importer.rs`, add a stub method to `impl ImportService`, right after `new`:

```rust
    /// Stub, replaced in Step 5.
    pub fn created_roots(&self) -> Vec<PathBuf> {
        Vec::new()
    }
```

Create `crates/rocket-import/tests/created_roots_test.rs`. `FailingCollectionDirs` delegates every `ImportFiles` method to `FsImportFiles`, except that `create_dir_all` fails for any folder inside `<workspace>/collections/`. The importer calls `create_dir_all` for a request subfolder AFTER `create` made the collection root, so the import returns `Err` with the root already on disk.

```rust
//! `ImportService::created_roots` lists exactly the collection folders one import made, even
//! when the import fails afterwards. The team network adapter removes only those.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rocket_collection::CollectionRepository;
use rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_import::{DirEntryInfo, EntryKind, ImportFiles, ImportService, ReadSeek, WorkDir};
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo, FsImportFiles};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

struct FsEnvFactory(PathBuf);

impl EnvironmentRepositoryFactory for FsEnvFactory {
    fn for_collection(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::new(
            self.0.join("collections").join(collection_name).join("environments"),
        ))
    }
}

/// The real port, except that folders inside `<workspace>/collections/` cannot be made.
struct FailingCollectionDirs {
    collections: PathBuf,
}

impl ImportFiles for FailingCollectionDirs {
    fn list_dir(&self, dir: &Path) -> io::Result<Vec<DirEntryInfo>> {
        FsImportFiles.list_dir(dir)
    }
    fn kind(&self, path: &Path) -> io::Result<EntryKind> {
        FsImportFiles.kind(path)
    }
    fn create_dir_all(&self, dir: &Path) -> io::Result<()> {
        if dir.starts_with(&self.collections) {
            return Err(io::Error::other("disk full"));
        }
        FsImportFiles.create_dir_all(dir)
    }
    fn copy(&self, src: &Path, dest: &Path) -> io::Result<()> {
        FsImportFiles.copy(src, dest)
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        FsImportFiles.read_to_string(path)
    }
    fn write(&self, path: &Path, text: &str) -> io::Result<()> {
        FsImportFiles.write(path, text)
    }
    fn regular_file_len(&self, path: &Path) -> io::Result<Option<u64>> {
        FsImportFiles.regular_file_len(path)
    }
    fn read_up_to(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        FsImportFiles.read_up_to(path, max_bytes)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        FsImportFiles.canonicalize(path)
    }
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn ReadSeek>> {
        FsImportFiles.open_read(path)
    }
    fn temp_dir(&self) -> io::Result<Box<dyn WorkDir>> {
        FsImportFiles.temp_dir()
    }
    fn write_stream(&self, path: &Path, from: &mut dyn Read) -> io::Result<u64> {
        FsImportFiles.write_stream(path, from)
    }
}

fn service(workspace: &Path, files: Arc<dyn ImportFiles>) -> ImportService {
    let base = workspace.to_path_buf();
    ImportService::new(
        base.clone(),
        Box::new(FsCollectionRepo::new_standalone(base.join("collections"))),
        Box::new(FsEnvFactory(base)),
        files,
    )
}

/// A modern collection with one request in a subfolder, stored as `<stem>.zip`.
fn zip_with_subfolder(dir: &Path, stem: &str) -> PathBuf {
    let path = dir.join(format!("{stem}.zip"));
    let file = std::fs::File::create(&path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    for (name, text) in [
        ("c/opencollection.yml", "opencollection: \"1.0.0\"\ninfo:\n  name: c\n"),
        ("c/sub/get.yml", "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x.test\n"),
    ] {
        writer.start_file(name, SimpleFileOptions::default()).expect("start file");
        writer.write_all(text.as_bytes()).expect("write entry");
    }
    writer.finish().expect("finish zip");
    path
}

#[test]
fn created_roots_names_the_folder_a_successful_import_made() {
    let ws = TempDir::new().expect("tempdir");
    let zips = TempDir::new().expect("tempdir");
    let zip = zip_with_subfolder(zips.path(), "Orders");
    let svc = service(ws.path(), Arc::new(FsImportFiles));

    svc.import_auto_from_zip(&zip, "default", false).expect("import");

    assert_eq!(svc.created_roots(), vec![ws.path().join("collections").join("Orders")]);
}

#[test]
fn created_roots_is_filled_when_the_import_fails_after_the_folder_was_made() {
    let ws = TempDir::new().expect("tempdir");
    let zips = TempDir::new().expect("tempdir");
    let zip = zip_with_subfolder(zips.path(), "Orders");
    let files = FailingCollectionDirs { collections: ws.path().join("collections") };
    let svc = service(ws.path(), Arc::new(files));

    let result = svc.import_auto_from_zip(&zip, "default", false);

    assert!(result.is_err(), "the subfolder cannot be made, so the import fails");
    let root = ws.path().join("collections").join("Orders");
    assert!(root.is_dir(), "create made the root before the failure");
    assert_eq!(svc.created_roots(), vec![root]);
}

#[test]
fn created_roots_is_empty_when_create_refuses_the_folder() {
    let ws = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(ws.path().join("collections")).expect("dir");
    // A plain file where the folder must go: `list()` does not see it, `create` refuses it.
    std::fs::write(ws.path().join("collections").join("Orders"), b"a file").expect("blocker");
    let zips = TempDir::new().expect("tempdir");
    let zip = zip_with_subfolder(zips.path(), "Orders");
    let svc = service(ws.path(), Arc::new(FsImportFiles));

    assert!(svc.import_auto_from_zip(&zip, "default", false).is_err());
    assert!(svc.created_roots().is_empty(), "nothing was made, so nothing may be removed");
    let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
    assert!(repo.list().expect("list").is_empty());
}
```

If `ImportFiles` has gained or lost a method since this was written, the compiler names it: delegate it to `FsImportFiles` the same way. If `create_dir_all` is not the call that fails first (the build reports the import as `Ok`), read `copy_collection_files` in `importer.rs` and fail the first write call it makes for a path inside `collections/` instead (`write` or `write_stream`); keep the assertions.

- [ ] **Step 4: Run the `created_roots` tests and see them fail**

Run: `cargo test -j4 -p rocket-import --test created_roots_test`
Expected: compiles; `created_roots_names_the_folder_a_successful_import_made` and `created_roots_is_filled_when_the_import_fails_after_the_folder_was_made` FAIL (the stub returns an empty list); `created_roots_is_empty_when_create_refuses_the_folder` PASSES.

- [ ] **Step 5: Record the created roots (GREEN)**

In `crates/rocket-import/src/importer.rs`:

(a) Add `use std::sync::Mutex;` to the imports (merge it into an existing `std::sync` import if there is one) and a field to `pub struct ImportService`, after `files`:

```rust
    /// The collection roots this service created, in order. See `created_roots`.
    created_roots: Mutex<Vec<PathBuf>>,
```

and `created_roots: Mutex::new(Vec::new()),` in every `Self { .. }` literal of the impl (in `new` and in the test-only `new_with_workspace_path`; the compiler lists any other).

(b) Replace the stub with the real method and add the helper next to it:

```rust
    /// The collection roots (`<workspace>/collections/<name>`) this service created, in order.
    /// A root is recorded right after `CollectionRepository::create` succeeded and before any
    /// request file is written, so an import that returns `Err` still lists the folder it made.
    /// `create` refuses an existing folder, so every recorded root is one this service made.
    pub fn created_roots(&self) -> Vec<PathBuf> {
        match self.created_roots.lock() {
            Ok(roots) => roots.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Creates a collection through the repository and records its root.
    fn create_collection_root(&self, name: &str) -> ImportResult<()> {
        self.collection_repo
            .create(name)
            .map_err(ImportError::DomainError)?;
        let root = self.workspace_path.join("collections").join(name);
        match self.created_roots.lock() {
            Ok(mut roots) => roots.push(root),
            Err(poisoned) => poisoned.into_inner().push(root),
        }
        Ok(())
    }
```

(c) At each of the five create sites (see "Facts" above), replace

```rust
        self.collection_repo
            .create(&<name>)
            .map_err(ImportError::DomainError)?;
```

with `self.create_collection_root(&<name>)?;`, keeping the `report.created_collections.push(..)` line after it. `<name>` is `resolved_name` in the legacy and modern imports and `col_name` in the Postman, WSDL and OpenAPI imports. Check with `grep -n "collection_repo\s*$\|\.create(&" crates/rocket-import/src/importer.rs` that no other `collection_repo.create` call is left in production code.

- [ ] **Step 6: Run the `created_roots` tests and the importer tests**

Run: `cargo test -j4 -p rocket-import --test created_roots_test`
Expected: 3 tests PASS.

Run: `cargo test -j4 -p rocket-import importer`
Expected: PASS, 0 failed (the existing importer tests are unchanged by the helper).

- [ ] **Step 7: Add the dependencies and write the adapter tests (RED)**

In `src-tauri/Cargo.toml`, at the end of `[dependencies]` (before the `[target.'cfg(target_os = "linux")'.dependencies]` table), add:

```toml
# The private per-process folder for the team network import copies.
tempfile = "3"
```

and under `[dev-dependencies]` after `tempfile = "3"`, add:

```toml
# Builds the test ZIPs for the transfer import adapter. Same version as rocket-import.
zip = "2"
```

In `src-tauri/src/import_wiring.rs`, add the adapter skeleton first so the tests compile and fail. After `impl ImportServiceFactory { .. }` and before `#[cfg(test)] mod tests`, add:

```rust
/// The workspace id handed to `import_auto_from_zip`. The importer ignores it for a
/// collection ZIP: the target workspace is the folder of `ImportServiceFactory`.
const IMPORT_WORKSPACE_ID: &str = "default";

/// The file a live adapter keeps locked inside its private folder.
const LOCK_FILE: &str = "lock";

/// This process's private folder for temp ZIP copies, with its lock held open.
struct ProcessDir {
    path: PathBuf,
    _lock: std::fs::File,
}

/// Imports a staged ZIP from another node into the active workspace.
pub struct ImporterAdapter {
    factory: ImportServiceFactory,
    import_lock: Mutex<()>,
    temp_root: PathBuf,
    process_dir: Mutex<Option<ProcessDir>>,
    started: SystemTime,
}

impl ImporterAdapter {
    /// `active_workspace_path` is the shared path the other import commands use. `temp_root`
    /// is `<network dir>/import-tmp`. Touches no disk; the private folder is made on first use.
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>, temp_root: PathBuf) -> Self {
        Self {
            factory: ImportServiceFactory::new(active_workspace_path),
            import_lock: Mutex::new(()),
            temp_root,
            process_dir: Mutex::new(None),
            started: SystemTime::now(),
        }
    }

    /// Test seam: pretends the adapter started at `started`.
    #[cfg(test)]
    fn with_started(mut self, started: SystemTime) -> Self {
        self.started = started;
        self
    }

    /// Stub, replaced in the GREEN step.
    pub fn purge_temp(&self) -> DomainResult<()> {
        Err(DomainError::Internal("not implemented".into()))
    }

    /// Stub, replaced in the GREEN step.
    fn process_dir(&self) -> DomainResult<PathBuf> {
        Err(DomainError::Internal("not implemented".into()))
    }

    /// Stub, replaced in the GREEN step.
    fn prepare_copy(&self, _zip_path: &Path, _display_name: &str) -> DomainResult<(CopyDir, PathBuf)> {
        Err(DomainError::Internal("not implemented".into()))
    }
}

/// A folder for one temp ZIP copy. It is removed when dropped, on every exit path.
struct CopyDir(PathBuf);

impl Drop for CopyDir {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.0) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, "could not remove a temp zip copy");
            }
        }
    }
}

/// Stub, replaced in the GREEN step.
fn remove_created_roots(_roots: &[PathBuf], _collections: &Path) {}

/// Stub, replaced in the GREEN step.
fn import_with(_service: &ImportService, _copy: &Path, _collections: &Path) -> DomainResult<ImportedCollection> {
    Err(DomainError::Internal("not implemented".into()))
}

impl CollectionImportPort for ImporterAdapter {
    fn existing_names(&self) -> DomainResult<Vec<String>> {
        Err(DomainError::Internal("not implemented".into()))
    }

    fn import_zip(&self, _zip_path: &Path, _display_name: &str) -> DomainResult<ImportedCollection> {
        Err(DomainError::Internal("not implemented".into()))
    }
}
```

and extend the imports at the top of the file:

```rust
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use rocket_app::network::{CollectionImportPort, ImportedCollection, UNSAFE_ZIP_MESSAGE};
use rocket_collection::{Collection, CollectionRepository};
use rocket_environment::EnvironmentRepository;
use rocket_environment::EnvironmentRepositoryFactory;
use rocket_import::transfer_zip::{self, is_hidden_format_char, is_reserved_device_name};
use rocket_import::{ImportError, ImportService};
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo, FsImportFiles};
use rocket_shared::error::{DomainError, DomainResult};
```

(replace the existing `use std::path::PathBuf;`, `use std::sync::{Arc, Mutex};` and `use rocket_import::ImportService;` lines; keep the rest.)

Inside the existing `#[cfg(test)] mod tests`, after the last existing test, add the helpers and tests. `write_zip` stays in this module: Plan 4 reuses it.

```rust
    // ---- ImporterAdapter ----

    use std::io::{self, Read, Write};
    use std::time::Duration;

    use rocket_app::effective_capabilities;
    use rocket_collection::settings::SandboxMode;
    use rocket_collection::CollectionTrustStore;
    use rocket_import::{DirEntryInfo, EntryKind, ImportFiles, ReadSeek, WorkDir};
    use rocket_infra::FsCollectionTrustStore;
    use zip::write::SimpleFileOptions;

    const HOUR: Duration = Duration::from_secs(3600);

    fn shared(ws: &Path) -> Arc<Mutex<PathBuf>> {
        Arc::new(Mutex::new(ws.to_path_buf()))
    }

    fn adapter() -> (tempfile::TempDir, ImporterAdapter) {
        let ws = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(ws.path().join("collections")).expect("collections dir");
        // The temp copies go inside the test workspace, outside `collections`.
        let adapter = ImporterAdapter::new(shared(ws.path()), ws.path().join("transfer-tmp"));
        (ws, adapter)
    }

    /// The ZIP copies left in any per-process folder under the temp root. The `lock` file and
    /// the empty process folders do not count.
    fn temp_leftovers(ws: &Path) -> Vec<String> {
        let Ok(dirs) = std::fs::read_dir(ws.join("transfer-tmp")) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for dir in dirs.filter_map(|e| e.ok()) {
            let Ok(entries) = std::fs::read_dir(dir.path()) else {
                continue;
            };
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if name != LOCK_FILE {
                    out.push(name);
                }
            }
        }
        out
    }

    /// Runs an import that passes the safety checks and still fails inside the importer: a
    /// plain FILE called `Blocked` sits where the collection folder must go, and `list()` does
    /// not show it, so the importer picks the name `Blocked` and `create` refuses it.
    fn failing_import(ws: &Path, adapter: &ImporterAdapter) -> DomainError {
        std::fs::write(ws.join("collections").join("Blocked"), b"a file, not a folder")
            .expect("blocker");
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "a.zip");
        adapter
            .import_zip(&zip, "Blocked")
            .expect_err("the import must fail")
    }

    fn write_zip(path: &Path, entries: &[(&str, &str)]) {
        let file = std::fs::File::create(path).expect("create zip");
        let mut writer = zip::ZipWriter::new(file);
        for (name, text) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .expect("start file");
            writer.write_all(text.as_bytes()).expect("write entry");
        }
        writer.finish().expect("finish zip");
    }

    /// A good wrapped collection whose `info.name` differs from any display name.
    fn good_zip(dir: &Path, file: &str) -> PathBuf {
        let path = dir.join(file);
        write_zip(
            &path,
            &[
                (
                    "Inner Name/opencollection.yml",
                    "opencollection: \"1.0.0\"\ninfo:\n  name: Inner Name\n",
                ),
                (
                    "Inner Name/get.yml",
                    "info:\n  name: get\nhttp:\n  method: GET\n  url: https://example.test\n",
                ),
            ],
        );
        path
    }

    fn collection_dirs(ws: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(ws.join("collections"))
            .expect("read collections")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    fn unsafe_message(err: &DomainError) -> bool {
        matches!(err, DomainError::InvalidInput(text) if text == UNSAFE_ZIP_MESSAGE)
    }

    /// The real port, except that making a folder inside `collections/` fails. Before it fails
    /// it creates the collection `Other`, as another import command or a git pull might in the
    /// middle of this import.
    struct ConcurrentCreatorFiles {
        collections: PathBuf,
    }

    impl ImportFiles for ConcurrentCreatorFiles {
        fn list_dir(&self, dir: &Path) -> io::Result<Vec<DirEntryInfo>> {
            FsImportFiles.list_dir(dir)
        }
        fn kind(&self, path: &Path) -> io::Result<EntryKind> {
            FsImportFiles.kind(path)
        }
        fn create_dir_all(&self, dir: &Path) -> io::Result<()> {
            if dir.starts_with(&self.collections) {
                FsCollectionRepo::new_standalone(self.collections.clone())
                    .create("Other")
                    .map_err(|e| io::Error::other(e.to_string()))?;
                return Err(io::Error::other("disk full"));
            }
            FsImportFiles.create_dir_all(dir)
        }
        fn copy(&self, src: &Path, dest: &Path) -> io::Result<()> {
            FsImportFiles.copy(src, dest)
        }
        fn read_to_string(&self, path: &Path) -> io::Result<String> {
            FsImportFiles.read_to_string(path)
        }
        fn write(&self, path: &Path, text: &str) -> io::Result<()> {
            FsImportFiles.write(path, text)
        }
        fn regular_file_len(&self, path: &Path) -> io::Result<Option<u64>> {
            FsImportFiles.regular_file_len(path)
        }
        fn read_up_to(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
            FsImportFiles.read_up_to(path, max_bytes)
        }
        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            FsImportFiles.canonicalize(path)
        }
        fn open_read(&self, path: &Path) -> io::Result<Box<dyn ReadSeek>> {
            FsImportFiles.open_read(path)
        }
        fn temp_dir(&self) -> io::Result<Box<dyn WorkDir>> {
            FsImportFiles.temp_dir()
        }
        fn write_stream(&self, path: &Path, from: &mut dyn Read) -> io::Result<u64> {
            FsImportFiles.write_stream(path, from)
        }
    }

    #[test]
    fn existing_names_lists_the_collections_of_the_active_workspace() {
        let (ws, adapter) = adapter();
        assert!(adapter.existing_names().expect("names").is_empty());
        FsCollectionRepo::new_standalone(ws.path().join("collections"))
            .create("Billing")
            .expect("create");
        assert_eq!(adapter.existing_names().expect("names"), vec!["Billing".to_string()]);
    }

    #[test]
    fn existing_names_follow_a_workspace_switch() {
        let (ws_a, _) = adapter();
        let ws_b = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(ws_b.path().join("collections")).expect("dir");
        FsCollectionRepo::new_standalone(ws_b.path().join("collections"))
            .create("OnlyInB")
            .expect("create");
        let shared = shared(ws_a.path());
        let adapter = ImporterAdapter::new(Arc::clone(&shared), ws_a.path().join("transfer-tmp"));
        assert!(adapter.existing_names().expect("names").is_empty());

        *shared.lock().expect("lock") = ws_b.path().to_path_buf();

        assert_eq!(adapter.existing_names().expect("names"), vec!["OnlyInB".to_string()]);
    }

    #[test]
    fn import_zip_names_the_collection_from_the_display_name() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "whatever-the-staged-file-is-called.zip");

        let imported = adapter.import_zip(&zip, "Orders (from Ana)").expect("import");

        assert_eq!(imported.name, "Orders (from Ana)");
        assert_eq!(collection_dirs(ws.path()), vec!["Orders (from Ana)".to_string()]);
        assert!(ws.path().join("collections/Orders (from Ana)/get.yml").exists());
        assert!(zip.exists(), "the staged file is the caller's to delete");
    }

    #[test]
    fn import_zip_adds_a_suffix_on_a_clash_and_never_overwrites() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "a.zip");
        adapter.import_zip(&zip, "Orders").expect("first");
        let marker = ws.path().join("collections/Orders/keep.txt");
        std::fs::write(&marker, "mine").expect("marker");

        let second = adapter.import_zip(&zip, "Orders").expect("second");

        assert_eq!(second.name, "Orders-1");
        assert_eq!(std::fs::read_to_string(&marker).expect("marker"), "mine");
    }

    #[test]
    fn import_zip_keeps_a_non_ascii_name() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "a.zip");

        let imported = adapter.import_zip(&zip, "Café – Ünï 日本語").expect("import");

        assert_eq!(imported.name, "Café – Ünï 日本語");
        assert!(ws.path().join("collections/Café – Ünï 日本語/opencollection.yml").exists());
        assert!(adapter
            .existing_names()
            .expect("names")
            .contains(&"Café – Ünï 日本語".to_string()));
    }

    #[test]
    fn import_zip_imports_a_flat_root_zip() {
        let (_ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = zips.path().join("flat.zip");
        write_zip(
            &zip,
            &[("opencollection.yml", "opencollection: \"1.0.0\"\ninfo:\n  name: x\n")],
        );
        let imported = adapter.import_zip(&zip, "Flat").expect("import");
        assert_eq!(imported.name, "Flat");
    }

    #[test]
    fn import_zip_refuses_a_single_file_zip_without_a_marker() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = zips.path().join("notes.zip");
        write_zip(&zip, &[("notes.txt", "hello")]);

        let err = adapter.import_zip(&zip, "Notes").expect_err("refused");

        assert!(unsafe_message(&err), "error was {err:?}");
        assert!(collection_dirs(ws.path()).is_empty());
    }

    #[test]
    fn import_zip_refuses_an_unsafe_zip_and_writes_nothing() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let slip = zips.path().join("slip.zip");
        write_zip(
            &slip,
            &[
                ("c/opencollection.yml", "info:\n  name: x\n"),
                ("../../evil.txt", "pwned"),
            ],
        );
        let not_a_zip = zips.path().join("junk.zip");
        std::fs::write(&not_a_zip, b"junk").expect("junk");
        let workspace_zip = zips.path().join("ws.zip");
        write_zip(
            &workspace_zip,
            &[
                ("w/workspace.yml", "opencollection: \"1.0.0\"\n"),
                ("w/opencollection.yml", "info:\n  name: x\n"),
            ],
        );

        for bad in [&slip, &not_a_zip, &workspace_zip] {
            let err = adapter.import_zip(bad, "Evil").expect_err("refused");
            assert!(unsafe_message(&err), "error for {bad:?} was {err:?}");
        }

        assert!(collection_dirs(ws.path()).is_empty());
        assert!(!ws.path().join("evil.txt").exists());
        assert!(!ws.path().parent().expect("parent").join("evil.txt").exists());
    }

    #[test]
    fn import_zip_refuses_names_that_are_not_a_plain_folder_name() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "a.zip");
        let bad_names = [
            "a/b",
            "a\\b",
            "..",
            ".hidden",
            "",
            "   ",
            "x\0y",
            "Orders.",
            "Orders ",
            " Orders",
            "a:b",
            "tab\there",
            "CON",
            "nul.txt",
            "a\u{202E}b",
            "a\u{200B}b",
        ];
        for name in bad_names {
            let err = adapter.import_zip(&zip, name).expect_err("refused");
            assert!(
                matches!(err, DomainError::InvalidInput(_)),
                "name {name:?} gave {err:?}"
            );
        }
        assert!(collection_dirs(ws.path()).is_empty());
    }

    #[test]
    fn two_imports_of_the_same_name_make_two_collections() {
        let (ws, adapter) = adapter();
        let adapter = Arc::new(adapter);
        let zips = tempfile::TempDir::new().expect("tempdir");
        let first = good_zip(zips.path(), "one.zip");
        let second = good_zip(zips.path(), "two.zip");

        let handles: Vec<_> = [first, second]
            .into_iter()
            .map(|zip| {
                let adapter = Arc::clone(&adapter);
                std::thread::spawn(move || adapter.import_zip(&zip, "Race"))
            })
            .collect();
        let mut names: Vec<String> = handles
            .into_iter()
            .map(|h| h.join().expect("thread").expect("import").name)
            .collect();
        names.sort();

        assert_eq!(names, vec!["Race".to_string(), "Race-1".to_string()]);
        assert_eq!(
            collection_dirs(ws.path()),
            vec!["Race".to_string(), "Race-1".to_string()]
        );
    }

    #[test]
    fn remove_created_roots_removes_only_the_listed_folders_inside_collections() {
        let (ws, _adapter) = adapter();
        let collections = ws.path().join("collections");
        let repo = FsCollectionRepo::new_standalone(collections.clone());
        repo.create("Made by the import").expect("listed");
        repo.create("Someone else's").expect("not listed");
        // A listed path outside `collections/` is never removed, whatever the list says.
        let outside = ws.path().join("outside");
        std::fs::create_dir_all(&outside).expect("outside dir");
        let missing = collections.join("Already gone");

        remove_created_roots(
            &[collections.join("Made by the import"), outside.clone(), missing],
            &collections,
        );

        assert_eq!(collection_dirs(ws.path()), vec!["Someone else's".to_string()]);
        assert!(outside.is_dir());
    }

    #[test]
    fn a_collection_created_during_a_failed_import_survives() {
        let (ws, adapter) = adapter();
        let collections = ws.path().join("collections");
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = zips.path().join("a.zip");
        write_zip(
            &zip,
            &[
                ("c/opencollection.yml", "opencollection: \"1.0.0\"\ninfo:\n  name: c\n"),
                ("c/sub/get.yml", "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x.test\n"),
            ],
        );
        let (_copy_dir, copy) = adapter.prepare_copy(&zip, "Pushed").expect("copy");
        let service = ImportService::new(
            ws.path().to_path_buf(),
            Box::new(FsCollectionRepo::new_standalone(collections.clone())),
            Box::new(FsEnvFactory(ws.path().to_path_buf())),
            Arc::new(ConcurrentCreatorFiles { collections: collections.clone() }),
        );

        let err = import_with(&service, &copy, &collections).expect_err("the import fails");

        assert!(
            matches!(&err, DomainError::Internal(text) if text.starts_with("The collection could not be imported")),
            "error was {err:?}"
        );
        assert_eq!(service.created_roots(), vec![collections.join("Pushed")]);
        // The import's own folder goes; the collection made during the import stays.
        assert_eq!(collection_dirs(ws.path()), vec!["Other".to_string()]);
    }

    #[test]
    fn a_failed_import_returns_an_import_error_and_leaves_no_new_collection() {
        let (ws, adapter) = adapter();
        let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
        repo.create("Keep").expect("older collection");

        let err = failing_import(ws.path(), &adapter);

        assert!(
            matches!(&err, DomainError::Internal(text) if text.starts_with("The collection could not be imported")),
            "error was {err:?}"
        );
        assert_eq!(adapter.existing_names().expect("names"), vec!["Keep".to_string()]);
        // The blocker file is the test's own and is not a collection.
        assert!(ws.path().join("collections/Blocked").is_file());
    }

    #[test]
    fn the_unsafe_message_is_used_only_for_validator_refusals() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let good = good_zip(zips.path(), "a.zip");

        // A bad name, a failed import and a missing staged file are not "unsafe".
        let bad_name = adapter.import_zip(&good, "a/b").expect_err("bad name");
        assert!(!unsafe_message(&bad_name), "got {bad_name:?}");
        assert!(matches!(bad_name, DomainError::InvalidInput(_)));
        let failed = failing_import(ws.path(), &adapter);
        assert!(!unsafe_message(&failed), "got {failed:?}");
        let missing = adapter
            .import_zip(&zips.path().join("missing.zip"), "Orders")
            .expect_err("missing file");
        assert!(!unsafe_message(&missing), "got {missing:?}");

        // A validator refusal is.
        let junk = zips.path().join("junk.zip");
        std::fs::write(&junk, b"junk").expect("junk");
        let refused = adapter.import_zip(&junk, "Orders").expect_err("refused");
        assert!(unsafe_message(&refused), "got {refused:?}");
    }

    #[test]
    fn a_missing_staged_file_is_an_io_error_not_the_unsafe_message() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");

        let err = adapter
            .import_zip(&zips.path().join("missing.zip"), "Orders")
            .expect_err("missing file");

        assert!(matches!(err, DomainError::Io(_)), "error was {err:?}");
        assert!(collection_dirs(ws.path()).is_empty());
    }

    #[test]
    fn the_validated_copy_is_the_file_that_is_imported() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "staged.zip");

        let (copy_dir, copy) = adapter.prepare_copy(&zip, "Pushed").expect("copy");
        let private = adapter.process_dir().expect("process dir");
        assert!(copy.starts_with(&private), "{copy:?} is not in {private:?}");
        assert_eq!(copy.file_name().expect("name"), "Pushed.zip");

        // Spoiling the staged original after the check changes nothing: the importer reads the
        // copy that was validated.
        std::fs::write(&zip, b"junk").expect("spoil the original");
        let base = ws.path().to_path_buf();
        let service = adapter.factory.service_for(base.clone());
        let imported = import_with(&service, &copy, &base.join("collections")).expect("import");
        assert_eq!(imported.name, "Pushed");

        drop(copy_dir);
        assert!(temp_leftovers(ws.path()).is_empty());

        // And a copy of an unsafe ZIP is refused and removed.
        let err = adapter.prepare_copy(&zip, "Junk").err().expect("refused");
        assert!(unsafe_message(&err), "got {err:?}");
        assert!(temp_leftovers(ws.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn the_private_temp_folders_are_mode_0700() {
        use std::os::unix::fs::PermissionsExt;

        let (ws, adapter) = adapter();
        let private = adapter.process_dir().expect("process dir");
        for dir in [ws.path().join("transfer-tmp"), private] {
            let mode = std::fs::metadata(&dir).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{dir:?} has mode {mode:o}");
        }
    }

    #[test]
    fn the_temp_copy_is_removed_on_success_and_on_failure() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "a.zip");

        adapter.import_zip(&zip, "Orders").expect("import");
        assert!(temp_leftovers(ws.path()).is_empty(), "left after success");

        let _ = failing_import(ws.path(), &adapter);
        assert!(temp_leftovers(ws.path()).is_empty(), "left after failure");

        let junk = zips.path().join("junk.zip");
        std::fs::write(&junk, b"junk").expect("junk");
        let _ = adapter.import_zip(&junk, "Orders");
        assert!(temp_leftovers(ws.path()).is_empty(), "left after a refusal");
    }

    #[test]
    fn purge_temp_is_fine_with_a_missing_root() {
        let (_ws, adapter) = adapter();
        adapter.purge_temp().expect("a missing folder is not an error");
    }

    #[test]
    fn purge_temp_keeps_the_folder_of_a_live_adapter() {
        let (ws, first) = adapter();
        let live = first.process_dir().expect("process dir");
        std::fs::write(live.join("in-flight.zip"), b"zip").expect("in-flight copy");
        // A second instance on the same root that started long after the first.
        let second = ImporterAdapter::new(shared(ws.path()), ws.path().join("transfer-tmp"))
            .with_started(SystemTime::now() + HOUR);

        second.purge_temp().expect("purge");
        first.purge_temp().expect("purge by the owner");

        assert!(live.join("in-flight.zip").exists(), "a live instance's copy was deleted");
        let zips = tempfile::TempDir::new().expect("tempdir");
        first.import_zip(&good_zip(zips.path(), "a.zip"), "Orders").expect("still imports");
    }

    #[test]
    fn purge_temp_removes_a_folder_whose_lock_is_free() {
        let (ws, adapter) = adapter();
        // What a process that exited leaves behind: its lock file is no longer held.
        let stale = ws.path().join("transfer-tmp/p4242-old");
        std::fs::create_dir_all(&stale).expect("stale dir");
        std::fs::write(stale.join(LOCK_FILE), b"").expect("lock file");
        std::fs::write(stale.join("Orders.zip"), b"zip").expect("stale copy");

        adapter.purge_temp().expect("purge");

        assert!(!stale.exists());
    }

    #[test]
    fn purge_temp_removes_an_old_folder_without_a_lock_and_keeps_a_young_one() {
        let (ws, _adapter) = adapter();
        let lockless = ws.path().join("transfer-tmp/p4243-nolock");
        std::fs::create_dir_all(&lockless).expect("dir");

        // Started before the folder was made: the folder may belong to a live instance that has
        // not written its lock yet, so it stays.
        ImporterAdapter::new(shared(ws.path()), ws.path().join("transfer-tmp"))
            .with_started(SystemTime::now() - HOUR)
            .purge_temp()
            .expect("purge");
        assert!(lockless.exists(), "a young folder without a lock must stay");

        // Started after the folder was made: it is a leftover.
        ImporterAdapter::new(shared(ws.path()), ws.path().join("transfer-tmp"))
            .with_started(SystemTime::now() + HOUR)
            .purge_temp()
            .expect("purge");
        assert!(!lockless.exists(), "an old folder without a lock must go");
    }

    #[test]
    fn an_imported_collection_has_no_trust_record_and_runs_only_in_safe_mode() {
        // No script gate exists (second-review R1): the scripts of a received collection run in
        // the Safe sandbox. What the trust gate denies is Developer mode, extra script folders,
        // agent runs, host env and vault access, until the user allows them.
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        // A pushed collection that asks for developer mode with the real key.
        let zip = zips.path().join("a.zip");
        write_zip(
            &zip,
            &[
                (
                    "c/opencollection.yml",
                    "opencollection: \"1.0.0\"\ninfo:\n  name: c\nextensions:\n  rocketapi:\n    sandboxMode: developer\n",
                ),
                ("c/get.yml", "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x.test\n"),
            ],
        );

        let imported = adapter.import_zip(&zip, "Pushed").expect("import");

        let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
        // The request reached the stored settings, so the Safe check below can fail.
        let stored = repo.get_settings(&imported.name).expect("settings");
        assert_eq!(
            stored.sandbox_mode,
            SandboxMode::Developer,
            "the fixture must store developer mode, or the Safe assertion proves nothing"
        );

        let store = FsCollectionTrustStore::new(ws.path().join("trust.yml"));
        let identity = repo.collection_identity(&imported.name).expect("identity");
        assert_eq!(store.grant_for(&identity).expect("read"), None, "no trust record");

        let effective = effective_capabilities(&repo, &store, &imported.name);
        assert!(!effective.vault_values && !effective.vault_certificates);
        assert!(!effective.process_env);
        assert!(!effective.agent_run);
        assert!(effective.context_roots.is_empty());
        assert_eq!(
            effective.sandbox_mode,
            SandboxMode::Safe,
            "the requested developer mode stays denied until the user trusts the collection"
        );
    }
```

If `rocket_collection::settings::SandboxMode` is not the public path, use the same import as `crates/rocket-app/src/collection_service.rs`. If the `stored.sandbox_mode` assertion fails with `Safe`, the importer rewrote `opencollection.yml` and dropped the `extensions` block. Then do not delete the effective `Safe` assertion: replace the stored-settings assertion with one that reads `collections/Pushed/opencollection.yml` and asserts it has no `sandboxMode: developer`, and record a `Ruling:` line in the ledger that the importer strips the request. Never use a `config:` key for the mode: it is not read anywhere.

- [ ] **Step 8: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket import_wiring`
Expected: compiles (the skeleton holds the stubs `purge_temp`, `process_dir`, `prepare_copy`, `remove_created_roots`, `import_with`, `existing_names` and `import_zip`); all 23 new adapter tests FAIL (`not implemented` from a stub, or an assertion for the stubbed cleanup; `purge_temp_is_fine_with_a_missing_root` fails because the stub returns an error); on Windows there are 22 (the mode test is `cfg(unix)`). The 3 older `import_wiring` tests still PASS.

- [ ] **Step 9: Implement the adapter (GREEN)**

In `src-tauri/src/import_wiring.rs`, replace the stubs (`purge_temp`, `process_dir`, `prepare_copy` in `impl ImporterAdapter`, the free stubs `remove_created_roots` and `import_with`, and the skeleton `impl CollectionImportPort for ImporterAdapter { .. }`) with the code below. The struct, `ProcessDir`, `CopyDir`, the constants, `new` and `with_started` stay.

```rust
/// True for a name that is one plain folder name on every platform. The receiver builds the
/// name with `receiver_display_name`, but the adapter does not trust its caller.
fn is_plain_folder_name(name: &str) -> bool {
    !name.is_empty()
        && name == name.trim()
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !is_reserved_device_name(name)
        && !name.chars().any(|c| {
            c.is_control()
                || is_hidden_format_char(c)
                || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

/// Removes the collection folders one failed import made, as `ImportService::created_roots`
/// reports them. Nothing else is touched, so a collection that anything else created in the
/// meantime survives. A root that is not directly inside `collections` is skipped.
fn remove_created_roots(roots: &[PathBuf], collections: &Path) {
    for root in roots {
        if root.parent() != Some(collections) {
            tracing::warn!("a reported collection root outside the collections folder was kept");
            continue;
        }
        match std::fs::remove_dir_all(root) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(error = %e, "could not remove a half-imported collection"),
        }
    }
}

/// Imports the validated copy with `service` and cleans up after a failure.
fn import_with(
    service: &ImportService,
    copy: &Path,
    collections: &Path,
) -> DomainResult<ImportedCollection> {
    let report = match service.import_auto_from_zip(copy, IMPORT_WORKSPACE_ID, false) {
        Ok(report) => report,
        Err(e) => {
            remove_created_roots(&service.created_roots(), collections);
            return Err(DomainError::Internal(format!(
                "The collection could not be imported: {e}"
            )));
        }
    };
    match report.created_collections.into_iter().next() {
        Some(name) => Ok(ImportedCollection { name }),
        None => {
            remove_created_roots(&service.created_roots(), collections);
            Err(DomainError::Internal("The import created no collection.".into()))
        }
    }
}

/// Maps a refusal of the safety checks. Only a ZIP the validator refuses gets
/// `UNSAFE_ZIP_MESSAGE` (Ruling 5). A file that cannot be read at all is an I/O error. The
/// reason goes to the log without the payload.
fn map_validation_error(e: ImportError) -> DomainError {
    match e {
        ImportError::IoError(io) => {
            tracing::warn!(error = %io, "a staged zip could not be read");
            DomainError::Io("The staged collection file could not be read.".into())
        }
        other => {
            tracing::warn!(reason = %other, "a transferred zip was rejected");
            DomainError::InvalidInput(UNSAFE_ZIP_MESSAGE.into())
        }
    }
}

/// Only the owner may enter the folder. Other platforms keep their default, which is per user.
#[cfg(unix)]
fn make_private(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn make_private(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Numbers the per-import copy folders inside this process's private folder.
static COPY_SEQ: AtomicU64 = AtomicU64::new(0);

impl ImporterAdapter {
    fn repo(&self) -> DomainResult<FsCollectionRepo> {
        let base = self.factory.workspace_path().map_err(DomainError::Internal)?;
        Ok(FsCollectionRepo::new_standalone(base.join("collections")))
    }

    /// This process's private folder, made on first use: a random name under `temp_root`
    /// (mode 0700 on unix) with a `lock` file this adapter keeps locked while it lives.
    fn process_dir(&self) -> DomainResult<PathBuf> {
        let mut slot = self
            .process_dir
            .lock()
            .map_err(|_| DomainError::Internal("The temp folder lock is poisoned.".into()))?;
        if let Some(dir) = slot.as_ref() {
            return Ok(dir.path.clone());
        }
        std::fs::create_dir_all(&self.temp_root)?;
        make_private(&self.temp_root)?;
        let path = tempfile::Builder::new()
            .prefix(&format!("p{}-", std::process::id()))
            .tempdir_in(&self.temp_root)?
            .keep();
        make_private(&path)?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join(LOCK_FILE))?;
        lock.try_lock()
            .map_err(|e| DomainError::Io(format!("The temp folder could not be locked: {e}")))?;
        *slot = Some(ProcessDir {
            path: path.clone(),
            _lock: lock,
        });
        Ok(path)
    }

    fn new_copy_dir(&self) -> DomainResult<CopyDir> {
        let n = COPY_SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = self.process_dir()?.join(n.to_string());
        std::fs::create_dir(&dir)?;
        Ok(CopyDir(dir))
    }

    /// Copies the staged file into the private folder as `<display_name>.zip` (the importer
    /// names the collection from the stem) and runs the strict checks on that copy, which is
    /// the file the importer reads. The copy goes when the returned guard is dropped.
    fn prepare_copy(&self, zip_path: &Path, display_name: &str) -> DomainResult<(CopyDir, PathBuf)> {
        let copy_dir = self.new_copy_dir()?;
        let copy = copy_dir.0.join(format!("{display_name}.zip"));
        std::fs::copy(zip_path, &copy).map_err(|e| {
            tracing::warn!(error = %e, "a staged zip could not be copied");
            DomainError::Io("The staged collection file could not be read.".into())
        })?;
        let files = FsImportFiles;
        transfer_zip::validate(&files, &copy)
            .and_then(|()| transfer_zip::require_collection_root(&files, &copy))
            .map_err(map_validation_error)?;
        Ok((copy_dir, copy))
    }

    /// True when `dir` belongs to an instance that is gone: its `lock` can be locked, or it has
    /// no `lock` and is older than this adapter's start.
    fn is_stale(&self, dir: &Path) -> DomainResult<bool> {
        let opened = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.join(LOCK_FILE));
        match opened {
            Ok(file) => match file.try_lock() {
                Ok(()) => {
                    // Released before the folder is removed, which Windows needs.
                    drop(file);
                    Ok(true)
                }
                Err(std::fs::TryLockError::WouldBlock) => Ok(false),
                Err(std::fs::TryLockError::Error(e)) => Err(e.into()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let modified = std::fs::metadata(dir)?.modified()?;
                Ok(modified < self.started)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Removes the private folders of instances that are gone, for example after a crash.
    /// Never this adapter's own folder and never a live instance's. A missing root is fine.
    /// Call it at app start, before the first import.
    pub fn purge_temp(&self) -> DomainResult<()> {
        let own = match self.process_dir.lock() {
            Ok(slot) => slot.as_ref().map(|d| d.path.clone()),
            Err(_) => return Err(DomainError::Internal("The temp folder lock is poisoned.".into())),
        };
        let entries = match std::fs::read_dir(&self.temp_root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if own.as_ref() == Some(&path) || !entry.file_type()?.is_dir() {
                continue;
            }
            if self.is_stale(&path)? {
                match std::fs::remove_dir_all(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => tracing::warn!(error = %e, "could not remove a stale temp folder"),
                }
            }
        }
        Ok(())
    }
}

impl CollectionImportPort for ImporterAdapter {
    fn existing_names(&self) -> DomainResult<Vec<String>> {
        Ok(self.repo()?.list()?.into_iter().map(|s| s.name).collect())
    }

    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection> {
        if !is_plain_folder_name(display_name) || Collection::validate_name(display_name).is_err()
        {
            return Err(DomainError::InvalidInput(
                "The collection name is not valid.".into(),
            ));
        }
        // The guard removes the copy on every exit path below.
        let (_copy_dir, copy) = self.prepare_copy(zip_path, display_name)?;

        // One import at a time: the importer resolves a free name and then creates it, which
        // is not atomic. Two imports of the same name would otherwise race.
        let _guard = self
            .import_lock
            .lock()
            .map_err(|_| DomainError::Internal("The import lock is poisoned.".into()))?;
        // One read of the workspace path, so the service and the cleanup agree after a switch.
        let base = self.factory.workspace_path().map_err(DomainError::Internal)?;
        let service = self.factory.service_for(base.clone());
        import_with(&service, &copy, &base.join("collections"))
    }
}
```

Notes for the implementer: `DomainError` has `From<std::io::Error>`, so the `?` on `std::fs` and `tempfile` calls converts. `File::try_lock` and `TryLockError` are std since Rust 1.89. The text of a failed import is the importer's own message and it reaches the sender as `Finished.message` (Plan 3); it carries no entry names because the importer reports file names only for skipped items, not in its error. If the build reports an unused import (`CollectionRepository` is needed for `list()`), remove only the unused one. The ZIP file stays where the caller staged it: the caller releases it. Plan 4 wraps `purge_temp` and `TransferStaging::purge` in its own startup helper; do not add one here.

- [ ] **Step 10: Fallback, only if the probe in Step 2 failed**

If the importer named the collection from `info.name` or the wrapper folder and not from the ZIP stem, replace the final `match report.created_collections.into_iter().next() { .. }` block of `import_with` with this (call `remove_created_roots(&service.created_roots(), collections)` before returning an error from `rename` too; the created collection is renamed through the repository's `rename`, which also moves a trust grant, though a fresh import has none; `import_with` then also needs the repo, built as `FsCollectionRepo::new_standalone(collections.to_path_buf())`):

```rust
        let created = report
            .created_collections
            .into_iter()
            .next()
            .ok_or_else(|| DomainError::Internal("The import created no collection.".into()))?;
        if created != display_name && !created.starts_with(&format!("{display_name}-")) {
            let names: Vec<String> = repo.list()?.into_iter().map(|s| s.name).collect();
            let mut target = display_name.to_string();
            let mut n = 1u32;
            while names.contains(&target) {
                target = format!("{display_name}-{n}");
                n += 1;
            }
            repo.rename(&created, &target)?;
            return Ok(ImportedCollection { name: target });
        }
        Ok(ImportedCollection { name: created })
```

(`display_name` is the copy's file stem: `copy.file_stem()`.) Update the probe test `the_zip_stem_names_the_collection_not_opencollection_yml` to document the observed behaviour instead of asserting the stem. Skip this step when the probe passed.

- [ ] **Step 11: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket import_wiring`
Expected: the 3 existing tests and the 23 new adapter tests PASS (26 in total; 25 on Windows).

Run: `cargo test -j4 -p rocket-import --test transfer_name_probe_test`
Expected: 6 tests PASS.

Run: `cargo test -j4 -p rocket-import --test created_roots_test`
Expected: 3 tests PASS.

- [ ] **Step 12: Check and lint**

Run: `cargo check -j4 -p rocket`
Expected: `Finished`, no errors. `ImporterAdapter` is not wired yet (Plan 4 does that), so a `dead_code` warning for it is expected; add `#[allow(dead_code)]` with the comment `// Wired in Plan 4.` on the struct only if the repo builds with `-D warnings`.

Run: `cargo clippy -j4 -p rocket --all-targets`
Expected: no new warnings from `import_wiring.rs` other than the expected `dead_code`.

Run: `cargo clippy -j4 -p rocket-import --all-targets`
Expected: no warnings from `importer.rs` or the two new test files.

- [ ] **Step 13: Whole-plan verification**

No commit; reviewed at the plan end. Run these one at a time:

Run: `cargo check -j4 -p rocket-network && cargo test -j4 -p rocket-network staging`
Expected: PASS.

Run: `cargo check -j4 -p rocket-import && cargo test -j4 -p rocket-import transfer_zip && cargo test -j4 -p rocket-import --test transfer_name_probe_test && cargo test -j4 -p rocket-import --test created_roots_test && cargo test -j4 -p rocket-import importer`
Expected: 32 `transfer_zip` unit tests, 6 probe tests, 3 `created_roots` tests and the existing importer tests PASS.

Run: `cargo check -j4 -p rocket-infra && cargo test -j4 -p rocket-infra network::staging`
Expected: 18 tests PASS (17 on Windows).

Run: `cargo check -j4 -p rocket-app && cargo test -j4 -p rocket-app transfer_ports`
Expected: 18 tests PASS.

Run: `cargo check -j4 -p rocket && cargo test -j4 -p rocket import_wiring`
Expected: 26 tests PASS (25 on Windows).

Run, one crate at a time: `cargo fmt -p rocket-network -- --check`, `cargo fmt -p rocket-import -- --check`, `cargo fmt -p rocket-infra -- --check`, `cargo fmt -p rocket-app -- --check`, `cargo fmt -p rocket -- --check`
Expected: no diff for each. If one prints a diff, run `cargo fmt -p <that crate>` and check it again.

---

## Self-review

Spec coverage:
- Section 2, decision 3 (new collection, never overwritten, suffix on clash): `receiver_display_name`, adapter suffix and race tests.
- Section 2, decision 6 and section 8 (a received collection has no trust record, so no Developer mode, extra script folders, agent runs, host env or vault access until the user allows them; its scripts DO run in the Safe sandbox, there is no script gate): `an_imported_collection_has_no_trust_record_and_runs_only_in_safe_mode`, whose fixture uses the real `extensions.rocketapi.sandboxMode: developer` key and first proves the stored setting is `Developer`.
- Section 6 (offer names) and section 8 (entry names: `:`, Cc and bidi/zero-width characters, trailing dot or space, Windows reserved names, case-folded duplicates): Task 1 validator tests and the shared helpers; Task 2 display-name mapping; Task 3 `is_plain_folder_name` (Ruling 12).
- Section 7 (the ZIP limits copied into `rocket-import`, pinned equal): Ruling 13 and `the_zip_limits_equal_the_protocol_limits`.
- Section 8, ZIP safety (absolute path, `..`, symlink, entry count, unzipped size counted by decompressing, lying declared size, existing extractor stays as second line): Task 1. The adapter's temp copy sits in a private 0700 per-process folder under the network dir, the validator runs on that copy, the copy is deleted on every exit path by a drop guard, and `purge_temp` removes only stale folders (Plan 4 calls it at start): Task 3, Ruling 10. The staged chunk file deleted on abort and at start: `discard`, `Drop` and `purge` in Task 2. Nothing about the payload logged: fixed texts and a test that the refusal text does not echo an entry name.
- Section 9, `rocket-import` (`validate`, `created_roots`), `rocket-infra`, `src-tauri` components (`CollectionImportPort`, display name rule, staging, adapter on `ImportServiceFactory`, name from ZIP stem): Tasks 1 to 3.
- Section 10, rows "Unsafe ZIP" (`UNSAFE_ZIP_MESSAGE`, returned only for validator refusals) and "Import fails" (`DomainError::Internal` text, which Plan 3 passes as `Finished.message`), and the cleanup paragraph (only the folders `created_roots()` reports are removed, never a diff of the collection list, so a collection created at the same time by anything else survives): Task 3, Ruling 9.
- Index rulings 7 (pinned `ImporterAdapter::new(active_workspace_path, temp_root)`), 8 (cleanup by `created_roots`, private temp folder, stale-only purge) and 9 (display name cap by bytes), and second-review rulings R1, R3, R5, R12, R15, R16: Rulings 8 to 15 above.
- Test counts, counted from the plan text: `transfer_zip` 32, `transfer_ports` 18, `network::staging` 18 (17 on Windows), probe 6, `created_roots_test` 3, `import_wiring` 26 (3 existing and 23 new adapter tests; 25 on Windows).
- Section 11, ZIP safety tests and the import-side part of the secret check (nothing here reads secrets; the export blanks them in Plan 1/3's scope): Task 1 tests. The keychain path of an import with environments is Plan 5 manual check 13 (Ruling 15).
- Section 12, first open item (stem probe with a rename fallback): Task 3 Steps 1, 2 and 10.
- Not covered here, by design: the state machines, `TransferService`, events and audit (Plans 1 and 3), the Tauri commands, the wiring of `ImporterAdapter` and `FsTransferStaging` in `lib.rs` and the startup purge helper (Plan 4), and all frontend work (Plan 5).
