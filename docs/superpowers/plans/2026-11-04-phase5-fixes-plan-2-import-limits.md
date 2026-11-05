# Phase 5 fixes, Plan 2: import limits (S-4, S-5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An import cannot be used to exhaust memory or disk: text files are read with a size cap, and a zip is refused when it has too many entries or expands past a total byte limit.

**Architecture:** One helper reads a text file through the port with `regular_file_len` (rejects devices and pipes, checks the size) and `read_up_to` (never reads past the cap). The Bruno, Postman and root-defaults readers use it. The zip extractor counts entries and bytes and stops the stream at the limit.

**Tech Stack:** Rust, `rocket-import`, the `zip` crate (tests build archives in memory).

**Spec:** `.claude/reviews/2026-11-04-ddd-phase5-audit.md` (S-4, S-5). Depends on Plan 1 only for `importer.rs` merge order, not for code.

## Global Constraints

- Constants (in `crates/rocket-import/src/limits.rs`, new): `MAX_IMPORT_FILE_BYTES: u64 = 32 * 1024 * 1024` (same as `MAX_WSDL_BYTES`), `MAX_ZIP_ENTRIES: usize = 20_000`, `MAX_ZIP_TOTAL_BYTES: u64 = 512 * 1024 * 1024`.
- A file over the cap is an error that names the file and the limit, and it never stops the other files of a folder import (it becomes a `SkippedItem`).
- No `unwrap` in production code. `cargo` commands take `-j4`; never `--workspace`.

## Review Focus

- A `.bru` that is a symlink to `/dev/zero` (or any non-regular file) is refused without reading it.
- A file of exactly `MAX_IMPORT_FILE_BYTES` is read; one byte more is refused.
- A zip whose entries lie about their size in the header is still bounded, because the stream itself is capped.
- A zip with `MAX_ZIP_ENTRIES + 1` entries is refused before extraction writes anything.
- A normal Bruno zip and a normal Postman file still import.

---

### Task 1: Capped text reads (S-5)

**Files:**
- Create: `crates/rocket-import/src/limits.rs` (constants and `read_text_capped`)
- Modify: `crates/rocket-import/src/lib.rs` (`mod limits;`), `crates/rocket-import/src/bru/mod.rs` (`parse_file`, `parse_env_file`), `crates/rocket-import/src/postman/parser.rs` (`parse_postman_json`), `crates/rocket-import/src/importer.rs` (`merge_root_defaults` read)
- Test: `limits.rs` `mod tests` with `FakeFiles`-style doubles

**Interfaces:**
- Produces: `pub(crate) fn read_text_capped(files: &dyn ImportFiles, path: &Path, max: u64) -> ImportResult<String>`. It returns `ImportError::IoError` with kind `InvalidInput` and the message `"<name> is not a regular file"` or `"<name> is larger than <max> bytes"`.
- Consumes: `ImportFiles::{regular_file_len, read_up_to}`.

- [ ] **Step 1: Write the failing tests.** `reads_a_file_at_the_cap` (len == max returns the text), `refuses_one_byte_over_the_cap` (len == max + 1 gives the error, and `read_up_to` is never called: the double panics if it is), `refuses_a_non_regular_file` (`regular_file_len` returns `None`), `parse_file_refuses_an_oversized_bru` (through `bru::parse_file`).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-import limits` — Expected: compile error (red).
- [ ] **Step 3: Implement** `read_text_capped` (check length first, then `read_up_to(path, max + 1)` and re-check the byte count, then `String::from_utf8`, a bad encoding is `InvalidData`). Swap the four `read_to_string` calls for it. In the folder walks, a failure on one file is reported as a `SkippedItem` the way parse errors already are.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-import` — Expected: PASS.
- [ ] **Step 5: Commit** with the `dev-workflow-skills:1-git-commit` skill (pathspec: the files above).

### Task 2: Zip entry and size limits (S-4)

**Files:**
- Modify: `crates/rocket-import/src/bru/zip_extractor.rs` (`extract_to_temp`)
- Test: same file, `mod tests` (zip built with `zip::ZipWriter` into a `Cursor<Vec<u8>>`; the port double serves it through `open_read`)

**Interfaces:**
- Consumes: `MAX_ZIP_ENTRIES`, `MAX_ZIP_TOTAL_BYTES` from `limits.rs`, `ImportError::ZipExtractionFailed(String)`.
- Produces: `extract_to_temp` keeps its signature; it returns `ZipExtractionFailed("the zip has more than 20000 entries")` or `ZipExtractionFailed("the zip expands to more than 536870912 bytes")`.

- [ ] **Step 1: Write the failing tests.** `a_zip_with_too_many_entries_is_refused_before_writing` (20,001 empty entries; the port double records zero `write_stream` calls), `a_zip_that_expands_past_the_limit_is_refused` (use small injected limits through an inner `extract_with_limits(files, zip, max_entries, max_bytes)` so the test stays tiny: one 2 KiB entry with `max_bytes = 1024`), `a_normal_zip_still_extracts` (two entries, both written).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-import zip_extractor` — Expected: FAIL.
- [ ] **Step 3: Implement** `extract_with_limits` as the real body and have `extract_to_temp` call it with the constants. Check `archive.len()` first. While streaming, wrap each entry in `Read::take(remaining)` where `remaining = max_bytes - written_so_far + 1`, add the bytes returned by `write_stream` to a running total, and fail when it passes `max_bytes`. Do not trust the entry's declared size.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-import`, `cargo clippy -j4 -p rocket-import -- -D warnings` — Expected: PASS.
- [ ] **Step 5: Commit** (pathspec `crates/rocket-import/src/bru/zip_extractor.rs`).
