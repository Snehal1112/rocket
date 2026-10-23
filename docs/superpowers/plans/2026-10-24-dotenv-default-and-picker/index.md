# .env default load, picker and editor: plan index

Spec: `docs/superpowers/specs/2026-10-18-env-extends-dotenv-design.md` (the `.env` feature as built). This series amends it. The amendments below are the decisions for this series. They are folded into that spec.

## Why

The `.env file` box in Manage Environments needs a typed path and gives no feedback. Bruno loads `<collection>/.env` with no setup. Users cannot create the file in Rocket.

## Amendments (decided by: user, 2026-10-24)

- A `.env` file in the collection folder is read by default, with no path set. A `.env` in the workspace folder is read by default for global environments. A missing default file is silent.
- The default file is the lowest `.env` layer. Order, lowest first: workspace `.env`, collection `.env`, then the explicit `dotEnvFilePath` layers (parent under child, as today). The real OS environment still wins over all of them.
- The default file is read even when no environment is selected, because Bruno loads it regardless.
- The trust gate is unchanged: a denied gate reads no file, default or explicit.
- The dialog shows a status line for the file and offers a file picker. A path inside the base folder is stored relative; a path outside is stored absolute (`..` stays refused).
- The status command never returns values, only the resolved path, whether it exists and a key count.

## Plans

| Plan | Content | Status |
|---|---|---|
| [plan-01-default-load.md](plan-01-default-load.md) | Default `.env` read on the backend (2 tasks) | done |
| [plan-02-status-and-picker.md](plan-02-status-and-picker.md) | Status command, status line, file picker (3 tasks) | done |
| plan-03-create-and-edit | Create / edit the `.env` file in the app (Bruno v3.1 style) | outline only, detail after plans 1 and 2 ship |

Run plan 1, then plan 2. Plan 3 is written in detail after both are merged, so it can use the status command and the real dialog layout.

## Plan 3 outline (not yet a plan)

- A button in the dialog that creates `<base>/.env` when missing and opens it.
- Open questions to settle in the plan 3 brainstorm: edit inside the dialog (key list with masked values) or open in the Monaco editor tab; writing the file from the backend (`rocket-infra`, atomic write, `.gitignore` hint); whether to add `.env` to the collection's `.gitignore` automatically.
- It must keep the rule that `.env` secrets never reach the Console, history or a saved file unmasked.
