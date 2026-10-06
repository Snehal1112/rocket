# Script files

Plain `.js` files in a collection that scripts load with `require('./x.js')`.
Design: `docs/superpowers/specs/2026-10-06-js-script-files-design.md`.

- Resolution lives in `crates/rocket-infra/src/scripting/local_modules.rs`. The op is
  `op_require_local`; the JS loader is in `scripting/bootstrap.js`.
- Allowed roots: the collection directory (Safe mode) plus `additionalContextRoots`
  (Developer mode only). Paths are canonicalised, so `..` and symlinks cannot escape.
  The specifier is normalised lexically and checked against the roots before any disk
  access (no existence oracle), then canonicalised and re-checked (symlinks).
- Only `.js` files load. `package.json` `main`, `index.js` and `.json` are not supported.
- `additionalContextRoots` is stored in `opencollection.yml` at
  `extensions.rocketapi.scripts.additionalContextRoots`. There is no settings UI yet.
  The TS `CollectionSettings.scriptContextRoots` carries it, and
  `buildSettingsForSave` (`src/lib/collection-settings-save.ts`) keeps it on UI saves.
- Files show in the tree as `CollectionItem::ScriptFile`. CRUD commands are
  `create/read/save/rename/delete_script_file`. The file watcher reports changes.
- Sidebar delete closes affected script tabs by whole path segments
  (`findAffectedTabs` in `tree-utils.ts`) and warns when one has unsaved edits.
- `renameScriptTabs` retargets a renamed file or every script tab under a renamed folder.
- Not covered yet: Flow Transform scripts, OpenCollection `.yml` `ScriptFile` items,
  a delete warning for referenced scripts, open tabs do not reload when the file
  changes on disk.
