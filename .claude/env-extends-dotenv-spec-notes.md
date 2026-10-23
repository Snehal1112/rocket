# Env extends and .env: spec notes for after the implementation

Spec: `docs/superpowers/specs/2026-10-18-env-extends-dotenv-design.md`. Ticket: #22. Plans: `docs/superpowers/plans/2026-10-18-env-extends-dotenv/` (4 plans, 12 tasks).

These items were found when comparing the spec with the `superpowers:brainstorming` skill. Handle them after the four plans have merged. None of them changes what the plans build.

## Gaps against the brainstorming skill

- **Anti-goals.** The spec has non-goals only. Add an `## Anti-goals` section: outcomes that would count as failure even if every check passes. Needs the user's input. Candidate drafts, to confirm or reject:
  - A `.env` secret reaches the Console, history or a saved file unmasked.
  - An untrusted collection makes Rocket read a file it names while the host environment gate is denied.
  - Inheritance silently drops variables with no warning or error.
  - A save writes inherited variables into the child's file.
- **Intent and success.** The Goal and Background only imply why now and what success looks like to the user. Add a short `## Intent` paragraph in the user's words.
- **Playback.** The interview did not play back what was understood in 200 to 300 word chunks. No document change needed. For later interviews, play back each part before moving on.
- **Size statement.** The size ("a project with a written design") was not said out loud during the interview. Process note only.

## Calls to confirm with the user

The spec lists these as `Decided by: call`. Re-check them against the shipped behavior:

- The frontend send uses a new backend command for flattened variables, with no TypeScript merge.
- `get_process_env_vars` takes the active environment names and returns `dotEnvKeys`.
- A renamed or deleted parent is not rewritten, and a cycle from a disk edit fails at send time.
- A global environment's relative `.env` path resolves against the workspace folder through a new `dot_env_base` on the repository.
- The `.env` file is not read when the gate denies access.
- The runner and flows get the `extends` flatten but no `.env` values.
- Only variables are inherited, not external secret bindings or client certificates.

## Choices made in the plans, not in the spec

- A disabled child variable shadows the parent's variable with the same key.
- A collection's `.env` file wins over the global environment's file, and the OS environment wins over both.
- The default `.env` of the workspace base and of the collection folder is read even with no environment selected, under the same gate. These are the lowest layers (workspace, then collection), the environments' explicit `.env` files override them, and the OS environment wins over all.
- A `.env` value shorter than 6 characters stays visible in the Console, because the existing minimum applies.

## Follow-ups already in the spec

- Inherit the parent's external secret bindings and client certificates.
- Show inherited variables, with secrets masked, in the assistant chips and the MCP environment views.
- Let backend-executed HTTP (flows, `rok.runRequest`, assistant `run_request`) resolve `{{process.env}}`.
