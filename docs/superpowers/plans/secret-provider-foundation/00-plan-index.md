# Secret provider foundation, Plan Index

**Spec:** [../../specs/2026-10-04-secret-provider-foundation-design.md](../../specs/2026-10-04-secret-provider-foundation-design.md)

**Scope:** sub-project 1 of 5. It adds the provider seam and builds no new provider. Azure Key Vault, AWS Secrets Manager, HashiCorp Vault and Google Secret Manager each get their own brainstorm, spec and plan afterwards.

## Plan breakdown: 4 plans, 11 tasks (max 3 per plan)

| # | Plan | Tasks | Area | Depends on |
|---|---|---|---|---|
| 01 | [Domain types, capabilities and persistence](2026-10-04-secret-provider-plan-01-domain.md) | 3 | `rocket-environment`, `rocket-infra` (tests) | none |
| 02 | [Dispatching fetcher, service validation and IPC](2026-10-04-secret-provider-plan-02-dispatch-and-service.md) | 3 | `rocket-infra`, `rocket-app`, `src-tauri` | 01 |
| 03 | [Fetch-on-reference and certificate gating](2026-10-04-secret-provider-plan-03-resolution-and-certs.md) | 1 done, 1 deferred | `rocket-environment`, `rocket-app`, `src-tauri` | 01, 02 |
| 04 | [Frontend provider selector and certificate gating](2026-10-04-secret-provider-plan-04-frontend.md) | 3 | frontend | 02 (03 for the save-time error) |

Plan 03 has 2 tasks. Task 2 (certificate gating) is done. Task 1 (fetch-on-reference) is deferred to the first cloud provider's plan. Each plan ends with a **Next Plan** section, so a fresh session opening any one file knows what to run next. Chain to the next plan automatically when a plan finishes, one at a time.

## Locked interface contract

Every plan is written against these names. If an implementer must deviate, update this index and every plan that mentions the name.

### `rocket-environment`

```rust
// secret_manager.rs
pub enum SecretProviderKind { RocketVault /* default */, Azure, Aws, Hashicorp, Gcp }
impl SecretProviderKind { pub fn is_default(&self) -> bool; pub fn display_name(&self) -> &'static str; }
pub enum ProviderConfig {}                       // no variants yet
pub struct SecretManagerConnection { /* existing fields */, pub provider: SecretProviderKind, pub config: Option<ProviderConfig> }
pub struct ConnectionProvider { pub kind: SecretProviderKind, pub capabilities: ProviderCapabilities }
pub trait ProviderCapabilityLookup: Send + Sync {
    fn provider_of(&self, connection_id: &str) -> DomainResult<Option<ConnectionProvider>>;
}

// vault_secret_fetcher.rs
pub struct ProviderCapabilities { pub certificates: bool, pub credential_optional: bool, pub fetch_on_reference: bool }
// VaultSecretFetcher gains (both defaulted):
fn capabilities(&self, connection: &SecretManagerConnection) -> ProviderCapabilities;   // default: all false
fn validate_connection(&self, connection: &SecretManagerConnection) -> DomainResult<()>; // default: Ok(())

// client_certificate_validation.rs
pub fn validate_vault_certificate_providers(
    certs: &[ClientCertificate],
    bindings: &[ExternalSecretBinding],
    lookup: &dyn ProviderCapabilityLookup,
) -> DomainResult<()>;
```

### `rocket-infra`

```rust
// secret_providers.rs
pub struct DispatchingSecretFetcher;   // implements VaultSecretFetcher
impl DispatchingSecretFetcher {
    pub fn new() -> Self;
    pub fn with_rocketvault() -> Self;
    pub fn register(&mut self, kind: SecretProviderKind, fetcher: Arc<dyn VaultSecretFetcher>);
}
```

### `rocket-app`

```rust
// environment_service.rs
pub fn save_with_capabilities(&self, env: &Environment, lookup: &dyn ProviderCapabilityLookup) -> DomainResult<()>;
// secret_manager_service.rs: impl ProviderCapabilityLookup for SecretManagerService
```

The execution_service.rs functions `resolve_external_secrets_partial` and `references_text` belong to the deferred Plan 03 Task 1 and are not part of this branch.

### IPC and frontend

```ts
// src/lib/tauri-api.ts
export type SecretProviderKind = 'rocketvault' | 'azure' | 'aws' | 'hashicorp' | 'gcp';
// SecretManagerConnection gains: provider?: SecretProviderKind; config?: Record<string, unknown> | null;

// src/lib/secret-providers.ts
export type ConnectionField = 'baseUrl' | 'clientId' | 'clientSecret' | 'verifySsl' | 'allowInsecureHttp';
export interface SecretProviderDescriptor { kind; label; selectable; connectionFields; scopeLabel; scopePlaceholder; supportsCertificates }
export const SECRET_PROVIDERS: readonly SecretProviderDescriptor[];
export function getProviderDescriptor(kind?: SecretProviderKind | null): SecretProviderDescriptor;
export function bindingScopeColumnLabel(bindings, connections): string;
export function canAddVaultCertificate(bindings, connections, connectionsLoaded: boolean): boolean;

// CertificatesTabProps gains: canAddVaultCertificate?: boolean (default true)
```

IPC JSON: the connection carries `provider` (lowercase string, omitted by older clients, defaulting to `rocketvault`) and an optional `config`. No command names or argument names change. `save_environment` and `save_global_environment` gain a Tauri-injected `State` argument that the frontend never sends.

## Status

Plans 01, 02 and 04 are complete. Plan 03 Task 2 (certificate gating) is complete. Plan 03 Task 1 (fetch-on-reference) is deferred to the first cloud provider's plan: it depends on uncommitted partial-failure resolution code outside this branch, and no provider uses it yet. The `fetch_on_reference` capability flag exists and is read by nothing.

## Decisions that differ from the first spec draft

The spec was corrected to match these before the plans were written, because the code showed the draft was incomplete:

1. `ProviderCapabilities` has three flags: `certificates`, `credential_optional` and `fetch_on_reference`.
2. Fetch-on-reference, when built, applies only to providers that opt in, and it is deferred. RocketVault does not opt in, so its behavior is unchanged. A secret name built dynamically in a script cannot be seen by a text check.
3. `EnvironmentService` holds no connections, so certificate gating uses a `ProviderCapabilityLookup` passed to `save_with_capabilities`.
4. The runner and the flow executor keep fetching every ref once per run.

## Verification after the last plan

- `cargo check -j4 -p rocket --tests`
- Targeted tests per plan, always with `-j4`. Never run `cargo test --workspace`.
- `yarn tsc --noEmit` and `yarn check`
- Manual check in the real app: open Settings, add a RocketVault connection, bind it in an environment, send a request that uses `{{alias.secret}}`, and confirm the certificate button appears only with a RocketVault binding.

## Carry into the provider plans

- Fetch-on-reference (the deferred Plan 03 Task 1): build it in the first cloud provider's plan, on top of the partial-failure resolution code once that is committed.
- Add a guard in `SecretManagerService::save` that a saved connection's provider cannot change, so a stored credential is never sent to a different provider over IPC.
- Forward `config` from the connections dialog on save (it currently builds the connection without `config`), or editing a connection will erase it once a provider uses `config`.
- Add the test for a non-RocketVault provider that needs a credential, saved with no stored credential, at connection time.
- Environment saves now read `secret_managers.yml` through `save_with_capabilities` when an environment has a `vault` certificate. A file written by a newer build with an unknown provider or a `config` fails that save loudly. Decide whether that is acceptable when the first provider lands.
