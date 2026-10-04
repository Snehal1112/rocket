# Design: Secret provider foundation

**Status:** Draft for review
**Sub-project:** 1 of 5 (foundation). Providers 2-5 each get their own spec and plan:
Azure Key Vault, AWS Secrets Manager, HashiCorp Vault, Google Secret Manager.
**Builds on:** [RocketVault external secrets](2026-09-22-rocketvault-external-secrets-spec.md)
and [vault certificate source](2026-10-02-vault-certificate-source-design.md).

> 📖 Before starting implementation, read `docs/superpowers/specs/opencollection-spec-reference.md`.

## 1. Problem

Rocket can pull secrets from one kind of secret manager, RocketVault. The code assumes
that in every layer: the connection shape (`base_url`, `client_id`, one `client_secret`),
the OAuth2 token endpoint, the secret-id based fetch, and the UI copy. Users also run
Azure Key Vault, AWS Secrets Manager, HashiCorp Vault and Google Secret Manager.

RocketVault keeps one extra capability the others lack: it can supply client
certificates (the `vault` certificate entry type and `fetch_certificate`).

## 2. Goals

- A connection has a provider. Existing RocketVault connections keep working with no
  migration and no change to saved files.
- Adding a provider later touches only that provider's implementation, its config
  fields and its UI form. It does not touch `rocket-app`, resolution, scripting or
  redaction.
- `{{alias.secretName}}` and `rok.getSecretVar('alias.secretName')` behave the same for
  every provider.
- Only providers that support certificates offer certificate features.
- The seam leaves room to stop hitting cloud providers with a fetch for every secret on
  every send (the work itself is deferred, see section 6).

## 3. Non-goals

- Implementing any provider other than RocketVault. This spec adds the seam only.
- Writing secrets back to a provider. The integration stays read-only.
- Generalizing the ACP agent config. It keeps its RocketVault-shaped fields and is
  documented as a limitation (see 9).
- Live calls to real cloud services in CI.

## 4. Data model

### 4.1 Connection

`SecretManagerConnection` (`crates/rocket-environment/src/secret_manager.rs`) gains:

| Field | Type | Default | Notes |
|---|---|---|---|
| `provider` | `SecretProviderKind` | `rocketvault` | `rocketvault`, `azure`, `aws`, `hashicorp`, `gcp` |
| `config` | `Option<ProviderConfig>` | `None` | Typed, non-secret, provider specific |

`SecretProviderKind` serializes as a lower-case string. `ProviderConfig` is an enum with
one struct per provider, added by each provider's own spec. In this foundation it has no
variants for the new providers, so a connection with an unknown provider is rejected on
load with a clear error naming the provider.

The existing fields (`base_url`, `client_id`, `verify_ssl`, `allow_insecure_http`) stay
as they are. RocketVault uses them as before. For other providers they may be empty and
are ignored. Validation (section 5.3) decides which fields each provider needs.

Persistence rules:

- Serde only. No camelCase rename on the persisted struct (project rule). The IPC DTO
  keeps camelCase.
- Every new field is `#[serde(default)]`. An old `secret_managers.yml` loads as
  `provider: rocketvault`, `config: None`.
- A provider-less row written by a new build still loads in an old build, because the
  new fields are skipped when they hold their defaults (`skip_serializing_if`).

### 4.2 Credential

The keychain keeps one string per connection id, in the existing
`com.rocketapi.vault-connection` service. The content is provider defined:

- RocketVault: the client secret string, exactly as today.
- Other providers: whatever the provider needs (a JSON document for a key pair or a
  service account, a token, and so on), parsed by that provider's implementation.

The credential becomes optional per provider, because some modes need no stored secret
(for example Azure CLI or an AWS profile). Core code never inspects the string. It only
passes it through.

### 4.3 Bindings and references

- `ExternalSecretBinding` is unchanged. The provider comes from the binding's
  connection, so old environment files load unchanged.
- `vault_name` keeps its persisted name. It is documented as the provider's scope:
  vault name for RocketVault, vault URL or name for Azure, region for AWS, mount for
  HashiCorp, project for GCP. Each provider's spec defines its meaning.
- `ExternalSecretRef.secret_id` becomes an opaque string defined by the provider (for
  example the secret name for Azure, a path plus key for HashiCorp). Core code never
  parses it.
- The alias rules do not change: letters, digits, `_` and `-`, no dot, unique per
  environment. The alias `flow-auth` is reserved because the flow executor shares the
  same map.

## 5. Backend

### 5.1 Contract

`VaultSecretFetcher` (`crates/rocket-environment/src/vault_secret_fetcher.rs`) stays the
contract: `list_secrets`, `get_secret_value`, `test_connection`, `forget_connection`,
and the two default-refusing certificate methods. It gains:

```rust
fn capabilities(&self, connection: &SecretManagerConnection) -> ProviderCapabilities;
```

`ProviderCapabilities { certificates: bool, credential_optional: bool,
fetch_on_reference: bool }`. The default is all false.

- `certificates`: the provider can list and export client certificates. Only RocketVault
  reports true.
- `credential_optional`: a new connection may be saved with no stored credential (Azure
  CLI mode, an AWS profile). RocketVault reports false.
- `fetch_on_reference`: reserved for limiting send-time fetches to referenced secrets
  (section 6). Nothing reads it yet. RocketVault reports false.

The trait documentation is reworded from "RocketVault" to "secret manager".

### 5.2 Dispatch

A `DispatchingSecretFetcher` in `crates/rocket-infra` implements the trait. It holds a
map from `SecretProviderKind` to `Arc<dyn VaultSecretFetcher>` and routes every call on
`connection.provider`. A provider with no registered implementation returns an error
that names the provider.

`src-tauri/src/lib.rs` builds the map once and injects one
`Arc<dyn VaultSecretFetcher>` exactly as today. `rocket-app` keeps holding only the
trait, so the DDD boundary rule holds.

`ReqwestVaultSecretFetcher` is registered as the `rocketvault` provider unchanged.
`forget_connection` is forwarded to the owning provider, because token and
certificate-id caches live inside each implementation.

### 5.3 Service validation

`SecretManagerService::validate_connection` moves to per-provider rules:

- Always: non-empty `id` and `label`.
- RocketVault: the current rules (http(s) `base_url`, non-empty `client_id`) stay in the
  service. A new connection still needs a `client_secret`.
- Other providers: rules supplied by the provider through a `validate_connection` hook
  on the trait, with a default that accepts. Each provider spec defines its own. A new
  connection needs a credential unless the provider reports `credential_optional`.

## 6. Resolution at send time

The merge of `alias.secretName` into `VariableContext.external_secrets`, redaction, the
write guard and scripting are unchanged. RocketVault keeps fetching every ref of every
binding, as it does today.

Fetching only the secrets a request references is deferred to the first cloud provider's
own plan, because no provider uses it yet. With a cloud provider every fetch costs
latency and money per call, so that plan will limit a single send (`execute()`) to the
refs the request references, using an exact `alias.secretName` text match. Two rules are
fixed now so that plan has a clear target:

- The limit applies only to connections whose provider reports `fetch_on_reference`.
  RocketVault does not, so its behavior never changes. A script that builds a secret name
  dynamically (`getSecretVar('al' + 'ias.name')`) cannot be seen by a text check, so a
  provider opts in only when its fetch cost justifies that limit.
- The runner and the flow executor keep fetching every ref once per run.

The `fetch_on_reference` flag exists in this foundation, defaults to false and is read
by nothing yet. The work also depends on the partial-failure resolution code (a failed
binding fails a send only when the request references that alias) that is in progress
outside this branch, so it cannot be built on committed code today.

## 7. Certificates

- `ClientCertificate::Vault`, `VaultCertificateBinding`, `CertificateMaterial::Deferred`,
  `fetch_certificate` and `list_certificates` remain RocketVault features.
- Validation rejects a `vault` entry whose binding points at a connection that does not
  report `certificates: true`. The pure validator in `rocket-environment` cannot see
  connections, and `EnvironmentService` holds only an environment repository. So a small
  `ProviderCapabilityLookup` trait in `rocket-environment` answers "which provider and
  capabilities does this connection id have". `SecretManagerService` implements it, and
  `EnvironmentService::save_with_capabilities` takes it as an argument. The environment
  save commands pass the service in. A binding whose connection no longer exists is not
  rejected here, because that case already has its own warning. The error names the
  provider.
- The UI hides the "Add RocketVault certificate" button when the environment has no
  binding to a certificate-capable connection.
- Other providers keep the generic path. A certificate stored as a secret (PEM text or
  base64 PKCS12) is referenced through the existing `certificateSecret`,
  `privateKeySecret` and `pkcs12Secret` fields.

## 8. Frontend and IPC

- `SecretManagerConnectionDto` gains `provider` and `config`, camelCase. The TypeScript
  types in `src/lib/tauri-api.ts` follow.
- `SecretManagerConnectionsDialog` gets a provider selector. The form shows that
  provider's fields with its own labels and placeholders. In this foundation only
  RocketVault is selectable. The form is driven by a per-provider descriptor so a new
  provider adds a descriptor and no new branches. A saved connection's provider cannot be
  changed, so its stored credential stays valid.
- `ExternalSecretsTab` labels the scope field from the descriptor ("Vault Name",
  "Region", and so on). The per-row "Test" action in the connections dialog keeps its
  free-text vault name field until a provider spec needs a different one.
- User-facing copy that says "RocketVault" in generic places becomes provider neutral.

## 9. Known limitations

- The ACP agent config (`AgentConfig.vault_*`, `AgentConfigsDialog`) stays RocketVault
  only. Choosing a non-RocketVault connection there is not offered.
- Load tests keep using `NullVaultSecretFetcher`, so external secrets do not resolve in
  load tests for any provider.
- The `vault` certificate type is not parseable by older builds. This foundation adds
  no new certificate variants, so it does not make that worse.

## 10. Errors

- Every provider error names the provider ("Azure Key Vault request failed: ...").
- Failures never fall back silently to another source or to an empty string.
- Secret values never appear in errors or logs. Existing redaction applies.

## 11. Testing

Rust (use `-j4`, targeted crates only):

- `rocket-environment`: an old `secret_managers.yml` row loads as RocketVault; a new row
  round-trips; an unknown provider is rejected with a named error.
- `rocket-infra`: `DispatchingSecretFetcher` routes by provider, errors for an
  unregistered provider, forwards `forget_connection`; the RocketVault tests pass
  unchanged.
- `rocket-app`: per-provider validation; fetch narrowing (only referenced refs are
  fetched; an unreferenced failing ref does not fail the send); certificate capability
  gating in `EnvironmentService`.
- `src-tauri`: DTO JSON shape.

Frontend: the provider selector, descriptor-driven labels, and hiding of the
certificate button. Checks: `yarn tsc --noEmit`, `yarn check`.

## 12. Compatibility and rollout

- No file migration. Old `secret_managers.yml` and environment files load unchanged.
- A new build writing a RocketVault connection produces the same file as before, so
  rolling back to an older build keeps working.
- The keychain service name and scope are unchanged.

## 13. Follow-on specs

Each provider spec defines its `ProviderConfig` variant, credential format, auth flow
(including any CLI or profile mode), `vault_name` and `secret_id` meaning, paging,
validation hook, descriptor and wiremock test plan.

1. Azure Key Vault
2. AWS Secrets Manager
3. HashiCorp Vault
4. Google Secret Manager
