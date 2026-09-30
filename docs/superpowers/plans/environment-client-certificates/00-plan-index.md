# Environment Client Certificates: Plan Index

**Spec:** [../../specs/2026-10-01-environment-client-certificates-design.md](../../specs/2026-10-01-environment-client-certificates-design.md)

**Also read:** [../../specs/2026-09-22-rocketvault-external-secrets-spec.md](../../specs/2026-09-22-rocketvault-external-secrets-spec.md) (the rules for vault values), [../../specs/opencollection-spec-reference.md](../../specs/opencollection-spec-reference.md) (section 4, `ClientCertificate`).

Already built and committed on `main`, and not part of these plans: mTLS for requests and OAuth2 token requests, redirect scoping, relative paths, vault references resolved in token requests, encrypted PEM keys (`pem_key::unencrypted_key_pem`). See section 1 of the spec.

## Plan breakdown: 3 plans, 9 tasks (max 3 per plan)

| # | Plan | Tasks | Crate or area | Depends on |
|---|---|---|---|---|
| B | [Model and loading](01-plan-b-model-and-loading.md) | 3 | `rocket-http`, `rocket-shared`, `rocket-environment`, `rocket-app`, `rocket-infra` | none |
| C | [Resolution and hygiene](02-plan-c-resolution-and-hygiene.md) | 3 | `rocket-app`, `rocket-infra`, docs | B |
| D | [Certificates UI](03-plan-d-certificates-ui.md) | 3 | frontend | B2 (types) |

Plan E (a native RocketVault certificate source) is not here. It waits for RocketVault's published export spec. See section 10 of the spec.

**Order:** B1 then B2 then B3. C needs B. D needs B2 and may run beside B3 and C. One plan at a time unless the user says otherwise.

**Model per task** (for the implementing subagent; every task is reviewed by the orchestrator): B1 sonnet, B2 sonnet, B3 sonnet, C1 sonnet, C2 sonnet builds and opus reviews, C3 haiku drafts and the orchestrator reviews, D1 sonnet, D2 sonnet, D3 sonnet.

## Global Constraints (every task includes these)

- Values fetched from RocketVault are never written to disk in any form: not the environment YAML, not history, not logs, not events, not the audit trail. Only references (`alias.secretName`) persist.
- Key bytes and passphrases held at runtime are `zeroize::Zeroizing` from resolution to use. The resolved certificate type is not `Serialize`, and its `Debug` never prints bytes or passphrases.
- A reference is `alias.secretName`, the same key RocketVault values already use in `VariableContext.external_secrets`. It is never a value.
- Each piece of material (certificate, private key, PKCS12 bundle) has exactly one source: a non-empty file path, or a reference.
- A file path or reference field must not hold key text. A value starting with `-----BEGIN` is rejected on save.
- An unresolved reference fails the request or token request only when that certificate is the one selected for the URL, with no fallback to another source.
- After decryption the PEM key must start exactly with `-----BEGIN PRIVATE KEY-----` (native TLS backend requirement).
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only, never on persistence structs, except where `ClientCertificate` already has it on its variants. Persisted fields stay backward compatible (optional fields or defaults).
- Rust: never `unwrap()` in production paths. Always pass `-j4` to cargo. Do not run `cargo test --workspace`. Use targeted crate tests plus `cargo check -j4 --workspace`.
- Frontend: shadcn/ui primitives only (no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`), `lucide-react` icons only, `SingleLineEditor` for single-line variable-aware fields, Monaco only for multi-line editors, never fully destructure Zustand store state at component top level. Checks: `yarn tsc --noEmit` and `yarn check`.
- Commits: conventional commits, path-scoped (`git add <paths>` then `git commit -- <paths>`, never `git add -A` or `git commit -a`), because other sessions share this working tree. Before any commit invoke the `dev-workflow-skills:1-git-commit` skill. Commit messages end with `Relates to: #21`.
- Every task that touches collection, environment or certificate data models starts with: read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Shared interface contract

All three plans use exactly these names and signatures. A plan that needs a change here must say so and stop.

### `rocket-shared/src/certificate.rs` (persisted, Plan B2)

```rust
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientCertificate {
    #[serde(rename = "pem", rename_all = "camelCase")]
    Pem {
        domain: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        certificate_file_path: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        private_key_file_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        certificate_secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        private_key_secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
    #[serde(rename = "pkcs12", rename_all = "camelCase")]
    Pkcs12 {
        domain: String,
        #[serde(rename = "pkcs12FilePath", default, skip_serializing_if = "String::is_empty")]
        pkcs12_file_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pkcs12_secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
}
impl ClientCertificate { pub fn domain(&self) -> &str; }
```

### `rocket-http/src/resolved_certificate.rs` (new, Plan B1, exported from `rocket-http`)

```rust
use zeroize::Zeroizing;

#[derive(Clone)]
pub enum CertificateSource { File(String), Inline(Zeroizing<Vec<u8>>) }

#[derive(Clone)]
pub enum CertificateMaterial {
    Pem { certificate: CertificateSource, private_key: CertificateSource, passphrase: Option<Zeroizing<String>> },
    Pkcs12 { bundle: CertificateSource, passphrase: Option<Zeroizing<String>> },
    Unavailable { reason: String },
}

#[derive(Clone)]
pub struct ResolvedClientCertificate { pub domain: String, pub material: CertificateMaterial }

impl ResolvedClientCertificate {
    pub fn pem(domain: impl Into<String>, certificate: CertificateSource, private_key: CertificateSource, passphrase: Option<String>) -> Self;
    pub fn pkcs12(domain: impl Into<String>, bundle: CertificateSource, passphrase: Option<String>) -> Self;
    pub fn unavailable(domain: impl Into<String>, reason: impl Into<String>) -> Self;
}
// Not Serialize or Deserialize. Debug is hand-written: it prints the domain, the piece kinds,
// `file <path>` or `inline <n> bytes` for a source, and `<redacted>` for a passphrase.
```

`rocket-http/src/client_cert.rs` (Plan B1 changes the parameter types, logic unchanged):

```rust
pub fn find_certificate<'a>(certs: &'a [ResolvedClientCertificate], url: &str) -> Option<&'a ResolvedClientCertificate>;
pub fn certificate_covers(cert: &ResolvedClientCertificate, url: &str) -> bool;
```

`rocket-http/src/request.rs` (Plan B1):

```rust
#[serde(skip)]
pub client_certificates: Vec<ResolvedClientCertificate>,   // in RequestOptions
```

`rocket-http/src/token_client.rs` (Plan B1):

```rust
fn client_for(&self, token_url: &str, verify_ssl: bool, certificates: &[ResolvedClientCertificate]) -> DomainResult<reqwest::Client>;
```

### `rocket-infra/src/reqwest_executor.rs` (Plans B1, B3, C1)

```rust
fn load_identity(cert: &ResolvedClientCertificate) -> DomainResult<reqwest::Identity>;
fn identity_for_url(certificates: &[ResolvedClientCertificate], url: &str) -> DomainResult<Option<ClientIdentity>>;
```

Behaviour by plan: B1 handles `CertificateSource::File` and returns `InvalidInput("Inline client certificate material is not supported yet")` for `Inline`. B3 adds `Inline`. `CertificateMaterial::Unavailable { reason }` is always an `InvalidInput(reason)` when that certificate is selected.

### `rocket-environment` (Plan B2)

```rust
// rocket-environment/src/client_certificate_validation.rs, re-exported from the crate root
pub fn validate_client_certificates(certs: &[ClientCertificate], bindings: &[ExternalSecretBinding]) -> DomainResult<()>;
```

Called from `crates/rocket-app/src/environment_service.rs` next to `validate_external_secret_bindings`.

### `rocket-app/src/client_certificates.rs` (Plans B1 then C1)

```rust
// B1 (no secrets yet)
pub(crate) fn environment_client_certificates(
    repo: &dyn EnvironmentRepository,
    environment_name: Option<&str>,
    collection_dir: Option<&Path>,
    vars: &HashMap<String, String>,
) -> Vec<ResolvedClientCertificate>;

// C1 adds one parameter, at the end
pub(crate) fn environment_client_certificates(
    repo: &dyn EnvironmentRepository,
    environment_name: Option<&str>,
    collection_dir: Option<&Path>,
    vars: &HashMap<String, String>,
    external_secrets: &HashMap<String, String>,   // keyed "alias.secretName"
) -> Vec<ResolvedClientCertificate>;
```

### Frontend (Plan D)

```ts
// src/lib/tauri-api.ts
export type ClientCertificate =
  | { type: 'pem'; domain: string; certificateFilePath?: string; privateKeyFilePath?: string;
      certificateSecret?: string; privateKeySecret?: string; passphrase?: string }
  | { type: 'pkcs12'; domain: string; pkcs12FilePath?: string; pkcs12Secret?: string; passphrase?: string };

export interface Environment {
  name: string; variables: Variable[]; externalSecrets?: ExternalSecretBinding[];
  clientCertificates?: ClientCertificate[]; extends?: string; dotEnvFilePath?: string;
  color?: string; description?: unknown;
}

// src/lib/vault-secret-options.ts (Plan D1)
export interface VaultSecretOption { value: string; label: string }          // value = `${alias}.${secretName}`
export function vaultSecretOptions(bindings: ExternalSecretBinding[]): VaultSecretOption[];

// src/lib/certificate-validation.ts (Plan D1)
export interface CertificateIssues { errors: string[]; warnings: string[] }
export function validateClientCertificates(certs: ClientCertificate[], bindings: ExternalSecretBinding[]): CertificateIssues;

// src/components/environments/CertificatesTab.tsx (Plan D2)
export interface CertificatesTabProps {
  certificates: ClientCertificate[];
  bindings: ExternalSecretBinding[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
  onAdd: (type: 'pem' | 'pkcs12') => void;
  onRemove: (idx: number) => void;
  onMove: (idx: number, direction: -1 | 1) => void;
  onSave: () => void;
  isDirty: boolean;
  saveState: SaveButtonState;
}
```

## Review Focus (input classes the spec implies, each pinned by a test in the owning task)

1. **A reference to a missing secret must not break unrelated requests.** Two certificates, the second has an unresolved reference and a different domain. A request to the first domain succeeds. Owner: C1.
2. **Vault values with CRLF line endings or trailing whitespace.** A PEM secret stored with `\r\n` or a trailing newline still loads. Owner: B3.
3. **A base64 PKCS12 secret wrapped across lines.** Whitespace inside the base64 is ignored when decoding. Owner: C1.
4. **Overlapping domains.** `*.example.com` listed before `api.example.com` wins by order, and the tab preserves and can reorder the order. Owners: B1 (existing matcher kept), D2 (reorder).
5. **A reference with no matching binding, or a pasted private key, in an environment file.** Rejected on save with a message that names the field. Owners: B2 (backend), D1 (frontend mirror).

## Conventions for every plan

- Each plan starts with the header required by the writing-plans skill and ends with a **Next Plan** section linking the next file.
- Steps are 2 to 5 minutes, test first, with the exact command and the expected result. No placeholders.
- Verification commands: `cargo test -j4 -p <crate> <filter>`, `cargo clippy -j4 -p <crate> --all-targets`, `cargo check -j4 --workspace`, `yarn vitest run <path>`, `yarn tsc --noEmit`, `yarn check`.
- The real handshake test is `cargo test -j4 -p rocket-infra mutual_tls_handshake -- --ignored` (needs the `openssl` CLI). Plans that change loading must run it.

## Decisions recorded after plan drafting

- Inline load errors name the piece as "(inline, for <domain>)" instead of the reference; the domain identifies the entry. `CertificateSource::Inline` stays label-free.
- `zeroize = "1"` is a direct dependency of rocket-http (B1) and rocket-app (C1); both commit `Cargo.lock`.
- `CertificatesTabProps` gains optional `variableContext`; `certificate-validation.ts` exports `isLiteralPassphrase`; `VaultSecretOption.label` equals `value` (Plan D).
- B2 and D1 both skip the `..` relative-path check for values starting with `{{`.
- The existing hand-written `Debug` on `ClientCertificate` is extended for the new reference fields in B2; C2's reviewer checks it.
- Load tests pass an empty secrets map, so vault-backed certificates are `Unavailable` there. Documented as a scope boundary, not fixed in these plans.
- C2 starts only if `crates/rocket-app/src/redaction.rs` has no foreign uncommitted hunks; otherwise stop and ask.
- Saving an environment with an invalid certificate now also blocks a Variables save, same as external secrets today.
