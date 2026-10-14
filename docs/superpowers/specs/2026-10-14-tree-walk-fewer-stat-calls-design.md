# Tree walk: fewer filesystem calls per folder and per entry

Source: GitHub issue #95 (perf P3), area `rocket-infra`, file `crates/rocket-infra/src/fs_collection/tree.rs`. Plans: `docs/superpowers/plans/2026-10-14-tree-walk-fewer-stat-calls/`.

## Goal

Cut the number of filesystem calls the recursive sidebar tree load (`build_tree`, used by `build_folder_tree` and `build_folder_tree_summaries`) makes per folder and per entry, without changing any observable behavior. The tree output, the on-disk formats and the IPC contracts stay exactly as they are. The symlink rules stay exactly as they are.

## Decisions

- Symlinked request files stay loadable (option A). Only symlinked directories and symlinked `.js` files are rejected today, and that does not change. Rejecting symlinked request files is a separate ticket.
- Approach: a minimal rewrite of the existing walker. A traversal crate (`walkdir`, `ignore`) was rejected because it adds a dependency and changes traversal order and symlink handling. A metadata cache between loads was rejected as a separate ticket, because it needs invalidation with the file watcher.
- #94 (P2) touches the same file but different code (summary parsing). This change stays in `build_tree` and its helpers, plus two `exists()` probes in `paths.rs`.

## Background (verified against the code)

Per entry the walk calls `path.is_dir()` (one stat that follows links), then `symlink_metadata` for directories and `.js` files. `DirEntry::file_type()` does not follow links and costs no extra stat on Linux, so it replaces both for ordinary entries.

Per folder the walk probes and reads in this order: `folder.yml` exists, then read, then `read_uid_from_yaml` (which probes and reads `opencollection.yml` and `folder.yml` again), then `current.exists()`, then `_order.yml` exists, then `_order.json` as a fallback, then `opencollection.yml` exists again. Each exists-then-read pair can be one read that treats `NotFound` as absent.

The `_order` handling builds the same position map and runs the same sort twice, once in the YAML branch and once in the JSON branch.

## Behavior that must not change

- A symlinked directory is skipped with a warning, whatever its name (`linked`, `dirlink.yml`, `tools.js`). A symlinked `.js` script file is skipped with a warning. A symlinked request file (`.yml`, `.yaml`, `.json`, `.bru`) is still read and listed, because `is_request_file` has no symlink check and `read_to_string` follows the link.
- A dangling symlinked request file keeps today's result: the full load returns an error (it reads with `?`), and the summaries load skips it.
- Hidden-entry rules (`is_hidden_entry`) are unchanged. A nested `flows` folder stays visible, a root `flows` folder stays hidden.
- `_order.yml` wins over `_order.json`. Content is parsed as a YAML string list first, then as a JSON string list. Listed names sort by position, unlisted names sort last by file name, and an unparseable order file falls back to sorting by file name without reading the other file.
- UID rules are unchanged: a non-empty `info.uid` in `folder.yml` wins and `info.name` becomes the folder name. Otherwise `read_uid_from_yaml` decides, including its legacy `.uid` migration and write-back. The collection root takes its uid from `opencollection.yml`.
- The summaries load and the full load produce the same tree as before for the same input.

## Design

1. Entry classification. A non-symlink entry gets its kind from `entry.file_type()` with no extra stat. A symlink, or a failed `file_type()`, falls back to `path.is_dir()`, so a link to a directory is skipped whatever its name. A symlinked directory is skipped with the existing warning. A `.js` file that is a symlink is skipped with the existing warning. If `file_type()` fails, behave as before: a directory is walked, a script file is treated as a symlink. A symlink to a non-script file keeps going through `is_request_file`.
2. Per-folder reads.
   - Read `folder.yml` once with `read_to_string`. If it parses, take the uid and the display name. A missing or unreadable file, an unparseable file and an empty uid all fall back to `read_uid_from_yaml`, which is what happens today.
   - Drop the two `exists()` probes in `read_uid_from_yaml` and read directly. The bodies stay the same.
   - Drop `current.exists()`. `read_dir` reporting `NotFound` returns the empty folder as before. Other errors still propagate.
   - Detect the collection root (`opencollection.yml`) from the entry list already in hand instead of a second `exists()`.
3. One order helper. `read_order_list(dir)` reads `_order.yml`, and reads `_order.json` only when the YAML file is absent (`NotFound`). An unreadable `_order.yml` gives no order and does not fall through to the JSON. One `sort_entries` replaces the two duplicated blocks.
4. Known edge. A dangling symlink named `opencollection.yml` would now count as the collection root, because detection uses the listing and not `exists()`, which follows links. This is extremely unlikely and has no security effect.

## Tests and measurement

- Characterization tests are written first, pass on the unchanged code and are not edited by the change. They render the loaded tree as a text outline and compare it with literal expected text, for both the full load and the summaries load.
- Measurement uses a generated fixture of 550 folders: 50 parents with 10 children each, and in each child 5 requests and an `_order.yml`. Count syscalls with `strace -c -f -e trace=file` for one summaries load before and after, and record both in `.claude/perf-tree-walk.md`. Test-binary start-up adds a small constant to both numbers.

## Non-goals

- No change to request-file parsing or to the P2 summary parsing (#94).
- No new symlink rejection for request files.
- No change to `count_request_files`, `path_exists` or other callers of `is_dir`.
- No new dependencies, no caching.

## Acceptance criteria

- Characterization tests pass before and after the change, without edits: symlinked folder skipped (also when named `dirlink.yml` and `tools.js`), symlinked `.js` skipped, symlinked request file still listed, dangling symlink unchanged, `_order.yml` and `_order.json` precedence and fallback, `folder.yml` uid and name, uid fallback to the legacy `.uid`, the root uid from `opencollection.yml`, and root `flows` hiding.
- The tree for a fixture is identical before and after (a test compares the full outline).
- The syscall count for one tree load on the 550-folder fixture drops, with before and after numbers recorded in `.claude/perf-tree-walk.md`. If it does not drop, the doc says so plainly.
- `cargo check -j4` and `cargo test -j4 -p rocket-infra fs_collection` pass. No unwrap calls in production paths.
