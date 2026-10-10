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

- Persistence keeps Rocket semantics, meaning what the code does today: every script write except `rok.setVar` already persists (`apply_script_side_effects` always passes `force_persist = true` for env and global writes, and collection writes are always saved). `EnvVarWrite.persist` is inert. New set and delete APIs persist the same way, and `{ persist: true }` stays accepted and inert. `setVar` and `deleteVar` are runtime-only. An earlier draft of this spec described the opposite (in-memory unless `persist`), which was wrong.
- No new architecture. Each API is a `bootstrap.js` wrapper, an op in `crates/rocket-infra/src/scripting/ops/`, a field on `ScriptInputState` or `ScriptOutputState`, and a typing in `src/components/editor/rok-types.ts`.

## Design

### 1. Reads from data already in context

`VariableContext` already holds the runtime, request, folder, env, collection, global and process-env scopes. Add JS wrappers and read ops for:

- `rok.getAllEnvVars`, `getAllVars`, `getAllGlobalEnvVars`
- `rok.hasVar`, `hasGlobalEnvVar`, `hasCollectionVar`
- `rok.getRequestVar`, `getProcessEnv`
- `rok.getFolderVar` is owned by the folder-settings plan 07 (same `variables.folder` read). Do not add it here. If that plan has not landed when this one is implemented, add it then and drop it from plan 07.

### 2. Writes and deletes

- Runtime: `deleteVar`, `deleteAllVars`.
- Env: `deleteAllEnvVars` (`deleteEnvVar` exists).
- Collection: `deleteCollectionVar`, `deleteAllCollectionVars`.
- Global env: `deleteGlobalEnvVar`, `deleteAllGlobalEnvVars`.

`EnvVarWrite` and `CollectionVarWrite` in `rocket-scripting` gain a delete variant, and a delete-all variant for the bulk calls. The persistence layer applies them after the engine returns, as it does for sets. Backward compatibility: new variants only, existing serialized shapes unchanged.

### 3. New context fields

- `collection_name` for `rok.getCollectionName()`.
- `rok.isSafeMode()` from the existing `sandbox_mode`.
- `rok.cwd()` and `__dirname`: Developer mode only, both the collection root. `__filename` is `undefined`, because the executing script's own path is not in `ScriptContext`; per-folder `__dirname` is a follow-up. In Safe mode `cwd()` throws and `__dirname` is undefined. Local modules loaded with `require` keep their own `__dirname` and `__filename`.
- `rok.getTestResults()`: the tests recorded so far by the current script. `rok.getAssertionResults()`: declarative assertion outcomes, computed before the tests script runs (the evaluation is a pure function of the assertions and the response, so the existing execution order is unchanged). Both are for the tests phase.
- `rok.getOauth2CredentialVar(key)` and `rok.resetOauth2Credential(id)` are deferred to a follow-up. OAuth2 tokens live in `OAuth2Service` and the frontend token flow, which `ScriptContext` cannot reach, so this needs its own design.

### 4. Response and runner extras

- `res.url` and `res.getUrl()` return the request URL, since `HttpResponse` does not track the final redirect URL. `res.getSize()` returns `{ body, headers, total }`.
- `res.setBody(body)`: changes what later scripts and tests see, not the stored response.
- `rok.setNextRequest(name)` as an alias of `rok.runner.setNextRequest`.
- `rok.runner.stopExecution()`: reuses the runner's existing stop path (`NextRequest::Stop`, plus `skip_request` in the before-request phase). No runner change. Standalone runs ignore it.
- `rok.runner.iterationIndex` returns 0 and `totalIterations` returns 1 until `iterationData` exists. `iterationData` itself is deferred.

### Non-goals

- `req.onFail` stays a no-op (B revisits it).
- No `bru` alias, no importer rewrite.

## Added while planning

- Reads see the same script's earlier writes (a small overlay in `bootstrap.js`). Today they read only the snapshot.
- Non-string runtime variables (numbers, objects) are kept as JSON text when merged between phases. Today they are silently dropped.
- `getProcessEnv` reads a snapshot of the host environment (`std::env::vars()`). The script engine empties the snapshot in Safe mode, so every key is undefined there. Only Developer mode sees the real environment.

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
