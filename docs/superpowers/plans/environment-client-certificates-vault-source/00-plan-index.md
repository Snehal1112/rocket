# RocketVault Certificate Source (Plan E): Plan Index

**Spec:** [../../specs/2026-10-02-vault-certificate-source-design.md](../../specs/2026-10-02-vault-certificate-source-design.md)

**Also read:** [../../specs/2026-10-01-environment-client-certificates-design.md](../../specs/2026-10-01-environment-client-certificates-design.md) (the parent design; this spec fills in its section 10), [../../specs/opencollection-spec-reference.md](../../specs/opencollection-spec-reference.md) (section 4, `ClientCertificate`).

Already built and committed on `main`, and not part of these plans: Plans B to D of the parent design (resolved certificate model, file and vault-secret sources, executor loading, redirect scoping, the Certificates tab). The executor client-cache lock fix the audit asked for has landed too (`fde5d074`, "fix(http): recover a poisoned client cache lock").

## Plan breakdown: 4 plans, 12 tasks (max 3 per plan)

| # | Plan | Tasks | Crate or area | Depends on |
|---|---|---|---|---|
| A | [Model, validation and deferred material](01-plan-a-model-and-deferred.md) | 3 | `rocket-shared`, `rocket-http`, `rocket-environment`, `rocket-app`, `rocket-infra` (executor, YAML), docs | none |
| B | [RocketVault client and fetcher](02-plan-b-client-and-fetcher.md) | 3 | `rocket-environment` (trait), `rocket-infra` (`rocketvault/`) | A1 (`VaultCertificateFormat`) |
| C | [Materialization and command](03-plan-c-materialization-and-command.md) | 3 | `rocket-app`, `src-tauri`, `src/lib/tauri-api.ts` | A, B |
| D | [Certificates UI](04-plan-d-certificates-ui.md) | 3 | frontend | A2 (persisted shape), C3 (command) |

**Order:** A1, A2, A3, then B1, B2, B3, then C1, C2, C3, then D1, D2, D3. B may start right after A1 if the user asks for parallel work, because B only needs `VaultCertificateFormat`. One plan at a time otherwise.

**Model per task** (for the implementing subagent; the orchestrator reviews every task):

| Task | Title | Model |
|---|---|---|
| A1 | `Deferred` material and the executor guard | sonnet |
| A2 | Persisted `vault` entry: model, validation and resolution to `Deferred` | sonnet |
| A3 | YAML round trip, load-test message, spec reference note | sonnet |
| B1 | Fetcher trait, domain types and the RocketVault contract module | sonnet |
| B2 | Paged certificate list and the name-to-id cache | sonnet |
| B3 | Certificate export: one-time password, size cap, 404 retry, error mapping | opus |
| C1 | Materialize the certificate selected for a URL | opus |
| C2 | Wire materialization into the send path and OAuth2 token requests | sonnet |
| C3 | `list_vault_certificates` command and its TypeScript binding | sonnet |
| D1 | Types, validation mirror and the Windows EC rule | sonnet |
| D2 | `VaultCertificateRow` picker | sonnet |
| D3 | Certificates tab and dialog integration | sonnet |

No task is pure documentation, so none goes to haiku. The spec-reference note rides along with A3.

## Global Constraints (every task includes these)

- Values fetched from RocketVault are never written to disk in any form: not the environment YAML, not history, not logs, not events, not the audit trail. Only references persist (an External Secrets alias and a certificate name). Runtime material is `zeroize::Zeroizing` from fetch to use.
- An unresolved or failed vault certificate fails a request or token request only when that certificate is the one selected for the URL, with no fallback to another entry. An entry for another domain causes no RocketVault call.
- A `CertificateMaterial::Deferred` that reaches the executor is an `InvalidInput` error, never a silent skip.
- A path or reference field must not hold key text: a value starting with `-----BEGIN` is rejected on save. For a `vault` entry this covers every field.
- Error and log text never contains key bytes, PKCS12 bundle bytes or the one-time password.
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only, plus the `ClientCertificate` variants, which already carry it. Persisted fields stay backward compatible (additive, with defaults).
- Rust: never call `unwrap` in production paths (tests may, though these plans use `expect`). Always pass `-j4` to cargo. Never run `cargo test --workspace`; use targeted crate tests plus `cargo check -j4 --workspace`.
- Frontend: shadcn/ui primitives only (no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`), `lucide-react` icons only, `SingleLineEditor` for single-line variable-aware fields, Monaco only for multi-line editors, never fully destructure Zustand store state at component top level. Checks: `yarn tsc --noEmit`, `yarn check`, `yarn vitest run <path>`.
- Commits: conventional commits, path-scoped (`git add <paths>` then `git commit -- <paths>`, never `git add -A` or `git commit -a`), because other sessions share this working tree. Every commit goes through the `dev-workflow-skills:1-git-commit` skill. Commit messages end with `Relates to: #21`.
- Every task that touches collection, environment or certificate data models starts with the line: `📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.` (All twelve tasks do.)

## Coordination notes (read before every task)

- **Flow auth work elsewhere.** The user is changing Flow auth (the auth node and the "inherit from parent" fix) on another machine. Keep edits in `crates/rocket-app/src/execution_service.rs` small (A3 changes one call in `run_load_test`, C2 changes `send_request` and adds one helper), and do not touch any flow execution module (`crates/rocket-app/src/flow_*.rs`). Flows reach the new code through `execute_with_external_secrets` and `send_request`, unchanged in signature.
- **Pull first.** Before A3 and C2 (the tasks that touch `execution_service.rs`), run `git status --short` and `git pull --ff-only`. If the pull is not a fast-forward, or `execution_service.rs` has foreign uncommitted hunks, stop and ask.
- **Shared tree.** Other sessions share this working tree and its git index. Stage and commit only the paths a task lists. Re-check `git diff --cached --stat` before each commit.
- **Unpushed RocketVault contract.** RocketVault v-4.0.0 is a local, unpushed branch, so field names, codes or routes may still shift. The whole HTTP contract lives in one module, `crates/rocket-infra/src/rocketvault/certificate_api.rs` (B1), so a change there is local.
- **Live instance.** The real-export test in B3 is `#[ignore]` and needs a live RocketVault v4 instance, which the `rocketvault-4b` session will provide. Never write into the RocketVault repository.

## Shared interface contract

All four plans use exactly these names and signatures. A task that needs a change here must say so and stop.

### `rocket-shared/src/certificate.rs` (persisted; A1 adds the format, A2 the variant)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VaultCertificateFormat { #[default] Pem, Pkcs12 }
impl VaultCertificateFormat { pub fn as_str(self) -> &'static str; }   // "pem" | "pkcs12"

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientCertificate {
    #[serde(rename = "pem", rename_all = "camelCase")]    Pem { /* unchanged */ },
    #[serde(rename = "pkcs12", rename_all = "camelCase")] Pkcs12 { /* unchanged */ },
    #[serde(rename = "vault", rename_all = "camelCase")]
    Vault {
        domain: String,
        binding: String,       // External Secrets alias of this environment
        certificate: String,   // certificate name in that vault
        #[serde(default)]
        format: VaultCertificateFormat,
    },
}
impl ClientCertificate { pub fn domain(&self) -> &str; }   // covers Vault
```

### `rocket-http/src/resolved_certificate.rs` (A1, re-exported from the crate root)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultCertificateBinding { pub alias: String, pub connection_id: String, pub vault_name: String }

pub enum CertificateMaterial {
    Pem { .. }, Pkcs12 { .. }, Unavailable { reason: String },   // unchanged
    Deferred { binding: VaultCertificateBinding, certificate: String, format: VaultCertificateFormat },
}
impl ResolvedClientCertificate {
    pub fn deferred(domain: impl Into<String>, binding: VaultCertificateBinding,
                    certificate: impl Into<String>, format: VaultCertificateFormat) -> Self;
    pub fn is_deferred(&self) -> bool;
}
```

`rocket-infra/src/reqwest_executor.rs` (A1): `load_identity` returns `InvalidInput("The RocketVault certificate {certificate} (binding {alias}) for {domain} was not fetched before the request was sent.")` for `Deferred`.

### `rocket-environment` (A2 validation, B1 fetcher)

```rust
// client_certificate_validation.rs (A2): signature unchanged, gains the Vault rules
pub fn validate_client_certificates(certs: &[ClientCertificate], bindings: &[ExternalSecretBinding]) -> DomainResult<()>;

// vault_secret_fetcher.rs (B1), re-exported from the crate root
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultCertificateSummary {
    pub id: String, pub name: String, pub exportable: bool, pub enabled: bool,
    pub key_algorithm: String, pub expires_at: Option<String>,
}
pub enum VaultCertificateMaterial {   // not Clone, not Serialize; Debug prints sizes only
    Pem { certificate: Zeroizing<Vec<u8>>, private_key: Zeroizing<Vec<u8>>, key_algorithm: String },
    Pkcs12 { bundle: Zeroizing<Vec<u8>>, password: Zeroizing<String>, key_algorithm: String },
}
impl VaultCertificateMaterial { pub fn format(&self) -> VaultCertificateFormat; pub fn key_algorithm(&self) -> &str; }

#[async_trait::async_trait]
pub trait VaultSecretFetcher: Send + Sync {
    // list_secrets, get_secret_value, test_connection: unchanged
    async fn list_certificates(&self, connection: &SecretManagerConnection, client_secret: &str,
                               vault_name: &str) -> DomainResult<Vec<VaultCertificateSummary>>;      // default: Err
    async fn fetch_certificate(&self, connection: &SecretManagerConnection, client_secret: &str,
                               vault_name: &str, certificate_name: &str,
                               format: VaultCertificateFormat) -> DomainResult<VaultCertificateMaterial>; // default: Err
}
```

### `rocket-infra/src/rocketvault/` (B1 to B3)

```rust
// certificate_api.rs (B1): the only module that knows the RocketVault certificate contract
pub(super) const LIST_PAGE_SIZE: usize = 200;
pub(super) const MAX_LIST_PAGES: usize = 100;
pub(super) const MAX_EXPORT_BYTES: usize = 1024 * 1024;
pub(super) const MAX_ERROR_BYTES: usize = 64 * 1024;
pub(super) const PKCS12_PASSWORD_LEN: usize = 32;
pub(super) const NOT_FOUND: &str;       // "Certificate not found in this vault."
pub(super) const NOT_EXPORTABLE: &str;  // "Certificate is not marked exportable."
pub(super) const MISSING_ROLE: &str;    // "The service account lacks the Certificate Exporter role."
pub(super) const DISABLED: &str;        // "Certificate is disabled."
pub(super) const TOKEN_REJECTED: &str;  // "RocketVault rejected the access token (401)."
pub(super) fn list_url(connection: &SecretManagerConnection, vault_name: &str, page: usize) -> DomainResult<url::Url>;
pub(super) fn export_url(connection: &SecretManagerConnection, vault_name: &str, certificate_id: &str) -> DomainResult<url::Url>;
pub(super) struct CertificatePage { pub certificates: Vec<VaultCertificateSummary>, pub total: Option<usize> }
pub(super) fn parse_certificate_page(body: &[u8]) -> DomainResult<CertificatePage>;
pub(super) fn is_last_page(page_len: usize, seen: usize, total: Option<usize>) -> bool;
pub(super) fn list_failed(status: reqwest::StatusCode) -> DomainError;
pub(super) fn export_request_body(format: VaultCertificateFormat, password: Option<&str>) -> DomainResult<Zeroizing<Vec<u8>>>;
pub(super) fn parse_export(body: &[u8], format: VaultCertificateFormat, password: Option<Zeroizing<String>>) -> DomainResult<VaultCertificateMaterial>;
pub(super) enum ExportFailure { TokenRejected, NotFound, Failed(DomainError) }
pub(super) fn classify_export_error(status: reqwest::StatusCode, body: &[u8]) -> ExportFailure;

// certificates.rs (B2, B3)
impl ReqwestVaultSecretFetcher {
    pub(super) async fn list_certificate_pages(&self, connection: &SecretManagerConnection, client_secret: &str,
        vault_name: &str, until: Option<&str>) -> DomainResult<Vec<VaultCertificateSummary>>;           // B2
    pub(super) fn remember_certificate_ids(&self, connection: &SecretManagerConnection, vault_name: &str,
        listed: &[VaultCertificateSummary]);                                                           // B2
    pub(super) async fn certificate_id(&self, connection: &SecretManagerConnection, client_secret: &str,
        vault_name: &str, name: &str, fresh: bool) -> DomainResult<String>;                             // B2
    pub(super) fn forget_certificate_id(&self, connection: &SecretManagerConnection, vault_name: &str, name: &str); // B2
    pub(super) async fn export_certificate(&self, connection: &SecretManagerConnection, client_secret: &str,
        vault_name: &str, name: &str, format: VaultCertificateFormat) -> DomainResult<VaultCertificateMaterial>; // B3
}
// mod.rs: ReqwestVaultSecretFetcher gains `certificate_ids: DashMap<String, String>` (B2).
```

### `rocket-app` (A2, A3, C1 to C3)

```rust
// client_certificates.rs
pub(crate) fn environment_client_certificates(repo: &dyn EnvironmentRepository, environment_name: Option<&str>,
    collection_dir: Option<&Path>, vars: &HashMap<String, String>,
    external_secrets: &HashMap<String, String>) -> Vec<ResolvedClientCertificate>;   // signature unchanged; A2 maps Vault to Deferred
pub(crate) fn unavailable_in_load_tests(certificates: &mut [ResolvedClientCertificate]);   // A3

// vault_certificates.rs (C1, new)
pub(crate) struct VaultCertificateAccess<'a> {
    pub connections: &'a dyn SecretManagerRepository,
    pub secret_store: &'a dyn SecretStore,
    pub fetcher: &'a dyn VaultSecretFetcher,
}
pub(crate) fn certificate_urls(request: &HttpRequest) -> Vec<&str>;
pub(crate) fn needs_fetch(certificates: &[ResolvedClientCertificate], urls: &[&str]) -> bool;
pub(crate) async fn materialize_selected(certificates: &mut [ResolvedClientCertificate], urls: &[&str],
    access: Option<&VaultCertificateAccess<'_>>);

// execution_service.rs (C2)
pub(crate) async fn send_request(&self, state: &PhaseState) -> DomainResult<HttpResponse>;   // signature unchanged
async fn with_vault_certificates<'r>(&self, request: &'r HttpRequest) -> std::borrow::Cow<'r, HttpRequest>;

// oauth2_service.rs (C2)
pub fn with_vault_access(self, connections: Box<dyn SecretManagerRepository>, secret_store: Arc<dyn SecretStore>,
    fetcher: Arc<dyn VaultSecretFetcher>) -> Self;
async fn certificates_for<'c>(&self, url: &str, certificates: &'c [ResolvedClientCertificate])
    -> std::borrow::Cow<'c, [ResolvedClientCertificate]>;

// secret_manager_service.rs (C3)
pub async fn list_certificates(&self, id: &str, vault_name: &str) -> DomainResult<Vec<VaultCertificateSummary>>;
```

### `src-tauri/src/commands/secret_managers.rs` (C3)

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultCertificateSummaryDto { pub id: String, pub name: String, pub exportable: bool,
    pub enabled: bool, pub key_algorithm: String, pub expires_at: Option<String> }

#[tauri::command]
pub async fn list_vault_certificates(connection_id: String, vault_name: String,
    svc: State<'_, SecretManagerService>) -> Result<Vec<VaultCertificateSummaryDto>, DomainError>;
```

### Frontend (C3 binding, Plan D)

```ts
// src/lib/tauri-api.ts
export interface VaultCertificateSummary { id: string; name: string; exportable: boolean; enabled: boolean;
  keyAlgorithm: string; expiresAt?: string | null }                                   // C3
export const listVaultCertificates: (connectionId: string, vaultName: string) => Promise<VaultCertificateSummary[]>; // C3
export type VaultCertificateFormat = 'pem' | 'pkcs12';                                // D1
export type ClientCertificate = /* pem | pkcs12 unchanged */
  | { type: 'vault'; domain: string; binding: string; certificate: string; format?: VaultCertificateFormat }; // D1

// src/lib/vault-certificates.ts (D1)
export function isEcKeyAlgorithm(keyAlgorithm: string | undefined): boolean;
export function isWindows(): boolean;
export function needsEcPemWarning(keyAlgorithm: string | undefined, format: VaultCertificateFormat, windows: boolean): boolean;
export function isSelectable(summary: VaultCertificateSummary): boolean;
export function vaultCertificateLabel(summary: VaultCertificateSummary): string;   // "name · alg[ · not exportable | · disabled]"

// src/lib/certificate-validation.ts (D1): signature unchanged, gains the vault rules
export function validateClientCertificates(certs: ClientCertificate[], bindings: ExternalSecretBinding[]): CertificateIssues;

// src/components/environments/VaultCertificateRow.tsx (D2)
export interface VaultCertificateRowProps {
  idx: number;
  cert: Extract<ClientCertificate, { type: 'vault' }>;
  bindings: ExternalSecretBinding[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
}

// src/components/environments/CertificatesTab.tsx (D3): one prop type widens
onAdd: (type: ClientCertificate['type']) => void;   // 'pem' | 'pkcs12' | 'vault'
```

## Review Focus (the five input classes most likely to bite, each pinned by a named test)

1. **A certificate name that sits on a later page.** The list has no name filter and is paged, so a name on page 3 must be found, and the walk must stop there. Owner B2: `certificate_id_walks_pages_until_the_name_is_found_on_page_three` in `crates/rocket-infra/src/rocketvault/certificates.rs`.
2. **A certificate deleted and created again under the same name.** Its id changes, so the cached id gets a 404; the client must look the name up again and retry once, and a name that stays missing is "not found". Owner B3: `export_refreshes_the_id_and_retries_once_after_a_404` and `a_certificate_that_stays_missing_is_not_found_after_one_retry` in `certificates.rs`; the stale name in the picker is pinned by `shows a stored name that is no longer in the vault as not found` in `src/components/environments/VaultCertificateRow.test.tsx` (D2).
3. **A vault entry for another domain.** It must cause zero RocketVault calls, for requests and token requests alike. Owners C1 and C2: `a_vault_certificate_for_another_domain_makes_no_vault_call` and `a_request_to_another_domain_never_calls_the_vault` in `crates/rocket-app/src/vault_certificates.rs`.
4. **A 401 in the middle of a send.** The cached token is evicted, the request fails with a clear reason, and no other entry is tried. Owners B3 and C1: `export_401_evicts_the_token_and_fails` in `certificates.rs`, `a_failed_fetch_fails_only_the_selected_entry_with_no_fallback` in `vault_certificates.rs`.
5. **A certificate that cannot be exported as configured.** A non-exportable certificate gets the clear message and is disabled in the picker, and an EC certificate picked as PEM on Windows shows the PKCS12 warning. Owners B3 and D2: `export_403_not_exportable_says_so` in `certificates.rs`; `lists a non-exportable certificate as disabled with its key algorithm` and `warns about an EC certificate with PEM on Windows only` in `VaultCertificateRow.test.tsx`.

Also pinned (not in the top five): the mutation check for a silent `Deferred` skip, `a_selected_deferred_certificate_fails_the_request_and_nothing_is_sent` (A1); a fresh random password per export, `each_pkcs12_export_uses_a_new_random_password_with_legacy_compat` (B3); load tests, `load_tests_turn_a_vault_certificate_into_a_clear_error` (A3).

## Spec versus code (decided in these plans; the details sit in each plan's header)

1. **Older builds do not ignore an unknown entry type** (spec section 4). `ClientCertificate` is `#[serde(tag = "type")]` with no catch-all, so a build without `Vault` fails to parse the whole environment file: `FsEnvironmentRepo::list` skips it as "corrupt" and `get` returns an error. The change stays additive for this and later builds, but an older Rocket hides an environment that holds a `vault` entry. Plan A records this in the spec reference note. The user should know before release.
2. **The PKCS12 password is generated in `rocket-infra`, not passed in** (spec 5.3 lists `fetch_certificate(..., name, format, password)`). The client creates it per export and returns it inside `VaultCertificateMaterial::Pkcs12`, so the one place that sends it is the one place that makes it, and the random-password test sits next to the call (B3).
3. **`Deferred.binding` is a resolved binding, not a bare alias.** `VaultCertificateBinding { alias, connection_id, vault_name }` is copied from the environment during resolution (A2), so materialization (C1) needs no second environment read. The field names are the spec's.
4. **Selection happens in `send_request`, after the pre-request script** (spec 5.2 says "before dispatch"). A script can change the URL, and `send_request` is the one dispatch point shared by `execute`, the Collection Runner and Flow runs (C2).
5. **The in-send OAuth2 client-credentials token fetch** (`ReqwestExecutor::apply_auth`) presents the request's certificate list to the token URL, which the spec does not mention. `certificate_urls` covers it (C1, C2).
6. **The 1 MiB cap and redaction forms of Plan C do not apply as written** (spec 5.5). `MAX_INLINE_SECRET_BYTES` guards the vault-secret path only; the export is capped at read time in `rocket-infra` (`MAX_EXPORT_BYTES`, B3). Redaction forms come from the secrets map, which certificate material never enters (spec section 9 relies on that).
7. **`VaultSecretFetcher` has seven implementors**, five of them test fakes. The two new methods get default bodies that return an error, so the fakes compile unchanged; `NullVaultSecretFetcher` and `ReqwestVaultSecretFetcher` override them (B1 to B3).
8. **`OAuth2Service` has no vault access today.** It gains `with_vault_access`, wired in `src-tauri/src/lib.rs` (C2).
9. **Load tests have two entry points**, `RequestExecutionService::run_load_test` and `LoadTestService::run`; both get the clear message (A3).
10. **The list response envelope is not in the spec.** B1 assumes `{"certificates": [...], "total": N}` with `id`, `name`, `exportable`, `enabled`, `key_algorithm`, `expires_at`, mirroring the secret list. It lives in `certificate_api.rs` and must be confirmed against RocketVault.
11. **The flat export route for the default vault is not used.** A binding always names its vault.
12. **OS detection for the EC warning** (spec risk 2): `@tauri-apps/plugin-os` is already a dependency (`src/App.tsx`, `src/components/title-bar/TitleBar.tsx`), so D1 uses its `type()`.

## Conventions for every plan

- Each plan starts with the header the writing-plans skill requires and ends with a **Next Plan** section.
- Steps are 2 to 5 minutes, test first, with the exact command and the expected result. No placeholders.
- Verification commands: `cargo test -j4 -p <crate> <filter>`, `cargo clippy -j4 -p <crate> --all-targets`, `cargo check -j4 --workspace`, `yarn vitest run <path>`, `yarn tsc --noEmit`, `yarn check`.
- The existing real handshake test is `cargo test -j4 -p rocket-infra mutual_tls_handshake -- --ignored` (needs the `openssl` CLI). The new live export test is `cargo test -j4 -p rocket-infra live_rocketvault_export -- --ignored` (needs the live instance and the environment variables in B3).
- Line numbers are from `main` at `0c5a83e9`. They shift after each task, so find code by the function or test name given.
