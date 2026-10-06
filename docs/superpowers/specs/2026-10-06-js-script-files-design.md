# JS Script Files Design

Date: 2026-10-06

## Goal

Let users keep reusable JavaScript in plain `.js` files inside a collection and
load them from any script tab with `require('./utils.js')`. The behaviour
follows Bruno's script-file support. Collections can also share scripts from
extra directories through `additionalContextRoots`.

Success criteria:

- A user can create, edit, rename and delete `.js` files from the sidebar.
- Pre-request, post-response and Tests scripts (and Flow Transform scripts, once
  they share the engine scope) can `require()` those files and use their
  `module.exports`.
- A script cannot read files outside the allowed roots, including through `..`
  or symlinks.

## Decisions Made

- Storage: plain `.js` files in the collection directory, next to request
  `.yml` files. They are git-tracked like any other collection file.
- The OpenCollection `.yml` `ScriptFile` item (`type: "script"`) is out of scope.
  It stays skipped by the scanner. A follow-up can add resolution for it.
- `additionalContextRoots` works in Developer sandbox mode only.
- No warning when deleting a script that other scripts reference.

## Bruno Reference Findings

From Bruno source (HEAD `b1dbf83`) and its docs:

- Relative `require` resolves from the requiring file's directory. The top-level
  script starts at the collection root.
- Resolution is synchronous. A per-run cache keys modules by resolved path, and
  circular requires receive partial exports.
- Allowed roots are the collection plus `additionalContextRoots`. Bruno's
  developer-mode check is lexical and has no symlink handling. Rocket uses
  `realpath` instead.
- Bruno's source has no sidebar item type for `.js` files. Its docs describe a
  "New Script" action. Rocket builds the sidebar flow from the docs.

## Section 1: Resolution and Security

### Context

`rocket-scripting` gains a plain-data type and a field on `ScriptContext`:

```rust
pub struct ScriptFileScope {
    pub collection_root: PathBuf,
    pub additional_roots: Vec<PathBuf>,
}
```

`ScriptContext` carries `Option<ScriptFileScope>`. `None` means local file
requires are disabled and throw a clear error. The existing `sandbox_mode`
field decides whether `additional_roots` is honoured.

The crate stays free of I/O. Only `rocket-infra` reads files.

### Require behaviour

- `./x`, `../x` and absolute paths are local files. Bare names keep the existing
  vendored lookup in `op_require_module` and are unchanged.
- The top-level script resolves from `collection_root`. A loaded file resolves
  its own requires from its own directory.
- Lookup order: the name as written (when it has an extension), then
  `name + ".js"`. No `package.json` `main` and no `index.js` in this version.
- A new sync op `op_require_local(from_dir, name)` returns the resolved path and
  source. `bootstrap.js` wraps it in the same `new Function("module", "exports",
  "require", ...)` loader, with `__filename` and `__dirname` added for local
  modules.
- Per-run cache keyed by canonical path. The module object is cached before
  execution so circular requires get partial exports. A failed load is evicted.

### Allowed roots

- Safe mode: `collection_root` only.
- Developer mode: `collection_root` plus `additional_roots`.
- Root and target are both canonicalised with `std::fs::canonicalize`, then
  compared with `Path::starts_with`. This blocks `..` and symlink escapes.
- A denied path throws a JS error naming the file and listing the allowed
  roots. A missing file throws `Cannot find module '<name>'`.

### Config

`additionalContextRoots` is read from `opencollection.yml` under
`extensions.rocketapi.scripts.additionalContextRoots`, next to the existing
`extensions.rocketapi.sandboxMode`. Relative entries resolve against the
collection root. Non-string entries are dropped. Reading Bruno's
`extensions.bruno.scripts` form on import is a follow-up.

The persistence side follows the existing pattern in
`fs_collection/settings.rs` (`sandbox_mode_from_extensions`,
`set_sandbox_mode_in_extensions`). Persistence structs get no camelCase rename.
The IPC DTO does.

## Section 2: Files in the Tree, New Script, Editor

### Domain and scanner

- `CollectionItem` (`rocket-collection/src/folder.rs`) gets a `ScriptFile`
  variant with `file_name` and a display name. It has no UID and no metadata.
- `fs_collection/tree.rs` adds a `.js` branch to the tree walker. Both the full
  and the summary loads list script files. Ordering reuses `_order.yml`.
- Skipped: `node_modules`, symlinks, anything outside the base directory (the
  existing rejection rules apply).
- `.yml` `ScriptFile` items stay skipped as today.

### Commands

Thin Tauri commands, routed through a `rocket-app` service trait with the I/O in
`rocket-infra`. Every path goes through `validate_path()`.

| Command | Behaviour |
|---|---|
| `create_script_file` | Takes a name and folder path. Appends `.js` if missing. Rejects path separators, an empty name and an existing file. Writes a short starter template with a `module.exports` example. |
| `read_script_file` | Returns the file text. |
| `save_script_file` | Writes the file atomically. |
| `rename_script_file` | Renames inside the same folder. Rejects collisions. |
| `delete_script_file` | Deletes the file. |

### Frontend

- Right-click on a collection or folder adds "New Script". Script nodes get
  Rename and Delete.
- Script nodes render with a lucide `FileCode` icon. shadcn primitives only.
- Clicking a script opens a tab with a new `script` type in `pane-store`.
- The editor is Monaco with the existing JS IntelliSense and `rok` typings. Save
  is Ctrl+S. There is no Send button.
- The tab shows the require hint `require('./name.js')`.

## Error Handling

- Path escapes, missing files and bad names return stable IPC errors. Production
  paths use `DomainResult` with explicit error mapping, never a panicking call.
- A script that throws while required reports the error through the existing
  script-error path, with the file name in the message.
- A file that cannot be read as UTF-8 is listed but fails on open with a clear
  error.

## Testing

Rust (targeted crates, always `-j4`, no full workspace run):

- Engine: require of a sibling file, a parent directory file, a nested require
  from a subfolder, `module.exports` reassignment, circular requires, cache
  reuse within one run, a missing file, `.js` auto-extension.
- Security: `..` escape, absolute path outside the roots, a symlink inside the
  collection pointing outside, additional roots denied in Safe mode and allowed
  in Developer mode.
- Scanner: `.js` files appear in folders and at the root, `node_modules` and
  symlinks are skipped, ordering follows `_order.yml`, and the summary load
  includes them.
- Commands: create, duplicate-name rejection, traversal rejection, rename
  collision, delete.
- Config: `additionalContextRoots` round-trips and keeps unrelated `extensions`.

Frontend (Vitest): the context menu shows "New Script", a script node renders and
opens a tab, and save calls the command.

Verification before merge: `cargo check -j4`, the targeted crate tests,
`yarn tsc --noEmit`, `yarn check`.

## Out of Scope (Follow-ups)

- Resolution of OpenCollection `.yml` `ScriptFile` items.
- `package.json` `main`, `index.js` and `.json` requires.
- Reading Bruno's `extensions.bruno.scripts` on import.
- Warning on delete when a script is referenced.
- Cross-collection script sharing UI for `additionalContextRoots`.

## Open Items for the Plan

- Where `ScriptFileScope` is built in `execution_service.rs` (the three phases
  and the Flow Transform path) and how the active collection root is obtained.
- Exact `pane-store` tab shape for the `script` type.
- A settings UI for `additionalContextRoots`, or edit-in-yml only for v1.

## Plan Notes

Every task here touches collection data, the scanner or script config, so each
plan task starts with: read
`docs/superpowers/specs/opencollection-spec-reference.md`. Plans are split into
at most three tasks each, per the repo's planning convention.
