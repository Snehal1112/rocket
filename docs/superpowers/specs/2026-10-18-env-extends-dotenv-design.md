# Environment extends and .env loading design

Ticket: https://github.com/Snehal1112/rocket/issues/22

## Goal

Make the stored `extends` and `dotEnvFilePath` fields on an environment do what the OpenCollection spec says: a child environment inherits its parent's variables, and the `.env` files are readable as `{{process.env.NAME}}`. Both get a minimal UI, so a user can set them without editing files.

## Background

`Environment.extends` and `Environment.dot_env_file_path` (`crates/rocket-environment/src/environment.rs`) round-trip through save and load, but no resolution logic reads them. `crates/rocket-environment/CLAUDE.md` wrongly says inheritance and `.env` loading happen upstream. Bruno merges the parent chain before interpolation, keeps the merged result off disk, and exposes `.env` values as `process.env`. This design follows Bruno.

## Non-goals

- Merging `.env` values into the plain variable scope.
- Extending an environment from another store (collection to global or the reverse).
- Showing inherited variables in the assistant chips or MCP environment views (see Follow-ups).
- Making backend-executed HTTP (flows, `rok.runRequest`, assistant `run_request`) resolve `{{process.env}}`. That limit in `.claude/collection-trust.md` stays.

## Design

### Extends

- A pure function in `rocket-environment` flattens an environment with its parent chain. The caller passes a parent lookup. The function does no I/O.
- Layers run from the furthest ancestor to the child. The child's variables win. Flattening merges raw values and happens before `{{var}}` interpolation, so a parent value may reference a child variable.
- The parent is looked up by name in the same store as the child, collection or global.
- A cycle or a self-reference is rejected on save by `EnvironmentService` and fails the send with an error naming the environments in the cycle. A missing parent only warns, and the child's own variables are used.
- The flattened result is never persisted. Saving an environment writes only its own variables and the `extends` name.
- `rocket-app` applies the function wherever an environment becomes a variable map (request execution, OAuth2, flows, the collection runner, gRPC, WebSocket, GraphQL subscriptions), through one shared call.

### .env files

- `rocket-infra` reads and parses the file named by `dotEnvFilePath`. Parsing uses the `dotenvy` crate, which is a call (see Decisions).
- A relative path resolves against the collection folder. For a global environment it resolves against the workspace folder. Absolute and `~/` paths are used as written. A path with `..` is refused.
- A missing or unreadable file warns only. The send is not blocked.
- Layering applies between files: the parent's file sits under the child's file, and the child's file wins.
- The values appear as `{{process.env.NAME}}`, only when the host environment gate allows it for the collection. The real OS environment wins on a name clash with a `.env` value.
- The active environment is needed to find the file, so the process env lookup takes the active environment name as well as the collection.
- The Console masks every `.env` value as a secret, fail closed, with the existing reveal toggle. No `.env` value is written to history or to a saved file.
- Default file (amendment): a `.env` in the collection folder is read with no path set, and a `.env` in the workspace folder is read for global environments. A missing default file is silent. It is read even when no environment is selected. A denied gate reads no default file either.
- Layer order, lowest first: workspace `.env`, collection `.env`, then the explicit `dotEnvFilePath` layers (parent under child). The real OS environment still wins over all of them.

### UI

- The environment dialog gets an "Inherits from" picker (single parent, never itself, never a cycle) and a `.env` path field.
- Inherited rows are shown read-only and labelled with their source environment. A missing parent shows a warning and does not block the dialog.
- Saving never writes inherited rows into the child.
- Status and picker (amendment): a status line under the `.env` field shows the resolved path, whether the file exists and a key count, never a value. It describes the saved path, so an unsaved path shows "Save to check this file." A denied gate shows "not read". Parse errors never quote file text. The picker stores a path inside the base folder as relative and a path outside as absolute (`..` stays refused). It is disabled until the status has loaded, so the base folder is known.

### Docs

- Correct `crates/rocket-environment/CLAUDE.md` and add the behaviour to the spec reference notes and `.claude/collection-trust.md`.

## Constraints

- The persistence structs keep `#[serde(default, skip_serializing_if = ...)]` and gain no camelCase rename. New IPC DTOs use camelCase.
- No `unwrap()` in production paths. No `git` CLI.
- UI uses shadcn/ui primitives and lucide-react icons only.

## Decisions

- Where does the merge happen? Options: a pure function in `rocket-environment` called from `rocket-app` where an environment becomes a variable map; inside `EnvironmentRepository::get` in `rocket-infra`; separately in each consumer. Chosen: the pure function in `rocket-environment`, with parent lookup and parsed `.env` text passed in by the caller. Decided by: user
- Precedence when variables collide? Options: per-environment layers (parent .env, parent variables, child .env, child variables); all .env files as one base layer; only the child's .env. Chosen: per-environment layers, lowest to highest as listed, repeated down a chain of any depth. Decided by: user
- Where does `extends` look for the parent, and what if it cannot? Options: same store, cycle fails the send and missing parent only warns; missing parent also fails the send; ignore both silently. Chosen: same store (collection or global) as the child, a cycle or self-reference is rejected on save and fails the send with an error naming it, a missing parent only warns and the child's own variables are used. Matches Bruno. Decided by: user
- Where are `.env` values exposed? Options: as `{{process.env.NAME}}` like Bruno; merged into the plain variable scope as the lowest layer; both. Chosen: `{{process.env.NAME}}` only, under the existing host environment trust gate. This amends the precedence decision: the layering (parent file under child file, child file wins) applies between `.env` files, and plain variables follow the existing environment inheritance only. Decided by: user
- How do `.env` values combine with the real OS environment, and where does the path resolve? Options: OS environment wins, relative path against the collection folder, missing file only warns; `.env` wins over the OS environment; missing file fails the send. Chosen: the real OS environment wins on a name clash; a relative `dotEnvFilePath` resolves against the collection folder (a global environment's against the workspace folder), absolute and `~/` paths are used as written and `..` is refused; a missing file only warns. Decided by: user
- How much does this spec build? Options: backend plus minimal UI; backend only; backend, UI and the assistant/MCP views. Chosen: backend behaviour plus a minimal UI (an "Inherits from" picker, a `.env` path field, inherited rows read-only with their source). The assistant and MCP views become a follow-up. Decided by: user
- Does the Console mask `.env` values? Options: every `.env` value masked as a secret; only values marked secret; names that look sensitive. Chosen: every `.env` value is masked as a secret, fail closed, with the existing reveal toggle. Decided by: user
- Which crate parses `.env` files? Options: the `dotenvy` crate; a small hand-written parser. Chosen: `dotenvy` (its parse API, without loading into the process environment). Decided by: call
- Do inherited environments reach the assistant and MCP views in this spec? Chosen: no, filed as a follow-up. Decided by: call
- How does the frontend HTTP send get flattened variables? Options: a backend command returns the flattened environment; the frontend re-implements the merge; the send moves to a backend-resolved map. Chosen: a new backend command returns the flattened variables, with each variable's source environment and secret flag, and `variable-context.ts` and `execute-request.ts` use it under its own query key. The raw environment queries stay for the editor. No TypeScript merge. Decided by: call
- How do the active environment name and the `.env` marks reach the process env lookup? Chosen: `get_process_env_vars` gains an optional active environment name argument sent by the frontend. The backend protocols pass the environment name they already resolve. The result also lists which keys came from `.env`, and the frontend masks those values in the Console. Decided by: call
- What happens when a parent is renamed, deleted or edited on disk? Chosen: nothing is rewritten. The child warns on a missing parent, and a cycle that arrives by a disk edit or a pull fails at send time. Decided by: call
- What does a global environment's relative `.env` path resolve against when no workspace folder is reachable? Chosen: that path is not resolved and warns. Collection environments are unaffected. No gitignore handling. Decided by: call
- Is the `.env` file read when the host environment gate denies access? Chosen: no, the file is not read at all, so an untrusted collection cannot make Rocket read a file it names. The `..` rule only refuses a literal `..` segment, the same as the client certificate paths. Decided by: call
- Do the collection runner and flows get `.env` values? Chosen: no, they follow the backend-executed rule in the Non-goals, and they still get the `extends` flatten. Decided by: call
- What is inherited? Chosen: variables only, with their secret flag kept, so inherited secrets are masked like any secret. The parent's external secret bindings and client certificates are not inherited here. Decided by: call

## Acceptance criteria

- An environment with `extends: base` resolves `{{var}}` from the parent when the child lacks it, and the child's value wins when both define it.
- A chain of three environments flattens from the furthest ancestor to the child.
- A parent value that references a child variable resolves against the child.
- Saving an environment that extends itself or forms a cycle is rejected with an error that names the environments.
- A cycle that reaches a send fails the send with an error naming the cycle.
- A missing parent warns, and the child's own variables are used.
- Saving an environment never writes inherited variables into its file.
- A `dotEnvFilePath` file in the collection folder is exposed as `{{process.env.NAME}}` when the host environment gate allows it, and withheld when the gate denies it.
- A parent's `.env` file is layered under the child's, and the child's value wins.
- The real OS environment wins over a `.env` value with the same name.
- A relative path resolves against the collection folder, a path with `..` is refused, and a missing file only warns.
- Every `.env` value is masked in the Console and the reveal toggle shows it.
- The environment dialog sets "Inherits from" and the `.env` path, shows inherited rows read-only with their source, and offers no self or cyclic parent.
- `crates/rocket-environment/CLAUDE.md` no longer claims upstream handling, and describes the real behaviour.
- The HTTP send uses the flattened variables from the backend command, so an inherited variable resolves in a real send.
- A denied host environment gate means the `.env` file is not read.
- Inherited secret variables keep their secret flag and are masked in the Console.
- `cargo check -j4`, the targeted crate tests, `yarn tsc --noEmit` and `yarn check` pass.

## Follow-ups

- Inherit the parent's external secret bindings and client certificates.
- Show inherited variables, with secrets masked, in the assistant chips and the MCP environment read views.
- Let backend-executed HTTP (flows, `rok.runRequest`, assistant `run_request`) resolve `{{process.env}}` so `.env` values work there.
