# Inherit Secrets and Certs, Views and Dialog Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** Flow lint, the MCP environment view and the environment dialog show inherited bindings and certificates, and the docs describe the new behaviour.

**Architecture:** Plan 1 put the merged lists in `Flattened`. This plan reads them in the lint and MCP views, sends metadata-only rows through `ResolvedEnvironmentDto`, and renders them read-only in the dialog.

**Tech Stack:** Rust (`rocket-app`, `src-tauri`), React, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-29-inherit-secrets-certs-design.md`

## File Structure

- `crates/rocket-app/src/flow_lint_context.rs`, `mcp_read_views.rs`, `mcp_tool_service.rs`: merged names.
- `src-tauri/src/commands/environments.rs`: DTO rows.
- `src/lib/tauri-api.ts`, `src/lib/certificate-validation.ts`: types and the optional `inheritedAliases` input.
- `src/components/environments/EnvironmentDialog.tsx`, `ExternalSecretsTab.tsx`, `CertificatesTab.tsx`: read-only inherited rows.
- `crates/rocket-environment/CLAUDE.md`, `docs/superpowers/specs/2026-10-18-env-extends-dotenv-design.md`: docs.

## Global Constraints

- New IPC DTOs use `#[serde(rename_all = "camelCase")]`.
- Rows sent to the frontend carry no path, passphrase, secret reference or secret value.
- UI uses shadcn/ui primitives and lucide-react icons only, never a raw button or input.
- Zustand: never fully destructure store state at component top level.
- Tests use `.expect("...")` and `expect_err`. Cargo commands are per crate.
- Flow lint completeness for environments with `extends` stays unchanged.

## Review Focus

1. A child overriding a parent binding shows once, with the child as source, and is not listed as inherited: `overridden_rows_are_not_listed_as_inherited` (Task 3).
2. An inherited certificate row exposes no secret: `resolved_dto_certificate_rows_carry_no_secret_fields` (Task 2).
3. A vault certificate naming a parent-only alias passes the dialog check: `accepts an alias from inheritedAliases` (Task 3).
4. The lint test `an_environment_with_inheritance_leaves_unknown_names_unanswered` still passes: Task 1 keeps it green.
5. An environment without a parent produces the same MCP view as before: `vault_references_unchanged_without_a_parent` (Task 1).

---

### Task 1: Flow lint names and MCP view include inherited secrets

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/flow_lint_context.rs:94-111` (and its call sites for `add_environment`)
- Modify: `crates/rocket-app/src/mcp_read_views.rs:314-330`, `crates/rocket-app/src/mcp_tool_service.rs:~713`

**Interfaces:**
- Consumes: `rocket_environment::{flatten_with_repo, Flattened::bindings}`.
- Produces: `MaskedEnvironment::from_environment_with_bindings(env: &Environment, bindings: &[ExternalSecretBinding]) -> MaskedEnvironment`; `from_environment(env)` keeps its signature and calls it with `env.external_secrets`.

- [ ] **Step 1: Read** both files, the call sites of `add_environment`, and the test `an_environment_with_inheritance_leaves_unknown_names_unanswered` (`flow_lint_context.rs:~449`).

- [ ] **Step 2: Write the failing tests.**
  - `mcp_read_views.rs`: `vault_references_include_parent_secrets` (parent binds `payments` with secret `apiKey`; child extends it; the view built with the merged bindings lists `payments.apiKey`) and `vault_references_unchanged_without_a_parent` (`from_environment(&env)` equals the old output).
  - `flow_lint_context.rs`: `lint_names_include_parent_secrets` (child extends a parent with binding `payments` and secret `apiKey`; the known names contain `payments.apiKey`, and `names_complete` is still false).

- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app vault_references` and `cargo test -j4 -p rocket-app lint_names`. Expected: compile error for the missing constructor, then assertion failure.

- [ ] **Step 4: Implement.** The lint adds the names of the flattened bindings read through the same repo lookup the context already uses for the scope, and keeps its return value rule (`extends.is_none() && dot_env_file_path.is_none()`). `get_environment` in `mcp_tool_service.rs` flattens through the repo from `self.environment_repo_factory.for_collection(collection)` and calls the new constructor; a flatten error returns that error.

- [ ] **Step 5: Run** `cargo test -j4 -p rocket-app flow_lint_context`, `cargo test -j4 -p rocket-app mcp_read_views`, `cargo test -j4 -p rocket-app mcp_tool_service` and `cargo check -j4 -p rocket-app`. Expected: pass.

---

### Task 2: Resolved environment DTO carries inherited rows

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/environments.rs:236-290` (`ResolvedEnvironmentDto`, `to_resolved_dto`, tests module)
- Modify: `src/lib/tauri-api.ts:1704-1720`

**Interfaces:**
- Consumes: `Flattened.external_secrets`, `Flattened.client_certificates`.
- Produces:
  - Rust `ResolvedBindingDto { alias, vault_name, connection_id, secret_count: usize, source }` and `ResolvedCertificateDto { kind: String, domain, source }` (`kind` is `"pem"`, `"pkcs12"` or `"vault"`, serialised as `type`), added to `ResolvedEnvironmentDto` as `external_secrets` and `client_certificates`.
  - TypeScript `ResolvedBinding`, `ResolvedCertificate` and the two arrays on `ResolvedEnvironment`.
  - The DTO lists every merged row; the UI filters the selected environment's own rows.

- [ ] **Step 1: Read** the DTO code, `to_resolved_dto`, and the existing test `a_cycle_maps_to_an_error_string_naming_the_environments`.

- [ ] **Step 2: Write the failing tests** in the `src-tauri` tests module:

```rust
#[test]
fn resolved_dto_lists_inherited_bindings_and_certificates_with_source() {
    // parent "p" binds "prod" with 2 secrets and has a pkcs12 cert for "a.com";
    // child "c" extends "p" with no lists of its own.
    let flat = rocket_environment::flatten_environment(&child, &lookup).expect("flatten");
    let json = serde_json::to_value(to_resolved_dto("c", &flat)).expect("json");
    assert_eq!(json["externalSecrets"][0]["alias"], "prod");
    assert_eq!(json["externalSecrets"][0]["secretCount"], 2);
    assert_eq!(json["externalSecrets"][0]["source"], "p");
    assert_eq!(json["clientCertificates"][0]["type"], "pkcs12");
    assert_eq!(json["clientCertificates"][0]["domain"], "a.com");
}

#[test]
fn resolved_dto_certificate_rows_carry_no_secret_fields() {
    // a PEM cert with a passphrase, a key path and a secret reference
    let text = serde_json::to_string(&to_resolved_dto("c", &flat)).expect("json");
    assert!(!text.contains("hunter2"));
    assert!(!text.contains("/keys/client.key"));
    assert!(!text.contains("payments.cert"));
}
```

- [ ] **Step 3: Run** `cargo test -j4 -p rocket-tauri resolved_dto` (use the package name from `src-tauri/Cargo.toml`). Expected: compile error `no field external_secrets`.

- [ ] **Step 4: Implement** the two DTOs and map the lists in `to_resolved_dto`; add the TypeScript interfaces next to `ResolvedEnvironment`.

- [ ] **Step 5: Run** the filter again, `cargo check -j4 -p <src-tauri package>` and `yarn tsc --noEmit`. Expected: pass.

---

### Task 3: Dialog shows inherited rows and accepts inherited aliases, docs updated

**Files:**
- Modify: `src/lib/certificate-validation.ts`, `src/components/environments/EnvironmentDialog.tsx:325-343,668-700`, `ExternalSecretsTab.tsx`, `CertificatesTab.tsx` and their tests
- Modify: `crates/rocket-environment/CLAUDE.md`, `docs/superpowers/specs/2026-10-18-env-extends-dotenv-design.md`

**Interfaces:**
- Consumes: `ResolvedEnvironment.externalSecrets` and `.clientCertificates` from Task 2.
- Produces: `validateClientCertificates(certs, bindings, inheritedAliases?: string[])`; `ExternalSecretsTab` and `CertificatesTab` take an optional `inherited` array prop of the resolved rows (default empty).

- [ ] **Step 1: Read** `certificate-validation.ts`, the dialog block that builds `inheritedRows`, both tabs and their tests (`EnvironmentDialog.test.tsx`, `ExternalSecretsTab.test.tsx`, `CertificatesTab.test.tsx`).

- [ ] **Step 2: Write the failing tests** (Vitest):
  - `certificate-validation`: `accepts an alias from inheritedAliases` (a vault entry naming `prod` with no own binding returns no issue when `inheritedAliases = ['prod']`, and an issue without it).
  - `ExternalSecretsTab.test.tsx`: `lists inherited bindings read-only with their source` (a row `prod` with source `base` is shown with the text `base` and no input or remove button for it).
  - `CertificatesTab.test.tsx`: `lists inherited certificates read-only with their source`.
  - `EnvironmentDialog.test.tsx`: `overridden_rows_are_not_listed_as_inherited` (resolved bindings contain `prod` with source equal to the selected environment: it is not shown as inherited).

- [ ] **Step 3: Run** `yarn test certificate-validation ExternalSecretsTab CertificatesTab EnvironmentDialog`. Expected: the four new tests fail.

- [ ] **Step 4: Implement.** In the dialog derive `inheritedBindings` and `inheritedCertificates` like `inheritedRows` (rows whose `source` differs from the selected name and whose alias or normalised domain the child does not define), pass them to the tabs, and pass `inheritedBindings.map(b => b.alias)` as `inheritedAliases` to the validator calls. Render the rows with shadcn components and lucide icons as the inherited variable rows do. Update the two docs: the "not inherited" line and the `flatten.rs` module-map line in `CLAUDE.md`, and a pointer line under the "What is inherited?" decision in the #22 spec.

- [ ] **Step 5: Run** `yarn test certificate-validation ExternalSecretsTab CertificatesTab EnvironmentDialog`, `yarn tsc --noEmit` and `yarn check`. Expected: pass.

Next plan: none
