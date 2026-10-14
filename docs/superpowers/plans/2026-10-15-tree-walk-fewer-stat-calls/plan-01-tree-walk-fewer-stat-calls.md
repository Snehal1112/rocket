# Tree walk fewer stat calls: pin and fixture Implementation Plan

> **For agentic workers:** This plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file. The driver commits, so there are no commit steps. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Pin, with characterization tests, everything the sidebar tree walk loads and skips today, and build the deep benchmark fixture with its single-load test, before any production behavior changes.

**Architecture:** One new test-only module, `fs_collection/tree_walk_tests.rs`, renders a loaded tree as a text outline and compares it with literal expected text. The only production edit is making `build_tree` `pub(super)` so a test can drive it with its own loader closure. A deep-fixture builder is added to `benches/support/mod.rs` and is shared by a new criterion bench case and by an `#[ignore]` integration test that does one `get_summaries` load for `strace`.

**Tech Stack:** Rust, `tempfile`, `criterion`, `rocket-infra` (`FsCollectionRepo`), `strace` (Linux).

**Spec:** `docs/superpowers/specs/2026-10-15-tree-walk-fewer-stat-calls-design.md`

## Global Constraints

- Behavior, on-disk formats and IPC contracts must not change. This plan changes no production behavior.
- Linux and macOS only, measured on Linux. Production code uses only std APIs that exist on every platform. Tests that need symlinks or raw byte file names are `cfg(unix)`.
- No new dependencies.
- No unwrap calls in production paths. Tests and bench support use `.expect("...")`.
- Cargo: always pass `-j4`. Use targeted `-p rocket-infra` runs. Never `cargo test --workspace` or `--all`.
- If a pinned expectation fails on the unchanged code, the code is the truth: fix the test to match the code and say so in your final reply.
- Symlinked request files stay loaded, symlinked directories and `.js` files stay skipped, and the legacy `.uid` migration writes stay as they are. The tests pin all of it.

## Review Focus

- A symlinked directory named like a request file (`dirlink.yml`) or a script (`tools.js`) is skipped without an error, also under `get_strict` (Task 1).
- A dangling symlinked request file keeps today's result: the full load errors, the summaries load skips it (Task 1).
- A root `flows` folder is hidden while a nested `flows` folder stays visible (Task 1).
- A file removed after the listing fails the full-load style loader and is skipped by the lenient one (Task 1).
- An unparseable `_order.yml` and an `_order.yml` that is a directory give name order and never read `_order.json` (Task 2).
- An empty `folder.yml` uid falls back to the legacy `.uid` file and the migration write and the delete are pinned (Task 2).
- The deep fixture really holds 500 folders and 2500 requests, and the single-load test checks the count (Task 3).

---

### Task 1: Symlink, hidden-entry and removal tests

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-infra/src/fs_collection/tree_walk_tests.rs`
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs` (register the module)
- Modify: `crates/rocket-infra/src/fs_collection/tree.rs` (visibility of `build_tree` only)

**Interfaces:**
- Produces, in `tree_walk_tests.rs`: `setup() -> (TempDir, FsCollectionRepo)`, `save(repo, path, name)`, `write(dir, rel, text)`, `folder_yml(name, uid) -> String`, `outline(&Folder) -> String`, `as_summary(&str) -> String`, `small() -> (TempDir, FsCollectionRepo)`, `rich() -> (TempDir, FsCollectionRepo)`, `assert_both_loads(repo, expected)` and the constants `SMALL_ONLY`, `RICH_BY_NAME`. Tasks 2 of this plan and of Plan 2 reuse them.

- [ ] **Step 1: Make `build_tree` visible to the tests**

In `crates/rocket-infra/src/fs_collection/tree.rs` change only the signature line `fn build_tree<F>(current: &Path, load_item: &mut F) -> DomainResult<Folder>` to:

```rust
pub(super) fn build_tree<F>(current: &Path, load_item: &mut F) -> DomainResult<Folder>
```

- [ ] **Step 2: Register the test module**

In `crates/rocket-infra/src/fs_collection/mod.rs`, after the `schema_shape_tests` block, add:

```rust
// Kept in its own group so rustfmt does not sort it before `tests`.
#[cfg(test)]
mod tree_walk_tests;
```

- [ ] **Step 3: Create the test file with the helpers and the symlink, hidden and removal tests**

Create `crates/rocket-infra/src/fs_collection/tree_walk_tests.rs`:

```rust
//! Pins what the tree walk loads and skips, so work on `tree.rs` cannot change it.

use std::fs;

use rocket_collection::{CollectionItem, CollectionRepository, Folder};
use rocket_shared::types::HttpMethod;
use tempfile::TempDir;

use super::tree::build_tree;
use super::FsCollectionRepo;

fn setup() -> (TempDir, FsCollectionRepo) {
    let dir = TempDir::new().expect("temp dir");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    (dir, repo)
}

/// Saves one HTTP request into collection `col`.
fn save(repo: &FsCollectionRepo, path: &str, name: &str) {
    let req = rocket_collection::Request::new(name, HttpMethod::Get, "https://example.test");
    repo.save_request("col", path, &req).expect("save request");
}

/// Writes `text` to `rel` below the temp dir and creates the parent folders.
fn write(dir: &TempDir, rel: &str, text: &str) {
    let path = dir.path().join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent folders");
    }
    fs::write(path, text).expect("write file");
}

fn folder_yml(name: &str, uid: &str) -> String {
    format!("info:\n  name: {name}\n  uid: {uid}\n")
}

/// Renders a folder as text: `dir:<dir_name>|<name>|<uid>` then one line per item.
/// The root uid is random, so the root line shows `uid-set` or `uid-empty`.
fn outline(folder: &Folder) -> String {
    let mut lines = Vec::new();
    outline_into(folder, 0, &mut lines);
    lines.join("\n")
}

fn outline_into(folder: &Folder, depth: usize, lines: &mut Vec<String>) {
    let pad = "  ".repeat(depth);
    let uid = if depth == 0 {
        if folder.uid.is_empty() {
            "uid-empty"
        } else {
            "uid-set"
        }
    } else {
        folder.uid.as_str()
    };
    let dir_name = folder.dir_name.as_deref().unwrap_or("-");
    lines.push(format!("{pad}dir:{dir_name}|{}|{uid}", folder.name));
    for item in &folder.items {
        match item {
            CollectionItem::Folder(f) => outline_into(f, depth + 1, lines),
            CollectionItem::Request(r) => lines.push(format!(
                "{pad}  req:{}|{}",
                r.file_name.as_deref().unwrap_or("-"),
                r.name
            )),
            CollectionItem::Summary(s) => lines.push(format!(
                "{pad}  sum:{}|{}",
                s.file_name.as_deref().unwrap_or("-"),
                s.name
            )),
            CollectionItem::ScriptFile(s) => lines.push(format!("{pad}  js:{}", s.file_name)),
            _ => lines.push(format!("{pad}  other")),
        }
    }
}

/// The summaries load shows requests as `sum:` lines.
fn as_summary(outline: &str) -> String {
    outline.replace("req:", "sum:")
}

/// A collection `col` with one request, `req-a.yml`.
fn small() -> (TempDir, FsCollectionRepo) {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    save(&repo, "req-a.yml", "Alpha");
    (dir, repo)
}

/// Asserts both loads give `expected` (the summaries load shows `sum:` for `req:`).
fn assert_both_loads(repo: &FsCollectionRepo, expected: &str) {
    let full = repo.get("col").expect("full load");
    assert_eq!(outline(&full.root), expected, "full load");
    let summaries = repo.get_summaries("col").expect("summaries load");
    assert_eq!(outline(&summaries.root), as_summary(expected), "summaries load");
}

const SMALL_ONLY: &str = "dir:col|col|uid-set\n  req:req-a.yml|Alpha";

/// A collection with folders, a script file, nested `flows`, and entries the tree hides.
fn rich() -> (TempDir, FsCollectionRepo) {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    save(&repo, "req-a.yml", "Alpha");
    save(&repo, "req-c.yml", "Gamma");
    write(&dir, "col/lib.js", "module.exports = 1;");
    write(&dir, "col/a-folder/folder.yml", &folder_yml("Alpha Display", "uid-a"));
    save(&repo, "a-folder/x.yml", "X");
    write(&dir, "col/b-folder/folder.yml", &folder_yml("b-folder", "uid-b"));
    write(&dir, "col/b-folder/nested/folder.yml", &folder_yml("nested", "uid-n"));
    write(&dir, "col/b-folder/nested/flows/folder.yml", &folder_yml("flows", "uid-f"));
    save(&repo, "b-folder/nested/flows/inner.yml", "Inner");
    // Entries the tree must hide.
    write(&dir, "col/.hidden/h.yml", "x");
    write(&dir, "col/node_modules/n.js", "x");
    write(&dir, "col/environments/e.yml", "x");
    write(&dir, "col/flows/skipped.yml", "x");
    (dir, repo)
}

const RICH_BY_NAME: &str = "dir:col|col|uid-set
  dir:a-folder|Alpha Display|uid-a
    req:x.yml|X
  dir:b-folder|b-folder|uid-b
    dir:nested|nested|uid-n
      dir:flows|flows|uid-f
        req:inner.yml|Inner
  js:lib.js
  req:req-a.yml|Alpha
  req:req-c.yml|Gamma";

#[test]
fn walk_hides_the_hidden_entries_and_keeps_a_nested_flows_folder() {
    let (_dir, repo) = rich();
    assert_both_loads(&repo, RICH_BY_NAME);
}

#[cfg(unix)]
#[test]
fn walk_skips_a_symlinked_directory() {
    let (dir, repo) = small();
    let valid = fs::read_to_string(dir.path().join("col/req-a.yml")).expect("read request");
    write(&dir, "outside/secret.yml", &valid);
    std::os::unix::fs::symlink(dir.path().join("outside"), dir.path().join("col/linked"))
        .expect("symlink");
    assert_both_loads(&repo, SMALL_ONLY);
}

#[cfg(unix)]
#[test]
fn walk_skips_a_symlinked_directory_named_like_a_request_file() {
    let (dir, repo) = small();
    let valid = fs::read_to_string(dir.path().join("col/req-a.yml")).expect("read request");
    write(&dir, "outside/secret.yml", &valid);
    std::os::unix::fs::symlink(dir.path().join("outside"), dir.path().join("col/dirlink.yml"))
        .expect("symlink");
    assert_both_loads(&repo, SMALL_ONLY);
    let strict = repo.get_strict("col").expect("strict load");
    assert_eq!(outline(&strict.root), SMALL_ONLY, "strict load");
}

#[cfg(unix)]
#[test]
fn walk_skips_a_symlinked_directory_named_like_a_script() {
    let (dir, repo) = small();
    let valid = fs::read_to_string(dir.path().join("col/req-a.yml")).expect("read request");
    write(&dir, "outside/secret.yml", &valid);
    std::os::unix::fs::symlink(dir.path().join("outside"), dir.path().join("col/tools.js"))
        .expect("symlink");
    assert_both_loads(&repo, SMALL_ONLY);
}

#[cfg(unix)]
#[test]
fn walk_skips_a_symlinked_js_file() {
    let (dir, repo) = small();
    write(&dir, "outside/real.js", "module.exports = 1;");
    std::os::unix::fs::symlink(dir.path().join("outside/real.js"), dir.path().join("col/link.js"))
        .expect("symlink");
    assert_both_loads(&repo, SMALL_ONLY);
}

#[cfg(unix)]
#[test]
fn walk_still_lists_a_symlinked_request_file() {
    let (dir, repo) = small();
    let valid = fs::read_to_string(dir.path().join("col/req-a.yml")).expect("read request");
    write(&dir, "outside/target.yml", &valid);
    std::os::unix::fs::symlink(
        dir.path().join("outside/target.yml"),
        dir.path().join("col/link.yml"),
    )
    .expect("symlink");
    assert_both_loads(
        &repo,
        "dir:col|col|uid-set\n  req:link.yml|Alpha\n  req:req-a.yml|Alpha",
    );
}

#[cfg(unix)]
#[test]
fn walk_keeps_its_behavior_for_a_dangling_symlinked_request_file() {
    let (dir, repo) = small();
    std::os::unix::fs::symlink(dir.path().join("nowhere.yml"), dir.path().join("col/dead.yml"))
        .expect("symlink");
    // The full load reads the file with `?`, so a dangling link is an error today.
    assert!(repo.get("col").is_err(), "full load errors on a dangling link");
    // The summaries load skips an unreadable file.
    let summaries = repo.get_summaries("col").expect("summaries load");
    assert_eq!(outline(&summaries.root), as_summary(SMALL_ONLY));
}

#[test]
fn a_file_removed_after_the_listing_fails_a_loader_that_reads_with_a_question_mark() {
    let (dir, repo) = small();
    save(&repo, "req-b.yml", "Bravo");
    let root = dir.path().join("col");
    // `a` sorts before `b`, so `b` is gone by the time the walk reaches it.
    let result = build_tree(&root, &mut |path, name| {
        if name == "req-a.yml" {
            fs::remove_file(root.join("req-b.yml")).expect("remove the later file");
        }
        let _text = fs::read_to_string(path)?;
        Ok(None)
    });
    assert!(result.is_err(), "the full-load style loader errors on the removed file");
}

#[test]
fn a_file_removed_after_the_listing_is_skipped_by_a_lenient_loader() {
    let (dir, repo) = small();
    save(&repo, "req-b.yml", "Bravo");
    let root = dir.path().join("col");
    let mut seen: Vec<String> = Vec::new();
    let result = build_tree(&root, &mut |path, name| {
        if name == "req-a.yml" {
            fs::remove_file(root.join("req-b.yml")).expect("remove the later file");
        }
        match fs::read_to_string(path) {
            Ok(_) => seen.push(name.to_string()),
            Err(_) => {}
        }
        Ok(None)
    });
    assert!(result.is_ok(), "the summaries style loader skips the removed file");
    assert_eq!(seen, vec!["req-a.yml".to_string()]);
}
```

- [ ] **Step 4: Run the new tests**

Run: `cargo test -j4 -p rocket-infra tree_walk_tests`
Expected: all pass (the unix-only ones run on Linux and macOS). If a pinned expectation fails on the unchanged code, read the failure, fix the test to match the code, and say which test and why in your final reply.

- [ ] **Step 5: Run the existing tree tests and the check**

Run: `cargo test -j4 -p rocket-infra fs_collection && cargo check -j4 -p rocket-infra`
Expected: pass, with no warnings from the new file.

---

### Task 2: Order, folder.yml, uid and migration tests

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection/tree_walk_tests.rs` (append)

**Interfaces:**
- Consumes: the helpers and constants from Task 1 (`setup`, `save`, `write`, `folder_yml`, `outline`, `small`, `rich`, `assert_both_loads`, `as_summary`, `RICH_BY_NAME`) and `build_tree`.
- Produces: the order and folder pins that Plan 2 must keep passing without edits.

- [ ] **Step 1: Append the order tests**

```rust
const RICH_ORDERED: &str = "dir:col|col|uid-set
  req:req-c.yml|Gamma
  js:lib.js
  dir:b-folder|b-folder|uid-b
    dir:nested|nested|uid-n
      dir:flows|flows|uid-f
        req:inner.yml|Inner
  dir:a-folder|Alpha Display|uid-a
    req:x.yml|X
  req:req-a.yml|Alpha";

#[test]
fn walk_applies_order_yml_and_puts_unlisted_names_last() {
    let (dir, repo) = rich();
    write(&dir, "col/_order.yml", "- req-c.yml\n- lib.js\n- b-folder\n");
    assert_both_loads(&repo, RICH_ORDERED);
}

#[test]
fn walk_applies_order_json_when_there_is_no_order_yml() {
    let (dir, repo) = rich();
    write(&dir, "col/_order.json", "[\"req-c.yml\", \"lib.js\", \"b-folder\"]");
    assert_both_loads(&repo, RICH_ORDERED);
}

#[test]
fn walk_prefers_order_yml_over_order_json() {
    let (dir, repo) = rich();
    write(&dir, "col/_order.yml", "- req-c.yml\n- lib.js\n- b-folder\n");
    write(&dir, "col/_order.json", "[\"req-a.yml\"]");
    assert_both_loads(&repo, RICH_ORDERED);
}

#[test]
fn walk_falls_back_to_name_order_for_an_unparseable_order_yml_without_reading_the_json() {
    let (dir, repo) = rich();
    write(&dir, "col/_order.yml", "{{{ not a list");
    write(&dir, "col/_order.json", "[\"req-c.yml\", \"lib.js\"]");
    assert_both_loads(&repo, RICH_BY_NAME);
}

#[test]
fn walk_gives_name_order_when_order_yml_is_a_directory_and_does_not_read_the_json() {
    let (dir, repo) = rich();
    fs::create_dir_all(dir.path().join("col/_order.yml")).expect("order dir");
    write(&dir, "col/_order.json", "[\"req-c.yml\", \"lib.js\"]");
    assert_both_loads(&repo, RICH_BY_NAME);
}

#[test]
fn walk_keeps_the_last_position_of_a_duplicate_and_ignores_unknown_names() {
    let (dir, repo) = rich();
    // req-c.yml is listed first and last, so it counts at position 3. The unknown name changes nothing.
    write(
        &dir,
        "col/_order.yml",
        "- req-c.yml\n- not-there.yml\n- req-a.yml\n- req-c.yml\n",
    );
    assert_both_loads(
        &repo,
        "dir:col|col|uid-set
  req:req-a.yml|Alpha
  req:req-c.yml|Gamma
  dir:a-folder|Alpha Display|uid-a
    req:x.yml|X
  dir:b-folder|b-folder|uid-b
    dir:nested|nested|uid-n
      dir:flows|flows|uid-f
        req:inner.yml|Inner
  js:lib.js",
    );
}

#[cfg(unix)]
#[test]
fn walk_lists_a_non_utf8_name_and_sorts_it_after_the_listed_names() {
    use std::os::unix::ffi::OsStrExt;
    let (dir, repo) = small();
    save(&repo, "req-b.yml", "Bravo");
    let valid = fs::read_to_string(dir.path().join("col/req-a.yml")).expect("read request");
    let odd = std::ffi::OsStr::from_bytes(b"req-\xFF.yml");
    fs::write(dir.path().join("col").join(odd), &valid).expect("write the odd name");
    write(&dir, "col/_order.yml", "- req-b.yml\n");
    // The listed name comes first, then the unlisted names by file name. The odd name shows lossily.
    assert_both_loads(
        &repo,
        "dir:col|col|uid-set\n  req:req-b.yml|Bravo\n  req:req-a.yml|Alpha\n  req:req-\u{FFFD}.yml|Alpha",
    );
}
```

- [ ] **Step 2: Append the folder.yml, uid and migration tests**

```rust
fn first_subfolder(folder: &Folder) -> &Folder {
    folder
        .items
        .iter()
        .find_map(|i| match i {
            CollectionItem::Folder(f) => Some(f),
            _ => None,
        })
        .expect("a subfolder")
}

#[test]
fn walk_takes_the_root_uid_from_opencollection_yml() {
    let (dir, repo) = small();
    let first = repo.get("col").expect("first load").root.uid;
    let text = fs::read_to_string(dir.path().join("col/opencollection.yml")).expect("read opencollection.yml");
    assert!(!first.is_empty());
    assert!(text.contains(&first), "the root uid is the one stored in opencollection.yml");
    let second = repo.get_summaries("col").expect("second load").root.uid;
    assert_eq!(first, second);
}

#[test]
fn walk_takes_uid_and_display_name_from_folder_yml() {
    let (_dir, repo) = rich();
    let full = repo.get("col").expect("full load");
    let folder = first_subfolder(&full.root);
    assert_eq!(folder.dir_name.as_deref(), Some("a-folder"));
    assert_eq!(folder.name, "Alpha Display");
    assert_eq!(folder.uid, "uid-a");
}

#[test]
fn walk_falls_back_to_the_legacy_uid_file_and_migrates_it_into_folder_yml() {
    let (dir, repo) = small();
    write(&dir, "col/legacy/folder.yml", "info:\n  name: legacy\n");
    write(&dir, "col/legacy/.uid", "legacy-uid\n");
    let full = repo.get("col").expect("full load");
    assert_eq!(first_subfolder(&full.root).uid, "legacy-uid");
    // The load wrote the uid into folder.yml and deleted the legacy file, as it does today.
    let migrated = fs::read_to_string(dir.path().join("col/legacy/folder.yml")).expect("read folder.yml");
    assert!(migrated.contains("legacy-uid"), "folder.yml now holds the uid: {migrated}");
    assert!(!dir.path().join("col/legacy/.uid").exists(), "the .uid file is deleted");
}

#[test]
fn walk_uses_the_legacy_uid_file_for_a_folder_without_folder_yml_and_leaves_it_alone() {
    let (dir, repo) = small();
    write(&dir, "col/plain/.uid", "plain-uid\n");
    write(&dir, "col/plain/p.yml", "not used");
    let full = repo.get_summaries("col").expect("summaries load");
    let folder = first_subfolder(&full.root);
    assert_eq!(folder.uid, "plain-uid");
    assert_eq!(folder.name, "plain");
    assert!(dir.path().join("col/plain/.uid").exists(), "no YAML was written, so .uid stays");
    assert!(!dir.path().join("col/plain/folder.yml").exists(), "no folder.yml is created");
}

#[test]
fn walk_keeps_the_directory_name_for_an_unparseable_folder_yml_and_does_not_touch_it() {
    let (dir, repo) = small();
    write(&dir, "col/broken/folder.yml", "info: [unclosed");
    let full = repo.get_summaries("col").expect("summaries load");
    let folder = first_subfolder(&full.root);
    assert_eq!(folder.name, "broken");
    assert!(!folder.uid.is_empty(), "a uid is generated");
    let text = fs::read_to_string(dir.path().join("col/broken/folder.yml")).expect("read folder.yml");
    assert_eq!(text, "info: [unclosed");
}

#[test]
fn walk_of_a_missing_folder_is_the_empty_folder() {
    let (dir, _repo) = setup();
    let gone = dir.path().join("no-such-folder");
    let folder = super::tree::build_folder_tree(&gone, false).expect("a missing folder is not an error");
    assert!(folder.items.is_empty());
    assert_eq!(folder.name, "no-such-folder");
    assert_eq!(folder.dir_name.as_deref(), Some("no-such-folder"));
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -j4 -p rocket-infra tree_walk_tests`
Expected: all pass. Fix any pinned expectation that does not match the unchanged code, as in Task 1, and say which one and why.

- [ ] **Step 4: Run the existing tree tests and the check**

Run: `cargo test -j4 -p rocket-infra fs_collection && cargo check -j4 -p rocket-infra`
Expected: pass.

---

### Task 3: Deep fixture, bench case, single-load test and baseline

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/benches/support/mod.rs` (add the deep fixture)
- Modify: `crates/rocket-infra/benches/collection.rs` (add the deep case)
- Create: `crates/rocket-infra/tests/tree_walk_syscalls.rs`
- Create: `.claude/perf-tree-walk.md`

**Interfaces:**
- Produces, in `benches/support/mod.rs`: `pub const DEEP_PARENTS: usize = 25`, `pub const DEEP_CHILDREN: usize = 19`, `pub const DEEP_REQUESTS: usize = 5`, `pub const DEEP_FOLDERS: usize` (500), `pub fn populate_deep(repo: &FsCollectionRepo, name: &str)` and `pub fn generate_deep() -> Fixture`. Plan 2 Task 3 uses the bench case `collection_tree/get_summaries/deep` and the tests `build_deep_fixture` and `load_deep_fixture_once`.

- [ ] **Step 1: Add the deep fixture to the bench support**

In `crates/rocket-infra/benches/support/mod.rs`, add `std::path::Path` to the imports (`use std::path::Path;` below the existing `use` lines), then append this at the end of the file:

```rust
/// The deep case: 25 parent folders with 19 child folders each, so 500 folders in all. Every folder
/// holds 5 HTTP requests and an `_order.yml` that lists its requests and then its child folders.
pub const DEEP_PARENTS: usize = 25;
pub const DEEP_CHILDREN: usize = 19;
pub const DEEP_REQUESTS: usize = 5;
pub const DEEP_FOLDERS: usize = DEEP_PARENTS * (1 + DEEP_CHILDREN);

/// Writes the deep collection `name` into `repo`.
pub fn populate_deep(repo: &FsCollectionRepo, name: &str) {
    repo.create(name).expect("create collection");
    let root = repo.collection_root_path(name).expect("collection root");
    for p in 0..DEEP_PARENTS {
        let parent = format!("p-{p}");
        repo.create_folder(name, &parent).expect("create parent folder");
        let children: Vec<String> = (0..DEEP_CHILDREN).map(|c| format!("c-{c}")).collect();
        fill_deep_folder(repo, name, &root, &parent, &children, p * (1 + DEEP_CHILDREN));
        for (c, child) in children.iter().enumerate() {
            let dir = format!("{parent}/{child}");
            repo.create_folder(name, &dir).expect("create child folder");
            fill_deep_folder(repo, name, &root, &dir, &[], p * (1 + DEEP_CHILDREN) + c + 1);
        }
    }
}

/// Saves the 5 requests of one deep folder and writes its `_order.yml`.
fn fill_deep_folder(
    repo: &FsCollectionRepo,
    name: &str,
    root: &Path,
    dir: &str,
    subfolders: &[String],
    seed: usize,
) {
    let mut order = String::new();
    for r in 0..DEEP_REQUESTS {
        let saved = repo
            .save_request(name, &format!("{dir}/r-{r}"), &http_request(seed * DEEP_REQUESTS + r))
            .expect("save request");
        let file = Path::new(&saved)
            .file_name()
            .and_then(|n| n.to_str())
            .expect("saved file name")
            .to_string();
        order.push_str(&format!("- {file}\n"));
    }
    for sub in subfolders {
        order.push_str(&format!("- {sub}\n"));
    }
    std::fs::write(root.join(dir).join("_order.yml"), order).expect("write _order.yml");
}

/// One collection `bench-0` with the deep shape, in a temp dir, verified against its labels.
pub fn generate_deep() -> Fixture {
    let dir = TempDir::new().expect("create temp dir");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    populate_deep(&repo, "bench-0");
    let fixture = Fixture {
        dir,
        repo,
        collection: "bench-0".to_string(),
        collections: 1,
        requests_per_collection: DEEP_FOLDERS * DEEP_REQUESTS,
        http_paths: Vec::new(),
    };
    fixture.verify();
    fixture
}
```

- [ ] **Step 2: Add the deep case to the bench**

In `crates/rocket-infra/benches/collection.rs` change the import line `use support::{generate, generate_collections, Size};` to:

```rust
use support::{generate, generate_collections, generate_deep, Size};
```

Add this function above `bench_list`:

```rust
fn bench_tree_deep(c: &mut Criterion) {
    // 500 folders with 5 requests each: the shape the per-folder savings are measured on.
    let fixture = generate_deep();
    let mut group = c.benchmark_group("collection_tree");
    group.sample_size(10);
    group.bench_with_input(BenchmarkId::new("get_summaries", "deep"), &fixture, |b, fx| {
        b.iter(|| black_box(fx.repo.get_summaries(&fx.collection).expect("summaries")))
    });
    group.finish();
}
```

and change the group macro to:

```rust
criterion_group!(benches, bench_tree, bench_tree_deep, bench_list, bench_get_request, bench_validate_path);
```

- [ ] **Step 3: Check that the bench case runs once**

Run: `cargo bench -j4 -p rocket-infra --bench collection -- --test deep`
Expected: it builds and criterion reports `Testing collection_tree/get_summaries/deep` then `Success`. The fixture builder's own `verify()` asserts 2500 requests.

- [ ] **Step 4: Create the single-load integration test**

Create `crates/rocket-infra/tests/tree_walk_syscalls.rs`:

```rust
//! One clean `get_summaries` load of the deep fixture, for `strace`.
//! Both tests are ignored: run them by hand, see `.claude/perf-tree-walk.md`.

#[path = "../benches/support/mod.rs"]
mod support;

use std::path::PathBuf;

use rocket_collection::CollectionRepository;
use rocket_infra::FsCollectionRepo;

const FIXTURE_ENV: &str = "ROCKET_TREE_WALK_FIXTURE";

fn fixture_repo() -> FsCollectionRepo {
    let base = std::env::var(FIXTURE_ENV).expect("set ROCKET_TREE_WALK_FIXTURE to a folder");
    FsCollectionRepo::new_standalone(PathBuf::from(base))
}

/// Builds the deep collection `bench-0` in the folder named by `ROCKET_TREE_WALK_FIXTURE`.
#[test]
#[ignore = "builds the deep fixture; run by hand, see .claude/perf-tree-walk.md"]
fn build_deep_fixture() {
    support::populate_deep(&fixture_repo(), "bench-0");
}

/// One summaries load of the prebuilt fixture. This is the run that `strace` counts.
#[test]
#[ignore = "loads the deep fixture once; run by hand, see .claude/perf-tree-walk.md"]
fn load_deep_fixture_once() {
    let collection = fixture_repo().get_summaries("bench-0").expect("summaries load");
    assert_eq!(collection.request_count(), support::DEEP_FOLDERS * support::DEEP_REQUESTS);
}
```

- [ ] **Step 5: Check the default test run ignores them and the file compiles**

Run: `cargo test -j4 -p rocket-infra --test tree_walk_syscalls`
Expected: `2 ignored`, 0 failed.

- [ ] **Step 6: Build the fixture and take the informational baseline**

Run from the worktree root:

```bash
export ROCKET_TREE_WALK_FIXTURE="$PWD/.superpowers/tree-walk-fixture"
rm -rf "$ROCKET_TREE_WALK_FIXTURE" && mkdir -p "$ROCKET_TREE_WALK_FIXTURE"
cargo test -j4 -p rocket-infra --test tree_walk_syscalls build_deep_fixture -- --ignored --exact
BIN=$(cargo test -j4 -p rocket-infra --test tree_walk_syscalls --no-run 2>&1 | sed -n 's/.*(\(.*tree_walk_syscalls-[0-9a-f]*\)).*/\1/p' | head -1)
echo "$BIN"
"$BIN" --ignored --exact load_deep_fixture_once
for i in 1 2 3; do strace -f -c -e trace=file -o ".superpowers/strace-info-$i.txt" "$BIN" --ignored --exact load_deep_fixture_once > /dev/null; tail -n 1 ".superpowers/strace-info-$i.txt"; done
cargo bench -j4 -p rocket-infra --bench collection -- deep
```
Expected: the build test passes, the plain load test passes, each `strace` run ends with a `total` row whose fourth column is the call count, and criterion prints a `time: [low mid high]` line for `collection_tree/get_summaries/deep`. `.superpowers/` is gitignored. The `BIN` path is the one cargo prints and is relative to the workspace root. Keep the smallest of the three totals, because the test-binary start-up adds a small constant.

- [ ] **Step 7: Write the measurement document**

Create `.claude/perf-tree-walk.md`. Replace each `<...>` in the table with the real number you measured: the `strace` total, the criterion low, middle and high times. No `<...>` may remain in the file.

````markdown
# Tree walk syscall counts and timing (issue #95)

Measures one `get_summaries` load of the deep fixture: 500 folders, 5 HTTP requests each, an `_order.yml` in every folder (25 parents with 19 children each, built by `populate_deep` in `crates/rocket-infra/benches/support/mod.rs`).

## How to measure

```bash
export ROCKET_TREE_WALK_FIXTURE="$PWD/.superpowers/tree-walk-fixture"
rm -rf "$ROCKET_TREE_WALK_FIXTURE" && mkdir -p "$ROCKET_TREE_WALK_FIXTURE"
cargo test -j4 -p rocket-infra --test tree_walk_syscalls build_deep_fixture -- --ignored --exact
BIN=$(cargo test -j4 -p rocket-infra --test tree_walk_syscalls --no-run 2>&1 | sed -n 's/.*(\(.*tree_walk_syscalls-[0-9a-f]*\)).*/\1/p' | head -1)
strace -f -c -e trace=file -o strace.txt "$BIN" --ignored --exact load_deep_fixture_once
cargo bench -j4 -p rocket-infra --bench collection -- deep
```

Run `strace` three times and keep the smallest `total` row, because start-up of the test binary adds a small constant. Timing comes from criterion (`time: [low mid high]`). For a before and after comparison use `-- --save-baseline before deep` and then `-- --baseline before deep`.

## Informational baseline (before any production change, Linux)

| measure | value |
|---|---|
| `strace` total calls, lowest of 3 | <total> |
| criterion `get_summaries/deep` time | [<low> <mid> <high>] |

This baseline is informational. The gate is the paired before and after run of the second plan, done in one worktree in one sitting.
````

- [ ] **Step 8: Final check**

Run: `cargo test -j4 -p rocket-infra fs_collection && cargo check -j4 -p rocket-infra --all-targets`
Expected: pass, with no warnings from the new files.

Next plan: docs/superpowers/plans/2026-10-15-tree-walk-fewer-stat-calls/plan-02-tree-walk-fewer-stat-calls.md
