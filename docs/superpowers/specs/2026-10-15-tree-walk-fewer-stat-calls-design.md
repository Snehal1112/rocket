# Tree walk: fewer filesystem calls per folder and per entry

Source: GitHub issue #95 (perf P3, impact medium, size S), area `rocket-infra`, file `crates/rocket-infra/src/fs_collection/tree.rs`. The ticket's own evidence came from reading the code at commit `09f882bd` and is re-verified here against the current code.

## Goal

Make the recursive sidebar tree load (`build_tree`, behind `get`, `get_strict` and `get_summaries`) measurably faster on large collections by removing redundant filesystem calls per folder and per entry, without changing any observable behavior: the tree output, the on-disk formats, the IPC contracts and every symlink rule stay exactly as they are.

## Decisions

The interview covered every topic. Each line records the question asked and your answer.

- **Purpose and who benefits.** Question: what outcome do you want from this ticket? Answer: a faster sidebar load, measured. A change with no measurable gain does not merge. Who benefits (written back and confirmed): users with large collections, hundreds of folders, whose sidebar load is dominated by filesystem calls.
- **Constraints.**
  - Question: which platforms must it work on and where is the speedup measured? Answer: Linux and macOS only, measured on Linux with `strace` and criterion.
  - Question: how should the change treat Windows, since Windows code exists in the repo? Answer: keep it portable, no Windows testing. Production code uses only std APIs that exist on every platform, and symlink tests are `cfg(unix)`.
  - New dependencies are excluded (a non-goal, see below).
- **Success checks and how each is measured.**
  - Question: what measured result counts as success? Answer: fewer syscalls and a clear benchmark gain. The `strace` total for one `get_summaries` load drops, and criterion reports an improvement for the deep case outside its noise band (p < 0.05). There is no fixed percentage.
  - Question: which fixture gates the result? Answer: add the ticket's deep fixture (500 folders with 5 requests each, every folder with an `_order.yml`) and gate on it. The existing sizes stay for context.
- **Edge cases and failure behavior.**
  - Question: how strictly should today's quirks be preserved? Answer: exactly, quirks included, pinned by characterization tests written before the change. Any improvement of a quirk is its own ticket.
  - Question: which extra edge cases are pinned? Answer: all four offered: non-UTF-8 file names, a file removed during the walk (documented behavior), an `_order.yml` that is a directory or unreadable, and duplicate or unknown names in the order list.
- **Security and compatibility.**
  - Question: what about symlinked request files? Answer: keep loading them and pin it with a test. Rejecting them is a separate ticket.
  - Question: what about the write side effects of a tree load (the legacy `.uid` migration)? Answer: keep them exactly, at the same moment, and pin the file contents before and after.
- **Scope against related tickets and branches.** Question: how should #95 relate to #94 (the double YAML parse), which edits other functions in the same file? Answer: do #95 now, independent of #94. #92 (the baseline) is covered by the existing criterion benches. No open pull request and no other branch touches `tree.rs` or `paths.rs`.
- **Non-goals.** Question: which candidates stay out? Answer after a recommendation: all four, listed under Non-goals.
- **Alternatives that were rejected, and why.** Question: is the list of rejected alternatives right? Answer: yes. See Rejected alternatives.

## Rejected alternatives

- **The `walkdir` or `ignore` crate.** It adds a dependency and changes traversal order and symlink handling.
- **A metadata cache between loads.** It needs invalidation with the file watcher.
- **A parallel directory walk.** It changes load and ordering.
- **Rejecting symlinked request files in this ticket.** It is a behavior change you ruled out.
- **Listing-aware probes** (use the directory listing to decide which of `folder.yml`, `_order.yml`, `_order.json` and `opencollection.yml` exist). It saves the most calls, but it changes behavior on case-insensitive filesystems: on macOS a file named `Folder.yml` is found by an existence check on `folder.yml` today and would be missed by an exact name match, while a case-insensitive name match would change Linux behavior. It contradicts the preserve-exactly rule.
- **A `FolderMeta` reader module.** Cleaner boundaries, but a larger diff and new types for a size-S ticket, with no extra speed. It can be a follow-up if `tree.rs` grows unwieldy.

## Behavior that must not change

- A symlinked directory is skipped with a warning, whatever its name (`linked`, `dirlink.yml`, `tools.js`). A symlinked `.js` file is skipped with a warning. A symlinked request file (`.yml`, `.yaml`, `.json`, `.bru`) is still read and listed, because `is_request_file` has no symlink check and `read_to_string` follows the link.
- A dangling symlinked request file keeps today's result. By reading the code the full load returns an error (it reads with `?`) and the summaries load skips it. The characterization tests confirm this on the unchanged code, and the code is the truth if the test disagrees.
- Hidden-entry rules (`is_hidden_entry`) are unchanged: a root `flows` folder is hidden, a nested `flows` folder is visible, and dot-entries, `node_modules` and `environments` are hidden.
- `_order.yml` wins over `_order.json`. The content is read as a YAML string list first, then as a JSON string list. Listed names sort by position, duplicates keep the last position, unknown and non-UTF-8 names sort last, then by file name. An unparseable `_order.yml` falls back to name order and does not read `_order.json`. An unreadable `_order.yml`, or one that is a directory, gives no order and does not fall through.
- UID rules are unchanged: a non-empty `info.uid` in `folder.yml` wins and `info.name` becomes the folder name. Otherwise `read_uid_from_yaml` decides, including its legacy `.uid` migration, which writes `folder.yml` or `opencollection.yml` and deletes the `.uid` file at the same moment as today.
- A folder that cannot be listed because it does not exist returns the empty folder, as today.
- The full load and the summaries load produce the same tree as before for the same input.

## Design

Approach: an in-place, ticket-literal rewrite of `build_tree` and its helpers. Files touched: `tree.rs`, `paths.rs` (`read_uid_from_yaml` only), and, for tests and measurement, `benches/support/mod.rs`, `benches/collection.rs`, a new `fs_collection/tree_walk_tests.rs`, a new integration test and `docs/engineering/perf-tree-walk.md`.

1. **Entry classification.** A non-symlink entry takes its kind from `entry.file_type()` with no extra syscall. A symlink, or a failed lookup, falls back to `path.is_dir()`, so a link to a directory is still skipped whatever its name. A symlinked directory is skipped with the existing warning. A `.js` file that is a symlink, or whose lookup failed, is skipped as today. Everything else goes through `is_request_file`, which is a name check with no syscall. If the lookup fails, a directory is still walked and a script is still treated as a symlink.
2. **Per-folder reads.**
   - `folder.yml` is read once with `read_to_string`. A missing file, an unreadable file, an unparseable file and an empty uid all reach the same `read_uid_from_yaml` fallback they reach today.
   - `read_uid_from_yaml` drops its two `exists()` probes and reads directly. Its bodies, including the migration writes, stay as they are.
   - `current.exists()` is dropped. If `read_dir` fails, one `exists()` call decides in that error path only: false returns the empty folder as today, true propagates the error as today.
   - The `opencollection.yml` root probe stays a real stat, because of case-insensitive filesystems, but it runs only when an entry is named `flows`. Its result is used only for that name, so behavior is identical and most folders skip the stat.
3. **One order helper.** `read_order_list(dir)` reads `_order.yml` and falls through to `_order.json` only on `NotFound`. One `sort_entries` replaces the two duplicated blocks.

Expected saving per folder: the root probe in most folders, `current.exists()`, the `folder.yml` probe and the `_order.yml` probe when those files exist, plus one stat per ordinary entry. A folder with no `_order.yml` or `_order.json` still pays two failing opens, so the real gain comes from the measurement, not from this estimate.

## Tests and measurement

- **Characterization tests first.** A new module, `fs_collection/tree_walk_tests.rs`, renders a loaded tree as a text outline and compares it with literal expected text, for the full load and the summaries load. They pass on the unchanged code and the change does not edit them. They cover symlinks (a directory, one named `dirlink.yml`, one named `tools.js`, a `.js` file, a request file, a dangling request file), order (precedence, JSON fallback, unparseable file, a directory as `_order.yml`, duplicate and unknown names, non-UTF-8 names on Unix), folder rules (uid and name, an empty uid with the legacy `.uid` including the file contents before and after the migration write, a folder with only `.uid`, an unparseable `folder.yml`), hidden entries, and a request file removed after the listing, driven through a custom loader closure.
- **One fixture, two uses.** A deep-fixture builder goes into `benches/support`: 500 folders with 5 requests each, every folder with an `_order.yml`. The criterion bench adds a `get_summaries/deep` case. An `#[ignore]` integration test shares the builder through `#[path]` and does one `get_summaries` load, so `strace` counts a single clean load.
- **Paired before and after.** The factory runs each plan as its own job in its own worktree, so the paired run happens in one step of the second plan. In one worktree and one sitting: check out the unchanged `tree.rs` and `paths.rs` from the merge base with `main`, run the deep bench with `--save-baseline before` and `strace -f -c -e trace=file` on the single-load test (three runs, keep the lowest total, because start-up adds a constant), restore the changed files, then run the bench with `--baseline before` and the same `strace`. Record both in `docs/engineering/perf-tree-walk.md`.
- **Plan split.** Plan 1 (pin and fixture): Task 1 characterization tests for symlinks, hidden entries and removal during the walk. Task 2 tests for order, `folder.yml`, uid and the migration. Task 3 the deep fixture, the bench case and the ignored test, with informational baseline numbers. Plan 2 (change and measure): Task 1 entry classification. Task 2 per-folder reads, the lazy root probe and the order helper. Task 3 the paired measurement, the results document and the final verification.

## Non-goals

- No caching, no parallel or threaded walk and no lazy loading of subfolders.
- No change to other `is_dir` callers: `count_request_files`, `path_exists` and the `is_dir` uses in `folders.rs`.
- No new dependencies. std only.
- No change to request-file parsing (`load_yaml_item`, `load_request_summary`), which is #94.
- No new symlink rejection and no removal of the migration writes from the read path.

## Acceptance criteria

- The characterization tests pass before and after the change, with no edits to them. They cover symlinked directories (including `dirlink.yml` and `tools.js`), a symlinked `.js` file, a symlinked request file that is still listed, a dangling symlinked request file, non-UTF-8 names, `_order.yml` and `_order.json` precedence and fallbacks, an `_order.yml` that is a directory, duplicate and unknown order names, `folder.yml` uid and name, the legacy `.uid` fallback and its migration writes, hidden entries and `flows`, and a request file removed during the walk.
- The full outline of the fixture tree is identical before and after the change, for the full load and the summaries load.
- The `strace` total for one `get_summaries` load of the deep fixture (500 folders, 5 requests each, an `_order.yml` per folder) is lower after the change than before, with both numbers recorded in `docs/engineering/perf-tree-walk.md`.
- The criterion `get_summaries/deep` bench reports an improvement outside its noise band (p < 0.05) against the saved baseline, recorded in the same document. If there is no gain, the document says so plainly and the ticket does not merge.
- Production code uses only std APIs that exist on Linux, macOS and Windows, adds no dependency, and has no unwrap calls.
- `cargo check -j4` and `cargo test -j4 -p rocket-infra fs_collection` pass.
