# DDD Phase 7, Plan 1: shrink rocket-shared (D-70, D-73)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `rocket-shared` loses the dead `proxy.rs` and the client-certificate model, which move to the crate that owns them, with no change to any file or wire format.

**Architecture:** D-70 deletes a module nothing imports. D-73 moves `certificate.rs` (`ClientCertificate`, `VaultCertificateFormat`) to `rocket-environment`, where `Environment` already holds the certificates; users switch their import path. `rocket-http` gains a dependency on `rocket-environment` for `VaultCertificateFormat` only. D-71, D-72 and D-75 are not in this plan (see Scope findings).

**Tech Stack:** Rust workspace crates `rocket-shared`, `rocket-environment`, `rocket-http`, `rocket-app`, `rocket-infra`, `src-tauri`.

**Spec:** `docs/roadmaps/ddd-roadmap.md` items D-70 and D-73; `docs/engineering/ddd-decisions.md`.

## Scope findings (checked in the code, 2026-11-04)

- **D-71 is blocked by D-74.** `acp.rs`, `grpc.rs` and `load_test.rs` are not only imported by a few crates: `DomainEvent` in `rocket-shared/src/events.rs` has fields of those types (`ConfigOption`, `PermissionOption`, `GrpcMetadataPair`, `LoadTestProgressEvent`, `LoadTestResult`). Moving them out would make `rocket-shared` depend on `rocket-acp`, `rocket-grpc` and `rocket-http`, the wrong direction. They can move once D-74 splits the event enum.
- **D-72 is blocked.** `rocket_shared::types::Auth::OAuth2(Box<OAuth2Flow>)` holds the persisted OAuth2 model, so it cannot leave shared unless `Auth` moves too. That needs its own decision.
- **D-75 depends on D-74** (it replaces the global event with a smaller progress port).
- **D-73 placement ruling:** `ClientCertificate` and `VaultCertificateFormat` go to `rocket-environment` (the aggregate that owns them). The alternative, `rocket-http`, would make the pure environment crate depend on the HTTP crate. Cost if wrong: another path rename.

## Global Constraints

- Serde shape of `ClientCertificate` and `VaultCertificateFormat` is unchanged. The existing tests move with the module and the infra schema-shape tests stay as they are.
- `Debug` for `ClientCertificate` keeps hiding secret fields (the existing impl moves as it is).
- No behavior change. No `unwrap` in production code. `cargo` commands take `-j4`; never `--workspace`.
- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (Task 2 touches the environment model and `oc` conversions).

## Review Focus

- Nothing outside `rocket-shared` still names `rocket_shared::certificate` or `rocket_shared::proxy` (grep over `crates`, `src-tauri`, docs).
- The `rocket-environment` public path is `rocket_environment::certificate::{ClientCertificate, VaultCertificateFormat}`, also re-exported at the crate root.
- No dependency cycle: `rocket-http` now depends on `rocket-environment`, which depends only on `rocket-shared`.
- The `Debug` output of a `ClientCertificate` with a passphrase or key reference still hides the value (the moved test pins it).
- `cargo check -j4 --all-targets` is clean for every crate, including `src-tauri`.

---

### Task 1: Delete the dead proxy module (D-70)

**Files:**
- Delete: `crates/rocket-shared/src/proxy.rs`
- Modify: `crates/rocket-shared/src/lib.rs` (remove `pub mod proxy;`)

**Interfaces:** none.

- [ ] **Step 1: Confirm it is dead.** Run `grep -rn "rocket_shared::proxy\|shared::{[^}]*proxy" crates src-tauri --include='*.rs'` — Expected: no output. If anything matches, stop and report it.
- [ ] **Step 2: Delete** `proxy.rs` and the `pub mod proxy;` line.
- [ ] **Step 3: Run** `cargo check -j4 --all-targets` — Expected: no errors (this is the proof; there is no test to write for a deletion).
- [ ] **Step 4: Commit** (`refactor(shared): remove the unused proxy module`).

### Task 2: Move the certificate model to rocket-environment (D-73)

**Files:**
- Move: `crates/rocket-shared/src/certificate.rs` to `crates/rocket-environment/src/certificate.rs` (with its tests)
- Modify: `crates/rocket-shared/src/lib.rs`, `crates/rocket-environment/src/lib.rs` (`pub mod certificate;` and a root re-export of both types), `crates/rocket-http/Cargo.toml` (add `rocket-environment`)
- Modify (import path `rocket_shared::certificate` to `rocket_environment::certificate`): the files that name it in `rocket-environment` (`environment.rs`, `flatten.rs`, `client_certificate_validation.rs`, `vault_secret_fetcher.rs`), `rocket-http` (`resolved_certificate.rs`), `rocket-app`, `rocket-infra` (including `oc/` and the tests), `src-tauri` (`commands/environments.rs`). Find them with `grep -rln "shared::certificate" crates src-tauri --include='*.rs'`.

**Interfaces:**
- Produces: `rocket_environment::certificate::{ClientCertificate, VaultCertificateFormat}` with the same variants, fields, serde attributes and methods as today.
- Consumes: nothing from Task 1.

- [ ] **Step 1: Move the file** with `git mv`, add the module and re-export to `rocket-environment`, remove it from `rocket-shared`. In `rocket-environment` the file's own `crate::` or `super::` paths must resolve (it imports only `serde`).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-environment certificate` — Expected: the moved tests pass in their new home (they are the pin for the serde shape and the `Debug` hiding).
- [ ] **Step 3: Switch the imports** in every file the grep lists, and add the `rocket-environment` dependency to `rocket-http/Cargo.toml`. A scripted replace of the path is fine; do not reformat unrelated lines.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets` — Expected: clean; and `grep -rn "shared::certificate" crates src-tauri docs --include='*'` — Expected: no hits except historic notes in `docs/roadmaps`.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-environment`, `cargo test -j4 -p rocket-http resolved_certificate`, `cargo test -j4 -p rocket-infra schema_shape` and `cargo test -j4 -p rocket-app client_certificates` — Expected: pass.
- [ ] **Step 6: Commit** (`refactor(environment): move the client certificate model from shared`, then `refactor: import the certificate model from rocket-environment`).

### Task 3: Docs and checks

**Files:**
- Modify: `docs/roadmaps/ddd-roadmap.md` (D-70 and D-73 done; D-71, D-72 and D-75 get the blockers above), `crates/rocket-shared/CLAUDE.md`, `crates/rocket-environment/CLAUDE.md`, `crates/rocket-http/CLAUDE.md` (the new dependency)

- [ ] **Step 1: Edit the docs.** Run `cargo clippy -j4 -p rocket-shared -p rocket-environment -p rocket-http --all-targets` — Expected: no new warnings.
- [ ] **Step 2: Commit** (`docs: mark DDD Phase 7 items D-70 and D-73 done`).
