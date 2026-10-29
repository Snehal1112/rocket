# Inherit external secret bindings and client certificates

Ticket: https://github.com/Snehal1112/rocket/issues/110 (follow-up of #22)

## Goal

Make a child environment inherit its parent chain's external secret bindings and client certificates, the way it already inherits variables, so a `{{alias.secret}}` placeholder, `rok.getSecretVar` and mutual TLS work in the child without copying the parent's setup.

## Overview

`extends` merges variables only. A child that extends `base` sees `base`'s variables, but not the RocketVault bindings or the client certificates `base` defines. A user must copy them into every child, and a copied binding drifts from the parent. Ticket #22 left this out on purpose (`docs/superpowers/specs/2026-10-18-env-extends-dotenv-design.md`, decision "What is inherited?").

## Current state

| Area | Today | Where |
|---|---|---|
| Flatten | Merges `variables` only, returns `Flattened { variables, warnings }` | `crates/rocket-environment/src/flatten.rs:9-22`, `:98-126` |
| Secret fetch | Reads `external_secrets` of the selected environment alone | `crates/rocket-app/src/execution_service.rs:678-705` (binding loop at `:696`) |
| Certificates for a send | Reads `client_certificates` and `external_secrets` of the selected environment alone | `crates/rocket-app/src/client_certificates.rs:31-50` |
| Save validation | Checks the environment's own bindings, and its certificates against its own bindings | `crates/rocket-app/src/environment_service.rs:52-60`, `:104-112` |
| Flow lint names | Adds `alias.secret` names from the environment's own bindings | `crates/rocket-app/src/flow_lint_context.rs:105-109` |
| MCP read view | `vault_references` from the environment's own bindings | `crates/rocket-app/src/mcp_read_views.rs:318-330` |
| IPC | `ResolvedEnvironmentDto` has variables and warnings only | `src-tauri/src/commands/environments.rs:244-250` |
| Dialog | Inherited rows exist for variables only | `src/components/environments/EnvironmentDialog.tsx:325-343` |

Gaps: every consumer above sees only the selected environment, so inherited aliases do not resolve and inherited certificates are never presented.

## Non-goals

- Inheriting `dotEnvFilePath` (already layered by #22).
- Inheriting across stores (collection to global or the reverse).
- Persisting the merged result. Saving writes only the environment's own lists.
- Showing inherited certificate passphrases or any secret value in the UI.

## Design

Flow: `Environment + parent lookup -> flatten (variables, bindings, certificates) -> consumers`.

### 1. Merge in `rocket-environment`

`flatten_environment` already walks the chain furthest ancestor first. It gains two outputs in `Flattened`:

```rust
pub struct FlatBinding { pub binding: ExternalSecretBinding, pub source: String }
pub struct FlatCertificate { pub certificate: ClientCertificate, pub source: String }
// Flattened gains:
pub external_secrets: Vec<FlatBinding>,
pub client_certificates: Vec<FlatCertificate>,
```

- Bindings merge by `alias`. A later layer (closer to the child) replaces an earlier binding with the same alias, and the replacement takes the later layer's position. Other bindings are kept, ancestors first.
- Certificates merge by `domain`, compared after trimming and ASCII lowercasing, on the raw text before `{{var}}` resolution. This key is coarser than the URL matcher in `crates/rocket-http/src/client_cert.rs:16-26`, which also understands a scheme, port and path. `https://a.com` and `a.com` are two entries in the merge, and the child-first order keeps the child preferred when both match. A child certificate replaces a parent certificate of the same domain. The result lists the child's own certificates first in their own order, then the inherited ones, so the first-match rule in `rocket_http::client_cert::find_certificate` prefers the child.
- These lists have no disabled state, so there is no shadowing rule.
- A cycle is an error and a missing parent only warns, exactly as for variables.
- Both new structs derive `Debug`, `Clone` and `PartialEq`, and `Flattened` keeps `Default`.

Example: `base` binds `prod` and `stage`; `child` binds `prod` with another vault. The merged list is `stage (base)`, `prod (child)`.

### 2. Consumers use the merged lists

- `resolve_external_secrets_partial` flattens the selected environment and loops over the merged bindings. `resolve_external_secrets_partial` returns `DomainResult<(map, failures)>` instead of a bare tuple, so a cycle is an `Err` that `resolve_external_secrets` and every caller of the partial form propagate. A normal send already fails earlier on the variable flatten (`execution_service.rs:227`), so this error matters for direct callers. Global environments inherit within the global store the same way. `rok.getSecretVar`, `{{alias.secret}}`, the collection runner, flows, WebSocket and OAuth2 all go through this call, so they inherit it.
- `environment_client_certificates` takes the merged certificates and merged bindings. The `{{var}}` placeholders in an inherited certificate resolve against the child's variables. On a cycle it falls back to the environment's own lists and logs a warning, because the variable flatten already failed the send.
- `flow_lint_context` adds the merged `alias.secret` names. Its completeness rule for environments with `extends` stays as it is (`flow_lint_context.rs:94-111`), so unknown names are still not flagged and the existing test `an_environment_with_inheritance_leaves_unknown_names_unanswered` keeps passing.
- `get_environment` keeps the environment's own bindings when the chain is broken by a cycle, like lint and the certificate build.
- `MaskedEnvironment` keeps `from_environment` and gains a second constructor that takes the merged bindings, used by `get_environment` in `mcp_tool_service.rs`. Inherited variables in that view stay a follow-up of #22.
- The wrapper at `execution_service.rs:1458` keeps its signature and passes the repo through, as does `oauth2_service.rs:195-210`.

### 3. Save validation uses the merged bindings

`EnvironmentService::save` validates `validate_external_secret_bindings` on the environment's own list (a duplicate alias inside one list is still an error). `validate_client_certificates` and `validate_vault_certificate_providers` receive the merged bindings (own plus inherited, own wins), so a child certificate can name an alias that only the parent defines. A missing parent only warns, and then only the child's own bindings count. The cycle check runs first, so the merge never sees a cycle. The merge uses the same `own` lookup as the cycle check, so the unsaved child is the one validated. `save_with_capabilities` may flatten again, which is cheap.

### 4. UI

- `ResolvedEnvironmentDto` gains `externalSecrets` and `clientCertificates`, each row with `source`.
  - Binding row: `alias`, `vaultName` (the binding's `vault_name`), `connectionId`, `secretCount` (`secret_names.len()`), `secretNames` (names only, no ids), `source`. The DTO lists every merged row, and the UI filters out rows whose `source` is the selected environment, as for variables.
  - Certificate row: `type`, `domain`, `source`. No path, passphrase or secret reference is sent.
- The External Secrets tab and the Client Certificates tab list inherited rows read-only after the child's own rows, labelled with the source environment, like inherited variables. Rows the child overrides are not listed as inherited.
- The dialog's client certificate validation (`src/lib/certificate-validation.ts`) takes an optional `inheritedAliases` list from the saved flatten and accepts those aliases. It does not see unsaved parent edits, so a parent change must be saved first, and the backend check at save is the authority. Saving never writes inherited rows into the child.

### 5. Docs

Update `crates/rocket-environment/CLAUDE.md` (its "not inherited" line and the `flatten.rs` module-map line), and point the "What is inherited?" decision in the #22 spec to this one.

## Error handling and edge cases

| Case | Behaviour |
|---|---|
| Same alias in parent and child | Child wins, parent's other aliases stay |
| Same domain, different case or spaces | Treated as the same domain, child wins |
| Child vault certificate names a parent-only alias | Save accepts it |
| Parent later deleted, child certificate names its alias | Save-time check is not rerun; at send the entry becomes `Unavailable` and fails only when selected |
| Parent missing at save | Warns, only the child's bindings count for validation |
| Cycle at secret fetch | Send fails with `Environment extends cycle: a -> b -> a` |
| Cycle at certificate build | Own lists used, warning logged, send already failed on variables |
| Inherited binding whose connection was deleted | Skipped silently, as today |
| Inherited binding fetch fails | Hard failure of the send, as today |
| Environment with no parent | Identical to today |

## Testing

- `rocket-environment` (`flatten.rs`): `child_binding_replaces_parent_binding_with_same_alias`, `parent_only_bindings_are_inherited`, `three_level_chain_merges_bindings_furthest_first`, `child_certificate_replaces_parent_certificate_with_same_domain`, `domain_match_ignores_case_and_spaces`, `certificates_list_child_first_then_inherited`, `cycle_still_errors_with_bindings_present`, `missing_parent_warns_and_keeps_own_lists`.
- `rocket-app` (`execution_service.rs`): `resolve_external_secrets_includes_a_parent_binding`, `resolve_external_secrets_cycle_returns_the_cycle_error`.
- `rocket-app` (`client_certificates.rs`): `inherited_certificate_is_presented_for_its_domain`, `inherited_vault_certificate_resolves_against_parent_alias`, `inherited_certificate_placeholders_use_child_variables`, `cycle_falls_back_to_own_certificates`.
- `rocket-app` (`environment_service.rs`): `save_accepts_a_vault_certificate_naming_a_parent_alias`, `save_rejects_a_vault_certificate_naming_an_unknown_alias`, `save_does_not_persist_inherited_bindings_or_certificates`.
- `rocket-app` (`flow_lint_context.rs`, `mcp_read_views.rs`): `lint_names_include_parent_secrets`, `vault_references_include_parent_secrets`.
- `src-tauri`: `resolved_dto_lists_inherited_bindings_and_certificates_with_source`, `resolved_dto_certificate_rows_carry_no_secret_fields`.
- Frontend: `EnvironmentDialog.test.tsx` shows inherited binding and certificate rows read-only with their source; `certificate-validation` accepts an inherited alias.
- Commands: `cargo test -j4 -p rocket-environment`, `cargo test -j4 -p rocket-app <name>`, `yarn test EnvironmentDialog`, then `cargo check -j4`, `yarn tsc --noEmit`, `yarn check`.

## Files changed

- `crates/rocket-environment/src/flatten.rs`, `src/lib.rs`, `CLAUDE.md`
- `crates/rocket-app/src/execution_service.rs`, `client_certificates.rs`, `environment_service.rs`, `flow_lint_context.rs`, `mcp_read_views.rs`, `oauth2_service.rs` (callers of the certificate helper, signature kept)
- `src-tauri/src/commands/environments.rs`
- `src/lib/tauri-api.ts`, `src/lib/certificate-validation.ts`, `src/components/environments/EnvironmentDialog.tsx`, `ExternalSecretsTab.tsx`, and the certificates tab component

## Constraints

- Persistence structs keep `#[serde(default, skip_serializing_if = ...)]` and gain no camelCase rename. New IPC DTOs use camelCase.
- No panicking unwrap calls in production paths, no `git` CLI.
- UI uses shadcn/ui primitives and lucide-react icons only.
- Rows sent to the frontend never carry key material, passphrases or secret values.
- Read `docs/superpowers/specs/opencollection-spec-reference.md` before touching environment code.

## Acceptance criteria

- A child resolves `{{alias.secret}}` and `rok.getSecretVar('alias.secret')` for a binding defined only by its parent.
- A child binding with the same alias as a parent binding wins, and the parent's other bindings stay.
- A chain of three environments merges bindings furthest ancestor first.
- A child presents an inherited client certificate for a matching URL, and its own certificate for the same domain wins.
- Domain matching for the merge ignores case and surrounding spaces.
- A child's `vault` certificate can name an alias only the parent defines, and save accepts it.
- Saving a child never writes inherited bindings or certificates into its file.
- A cycle fails secret fetch with an error naming the environments.
- Flow lint's known names and the MCP read view's `vaultReferences` include the parent's `alias.secret` names.
- The environment dialog lists inherited bindings and certificates read-only with their source environment, and sends no secret field.
- An environment with no `extends` behaves exactly as before.
- `cargo check -j4`, the targeted crate tests, `yarn tsc --noEmit` and `yarn check` pass.

## Out of scope

- Per-item opt-out of inheritance.
- Making flow lint flag unknown names for inheriting environments.
- Inherited variables in the MCP environment view.
- Inheriting from an environment in another store.
- Editing an inherited row from the child dialog.

## Follow-ups

None. The ticket is fully built by this spec.

## Decisions

- How do bindings combine along the chain? Options: merge by alias with child winning; child list replaces parent list; parent list only when child is empty. Chosen: merge by alias, child wins. Decided by: user
- How do client certificates combine? Options: merge by domain with child winning; child list replaces parent list; concatenate child first. Chosen: merge by domain, child wins. Decided by: user
- Save check for a child certificate naming a parent-only alias? Options: validate against merged bindings; validate the child alone. Chosen: validate against merged bindings. Decided by: user
- How far does inheritance reach? Options: everywhere bindings and certificates are read; send paths only. Chosen: everywhere. Decided by: user
- What does the dialog show? Options: read-only inherited rows with source; no UI change. Chosen: read-only rows with source. Decided by: user
- Domain comparison for the merge? Options: exact text; trimmed and case-insensitive. Chosen: trimmed and ASCII case-insensitive on the raw text, because hostnames are case-insensitive. Decided by: call
- Certificate order in the merged list? Chosen: child's own first, then inherited, so the first-match rule prefers the child. Decided by: call
- What do inherited certificate rows expose to the UI? Chosen: type, domain and source only, so no passphrase, path or secret reference leaves the backend. Decided by: call
- Cycle at certificate build? Chosen: fall back to own lists with a warning, because the variable flatten already failed the send. Decided by: call
- Where does the merge live? Chosen: in `flatten_environment`, next to the variable merge, so one walk serves all consumers. Decided by: call
- Does flow lint start flagging unknown names for inheriting environments? Chosen: no, its completeness rule stays, because changing it alters existing lint behaviour. Decided by: call
- How does a cycle fail the secret fetch? Chosen: the partial function returns a `DomainResult`, so the cycle is an `Err`. Decided by: call
- Does the dialog see unsaved parent edits? Chosen: no, it uses the saved flatten and the backend validates at save. Decided by: call
- Should the inherited binding rows list secret names? Chosen: yes, names only and never ids or values, because the dialog must check and offer a certificate's secret reference against inherited bindings. This replaces the first plan's count-only rows. Decided by: call
- Should an MCP read of a cyclic environment fail? Chosen: no, it keeps the own bindings, so all three readers behave the same on a cycle. Decided by: call
