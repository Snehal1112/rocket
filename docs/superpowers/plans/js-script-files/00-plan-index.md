# JS Script Files Plan Index

Spec: `docs/superpowers/specs/2026-10-06-js-script-files-design.md`

Plans run one at a time, in order. Each plan has at most three tasks and ends
with working, checked software.

| # | Plan | Delivers |
|---|---|---|
| 01 | `2026-10-06-js-script-files-plan-01-require-engine.md` | `require('./x.js')` works in scripts; roots enforced; `additionalContextRoots` config read and wired from `ExecutionService`. |
| 02 | `2026-10-06-js-script-files-plan-02-script-file-backend.md` | `.js` files appear in the collection tree; create/read/save/rename/delete IPC commands. |
| 03 | `2026-10-06-js-script-files-plan-03-script-file-ui.md` | Sidebar node, "New Script" context menu, script editor tab, docs. |

## Cross-plan facts

- Collections are addressed by directory name (`collection: &str`), not by
  absolute path. Only the repository knows the absolute path, so plan 01 adds
  `CollectionRepository::collection_root_path`.
- Always pass `-j4` to cargo. Never run `cargo test --workspace`; use
  `cargo test -p <crate> <filter>` and `cargo check -j4 --workspace`.
- Commits go through the `dev-workflow-skills:1-git-commit` skill, with
  conventional-commit subjects (`feat:`, `test:`, `docs:`). Stage by path only;
  other sessions share this checkout.
- Every task that touches collection data, the scanner or script config starts
  with: read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Deviations from the spec (decided while planning)

- `require()` of a local file accepts only the `.js` extension. The spec's
  "name as written when it has an extension" would otherwise let a script read
  any text file in the collection through the parse error it triggers.
- The tree walker skips directories named `node_modules`.
- Flow Transform scripts do not go through `ExecutionService::begin_phases`, so
  they get no file scope in v1 and `require('./x')` there throws the "local file
  requires are not available" error. This matches the spec's "once they share
  the engine scope" wording.
