# Scripting Security Roadmap

**Source:** Security audit of `crates/rocket-scripting` and its `rocket-infra/src/scripting`
implementation (2026-09-16 conversation). 6 findings, all specced below.

This roadmap sequences the fixes by dependency and priority, and links each finding to its spec
and (once written) implementation plan.

## Dependency graph

```
Item 1 (secret storage hardening) ──┬──> Item 4 (secret-aware VariableContext)
        [High, foundational]        │
                                     └──> Item 3 (env var write audit trail)
                                              [Medium]

Item 2 (hide Deno.core.ops)     — independent, any order
        [Medium]

Item 5 (execution timeout)      — independent, any order
        [Low, cheap high-value]

Item 6 (SSRF opt-in guard)      — independent, any order, lowest priority
        [Low, by-design hardening]
```

Item 1 must land before items 3 and 4 — both rely on `Variable.secret` actually round-tripping
through `EnvironmentRepository`, which item 1 is the one that fixes. Items 2, 5, and 6 touch
completely disjoint code paths (bootstrap.js/engine.rs isolate setup; engine.rs execution timing;
reqwest_executor/execution_service request dispatch, respectively) and have no dependency on the
secret-storage work or on each other — they can be built in parallel with the 1→3/4 chain and with
each other.

## Recommended build order

1. **Item 1 — Secret storage hardening** (High). Foundational; unblocks 3 and 4.
2. **Item 4 — Secret-aware VariableContext** (Medium). Small, self-contained once item 1 lands.
3. **Item 3 — Env var write audit trail** (Medium). Also fixes a real correctness bug (script
   writes silently stripping the `secret` flag) discovered while grounding this spec — found only
   by reading `Environment::set_variable`'s full-replace semantics directly, not in the original
   audit. Do this after item 1 so the fix routes through item 1's now-correct persistence path.
4. **Item 2 — Hide `Deno.core.ops`** (Medium). Independent; can be done any time, including in
   parallel with 1/3/4.
5. **Item 5 — Execution timeout** (Low severity, but cheap and high-value — closes an unbounded-hang
   DoS with a small, well-contained change). Independent; do early if capacity allows.
6. **Item 6 — SSRF opt-in guard** (Low, by-design hardening, not a bug fix). Do last — it's the
   only item that adds new user-facing settings surface rather than fixing an existing gap.

## Items

| # | Severity | Title | Spec | Plan |
|---|---|---|---|---|
| 1 | High | Secret storage hardening | [spec](../specs/2026-09-16-secret-storage-hardening-spec.md) | [plan](2026-09-16-secret-storage-hardening-plan.md) |
| 2 | Medium | Hide `Deno`/`Deno.core.ops` from user scripts | [spec](../specs/2026-09-16-sandbox-deno-ops-lockdown-spec.md) | [plan](2026-09-16-sandbox-deno-ops-lockdown-plan.md) |
| 3 | Medium | Audit trail + secret-flag preservation for script var writes | [spec](../specs/2026-09-16-env-var-write-audit-spec.md) | [plan](2026-09-16-env-var-write-audit-plan.md) |
| 4 | Medium | Secret-aware VariableContext + console/test redaction | [spec](../specs/2026-09-16-secret-aware-variable-context-spec.md) | [plan](2026-09-16-secret-aware-variable-context-plan.md) |
| 5 | Low | Script execution timeout + isolate termination | [spec](../specs/2026-09-16-script-execution-limits-spec.md) | [plan](2026-09-16-script-execution-limits-plan.md) |
| 6 | Low | Opt-in host guard for script-driven request mutations | [spec](../specs/2026-09-16-request-mutation-host-guard-spec.md) | [plan](2026-09-16-request-mutation-host-guard-plan.md) |

## Cross-cutting facts established while grounding these specs

Worth keeping in mind across all six plans:

- **`rocket-audit`** (`crates/rocket-audit`) already provides `SecurityAuditPublisher` and
  `AuditEventKind::SecretVariableWritten` — items 1 and 3 build directly on this existing
  infrastructure rather than inventing a new audit mechanism.
- **`keyring`** (v3, with `apple-native`/`windows-native`/`sync-secret-service` features) is
  already a proven dependency in this codebase for exactly this kind of secret — used correctly
  for git credentials in `src-tauri/src/commands/git.rs:317-342`. It is currently scoped to
  `src-tauri` only; item 1 adds it to `rocket-infra` as well, which is the architecturally correct
  crate for it (concrete I/O belongs in `rocket-infra`, not in Tauri command handlers).
  `src-tauri`'s existing git-credential usage is untouched — no shared abstraction between the two
  is required for this roadmap.
  `AuditEventKind::SecretVariableWritten` — items 1 and 3 build directly on this existing
  infrastructure rather than inventing a new audit mechanism.
- **`OcEnvironment.variables` is currently `Vec<OcVariable>`** with no room for a secret variant at
  all — this is the structural root cause of item 1, found only by reading the actual struct
  definitions (`crates/rocket-infra/src/oc/environment.rs:19`, `crates/rocket-infra/src/oc/variables.rs`),
  not visible from the original audit's op-surface-focused investigation.
- **`Environment::set_variable` does a full-struct replace**, not a field merge
  (`crates/rocket-environment/src/environment.rs:43-49`) — this is why item 3 exists as a distinct
  finding beyond "add an audit event": script-driven writes were also silently destroying the
  `secret` flag on the variable they touched, independent of any audit-trail gap.
- **Isolation between script executions is already sound** (confirmed by the original audit, not
  revisited by any of these 6 items) — fresh `JsRuntime` per `execute()` call, no shared/static
  Rust state. Item 5's timeout work builds on this: terminating one isolate's execution has no
  effect on any other concurrent or subsequent execution.
- **Vendored JS modules and the ops phase-guards are already clean** (also confirmed, not
  revisited) — no `eval`/`new Function` misuse in user-facing modules, `axios.js` is intentionally
  stubbed, `req.set*`/`res.*` phase guards are correctly enforced and test-backed. None of the 6
  items touch this surface.

## Definition of done for the roadmap

All 6 plans implemented, each plan's own acceptance criteria met, `cargo check`/`cargo test`
across the affected crates and `yarn tsc --noEmit`/`yarn check` clean, per this project's standard
verification bar (see `CLAUDE.md` Commands section and `.claude/rules/*.md` Verification sections).
