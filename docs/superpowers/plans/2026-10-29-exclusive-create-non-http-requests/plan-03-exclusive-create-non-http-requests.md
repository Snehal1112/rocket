# Exclusive Create for Non-HTTP Requests, Importer Dedup Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** A Bruno or Postman import never writes two source items to the same file: a clash gets a numeric suffix.

**Architecture:** One helper picks a free output path from a run-wide set of folded paths. It is threaded through the recursive Bruno walk and the Postman item writer. Both importers target a fresh collection, so only files this run wrote can clash.

**Tech Stack:** Rust, `rocket-import`.

**Spec:** `docs/superpowers/specs/2026-10-29-exclusive-create-non-http-requests-design.md`

## File Structure

- `crates/rocket-import/src/importer.rs`: the helper, `walk_requests` (`:323`), `import_legacy_collection` (`:134`), `write_postman_items` (`:683`) and its caller, and the tests module.
- `crates/rocket-import/CLAUDE.md`: a note on the dedup rule.

## Global Constraints

- OpenAPI, WSDL and Bruno 3 (`copy_collection_files`) imports are not changed. OpenAPI and WSDL already deduplicate with `unique_segment`.
- The request `name` stored inside the file is unchanged. A renamed file counts as imported and the report format does not change.
- The existing quirk `let _ = repo.save_request(...)` (a failed Bruno HTTP save is counted as imported) is out of scope. Tests assert file names, not the imported count.
- No panicking unwrap calls in production code. Tests use `.expect("...")`. Cargo commands are per crate: `cargo test -j4 -p rocket-import <filter>`, `cargo check -j4 -p rocket-import`.
- Edit only the files listed. No formatting, import-order or clippy edits elsewhere.

## Review Focus

1. Two Bruno requests whose names differ only in case (`Get.bru`, `get.bru`) both survive as two files: `bruno_import_keeps_both_files_when_case_variants_clash` (Task 1).
2. A third clash gets the next number, and a request that does not clash keeps its exact name: `bruno_import_numbers_a_third_clash` and `unique_import_path_keeps_a_free_name_unchanged` (Task 1).
3. A folder and a request with the same name do not clash: `unique_import_path_ignores_folders` (Task 1).
4. Postman items with one sanitized name in one folder both survive, and the same name in two folders does not clash: `postman_import_keeps_both_items_with_one_sanitized_name` and `postman_import_allows_the_same_name_in_two_folders` (Task 2).

---

### Task 1: The dedup helper and the Bruno import

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-import/src/importer.rs` (`import_legacy_collection` `:134-160`, `walk_requests` `:323-436`, tests module near `unique_segment_stays_unique_and_short_after_truncation`, `:1963`)

**Interfaces:**
- Produces:
  - `fn unique_import_path(used: &mut std::collections::HashSet<String>, path: &str) -> String`. Normalise `path` with `rocket_collection::request_filename_for`. If `rocket_collection::fold_name(&normalized)` is not in `used`, insert it and return the normalised path. Otherwise take the stem (the normalised path without `.yml` or `.yaml`, folder prefix kept) and for `n` from 2 upward call `rocket_collection::candidate_filename(stem, n)` until its folded form is free, insert it and return it.
  - `walk_requests` gains a last parameter `used: &mut HashSet<String>`; `import_legacy_collection` creates one empty set per run and passes it down, including in the recursive call. Each of the four `save_*` calls uses `let out_path = unique_import_path(used, &rel_path.with_extension("yml").to_string_lossy())`, computed once per source file before the match, so the path used for HTTP, GraphQL, gRPC and WebSocket is the same value.

- [ ] **Step 1: Read** `importer.rs:134-160`, `:323-440`, and the Bruno import tests in the same file (find one that builds a temp Bruno collection directory with `.bru` files and runs the legacy import; reuse its setup).

- [ ] **Step 2: Write the failing tests.**

```rust
#[test]
fn unique_import_path_keeps_a_free_name_unchanged() {
    let mut used = std::collections::HashSet::new();
    assert_eq!(unique_import_path(&mut used, "api/get.yml"), "api/get.yml");
}

#[test]
fn unique_import_path_numbers_a_clash_and_ignores_case() {
    let mut used = std::collections::HashSet::new();
    assert_eq!(unique_import_path(&mut used, "api/Get.yml"), "api/Get.yml");
    assert_eq!(unique_import_path(&mut used, "api/get.yml"), "api/get 2.yml");
    assert_eq!(unique_import_path(&mut used, "api/GET.yml"), "api/GET 3.yml");
}
```

| Test | Setup | Assertion |
|---|---|---|
| `unique_import_path_ignores_folders` | `unique_import_path(&mut used, "a/b.yml")` after `"a.yml"` | both returned unchanged (a folder `a` and a file `a.yml` are different keys) |
| `bruno_import_keeps_both_files_when_case_variants_clash` | temp Bruno collection with `Get.bru` and `get.bru` in one folder, each a valid HTTP request with a different URL | after import, the collection folder holds two request `.yml` files, one of them named with ` 2.yml`; each file's `url` matches one source URL |
| `bruno_import_numbers_a_third_clash` | `Get.bru`, `get.bru` and `GET.bru` | three distinct files |

- [ ] **Step 3: Run** `cargo test -j4 -p rocket-import unique_import_path` and `cargo test -j4 -p rocket-import bruno_import_keeps_both_files`. Expected: `cannot find function unique_import_path in this scope`, then (after a stub) the Bruno test fails with one file instead of two.

- [ ] **Step 4: Implement** the helper and thread the set through `walk_requests` and `import_legacy_collection`.

- [ ] **Step 5: Run** `cargo test -j4 -p rocket-import bruno_import`, `cargo test -j4 -p rocket-import unique_import_path`, `cargo test -j4 -p rocket-import` and `cargo check -j4 -p rocket-import`. Expected: all pass, and the existing Bruno import tests are unchanged.

---

### Task 2: The Postman import and the docs note

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-import/src/importer.rs` (`write_postman_items` `:683-720` and its caller near `:635-640`)
- Modify: `crates/rocket-import/CLAUDE.md`

**Interfaces:**
- Consumes: `unique_import_path` from Task 1.
- Produces: `write_postman_items` gains a last parameter `used: &mut HashSet<String>` and its caller creates one empty set per Postman import. The request path used for `save_request` is `unique_import_path(used, &request_path)` and the recursive call for a folder passes the same set. The key includes the folder prefix, so the same name in two folders does not clash.

- [ ] **Step 1: Read** `importer.rs:630-730`, `sanitize_postman_filename` (`:1274`), and an existing Postman import test in the tests module (reuse its collection JSON builder).

- [ ] **Step 2: Write the failing tests.**

| Test | Setup | Assertion |
|---|---|---|
| `postman_import_keeps_both_items_with_one_sanitized_name` | Postman collection with two requests named `List users` and `List-users` in one folder (both sanitize to `List-users`) | the collection folder holds two request files, one named ` 2.yml`; their URLs differ |
| `postman_import_allows_the_same_name_in_two_folders` | two folders each holding a request named `Get` | both keep `Get.yml`, one in each folder, no ` 2` file |

- [ ] **Step 3: Run** `cargo test -j4 -p rocket-import postman_import_keeps_both_items`. Expected: failure, one request file instead of two (the second overwrote the first).

- [ ] **Step 4: Implement** the threading and the call, then add a short note to `crates/rocket-import/CLAUDE.md` that Bruno and Postman imports number a clashing output file (` 2.yml`) through `unique_import_path`, keyed by the folded full relative path.

- [ ] **Step 5: Run** `cargo test -j4 -p rocket-import postman_import`, `cargo test -j4 -p rocket-import`, `cargo check -j4 -p rocket-import`. Expected: all pass.

Next plan: none
