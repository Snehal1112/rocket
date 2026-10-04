# Azure Key Vault provider, Plan Index

**Spec:** [../../specs/2026-10-04-azure-key-vault-provider-design.md](../../specs/2026-10-04-azure-key-vault-provider-design.md)

**Scope:** sub-project 2 of 5. It builds the Azure Key Vault provider on the foundation seam. AWS, HashiCorp and Google each get their own brainstorm, spec and plan afterwards.

> 📖 Before starting any plan, read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Plan breakdown: 3 plans, 9 tasks (max 3 per plan)

| # | Plan | Tasks | Area | Depends on |
|---|---|---|---|---|
| 01 | [Domain, IPC and service](2026-10-04-azure-plan-01-domain-and-service.md) | 3 | `rocket-environment`, `src-tauri`, `rocket-app`, `rocket-infra` (repo) | none |
| 02 | [Azure fetcher and wiring](2026-10-04-azure-plan-02-fetcher.md) | 3 | `rocket-infra`, `src-tauri` | 01 |
| 03 | [Frontend](2026-10-04-azure-plan-03-frontend.md) | 3 | frontend | 01 (IPC shape), 02 (to try it for real) |

Each plan ends with a **Next Plan** section. Chain to the next plan automatically when a plan finishes, one at a time.

## Locked interface contract

Every plan is written against these names. If an implementer must deviate, update this index and every plan that mentions the name.

### `rocket-environment`

```rust
// secret_manager.rs
pub enum ProviderConfig {
    Azure {
        tenant_id: String,
        authority_host: Option<String>, // skipped when None
    },
}
```

Persisted as a serde YAML tag: `config: !Azure {tenant_id: ...}`. No camelCase.

### `src-tauri`

```rust
// commands/secret_managers.rs
#[serde(tag = "kind")]
pub enum ProviderConfigDto {
    #[serde(rename = "azure", rename_all = "camelCase")]
    Azure { tenant_id: String, authority_host: Option<String> },
}
```

JSON on the wire: `{ "kind": "azure", "tenantId": "...", "authorityHost": "..." }`. `SecretManagerConnectionDto.config` is `Option<ProviderConfigDto>`.

### `rocket-infra`

```rust
// azurekeyvault/mod.rs
pub struct AzureKeyVaultFetcher { /* http client, token cache */ }
impl AzureKeyVaultFetcher { pub fn new() -> Self }
// implements VaultSecretFetcher

// secret_providers.rs
DispatchingSecretFetcher::with_providers() // replaces with_rocketvault()

// rocketvault/mod.rs, widened from private to pub(crate) for reuse
pub(crate) const REQUEST_TIMEOUT: Duration;
pub(crate) fn secret_fingerprint(secret: &str) -> String;
pub(crate) fn token_expiry_cutoff(expires_at: Instant) -> Instant;
pub(crate) fn compute_token_ttl(expires_in: u64) -> Duration;
pub(crate) fn is_loopback_url(url: &url::Url) -> bool;
```

### Frontend

```ts
// src/lib/tauri-api.ts
export interface AzureConfig { kind: 'azure'; tenantId: string; authorityHost?: string }
export type ProviderConfig = AzureConfig;
// SecretManagerConnection.config?: ProviderConfig | null

// src/lib/secret-providers.ts
export type ConnectionField = 'baseUrl' | 'tenantId' | 'clientId' | 'clientSecret' | 'verifySsl' | 'allowInsecureHttp';
// SecretProviderDescriptor.fieldLabels: per-provider label overrides
```

## Global constraints

- `cargo` commands always take `-j4`. No full `cargo test --workspace`. Use targeted crate tests plus `cargo check -j4`.
- Never call `unwrap` in production paths. Tests may use `expect`.
- `#[serde(rename_all = "camelCase")]` on IPC DTOs only, never on persistence structs.
- Commit through the `dev-workflow-skills:1-git-commit` skill, with conventional commit prefixes.
- Do not touch or port the uncommitted `crates/rocket-app/src/execution_service.rs` work in the main checkout. Fetch-on-reference is out of scope.
- This worktree needs its own cargo target dir. Do not share main's `target/`.
- Frontend: shadcn primitives and lucide icons only. Run `yarn tsc --noEmit` and `yarn check`.

## Final verification (after plan 03)

```bash
cargo check -j4
cargo test -j4 -p rocket-environment secret_manager
cargo test -j4 -p rocket-app secret_manager_service
cargo test -j4 -p rocket-infra azurekeyvault
cargo test -j4 -p rocket-infra secret_providers
cargo test -j4 -p rocket-infra fs_secret_manager_repo
cargo test -j4 -p rocket secret_managers
yarn tsc --noEmit && yarn check && yarn test secret
```
