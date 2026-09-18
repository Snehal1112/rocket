# Spec: Secret-Aware Variable Context and Console/Test-Output Redaction

**Status:** Draft
**Severity:** Medium
**Roadmap:** [2026-09-16-scripting-security-roadmap.md](../plans/2026-09-16-scripting-security-roadmap.md), item 4
**Depends on:** item 1 ([secret-storage-hardening-spec.md](2026-09-16-secret-storage-hardening-spec.md)) — this spec relies on `Variable.secret` actually round-tripping correctly through `EnvironmentRepository`, which item 1 fixes. Implement item 1 first.

> Before starting implementation, read `docs/superpowers/specs/opencollection-spec-reference.md`.

## 1. Problem

`VariableContext` (`crates/rocket-environment/src/context.rs:3-12`) is plain
`HashMap<String, String>` per scope — there is no way to represent "this value is a secret." Every
scope that can legitimately carry a secret-flagged `Variable` (`env`, `collection`, `global_env`)
loses that flag the moment its value is copied into `VariableContext` in
`RequestExecutionService::build_variable_scopes` (`crates/rocket-app/src/execution_service.rs:169-175`,
`ctx.env.insert(k.to_string(), v.to_string())` — no filter, no tag).

Consequence: `rok.getEnvVar`/`getVar`/`getCollectionVar`/`getGlobalEnvVar`/`interpolate`
(`crates/rocket-infra/src/scripting/ops/rok.rs:8-62`) all return plaintext secret values
indiscriminately to script code, which is **partly necessary** (scripts legitimately sign requests
and refresh tokens using secret values) but currently has no mechanism to prevent the value from
then leaking back out through `console.log`, a thrown error, or a failed `rok.test()` assertion
message — all of which flow into `ScriptOutputState`/`ConsoleEntry`/`TestResult.error`
(`crates/rocket-scripting/src/result.rs`) and are surfaced live in the UI's Console panel and over
the Tauri event bus (`DomainEvent::TestsCompleted`, `execution_service.rs:800-816`) to every
frontend listener. (Confirmed clean: this output does **not** reach `rocket-history` — history
records only method/url/status/duration, per `crates/rocket-history/src/entry.rs:5-17` — so the
leak surface is "live UI + event bus," not "persisted to disk.")

## 2. Goal

Preserve the ambient read capability scripts need (do not remove `rok.getEnvVar` et al., do not add
an allowlist/permission-prompt system — out of scope, see §4) but prevent secret **values** from
appearing verbatim in anything a script emits as output: `console.log/warn/error` messages and
`rok.test()` failure error strings.

## 3. Design

### 3.1 `VariableContext` gains per-scope secret markers

```rust
// crates/rocket-environment/src/context.rs
#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    pub runtime:     HashMap<String, String>,
    pub request:     HashMap<String, String>,
    pub folder:      HashMap<String, String>,
    pub env:         HashMap<String, String>,
    pub collection:  HashMap<String, String>,
    pub global_env:  HashMap<String, String>,
    pub process_env: HashMap<String, String>,
    /// Keys (from any scope) whose *value* must be redacted if it appears in
    /// script-emitted console/test-error text. Not a per-scope map — a value is
    /// either sensitive or not, regardless of which scope surfaced it.
    pub secret_values: std::collections::HashSet<String>,
}
```

Only `secret_values` (the actual **values**, not keys — redaction matches on value content) is
new; `flatten()`/`flatten_with_process_env()` are unchanged (they never touch this field). Storing
values (not keys) directly avoids a second lookup at redaction time and is robust to a script
copying a secret value into a *different* variable name (`rok.setVar('x', rok.getEnvVar('API_KEY'))`)
— redaction is content-based, not name-based, so that copy is still caught.

### 3.2 Populate `secret_values` at variable-scope build time

`RequestExecutionService::build_variable_scopes` (`execution_service.rs:150-194`) already has
`Variable`/`CollectionVariable` in hand when it inserts into `ctx.env`/`ctx.collection`. Add, for
every variable whose `secret` field is `true` and whose value is non-empty and at least
`MIN_REDACTION_LEN` characters (see §3.4):

```rust
if let Some(name) = environment_name {
    if let Ok(env) = self.env_repo.get(name) {
        for var in &env.variables {
            if !var.enabled { continue; }
            ctx.env.insert(var.key.clone(), var.value.clone());
            if var.secret && var.value.len() >= MIN_REDACTION_LEN {
                ctx.secret_values.insert(var.value.clone());
            }
        }
    }
}
```

Same pattern for `collection` (once `CollectionVariable.secret` exists — see §5 note) and
`global_env`. `runtime`/`request`/`folder`/`process_env` are never populated from a
secret-flagged source today, so no change needed there.

### 3.3 Redact at the console/test-failure ops, not at read time

Redaction happens where output is *produced*, not where secrets are *read* — this is the only
point that can catch a secret value regardless of how it got into the output string (direct log,
string concatenation, JSON.stringify of an object containing it, etc.).

`ScriptInputState` (`crates/rocket-infra/src/scripting/state.rs`) gains the redaction list, copied
from `ScriptContext.variables.secret_values` when the op state is seeded
(`crates/rocket-infra/src/scripting/engine.rs:146-160`, inside `run_script`).

```rust
// crates/rocket-infra/src/scripting/ops/console.rs
fn redact(state: &OpState, msg: String) -> String {
    let secrets = &state.borrow::<ScriptInputState>().secret_values;
    if secrets.is_empty() { return msg; }
    let mut out = msg;
    for s in secrets {
        out = out.replace(s.as_str(), "••••••");
    }
    out
}

#[op2]
#[string]
pub fn op_console_log(state: &mut OpState, #[string] msg: String) -> /* existing signature */ {
    let redacted = redact(state, msg);
    state.borrow_mut::<ScriptOutputState>().add_console(ConsoleLevel::Log, redacted);
}
```

Same treatment for `op_console_warn`/`op_console_error`
(`crates/rocket-infra/src/scripting/ops/console.rs`) and `op_test_fail`'s `error` parameter
(`crates/rocket-infra/src/scripting/engine.rs:52`, the test-runner op registered inline in
`engine.rs` rather than `ops/`).

`rok.interpolate` and the `req`/`res` ops are **not** redacted — their whole purpose is producing
real values for use in the actual outgoing request (headers, body, URL), which must contain the
real secret to function. Redaction only applies to the observability surface
(console/test-failure text), never to functional data paths.

### 3.4 Minimum redaction length

Redacting every occurrence of a secret's exact value risks pathological over-redaction if a
secret's value is short or a common substring (e.g. a secret literally equal to `"1"` or `"true"`
would redact unrelated output). Set `const MIN_REDACTION_LEN: usize = 6;` (a documented, tunable
constant) — secrets shorter than this are not added to `secret_values` and are not redacted. This
is a deliberate, documented trade-off: short "secrets" get materially weaker protection, which is
reasonable since a 5-character-or-shorter credential is already weak in isolation.

## 4. Non-goals

- Not building a permission/allowlist system restricting *which* scripts can read *which* secrets.
  The audit's own assessment was that ambient read access is "partly necessary" for legitimate
  signing/token-refresh use cases; restricting it further is a larger product decision (e.g.
  per-request "this script may access these secrets" declarations) explicitly out of scope here.
- Not redacting secret values out of the *request itself* (headers/body/URL) — that would break
  the feature scripts use secrets for. Redaction is output-surface-only (§3.3).
- Not attempting redaction inside `ScriptResult.error` (the top-level uncaught-exception message,
  `crates/rocket-infra/src/scripting/engine.rs:170-172`) in this pass — that string comes from
  `deno_core`'s own exception formatting, not from a script-controlled `console.log` call, and
  redacting it would need the same `secret_values` list threaded into `run_script`'s error-mapping
  path. Flagged as a natural follow-up using the same `redact()` helper once this lands, not
  blocking — the primary, most-likely leak vector (`console.log(secret)`) is what this spec closes.

## 5. Interfaces (for the implementation plan)

- `VariableContext.secret_values: HashSet<String>` — new field, `rocket-environment`.
- `ScriptInputState.secret_values: HashSet<String>` — new field, `rocket-infra/src/scripting/state.rs`, populated from `ScriptContext.variables.secret_values` in `run_script` (`engine.rs`).
- `crates/rocket-infra/src/scripting/ops/console.rs::redact(state: &OpState, msg: String) -> String` — new helper, used by `op_console_log`/`op_console_warn`/`op_console_error`.
- `op_test_fail` (`engine.rs`) — updated to call the same `redact()` helper (may need to move `redact()` to a shared location, e.g. `ops/mod.rs`, if `engine.rs`'s inline ops need it too — resolve at implementation time based on module visibility).
- Note: `CollectionVariable` (`rocket-collection`) already has a `secret: bool` field (see
  `crates/rocket-infra/src/conversions/variables.rs:13-19`, currently hardcoded `false` on every
  conversion, same unfixed gap as environment variables pre-item-1). This spec assumes collection
  variables can also be secret-flagged for §3.2's collection-scope population — if
  `CollectionVariable.secret` is still always-`false` when this is implemented (item 1 doesn't fix
  it, see item 1's §7 follow-ups), the collection-scope redaction branch in §3.2 will simply never
  trigger, which is a safe (if incomplete) default, not a bug — document this explicitly in the
  plan rather than expanding this spec's scope to also fix `CollectionVariable`.

## 6. Acceptance criteria

1. Given an environment variable `API_KEY` marked `secret: true` with value `sk-live-abcdef123`,
   a script running `console.log(rok.getEnvVar('API_KEY'))` produces a `ConsoleEntry` whose
   message is `"••••••"`, not the literal key.
2. A script running `console.log("token=" + rok.getEnvVar('API_KEY'))` produces
   `"token=••••••"` — substring redaction inside a larger string works.
3. A script running `rok.setVar('copy', rok.getEnvVar('API_KEY')); console.log(rok.getVar('copy'))`
   still redacts — content-based matching catches the copy.
4. A failing `rok.test('leaks secret', () => { throw new Error(rok.getEnvVar('API_KEY')) })`
   produces a `TestResult.error` with the value redacted.
5. A non-secret variable's value is never redacted, including when it happens to be a substring of
   some other secret's value (order of replacement doesn't need to be specified beyond "not
   incorrect" — verify no crash/panic on overlapping substrings).
6. A secret value shorter than 6 characters is not redacted (documented limitation, not a test
   failure — assert the documented behavior explicitly so it doesn't get "fixed" accidentally later
   without revisiting the trade-off).
7. `req.setHeader('Authorization', 'Bearer ' + rok.getEnvVar('API_KEY'))` followed by sending the
   request is unaffected — the real value reaches the actual HTTP request; only console/test
   output is redacted.
8. `cargo test -p rocket-environment -p rocket-infra -p rocket-app` passes.
