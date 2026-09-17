# Spec: Audit Trail and Secret-Flag Preservation for Script-Driven Variable Writes

**Status:** Draft
**Severity:** Medium
**Roadmap:** [2026-09-16-scripting-security-roadmap.md](../plans/2026-09-16-scripting-security-roadmap.md), item 3
**Depends on:** item 1 ([secret-storage-hardening-spec.md](2026-09-16-secret-storage-hardening-spec.md)) — this spec's fix to `apply_env_writes` routes secret-preserving writes through the same `env_repo.save()` path item 1 hardens; implement item 1 first so `secret`/`secret_type` actually persist correctly once preserved here.

> Before starting implementation, read `docs/superpowers/specs/opencollection-spec-reference.md`.

## 1. Problem

Three distinct, compounding issues in how script-driven variable writes reach disk, all in
`crates/rocket-app/src/execution_service.rs`:

### 1a. Script writes silently strip the `secret` flag on overwrite

`apply_env_writes` (`execution_service.rs:320-349`) reconstructs the variable from scratch on every
write:

```rust
env.set_variable(rocket_environment::Variable::new(write.key.clone(), str_val));
```

`Variable::new` always produces `secret: false` (`rocket-environment/src/variable.rs:67-77`), and
`Environment::set_variable` (`rocket-environment/src/environment.rs:43-49`) does a full
`*existing = variable` replace, not a field-level merge. So if a script calls
`rok.setEnvVar('API_KEY', 'x')` on a key that was previously marked secret, the write not only
changes the value — it silently un-marks the variable as secret. Combined with item 1's fix, this
means a script (including one from an untrusted imported collection, run once) can downgrade a
secret variable back to plaintext-on-disk with no warning, by writing to it.

### 1b. Script writes bypass the audit/event plumbing that manual saves go through

Manual environment edits go through `EnvironmentService::save`
(`crates/rocket-app/src/environment_service.rs:41-74`), which publishes
`DomainEvent::EnvironmentSaved` and, for each secret variable whose value changed,
`AuditEventKind::SecretVariableWritten` via `SecurityAuditPublisher`.

Script-driven writes never go through `EnvironmentService` at all — `apply_env_writes` talks to
`self.env_repo` (the raw `EnvironmentRepository`) directly (`execution_service.rs:333,347`). So a
script can silently persist (including overwriting a secret's value) with **zero** audit trail and
**zero** `DomainEvent`, even though `RequestExecutionService` already holds both
`self.events: Box<dyn EventPublisher>` and `self.audit: Arc<dyn SecurityAuditPublisher>`
(`execution_service.rs:86-87`) — the plumbing exists, it's just not connected here.

Same gap applies to `apply_collection_var_write` (`execution_service.rs:309-313`, backs
`rok.setCollectionVar`, writes into the git-tracked `opencollection.yml`) — no event either.

Consequence for the frontend: the in-app environment editor keeps displaying the stale
pre-write value after a script runs, because nothing tells the Zustand store to reload
(`src/lib/execute-request.ts` never listens for a variable-write signal — confirmed no such event
exists to listen for).

### 1c. Stale documentation contradicts actual behavior

- `execution_service.rs:317-319` doc comment: "For active-environment writes, only entries with
  `persist: true` are saved" — false since commit `3e116d6`; both call sites
  (`execution_service.rs:272,279`) now pass `force_persist: true` unconditionally.
- `crates/rocket-scripting/src/result.rs:98-99` doc comment on `EnvVarWrite.persist`: "When `true`,
  write is persisted to the environment `.yml` file" — same staleness; the field is currently
  inert for both env-var paths (always force-persisted regardless of its value).

## 2. Scope decision: persist-by-default stays

Commits `3e116d6`/`1d67d3d` deliberately moved `rok.setEnvVar` to always-persist, and updated a
test (`post_response_script_env_var_no_persist_skips_env_repo_save` →
`..._always_calls_env_repo_save`) to assert the new behavior on purpose. That is a recent,
intentional product decision, not an accident this spec should silently reverse. This spec makes
that already-shipped behavior **safe and visible** (audit trail, correct secret handling, UI
freshness) rather than re-litigating persist-by-default. If product/security wants to revisit
opt-in persistence later, that's a separate decision with its own spec.

## 3. Design

### 3.1 Preserve existing variable metadata on script-driven write

Change `apply_env_writes` to look up the existing variable before replacing it, and merge only the
fields a script write is actually allowed to change (`value`, `enabled`):

```rust
fn apply_env_writes(&self, env_name: &str, writes: &[rocket_scripting::EnvVarWrite], force_persist: bool) {
    let persist_writes: Vec<_> = writes.iter().filter(|w| force_persist || w.persist).collect();
    if persist_writes.is_empty() { return; }

    let Ok(mut env) = self.env_repo.get(env_name) else { return };
    let before = env.clone(); // for the audit/event diff in 3.2

    for write in persist_writes {
        if write.value.is_null() {
            env.remove_variable(&write.key);
            continue;
        }
        let str_val = write.value.as_str().map(str::to_owned)
            .unwrap_or_else(|| write.value.to_string());
        let existing = env.variables.iter().find(|v| v.key == write.key);
        let updated = rocket_environment::Variable {
            key: write.key.clone(),
            value: str_val,
            enabled: existing.map(|v| v.enabled).unwrap_or(true),
            secret: existing.map(|v| v.secret).unwrap_or(false),
            description: existing.and_then(|v| v.description.clone()),
            value_variants: None,
            secret_type: existing.and_then(|v| v.secret_type.clone()),
        };
        env.set_variable(updated);
    }

    if self.env_repo.save(&env).is_ok() {
        self.publish_var_write_events(&before, &env, env_name);
    }
}
```

A brand-new key a script introduces (no `existing`) defaults to `secret: false` — deliberate:
only the user, via the environment editor UI, can *promote* a variable to secret status; a script
(potentially from an untrusted imported collection) must never be able to implicitly create a
"secret" that then gets keyring-backed protection it didn't ask the user to grant, nor silently
create a plaintext variable that shadows a naming convention the user reserves for secrets.

### 3.2 Route script-driven writes through the same event/audit plumbing as manual saves

Extract the diff-and-publish logic already in `EnvironmentService::save`
(`environment_service.rs:46-71`) into a shared free function so it isn't duplicated:

```rust
// crates/rocket-app/src/env_audit.rs (new file)
pub fn publish_env_write_events(
    events: &dyn EventPublisher,
    audit: &dyn SecurityAuditPublisher,
    env_name: &str,
    before: &Environment,
    after: &Environment,
) {
    events.publish(DomainEvent::EnvironmentSaved { name: env_name.to_string() });
    for var in &after.variables {
        if !var.secret || var.value.is_empty() { continue; }
        let changed = before.variables.iter().find(|v| v.key == var.key)
            .map(|v| v.value != var.value || !v.secret)
            .unwrap_or(true);
        if changed {
            audit.publish("system".into(), None, AuditEventKind::SecretVariableWritten {
                environment: env_name.to_string(),
                variable_key: var.key.clone(),
            });
        }
    }
}
```

Call it from both `EnvironmentService::save` (replacing its inline loop) and
`RequestExecutionService::apply_env_writes`/`apply_script_side_effects` after a successful
`env_repo.save`. Same treatment for `apply_collection_var_write` — publish a
`DomainEvent::CollectionSettingsSaved`-style event (check `rocket_shared::events::DomainEvent` for
an existing matching variant before adding a new one; if none exists, add
`DomainEvent::CollectionVariableWritten { collection: String, key: String }`).

Also add one new event so the frontend can distinguish "a script changed a variable" from "the
user edited it manually" without needing to correlate timestamps:
`DomainEvent::ScriptVariableWritten { scope: String, environment: Option<String>, collection: Option<String>, key: String }`,
published alongside (not instead of) `EnvironmentSaved`/`CollectionVariableWritten` from the
script-side-effect path only.

### 3.3 Frontend: refresh stores on script-driven writes

`src/lib/execute-request.ts` (currently listens for/handles `ScriptError`, `ConsoleOutput`,
`TestsCompleted`, `RequestExecuted` per the existing event-publish call sites in
`execution_service.rs`) gains a listener for `ScriptVariableWritten` that triggers the
environment/collection Zustand store to re-fetch the affected environment or collection settings.
This closes the "editor shows stale value after a script runs" gap.

### 3.4 Fix stale doc comments

- `execution_service.rs:317-319` → replace with an accurate description of current behavior:
  "Both active-environment and global-environment writes are currently always persisted
  (`force_persist: true` at both call sites) regardless of the script-supplied `persist` flag.
  `EnvVarWrite.persist` is preserved for wire/API compatibility but has no effect today."
- `rocket-scripting/src/result.rs:98-99` → same correction on the `persist` field's doc comment.

## 4. Non-goals

- Not reverting persist-by-default (§2).
- Not adding a confirmation dialog before a script persists a write — that's a larger UX decision
  (interrupting an automated pre-request/post-response/test flow with a blocking prompt is a
  significant behavior change); the audit-trail + UI-refresh work here is the minimum viable
  transparency fix. Flag a confirmation-dialog follow-up as a future consideration, not required
  here.
- Not touching `rok.setGlobalEnvVar`'s `persist: false` hardcode at `ops/rok.rs:114` — it's already
  force-persisted by its caller (`execution_service.rs:279` passes `force_persist: true`
  regardless), consistent with active-env writes; no behavior change needed there beyond the shared
  event plumbing in §3.2.

## 5. Interfaces (for the implementation plan)

- `rocket_app::env_audit::publish_env_write_events(events: &dyn EventPublisher, audit: &dyn SecurityAuditPublisher, env_name: &str, before: &Environment, after: &Environment)` — new shared helper.
- `RequestExecutionService::apply_env_writes` — signature unchanged, internals rewritten per §3.1/§3.2.
- `EnvironmentService::save` — internals refactored to call the new shared helper; public signature unchanged.
- `DomainEvent::ScriptVariableWritten { scope: String, environment: Option<String>, collection: Option<String>, key: String }` — new variant in `rocket_shared::events::DomainEvent`.
- `DomainEvent::CollectionVariableWritten { collection: String, key: String }` — new variant, only if no existing equivalent is found during implementation (verify against the full `DomainEvent` enum first).

## 6. Acceptance criteria

1. A script that calls `rok.setEnvVar('API_KEY', 'new-value')` against an environment where
   `API_KEY` was previously `secret: true` results in the on-disk/keyring state still showing
   `secret: true` after the write (value updated, flag preserved).
2. The same write publishes exactly one `AuditEventKind::SecretVariableWritten { environment, variable_key: "API_KEY" }` via the injected `SecurityAuditPublisher`.
3. The same write publishes `DomainEvent::EnvironmentSaved` and `DomainEvent::ScriptVariableWritten`.
4. A script writing a **non-secret** variable does *not* publish `SecretVariableWritten`, mirroring the existing `EnvironmentService` test `save_emits_security_audit_event`.
5. `rok.setCollectionVar` writes publish a collection-scoped event (new or existing variant).
6. Existing tests `post_response_script_env_var_no_persist_skips_env_repo_save`-successor (the
   currently-named `..._always_calls_env_repo_save` test) and all other `execution_service.rs`
   script-side-effect tests continue to pass.
7. `cargo test -p rocket-app -p rocket-environment` passes; `yarn tsc --noEmit` passes for the
   `execute-request.ts` store-refresh addition.
