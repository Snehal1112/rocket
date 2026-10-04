# Design: Azure Key Vault provider

**Status:** Draft for review
**Sub-project:** 2 of 5. Builds on the provider seam; AWS, HashiCorp and Google follow.
**Builds on:** [Secret provider foundation](2026-10-04-secret-provider-foundation-design.md).

> 📖 Before starting implementation, read `docs/superpowers/specs/opencollection-spec-reference.md`.

## 1. Problem

The Connections dialog lists Azure Key Vault as "not available yet". The foundation
added the seam (provider tag, `VaultSecretFetcher` dispatch, capabilities, descriptor
table) but no provider other than RocketVault. Users cannot resolve
`{{alias.secretName}}` from an Azure vault.

## 2. Goals

- Selecting Azure Key Vault in the dialog works end to end: save, test connection,
  list secrets for a binding, and resolve `{{alias.secretName}}` on a send.
- Authentication is an Azure AD service principal (tenant id, client id, client secret)
  on the public cloud.
- RocketVault connections and saved files are unchanged.
- The foundation's carry-over items are closed (section 8).

## 3. Non-goals

- Managed identity, `az login` or any credential-optional mode.
- Sovereign clouds (US Gov, China). Only the public cloud authority and scope.
- Certificates from Azure. Capabilities report `certificates: false`.
- Writing secrets back. The integration stays read-only.
- Fetch-on-reference. Every bound secret is fetched per send, as for RocketVault.
  This is a recorded follow-up. It needs the user's uncommitted
  `execution_service.rs` partial-failure work committed first.
- Real Azure calls in CI.

## 4. Data model (`rocket-environment`)

- `ProviderConfig` gains `Azure { tenant_id: String, authority_host: Option<String> }`.
  `authority_host` defaults to `https://login.microsoftonline.com`. It exists mainly so
  tests can target wiremock. It is validated as https, with loopback http allowed, the
  same rule RocketVault uses for its base URL.
- Persistence structs carry no camelCase rename. The variant is written as a serde YAML
  tag, so a build without the variant cannot read the file. This is accepted because
  the variant ships with the provider selector (section 8, item 4).
- The vault URL lives in the existing `base_url`. `client_id` reuses `client_id`. The
  client secret is the single keychain string under the existing vault-connection
  scope. `verify_ssl` and `allow_insecure_http` keep their defaults and are ignored.
- One connection is one vault, so the fetcher ignores the `vault_name` argument. The
  binding contract still requires a non-empty `vault_name`
  (`external_secret.rs:60`) and the External Secrets tab needs it before "Fetch" works.
  Changing both is out of scope, so Azure keeps the scope field and only relabels it
  (section 7). Users type any short name. Hiding the column is a follow-up.

## 5. Fetcher (`rocket-infra/src/azurekeyvault/`)

New module implementing `VaultSecretFetcher`, registered with
`DispatchingSecretFetcher::register(SecretProviderKind::Azure, ...)`. The constructor
`with_rocketvault()` is renamed `with_providers()` and the call in
`src-tauri/src/lib.rs` is updated.

| Operation | Behavior |
|---|---|
| Token | Form POST to `{authority}/{tenant}/oauth2/v2.0/token` with `grant_type=client_credentials`, `client_id`, `client_secret`, `scope=https://vault.azure.net/.default`. The tenant is validated and percent-encoded as a path segment. An empty token is rejected. |
| Token cache | `DashMap` keyed by connection id. A hit needs matching tenant, vault URL, client id and a sha256 fingerprint of the secret, and must not be inside the early-refresh window. Cleared on 401 and in `forget_connection`. |
| `list_secrets` | `GET {vault}/secrets?api-version=7.4&maxresults=25` with a bearer token. Follows `nextLink` only when its host equals the vault host, with a page cap. Skips entries with `attributes.enabled == false`. The secret id is the last path segment of `id`. |
| `get_secret_value` | `GET {vault}/secrets/{name}?api-version=7.4`, name percent-encoded. 404 and a disabled secret (403 `SecretDisabled`) return `Ok(None)`. |
| `test_connection` | Fetch a token, then list one secret. A 403 returns a message naming the "Key Vault Secrets User" role or the access policy. |
| `validate_connection` | Requires `ProviderConfig::Azure` with a valid tenant, an https vault URL (loopback http allowed) and a valid authority host. |
| `capabilities` | `certificates: false`, `credential_optional: false`, `fetch_on_reference: false`. |

HTTP rules: redirects disabled, shared request timeout, bearer auth. Errors map to
`DomainError::Http` or `InvalidInput`. Error text never includes token or secret
bodies, and a JSON decode failure yields a generic message. 429 and 503 map to a
retryable-style message that includes `Retry-After` when present.

## 6. IPC (`src-tauri`)

`SecretManagerConnectionDto.config` currently reuses `ProviderConfig`, which would
cross IPC as PascalCase tags with snake_case fields. Add a dedicated camelCase
`ProviderConfigDto` with `From` conversions both ways. `ProviderConfig` itself stays
without camelCase.

## 7. Frontend

- `ConnectionField` gains `tenantId`. The Azure descriptor becomes `selectable: true`
  with fields `baseUrl` (label "Vault URL"), `tenantId`, `clientId` and `clientSecret`.
  Its scope label is "Vault name" with the placeholder "Any name (the connection
  sets the vault)", because the fetcher ignores the value.
- `tauri-api.ts` types `config` with an `AzureConfig` interface (`tenantId`,
  `authorityHost?`) matching the DTO.
- The dialog sends `config` on save and restores it in `startEdit`. Its validation
  message is built from the selected provider's required fields instead of the
  hard-coded RocketVault text. Field labels (Vault URL, and "Client Secret (required)")
  come from the descriptor. The row subtitle shows the vault URL.
- shadcn primitives and lucide icons only, per the project rules.

## 8. Carry-overs from the foundation

1. `SecretManagerService::save` rejects a provider change on an existing id, so a
   stored credential cannot be sent to a different provider over IPC.
2. The dialog forwards `config` (section 7).
3. A test saves a credential-required non-RocketVault connection with no stored
   credential and checks the connection-time error.
4. `save_with_capabilities` reads `secret_managers.yml` when an environment has a
   `vault` certificate. If the file holds an unknown provider or `config` from a newer
   build, the save fails with a clear message that names the cause, not a parse error.

## 9. Testing

- Rust (`rocket-infra`, wiremock): token request shape, token cache hit and key
  mismatch (tenant, vault URL, secret change), 401 clears cache, list paging, foreign
  `nextLink` host is not followed, page cap, disabled secrets skipped, get 404 and
  disabled return `None`, test-connection 403 message, no token or secret in errors.
- Rust (`rocket-environment`): `ProviderConfig::Azure` YAML round trip, RocketVault
  rows still omit `provider` and `config`, authority host and tenant validation.
- Rust (`rocket-app`): provider-change guard, credential-required test, certificate
  gating rejects a vault certificate on an Azure connection.
- Rust (`src-tauri`): DTO camelCase round trip.
- Frontend (Vitest): descriptor fields and `selectable`, dialog save includes `config`,
  edit restores it, validation message follows the provider.
- Verification: `cargo check -j4`, targeted crate tests with `-j4`, `yarn tsc --noEmit`,
  `yarn check`, targeted `yarn test`.

## 10. Risks

- Going back to a build without the `Azure` variant makes the whole
  `secret_managers.yml` unreadable, not just the Azure row. Accepted: the variant ships
  with the selector. The foundation branch is unmerged, so no released build lacks it.
- Each send makes one Azure GET per bound secret until fetch-on-reference lands. This
  costs latency and vault request quota (about 4000 GET per 10 seconds per vault), not
  correctness.
- A secret past its `exp` or before its `nbf` is still readable through the API. Azure
  does not enforce those dates, and this provider does not filter on them.

## 11. Follow-ups

Fetch-on-reference, managed identity and `az login`, sovereign clouds, then the AWS,
HashiCorp and Google providers.
