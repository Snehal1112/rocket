# rok JS API parity, sub-project A: sync gaps

Date: 2026-10-07

## Goal

A script written from Bruno's JavaScript API Reference runs in Rocket with `bru` replaced by `rok`. `req`, `res`, `test` and `expect` keep their names. No `bru` alias is added and imported `bru.*` text is not rewritten (ruling of 2026-10-05).

## Series

| Sub-project | Scope | Status |
|---|---|---|
| A (this spec) | Sync, state-only gaps | Designed |
| B | Async host calls: `sendRequest`, `runRequest`, `sleep`, real `req.onFail` | Not started |
| C | Cookies: `rok.cookies.*` and `jar()` | Not started |
| Deferred | `runner.iterationData` (needs a runner data-file feature), `rok.grpc` (needs the gRPC parity plans) | Out of scope |

## Decisions

- Persistence keeps Rocket semantics. Env and collection writes stay in memory unless `{ persist: true }` is passed. Global env writes keep persisting. `setVar` is runtime-only. New set and delete APIs follow the same rule.
- No new architecture. Each API is a `bootstrap.js` wrapper, an op in `crates/rocket-infra/src/scripting/ops/`, a field on `ScriptInputState` or `ScriptOutputState`, and a typing in `src/components/editor/rok-types.ts`.

## Design

### 1. Reads from data already in context

`VariableContext` already holds the runtime, request, folder, env, collection, global and process-env scopes. Add JS wrappers and read ops for:

- `rok.getAllEnvVars`, `getAllVars`, `getAllGlobalEnvVars`
- `rok.hasVar`, `hasGlobalEnvVar`, `hasCollectionVar`
- `rok.getFolderVar`, `getRequestVar`, `getProcessEnv`

### 2. Writes and deletes

- Runtime: `deleteVar`, `deleteAllVars`.
- Env: `deleteAllEnvVars` (`deleteEnvVar` exists).
- Collection: `deleteCollectionVar`, `deleteAllCollectionVars`.
- Global env: `deleteGlobalEnvVar`, `deleteAllGlobalEnvVars`.

`EnvVarWrite` and `CollectionVarWrite` in `rocket-scripting` gain a delete variant, and a delete-all variant for the bulk calls. The persistence layer applies them after the engine returns, as it does for sets. Backward compatibility: new variants only, existing serialized shapes unchanged.

### 3. New context fields

- `collection_name` for `rok.getCollectionName()`.
- `rok.isSafeMode()` from the existing `sandbox_mode`.
- `rok.cwd()`, `__dirname`, `__filename`: Developer mode only, derived from the collection root and the executing script path. They reuse `local_roots`. In Safe mode they are unavailable, as in Bruno.
- `rok.getTestResults()` and `getAssertionResults()`: a snapshot of results recorded before the script ran, available in the test phase only.
- `rok.getOauth2CredentialVar(key)`: reads a snapshot of the stored token for the request. `rok.resetOauth2Credential(id)` sets an output flag the host applies after the script returns.

### 4. Response and runner extras

- `res.url`, `res.getUrl()`, `res.getSize()` returning `{ body, headers, total }`.
- `res.setBody(body)`: changes what later scripts and tests see, not the stored response.
- `rok.setNextRequest(name)` as an alias of `rok.runner.setNextRequest`.
- `rok.runner.stopExecution()`: new output flag that the collection runner honors. Standalone runs ignore it.
- `rok.runner.iterationIndex` returns 0 and `totalIterations` returns 1 until `iterationData` exists. `iterationData` itself is deferred.

### Non-goals

- `req.onFail` stays a no-op (B revisits it).
- No `bru` alias, no importer rewrite.

## Errors and safety

- Phase misuse throws the same JS errors as today, for example `res.setBody` before a response exists.
- Secret values stay redacted in console and test-error text.
- No `unwrap()` in production paths. Missing keys return `undefined`, delete of a missing key is a no-op.

## Testing

- Rust engine unit tests per op, in the existing `engine.execute(ctx)` style.
- A check that `rok-types.ts` and `bootstrap.js` expose the same names.
- Persistence-layer tests for the new delete variants, with `tempfile` fixtures.
- Monaco IntelliSense and the snippets sidebar get the new names.
- Verification: `cargo check -j4`, targeted crate tests with `-j4`, `yarn tsc --noEmit`, `yarn check`.

## Required reading for implementation

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Also see `.claude/script-files.md` and `docs/superpowers/specs/2026-10-07-js-script-security-design.md` (trust model for Developer-mode APIs).
