# rocket-scripting

Domain crate that defines the **JS scripting contract** for RocketAPI.

## Purpose

Owns the `ScriptEngine` trait, `ScriptContext` (input), `ScriptResult` (output),
`ScriptPhase`, and all supporting types. Does NOT contain any JS engine code.

## Dependency rule

Only imports from:
- `rocket-shared` (DomainError, DomainResult)
- `rocket-environment` (VariableContext)
- `rocket-http` (HttpRequest, HttpResponse)

Never imports `rocket-infra`, `rocket-app`, or `src-tauri`.

## Key types

| Type | File | Purpose |
|---|---|---|
| `ScriptEngine` | `engine.rs` | Async trait — `execute(ctx) -> DomainResult<ScriptResult>` |
| `ScriptContext` | `context.rs` | Immutable input snapshot passed to engine |
| `ScriptResult` | `result.rs` | All side-effects to apply after execution |
| `ScriptPhase` | `phase.rs` | `BeforeRequest` / `AfterResponse` / `Tests` |
| `RequestMutations` | `result.rs` | `req.set*` changes from before-request scripts |
| `EnvVarWrite` | `result.rs` | `rok.setEnvVar` writes; `persist` flag controls disk write |
| `TestResult` | `result.rs` | Outcome of a single `rok.test()` block |
| `ConsoleEntry` | `result.rs` | Captured `console.log/warn/error` output |

## Execution model

`DenoScriptEngine` in `rocket-infra` implements `ScriptEngine`.
`HttpService` in `rocket-app` calls `engine.execute(ctx)` three times per send:
1. `BeforeRequest` — may mutate the outgoing request
2. `AfterResponse` — may write env/collection vars
3. `Tests` — collects `TestResult` entries

Mutations in `ScriptResult` are applied by `HttpService` after each call returns.
The engine never applies anything itself.
