# Tree walk fewer stat calls: change and measure Implementation Plan

> **For agentic workers:** This plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file. The driver commits, so there are no commit steps. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make `build_tree` call the filesystem less per folder and per entry with an identical tree output, then prove the gain with a paired before and after measurement.

**Architecture:** Plan 1 added characterization tests in `fs_collection/tree_walk_tests.rs` and a deep fixture with a bench case and a single-load test. This plan changes only `tree.rs` and `read_uid_from_yaml` in `paths.rs`: entries are classified with `DirEntry::file_type()`, `folder.yml` and the order files are read once, one order helper replaces the duplicated sort, and the `opencollection.yml` probe runs only for an entry named `flows`. Task 3 runs the paired measurement in one worktree and records it.

**Tech Stack:** Rust, `rocket-infra`, `criterion`, `strace` (Linux).

**Spec:** `docs/superpowers/specs/2026-10-15-tree-walk-fewer-stat-calls-design.md`

## Global Constraints

- Behavior, on-disk formats and IPC contracts must not change. The Plan 1 characterization tests are the proof: do not edit their expectations. Only the import line named in Task 2 may change.
- Linux and macOS only, measured on Linux. Production code uses only std APIs that exist on every platform.
- No new dependencies. No unwrap calls in production paths.
- Do not touch `load_yaml_item`, `load_request_summary` or any request-file parsing (that is #94).
- Do not change other `is_dir` callers: `count_request_files`, `path_exists`, `folders.rs`.
- The `opencollection.yml` root probe stays a real stat (case-insensitive filesystems), it just runs less often. Never replace it with a name match on the directory listing.
- Cargo: always pass `-j4`. Use targeted `-p rocket-infra` runs. Never `cargo test --workspace` or `--all`.
- Success means a lower `strace` total and a criterion improvement outside its noise band (p < 0.05) on `get_summaries/deep`. If the numbers show no gain, say so plainly in the document and report `BLOCKED:` with the numbers.

## Review Focus

- A symlink that points at a directory is still skipped whatever its name (`linked`, `dirlink.yml`, `tools.js`), pinned by the Plan 1 tests (Task 1).
- If `entry.file_type()` fails, a directory is still walked and a script file is treated as a symlink (Task 1).
- A missing folder still returns the empty folder, and a listable-but-unstat-able folder still errors (Task 2, pinned by `walk_of_a_missing_folder_is_the_empty_folder`).
- A root `flows` folder is hidden and a nested `flows` folder is visible although the root probe is now lazy (Task 2, pinned by the hidden-entry test).
- An unreadable or directory `_order.yml` gives no order and never falls through to `_order.json` (Task 2, `read_order_list` tests).
- The migration writes of `read_uid_from_yaml` still happen at the same moment (Task 2, pinned by the legacy `.uid` test).
- The measurement swaps production files and must restore them byte for byte (Task 3).

---

### Task 1: Classify entries with `file_type()`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection/tree.rs` (the entry loop in `build_tree`)

**Interfaces:**
- Consumes: the Plan 1 pins in `tree_walk_tests.rs`, which must pass unedited.

- [ ] **Step 1: Confirm the pins are green before the change**

Run: `cargo test -j4 -p rocket-infra tree_walk_tests && cargo test -j4 -p rocket-infra fs_collection`
Expected: all pass. If a pin already fails, stop and report `BLOCKED:`; do not change the tests.

- [ ] **Step 2: Replace the classification branches**

In `build_tree` in `crates/rocket-infra/src/fs_collection/tree.rs`, replace the code from `if path.is_dir() {` through the end of the `else if is_script_file(&path) { ... }` block (the lines right before `} else if is_request_file(&path) {`) with:

```rust
        // `file_type()` does not follow links and costs no stat on Linux and macOS. A symlink or a
        // failed lookup falls back to `path.is_dir()`, so a link to a directory is still found
        // whatever its name.
        let file_type = entry.file_type().ok();
        let is_symlink = file_type.map(|t| t.is_symlink()).unwrap_or(false);
        let is_dir = match file_type {
            Some(t) if !t.is_symlink() => t.is_dir(),
            _ => path.is_dir(),
        };
        if is_dir {
            // Skip symlinked directories to prevent exfiltration.
            if is_symlink {
                tracing::warn!(path = %path.display(), "skipping symlinked directory in folder tree");
                continue;
            }
            folder.add_subfolder(build_tree(&path, load_item)?);
        } else if is_script_file(&path) {
            // Symlinked scripts are skipped, as symlinked directories are above.
            // A failed lookup counts as a symlink, as before.
            if file_type.map(|t| t.is_symlink()).unwrap_or(true) {
                tracing::warn!(path = %path.display(), "skipping symlinked script file");
                continue;
            }
            folder
                .items
                .push(CollectionItem::ScriptFile(ScriptFileItem::new(&entry_name)));
```

The next line in the file stays `} else if is_request_file(&path) {` and its body is unchanged. Check that no `symlink_metadata` call is left in `build_tree`.

- [ ] **Step 3: Run the pins and the existing tests**

Run: `cargo test -j4 -p rocket-infra tree_walk_tests && cargo test -j4 -p rocket-infra fs_collection`
Expected: all pass, with no test edited.

- [ ] **Step 4: Check**

Run: `cargo check -j4 -p rocket-infra`
Expected: no errors and no new warnings.

---

### Task 2: Per-folder reads, lazy root probe and the order helper

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection/tree.rs` (top of `build_tree`, the listing and order block, the loop head, two new functions)
- Modify: `crates/rocket-infra/src/fs_collection/paths.rs` (`read_uid_from_yaml`)
- Modify: `crates/rocket-infra/src/fs_collection/tree_walk_tests.rs` (one import line, then append)

**Interfaces:**
- Produces: `pub(super) fn read_order_list(dir: &Path) -> Option<Vec<String>>` and `fn sort_entries(entries: &mut [fs::DirEntry], order: Option<Vec<String>>)` in `tree.rs`.

- [ ] **Step 1: Write the helper tests (they fail to compile before the helper exists)**

In `tree_walk_tests.rs` change the import line `use super::tree::build_tree;` to:

```rust
use super::tree::{build_tree, read_order_list};
```

and append:

```rust
#[test]
fn read_order_list_prefers_yml_and_falls_back_to_json_only_when_yml_is_absent() {
    let (dir, _repo) = setup();
    write(&dir, "d/_order.yml", "- a.yml\n- b.yml\n");
    write(&dir, "d/_order.json", "[\"z.yml\"]");
    assert_eq!(
        read_order_list(&dir.path().join("d")),
        Some(vec!["a.yml".to_string(), "b.yml".to_string()])
    );
    write(&dir, "j/_order.json", "[\"z.yml\", \"y.yml\"]");
    assert_eq!(
        read_order_list(&dir.path().join("j")),
        Some(vec!["z.yml".to_string(), "y.yml".to_string()])
    );
}

#[test]
fn read_order_list_is_none_for_garbage_a_directory_or_no_file() {
    let (dir, _repo) = setup();
    write(&dir, "g/_order.yml", "{{{ not a list");
    write(&dir, "g/_order.json", "[\"z.yml\"]");
    assert_eq!(read_order_list(&dir.path().join("g")), None);
    fs::create_dir_all(dir.path().join("dd/_order.yml")).expect("order dir");
    write(&dir, "dd/_order.json", "[\"z.yml\"]");
    assert_eq!(read_order_list(&dir.path().join("dd")), None);
    write(&dir, "e/other.yml", "x");
    assert_eq!(read_order_list(&dir.path().join("e")), None);
}
```

Run: `cargo test -j4 -p rocket-infra tree_walk_tests::read_order_list`
Expected: FAIL to compile, `read_order_list` is not found in `tree`.

- [ ] **Step 2: Add the helpers to `tree.rs`**

Add `use std::collections::HashMap;` to the imports at the top of `tree.rs`. Add these two functions below `build_tree`:

```rust
/// Reads the explicit order from `_order.yml`, or from `_order.json` when the YAML file is absent.
/// The content is read as a YAML list first and then as a JSON list. `None` means no usable order:
/// an unreadable `_order.yml`, or one that is a directory, does not fall through to the JSON file.
pub(super) fn read_order_list(dir: &Path) -> Option<Vec<String>> {
    let content = match fs::read_to_string(dir.join("_order.yml")) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::read_to_string(dir.join("_order.json")).ok()?
        }
        Err(_) => return None,
    };
    serde_yaml::from_str::<Vec<String>>(&content)
        .ok()
        .or_else(|| serde_json::from_str::<Vec<String>>(&content).ok())
}

/// Sorts by position in `order`, then by file name. A duplicate keeps its last position, and unknown
/// or non-UTF-8 names come last. Without an order the entries sort by file name.
fn sort_entries(entries: &mut [fs::DirEntry], order: Option<Vec<String>>) {
    let Some(ordered) = order else {
        entries.sort_by_key(|e| e.file_name());
        return;
    };
    let pos: HashMap<String, usize> = ordered
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, i))
        .collect();
    let rank = |e: &fs::DirEntry| {
        e.file_name()
            .to_str()
            .and_then(|n| pos.get(n))
            .copied()
            .unwrap_or(usize::MAX)
    };
    entries.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });
}
```

- [ ] **Step 3: Read `folder.yml` once**

In `build_tree`, replace the block from the comment `// Parse folder.yml once to extract both uid and display name.` through the line `folder.dir_name = Some(dir_name);` (the whole `if folder_yml.exists() { ... } else { ... }`) with:

```rust
    // Read folder.yml once for both the uid and the display name. The collection root has no
    // folder.yml, and a missing, unreadable, unparseable or uid-less file all fall back to
    // read_uid_from_yaml, which reads opencollection.yml and the legacy .uid file.
    if let Ok(content) = fs::read_to_string(current.join("folder.yml")) {
        if let Ok(oc_folder) = parse_folder_yml(&content) {
            if let Some(ref uid) = oc_folder.info.uid {
                if !uid.is_empty() {
                    folder.uid = uid.clone();
                }
            }
            folder.name = oc_folder.info.name;
        }
    }
    if folder.uid.is_empty() {
        folder.uid = read_uid_from_yaml(current);
    }
    folder.dir_name = Some(dir_name);
```

- [ ] **Step 4: Replace the listing and the order block**

In `build_tree`, replace everything from the line `if !current.exists() {` through the closing `}` of the final `else { entries.sort_by_key(|e| e.file_name()); }` block (the lines right before the comment `// Only the collection root holds Rocket's flow files`) with:

```rust
    let read_dir = match fs::read_dir(current) {
        Ok(read_dir) => read_dir,
        // A folder that is gone is the empty folder. Any other listing failure is still an error.
        Err(_) if !current.exists() => return Ok(folder),
        Err(e) => return Err(e.into()),
    };
    let mut entries: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
    // Apply explicit order from _order.yml (or _order.json for backward compat).
    sort_entries(&mut entries, read_order_list(current));
```

- [ ] **Step 5: Make the root probe lazy**

In `build_tree`, find these two lines just above `for entry in entries {`:

```rust
    // Only the collection root holds Rocket's flow files, so a nested `flows` folder stays visible.
    let is_collection_root = current.join("opencollection.yml").exists();
```

and replace them with:

```rust
    // Only the collection root holds Rocket's flow files, so a nested `flows` folder stays visible.
    // The probe is a real stat, so a case-insensitive filesystem behaves as before, and its result
    // matters only for an entry named `flows`, so it runs only then.
    let is_collection_root = || current.join("opencollection.yml").exists();
```

Then, inside the loop, replace the line `if is_hidden_entry(&entry_name, is_collection_root) {` with:

```rust
        if is_hidden_entry(&entry_name, entry_name == "flows" && is_collection_root()) {
```

- [ ] **Step 6: Drop the two `exists()` probes in `read_uid_from_yaml`**

In `crates/rocket-infra/src/fs_collection/paths.rs` replace the whole function `read_uid_from_yaml` (from its doc comment to its closing brace) with:

```rust
/// Read UID from YAML metadata (opencollection.yml or folder.yml).
/// Falls back to legacy .uid file, migrating the value into YAML.
pub(super) fn read_uid_from_yaml(dir: &Path) -> String {
    // Try opencollection.yml first (collection root). A missing file is a failed read.
    let oc_path = dir.join("opencollection.yml");
    if let Ok(content) = fs::read_to_string(&oc_path) {
        if let Ok(mut oc) = serde_yaml::from_str::<OcCollection>(&content) {
            if let Some(ref uid) = oc.uid {
                if !uid.is_empty() {
                    return uid.clone();
                }
            }
            // No UID in YAML — check legacy .uid file.
            let uid = read_legacy_uid(dir);
            oc.uid = Some(uid.clone());
            if let Ok(yaml) = serde_yaml::to_string(&oc) {
                if atomic_write(&oc_path, yaml.as_bytes()).is_ok() {
                    cleanup_legacy_uid(dir);
                }
            }
            return uid;
        }
    }

    // Try folder.yml (subfolder).
    let folder_path = dir.join("folder.yml");
    if let Ok(content) = fs::read_to_string(&folder_path) {
        if let Ok(mut oc_folder) = parse_folder_yml(&content) {
            if let Some(ref uid) = oc_folder.info.uid {
                if !uid.is_empty() {
                    return uid.clone();
                }
            }
            let uid = read_legacy_uid(dir);
            oc_folder.info.uid = Some(uid.clone());
            if write_folder_yml(&folder_path, &oc_folder).is_ok() {
                cleanup_legacy_uid(dir);
            }
            return uid;
        }
    }

    // No YAML metadata at all — use legacy .uid.
    read_legacy_uid(dir)
}
```

- [ ] **Step 7: Run everything**

Run: `cargo test -j4 -p rocket-infra tree_walk_tests && cargo test -j4 -p rocket-infra fs_collection && cargo check -j4 -p rocket-infra`
Expected: all pass, the two new helper tests included, no Plan 1 test edited, no new warnings.

---

### Task 3: Paired measurement, results document and final check

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `.claude/perf-tree-walk.md` (append the result)

**Interfaces:**
- Consumes: the bench case `collection_tree/get_summaries/deep` and the ignored tests `build_deep_fixture` and `load_deep_fixture_once` from Plan 1, and the changes from Tasks 1 and 2.

All commands run from the worktree root. The paired run swaps two production files for their pre-change versions and restores them, so it must finish with the files identical to your own versions.

- [ ] **Step 1: Save your versions and put the pre-change versions in place**

```bash
mkdir -p .superpowers/after
cp crates/rocket-infra/src/fs_collection/tree.rs crates/rocket-infra/src/fs_collection/paths.rs .superpowers/after/
BASE=$(git merge-base HEAD main); echo "$BASE"
git show "$BASE:crates/rocket-infra/src/fs_collection/tree.rs" > crates/rocket-infra/src/fs_collection/tree.rs
git show "$BASE:crates/rocket-infra/src/fs_collection/paths.rs" > crates/rocket-infra/src/fs_collection/paths.rs
git diff --stat -- crates/rocket-infra/src/fs_collection/tree.rs crates/rocket-infra/src/fs_collection/paths.rs
```
Expected: `BASE` prints a sha, and the diff stat shows the two files differing from HEAD only if the driver already committed your Task 1 and 2 changes (that is fine). The two files now hold the pre-change code.

- [ ] **Step 2: Measure before**

```bash
export ROCKET_TREE_WALK_FIXTURE="$PWD/.superpowers/tree-walk-fixture"
rm -rf "$ROCKET_TREE_WALK_FIXTURE" && mkdir -p "$ROCKET_TREE_WALK_FIXTURE"
cargo test -j4 -p rocket-infra --test tree_walk_syscalls build_deep_fixture -- --ignored --exact
BIN=$(cargo test -j4 -p rocket-infra --test tree_walk_syscalls --no-run 2>&1 | sed -n 's/.*(\(.*tree_walk_syscalls-[0-9a-f]*\)).*/\1/p' | head -1)
for i in 1 2 3; do strace -f -c -e trace=file -o ".superpowers/strace-before-$i.txt" "$BIN" --ignored --exact load_deep_fixture_once > /dev/null; tail -n 1 ".superpowers/strace-before-$i.txt"; done
cargo bench -j4 -p rocket-infra --bench collection -- --save-baseline before deep
```
Expected: three `total` rows, and a criterion `time: [low mid high]` line for `collection_tree/get_summaries/deep`.

- [ ] **Step 3: Restore your versions and verify them byte for byte**

```bash
cp .superpowers/after/tree.rs crates/rocket-infra/src/fs_collection/tree.rs
cp .superpowers/after/paths.rs crates/rocket-infra/src/fs_collection/paths.rs
diff -q .superpowers/after/tree.rs crates/rocket-infra/src/fs_collection/tree.rs && diff -q .superpowers/after/paths.rs crates/rocket-infra/src/fs_collection/paths.rs && echo restored
```
Expected: `restored`.

- [ ] **Step 4: Measure after**

```bash
BIN=$(cargo test -j4 -p rocket-infra --test tree_walk_syscalls --no-run 2>&1 | sed -n 's/.*(\(.*tree_walk_syscalls-[0-9a-f]*\)).*/\1/p' | head -1)
for i in 1 2 3; do strace -f -c -e trace=file -o ".superpowers/strace-after-$i.txt" "$BIN" --ignored --exact load_deep_fixture_once > /dev/null; tail -n 1 ".superpowers/strace-after-$i.txt"; done
cargo bench -j4 -p rocket-infra --bench collection -- --baseline before deep
```
Expected: three `total` rows, and criterion prints a `change: [...] (p = ... < 0.05)` line with `Performance has improved.` when the gain is real.

- [ ] **Step 5: Take the lowest totals**

```bash
for p in before after; do echo -n "$p: "; for f in .superpowers/strace-$p-*.txt; do tail -n 1 "$f" | awk '{print $4}'; done | sort -n | head -1; done
```
Expected: two numbers, the lowest `calls` column of the three runs for each side.

- [ ] **Step 6: Record the result**

Append this to `.claude/perf-tree-walk.md`, with the real numbers and the real criterion lines. No `<...>` may remain in the file.

````markdown
## Result of the change (paired run: one worktree, one sitting, Linux)

The "before" run used the `tree.rs` and `paths.rs` of the merge base with `main`, the "after" run used the changed files. Criterion compared the two with `--save-baseline before` and `--baseline before`.

| measure | before | after |
|---|---|---|
| `strace` total calls, lowest of 3 | <before total> | <after total> |
| criterion `get_summaries/deep` middle estimate | <before time> | <after time> |

Criterion change line: `<paste the change: [...] (p = ...) line>` and verdict line: `<paste the verdict line>`.

Gate: the `strace` total dropped and criterion reports an improvement outside its noise band: <yes or no, with the numbers>.
````

If the `strace` total did not drop, or criterion reports no change or a regression, write that plainly in the document instead, and begin your final reply with `BLOCKED:` followed by the numbers. Do not hide a missed gate.

- [ ] **Step 7: Final verification**

```bash
git status --short
cargo test -j4 -p rocket-infra fs_collection
cargo check -j4 -p rocket-infra --all-targets
cargo clippy -j4 -p rocket-infra --all-targets -- -D warnings
```
Expected: `git status` shows only your intended changes (the two production files with your versions, the tests and `.claude/perf-tree-walk.md`), the tests pass, and the check passes. If clippy reports warnings in files you changed, fix them. Warnings in files you did not touch are not yours: list them in your reply and leave them.

Next plan: none
