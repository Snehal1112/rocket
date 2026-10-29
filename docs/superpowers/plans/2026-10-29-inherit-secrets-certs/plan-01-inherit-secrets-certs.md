# Inherit Secrets and Certs, Backend Merge Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** A child environment inherits its parent chain's external secret bindings and client certificates in the flatten, the secret fetch, the certificate build and the save checks.

**Architecture:** `flatten_environment` already walks the chain furthest ancestor first. It also merges bindings (by alias) and certificates (by domain) into `Flattened`. `rocket-app` consumers read the merged lists instead of the selected environment's own lists.

**Tech Stack:** Rust, `rocket-environment`, `rocket-app`.

**Spec:** `docs/superpowers/specs/2026-10-29-inherit-secrets-certs-design.md`

## File Structure

- `crates/rocket-environment/src/flatten.rs`: add `FlatBinding`, `FlatCertificate`, the two `Flattened` fields and the merge.
- `crates/rocket-environment/src/lib.rs`: re-export the two structs.
- `crates/rocket-app/src/execution_service.rs`: `resolve_external_secrets_partial` reads merged bindings and returns `DomainResult`.
- `crates/rocket-app/src/client_certificates.rs`: `environment_client_certificates` reads merged lists.
- `crates/rocket-app/src/environment_service.rs`: save validates against merged bindings.

## Global Constraints

- Persistence structs keep `#[serde(default, skip_serializing_if = ...)]` and gain no camelCase rename.
- No panicking unwrap calls in production paths. Tests use `.expect("...")` and `expect_err`.
- Bindings merge by `alias`. Certificates merge by `domain` trimmed and ASCII lowercased, raw text before `{{var}}` resolution.
- The merged certificate list is the child's own entries first, then inherited ones.
- Cargo commands are per crate: `cargo check -j4 -p <crate>`, `cargo test -j4 -p <crate> <filter>`.

## Review Focus

1. A parent binding and a child binding share an alias but differ in case (`Prod` and `prod`): they are different aliases, both kept. Pinned by `aliases_differing_in_case_are_distinct` (Task 1).
2. A certificate domain with surrounding spaces and mixed case matches its parent twin: `domain_match_ignores_case_and_spaces` (Task 1).
3. A child with no `extends` returns exactly its own lists: `no_extends_returns_own_lists_unchanged` (Task 1).
4. A missing parent keeps the child's own lists and warns: `missing_parent_warns_and_keeps_own_lists` (Task 1).
5. A cycle makes the secret fetch fail with the cycle text instead of sending without secrets: `resolve_external_secrets_cycle_returns_the_cycle_error` (Task 2).

---

### Task 1: Merge bindings and certificates in the flatten

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-environment/src/flatten.rs` (types at `:9-22`, `flatten_environment` at `:98-126`, tests module)
- Modify: `crates/rocket-environment/src/lib.rs:29`

**Interfaces:**
- Produces:
  - `pub struct FlatBinding { pub binding: ExternalSecretBinding, pub source: String }` and `pub struct FlatCertificate { pub certificate: ClientCertificate, pub source: String }`, both `Debug, Clone, PartialEq`, exported from the crate root.
  - `Flattened` gains `pub external_secrets: Vec<FlatBinding>` and `pub client_certificates: Vec<FlatCertificate>`; it keeps `Default`.
  - `impl Flattened { pub fn bindings(&self) -> Vec<ExternalSecretBinding>; pub fn certificates(&self) -> Vec<ClientCertificate> }` returning clones without the source.

- [ ] **Step 1: Read `flatten.rs`, `external_secret.rs` and `rocket_shared::certificate::ClientCertificate`.**

- [ ] **Step 2: Write the failing tests in `flatten.rs` tests.** Add helpers `binding(alias, vault) -> ExternalSecretBinding` (connection `"conn"`, empty `secret_names`) and `pkcs12(domain) -> ClientCertificate` (`Pkcs12 { domain, pkcs12_file_path: "/c.p12", pkcs12_secret: None, passphrase: None }`). Reuse `env`, `store` and `flat`. Example:

```rust
#[test]
fn child_binding_replaces_parent_binding_with_same_alias() {
    let mut p = env("p", None, &[]);
    p.external_secrets = vec![binding("prod", "parent-vault"), binding("stage", "s")];
    let mut c = env("c", Some("p"), &[]);
    c.external_secrets = vec![binding("prod", "child-vault")];
    let out = flat(&c, &store(vec![p, c.clone()])).expect("flatten");
    let got: Vec<(&str, &str, &str)> = out
        .external_secrets
        .iter()
        .map(|f| (f.binding.alias.as_str(), f.binding.vault_name.as_str(), f.source.as_str()))
        .collect();
    assert_eq!(got, vec![("stage", "s", "p"), ("prod", "child-vault", "c")]);
}
```

| Test | Setup | Assertion |
|---|---|---|
| `parent_only_bindings_are_inherited` | p binds `a`, c binds nothing | one binding `a`, source `p` |
| `three_level_chain_merges_bindings_furthest_first` | g binds `g1`, p binds `p1`, c binds `c1` | aliases `["g1","p1","c1"]` |
| `aliases_differing_in_case_are_distinct` | p binds `Prod`, c binds `prod` | two bindings |
| `child_certificate_replaces_parent_certificate_with_same_domain` | p cert `a.com`, c cert `a.com` with `/child.p12` | one cert, source `c`, path `/child.p12` |
| `domain_match_ignores_case_and_spaces` | p cert `a.com`, c cert ` A.COM ` | one cert, source `c` |
| `certificates_list_child_first_then_inherited` | p certs `p.com`; c certs `c.com` | domains `["c.com","p.com"]` |
| `no_extends_returns_own_lists_unchanged` | env with one binding and one cert, no parent | both lists equal the env's, source is its name |
| `missing_parent_warns_and_keeps_own_lists` | c extends `gone` with one binding | binding kept, one warning |
| `cycle_still_errors_with_bindings_present` | a extends b extends a, each with a binding | `expect_err`, message contains `a -> b -> a` |

- [ ] **Step 3: Run `cargo test -j4 -p rocket-environment flatten`.** Expected: compile failure `no field external_secrets on type Flattened`.

- [ ] **Step 4: Implement** the two structs, the fields, the two accessors and the merge inside `flatten_environment` (same `chain.layers.iter().rev()` walk). For bindings, replace in place on an alias match, else push. For certificates, collect per layer then build the final list child first: walk layers child to root, skip a certificate whose normalised domain was already taken. Re-export from `lib.rs`.

- [ ] **Step 5: Run `cargo test -j4 -p rocket-environment`.** Expected: all pass, including the nine new tests. Then `cargo check -j4 -p rocket-environment`.

---

### Task 2: Secret fetch and certificate build read the merged lists

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (`resolve_external_secrets` `:658`, `resolve_external_secrets_partial` `:678`, caller `:2860`, test fixture `svc_with_vault`)
- Modify: `crates/rocket-app/src/client_certificates.rs:31-50` (tests use `StaticEnvRepo`, `binding`, `vault_entry`)

**Interfaces:**
- Consumes: `rocket_environment::{flatten_with_repo, Flattened::bindings, Flattened::certificates}` from Task 1.
- Produces:
  - `async fn resolve_external_secrets_partial(&self, collection: Option<&str>, environment_name: Option<&str>) -> DomainResult<(HashMap<String, String>, Vec<UnresolvedBinding>)>`. A missing environment still returns `Ok` with empty results. A cycle returns `Err`.
  - `environment_client_certificates` keeps its signature.
  - Test helpers: `svc_with_vault_envs(envs: Vec<Environment>, fetcher: Arc<FakeVaultFetcher>) -> ExecutionService` next to `svc_with_vault`, and `MapEnvRepo(HashMap<String, Environment>)` implementing `EnvironmentRepository` next to `StaticEnvRepo`.

- [ ] **Step 1: Read** the code above, `flatten_scope` (`execution_service.rs:223`) and the existing `resolve_external_secrets_*` tests.

- [ ] **Step 2: Write the failing tests.**

In `execution_service.rs` tests (use `binding_with_refs`, `FakeVaultFetcher`, `FakeSecretOutcome` as `resolve_external_secrets_resolves_all_secret_names_in_one_binding` does):

```rust
#[tokio::test]
async fn resolve_external_secrets_includes_a_parent_binding() {
    let mut base = Environment::new("base");
    base.external_secrets
        .push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));
    let mut child = Environment::new("child");
    child.extends = Some("base".into());
    let fetcher = Arc::new(FakeVaultFetcher::new(vec![(
        "sec-1",
        FakeSecretOutcome::Value("sk-live-key".to_string()),
    )]));
    let svc = svc_with_vault_envs(vec![base, child], Arc::clone(&fetcher));
    let result = svc
        .resolve_external_secrets(None, Some("child"))
        .await
        .expect("resolve_external_secrets");
    assert_eq!(result.get("payments.apiKey"), Some(&"sk-live-key".to_string()));
}
```

- `resolve_external_secrets_child_binding_wins_over_parent_alias`: parent and child both bind `payments` with different secret ids; the fetcher holds only the child's id; the result holds the child's value and the fetcher saw no call for the parent's id.
- `resolve_external_secrets_cycle_returns_the_cycle_error`: `a` extends `b`, `b` extends `a`; `expect_err` text contains `a -> b -> a`.
- `resolve_external_secrets_missing_environment_is_ok_and_empty`: unknown name returns `Ok` with an empty map.

In `client_certificates.rs` tests:

| Test | Setup | Assertion |
|---|---|---|
| `inherited_certificate_is_presented_for_its_domain` | parent cert `api.example.com`, child extends | child result has that domain |
| `inherited_vault_certificate_resolves_against_parent_alias` | parent binds `prod` and has `vault_entry("api.example.com","prod")` | result is `Deferred`, not `Unavailable` |
| `inherited_certificate_placeholders_use_child_variables` | parent cert domain `{{apiHost}}`, vars `apiHost=x.com` | domain `x.com` |
| `cycle_falls_back_to_own_certificates` | a and b cycle, `a` has its own cert | result is `a`'s own cert only |

Use `MapEnvRepo` and `describe_all` as `resolve_env` does.

- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app resolve_external_secrets` and `cargo test -j4 -p rocket-app client_certificates`. Expected: compile errors for `svc_with_vault_envs`, `MapEnvRepo`, then assertion failures for the inherited cases.

- [ ] **Step 4: Implement.**
  - Partial: `get(name)` failing stays `Ok(empty)`. Otherwise `flatten_with_repo(&env, repo.as_ref())?`, log its warnings, loop over `flat.bindings()` instead of `env.external_secrets`. Wrap the return in `Ok`. `resolve_external_secrets` and the caller at `:2860` use `?`. Update every other caller found with `grep resolve_external_secrets_partial`.
  - Certificates: after `repo.get(name)`, flatten with `flatten_with_repo`. On `Err` log a `tracing::warn!` and use `env.client_certificates` and `env.external_secrets` as today. On `Ok` use `flat.certificates()` and `flat.bindings()`. Keep the rest of the pipeline.

- [ ] **Step 5: Run** both filters again, then `cargo test -j4 -p rocket-app execution_service` and `cargo check -j4 -p rocket-app`. Expected: pass.

---

### Task 3: Save validates against the merged bindings

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/environment_service.rs:52-112` (tests near `:382-440`)

**Interfaces:**
- Consumes: `flatten_environment`, `Flattened::bindings` from Task 1.
- Produces: `save` and `save_with_capabilities` validate `validate_client_certificates` and `validate_vault_certificate_providers` against the merged bindings; `validate_external_secret_bindings` still checks the environment's own list.

- [ ] **Step 1: Read** `save`, `save_with_capabilities` and the tests that build `ClientCertificate::Vault` entries (`:382-440`).

- [ ] **Step 2: Write the failing tests** using the service fixture those tests use, saving the parent first:
  - `save_accepts_a_vault_certificate_naming_a_parent_alias`: parent `base` saved with binding `prod`; child `extends: base` with `ClientCertificate::Vault { domain: "api.example.com", binding: "prod", certificate: "c1", format: VaultCertificateFormat::Pkcs12 }`; `save` is `Ok`.
  - `save_rejects_a_vault_certificate_naming_an_unknown_alias`: same child, parent without the binding; `expect_err`, message names `prod`.
  - `save_does_not_persist_inherited_bindings_or_certificates`: after saving the first child, `svc.get("child")` has empty `external_secrets` and one certificate.
  - `save_with_missing_parent_uses_only_own_bindings`: child extends `gone` with a vault certificate naming `prod` and no own binding; `expect_err`.

- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app environment_service`. Expected: `save_accepts_a_vault_certificate_naming_a_parent_alias` fails with the "no binding" error.

- [ ] **Step 4: Implement.** Move the cycle check to the top of `save`, before the validations. Build the lookup closure `own` once. Then `flatten_environment(env, &own)`; on `Err` return it, on `Ok` use `flat.bindings()` for the two certificate validations. A missing parent warns (`tracing::warn!` for each flatten warning) and flatten has then returned the child's own lists. `save_with_capabilities` does the same flatten with the repo lookup plus the unsaved child, then calls `save`.

- [ ] **Step 5: Run** `cargo test -j4 -p rocket-app environment_service`, `cargo test -j4 -p rocket-app`-scoped filters for the earlier tasks, and `cargo check -j4 -p rocket-app`. Expected: pass.

Next plan: docs/superpowers/plans/2026-10-29-inherit-secrets-certs/plan-02-inherit-secrets-certs.md
