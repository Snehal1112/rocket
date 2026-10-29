# HTTP create works without hard links Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Creating a new HTTP request works on filesystems without hard links (exFAT, FAT, some SMB shares) and still never replaces an existing file.

**Architecture:** `create_request_exclusive` has its own copy of the temp-file-and-hard-link code, which fails on those filesystems. The GraphQL, gRPC and WebSocket creates already use the shared `write_new_file_exclusive`, which falls back to `create_new`. This plan makes the HTTP create call the same function, through a seam that lets a test inject the link operation.

**Tech Stack:** Rust (`rocket-infra`), `tempfile` in tests.

**Spec:** GitHub issue #125 (https://github.com/Snehal1112/rocket/issues/125), a follow-up of `docs/superpowers/specs/2026-10-29-exclusive-create-non-http-requests-design.md`. The ticket is one line, so the decisions below are this plan's own calls.

## Global Constraints

- Touch only `crates/rocket-infra/src/fs_collection/requests.rs` and `crates/rocket-infra/src/fs_collection/tests.rs`.
- `crates/rocket-infra/**` matches the factory's risk rules. If the factory ever runs this plan, expect `needs-human`. Run it natively or approve it by hand.
- The public behaviour of `create_request_exclusive` does not change on filesystems with hard links: same temp name pattern, same `AlreadyExists(format!("{collection}/{path}"))` label, same returned relative path, same empty-uid error, same collection lock.
- Cargo commands carry `-j4` and `-p rocket-infra`: `cargo test -j4 -p rocket-infra fs_collection`, `cargo check -j4 -p rocket-infra`. Never a bare workspace run.
- No production unwrap calls. Tests use `.expect("...")` and `expect_err`.
- Read `docs/superpowers/specs/opencollection-spec-reference.md` first (this touches `FsCollectionRepo`).
- No formatting edits elsewhere in either file.

## Decisions (this plan's calls)

- The seam is a new function `create_request_exclusive_with(..., link)`, and `create_request_exclusive` becomes a thin wrapper that passes `|from, to| std::fs::hard_link(from, to)`. This is the same shape as `write_new_file_exclusive`, whose test already injects the link operation.
- The typed creates already share `create_typed_request_exclusive`. Folding the HTTP create into that helper would mean generalising its `uid` and `to_yaml` inputs. That is a larger change than the ticket, so it is left as is.
- The duplicated temp-file code in `create_request_exclusive` is deleted, not kept as a second copy.

## Review Focus

1. A filesystem without hard links (the link call fails with `Unsupported`): the file is created and no temp file is left. Pinned by `create_request_exclusive_falls_back_when_hard_link_is_unsupported`.
2. The same filesystem with a file already there: the create fails with `AlreadyExists` and the old file is untouched. Pinned by `create_request_exclusive_fallback_never_replaces_a_file`.
3. A name that appears between the caller's check and the link: `AlreadyExists`, no temp file. Pinned by `create_request_exclusive_maps_a_link_clash_to_already_exists`.
4. A path whose parent folder is missing: the create fails and does not create the folder, with or without hard-link support. Pinned by `create_request_exclusive_never_creates_a_missing_folder`.
5. The normal path with hard links is unchanged. Pinned by the existing `create_request_exclusive_never_replaces_a_file`.

## File Structure

- Modify `crates/rocket-infra/src/fs_collection/requests.rs`: add `create_request_exclusive_with`, make `create_request_exclusive` call it, delete the inline temp-file code.
- Modify `crates/rocket-infra/src/fs_collection/tests.rs`: four new tests next to `create_request_exclusive_never_replaces_a_file`.

---

### Task 1: Route the HTTP create through the shared exclusive writer

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection/requests.rs` (`create_request_exclusive`, around line 136)
- Test: `crates/rocket-infra/src/fs_collection/tests.rs` (after `create_request_exclusive_never_replaces_a_file`, around line 3269)

**Interfaces:**
- Consumes: `write_new_file_exclusive(file_path: &Path, bytes: &[u8], label: &str, link: impl Fn(&Path, &Path) -> std::io::Result<()>) -> DomainResult<()>` (already in `requests.rs`), the test helpers `setup() -> (TempDir, FsCollectionRepo)` and `leftover_temp_files(dir: &Path) -> Vec<_>`.
- Produces: `pub(super) fn create_request_exclusive_with(repo: &FsCollectionRepo, collection: &str, path: &str, request: &Request, link: impl Fn(&Path, &Path) -> std::io::Result<()>) -> DomainResult<String>`. It returns the file's path relative to the collection directory, as `create_request_exclusive` does.

- [ ] **Step 1: Read the code**

Read `docs/superpowers/specs/opencollection-spec-reference.md`. Read `create_request_exclusive`, `write_new_file_exclusive` and `create_typed_request_exclusive` in `crates/rocket-infra/src/fs_collection/requests.rs`, and the tests `create_request_exclusive_never_replaces_a_file`, `exclusive_create_falls_back_when_hard_link_is_unsupported` and `leftover_temp_files` in `crates/rocket-infra/src/fs_collection/tests.rs`.

- [ ] **Step 2: Write the failing tests**

One full test; build the request as `create_request_exclusive_never_replaces_a_file` does (`Request::new("One", HttpMethod::Get, "https://one.example.com")`).

```rust
#[test]
fn create_request_exclusive_falls_back_when_hard_link_is_unsupported() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    let request = Request::new("One", HttpMethod::Get, "https://one.example.com");
    let path = requests::create_request_exclusive_with(&repo, "col", "one.yml", &request, |_, _| {
        Err(std::io::ErrorKind::Unsupported.into())
    })
    .expect("fallback create");
    assert_eq!(path, "one.yml");
    let file = dir.path().join("col/one.yml");
    assert!(fs::read_to_string(&file).expect("file").contains("https://one.example.com"));
    assert!(leftover_temp_files(&dir.path().join("col")).is_empty());
}
```

| Test | Setup | Assertions |
|---|---|---|
| `create_request_exclusive_fallback_never_replaces_a_file` | create `one.yml` with the normal `repo.create_request_exclusive`, keep its text, then call `create_request_exclusive_with` with a link that returns `Unsupported` and a different request | `expect_err` is `DomainError::AlreadyExists(ref l)` with `l == "col/one.yml"`; the file text equals the saved text; no leftover temp file |
| `create_request_exclusive_maps_a_link_clash_to_already_exists` | empty collection; link closure returns `Err(std::io::ErrorKind::AlreadyExists.into())` | `expect_err` is `AlreadyExists`; `col/one.yml` does not exist; no leftover temp file |
| `create_request_exclusive_never_creates_a_missing_folder` | collection `col`; path `missing/one.yml` (the parent folder does not exist); link closure returns `Unsupported` | `create_request_exclusive_with(...).is_err()`; `col/missing` does not exist. Do not assert the error variant; it comes from `validate_path` or the temp-file write |

- [ ] **Step 3: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-infra fs_collection`
Expected: the new tests fail to compile with `cannot find function 'create_request_exclusive_with' in module 'requests'`.

- [ ] **Step 4: Implement `create_request_exclusive_with` and the wrapper**

Move the body of `create_request_exclusive` into `create_request_exclusive_with` with the extra `link` parameter. Keep the name validation, the collection lock, the empty-uid error, `validate_path`, and the YAML serialisation. Replace everything from the `nanos` line to the end of the `match linked { ... }` block with one call: `write_new_file_exclusive(&file_path, yaml.as_bytes(), &format!("{collection}/{path}"), link)?;`. Then return the relative path as before. `create_request_exclusive` becomes `create_request_exclusive_with(repo, collection, path, request, |from, to| std::fs::hard_link(from, to))`. Remove the now-unused `use std::io::Write;` inside the function. Update the doc comment: it should say the file is linked into place when the filesystem supports hard links and created with `create_new` otherwise.

- [ ] **Step 5: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-infra fs_collection` and `cargo check -j4 -p rocket-infra`
Expected: the four new tests and all existing `fs_collection` tests pass, and the check is clean. Run `git diff --stat` and confirm only `requests.rs` and `tests.rs` changed.

- [ ] **Step 6: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill, path-scoped: `git commit --only -- crates/rocket-infra/src/fs_collection/requests.rs crates/rocket-infra/src/fs_collection/tests.rs`. Subject: `fix(infra): create HTTP requests without hard links`. Footer: `Fixes: #125`.

Next plan: docs/superpowers/plans/2026-10-29-small-followups/plan-05-curl-form-parse.md
