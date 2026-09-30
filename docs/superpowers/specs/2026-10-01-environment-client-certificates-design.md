# Environment client certificates: vault-backed material and the certificates UI

**Date:** 2026-10-01
**Status:** Draft for review
**Scope:** `rocket-shared`, `rocket-http`, `rocket-environment`, `rocket-app`, `rocket-infra`, `src-tauri`, and the frontend environment dialog.
**Related:** [external secrets spec](2026-09-22-rocketvault-external-secrets-spec.md), [RocketVault export requirements](../../../.claude/rocketvault-certificate-export-requirements.md), [OpenCollection reference](opencollection-spec-reference.md) section 4.

## 1. Context

Rocket presents an environment's client certificates for mutual TLS (mTLS). Already built and committed on `main` (not pushed):

| Commit | What |
|---|---|
| `8f7f5b62` | Present the matching certificate (PKCS12, PEM) for a request, matched by `domain`. |
| `efbee1e7` | A redirect that leaves the certificate's domain is not followed. |
| `3f678157` | Relative file paths resolve against the collection folder, `..` rejected. |
| `9871230b` | OAuth2 token requests present the certificate too (`TokenClientProvider`). |
| `5666e771` | OAuth2 commands resolve RocketVault references, so `{{alias.secret}}` works for the passphrase. |
| `dcb695c6` | Encrypted PKCS#8 PEM keys are decrypted in memory. |

Today the certificate material always comes from **files**. The passphrase, paths and domain may hold `{{placeholders}}`, including RocketVault references. There is no UI to edit certificates.

## 2. Decisions already made

1. **Passphrase:** a RocketVault secret reference such as `{{vault.CERT_PASS}}`. The UI warns when a literal passphrase is typed. No secret-store change.
2. **Browse:** keeps the absolute path the picker returns, with a hint that relative paths start at the collection folder.
3. **Vault scope:** certificate and key material may come from **ordinary RocketVault secrets** (PEM text, or base64 PKCS12). Native vault certificate objects are a later plan (section 10) that waits for RocketVault's export route.
4. **Scope of the UI:** collection environments only. The backend reads certificates only from a collection's environment.
5. **Encrypted PEM:** PKCS#8 only (done, `dcb695c6`). Old `Proc-Type` and PBES1 keys stay rejected with a hint.

## 3. Goals and non-goals

**Goals**
- A certificate entry can take each piece of material (certificate, private key, PKCS12 bundle) from a file **or** from a vault secret.
- Material from the vault is never persisted, never logged, never serialized, and is wiped when dropped.
- A certificates tab in the environment dialog to create and edit entries, including picking vault secrets.
- Requests and OAuth2 token requests behave identically.

**Non-goals**
- Native RocketVault certificate and key objects (Plan E, section 10).
- Global-environment certificates. The backend does not read them.
- Auto-shortening Browse paths, a "test this certificate" button, storing passphrases in the secret store.
- Old `Proc-Type` and PBES1 key formats.

## 4. Persisted model

`ClientCertificate` (`rocket-shared`) keeps its two variants and gains optional **references**. A reference is `alias.secretName`, the same key RocketVault values already use in `VariableContext.external_secrets`. It is never a value.

| Variant | File fields (today) | New vault reference fields |
|---|---|---|
| `pem` | `certificateFilePath`, `privateKeyFilePath` | `certificateSecret`, `privateKeySecret` |
| `pkcs12` | `pkcs12FilePath` | `pkcs12Secret` (the base64 text of the DER bundle) |

Rules:
- For each piece, **exactly one** source: a non-empty file path, or a reference. The file path fields become `#[serde(default)]` strings that are empty when the piece comes from the vault, and are not written then (`skip_serializing_if` empty). The new fields are `Option<String>`, skipped when `None`.
- `passphrase` is unchanged: a literal or a `{{...}}` placeholder.
- Backward compatible: every existing file still loads and round-trips.
- **Schema:** the new keys are Rocket extensions outside the OpenCollection `ClientCertificate` schema, like `externalSecrets`. The schema-shape test and the spec reference document them as such.

```yaml
clientCertificates:
  - type: pem
    domain: api.example.com
    certificateSecret: vault.clientCertPem     # reference, not a value
    privateKeySecret: vault.clientKeyPem
    passphrase: "{{vault.clientKeyPass}}"
  - type: pkcs12
    domain: "*.internal.example.com"
    pkcs12Secret: vault.clientBundleB64
    passphrase: "{{vault.bundlePass}}"
```

**Validation on environment save** (`rocket-environment`, next to `validate_external_secret_bindings`):
- each piece has exactly one source;
- each reference has the form `alias.secretName`, the alias exists in the environment's bindings, and the name is one of that binding's `secretNames`;
- a file path or reference field must not hold PEM text (a value starting with `-----BEGIN`). This keeps private keys out of the environment file and out of git;
- `domain` is non-empty (an empty domain never matches anyway).

## 5. Runtime model

The resolved request must not carry a serializable type that can hold key bytes. Add in `rocket-http`:

```text
ResolvedClientCertificate { domain, material }
material = Pem { certificate: Source, private_key: Source, passphrase: Option<Secret> }
         | Pkcs12 { bundle: Source, passphrase: Option<Secret> }
         | Unavailable { reason }            // a reference could not be resolved
Source   = File(path) | Inline(Zeroizing<Vec<u8>>)
```

- Not `Serialize`. `Debug` prints sizes and the source kind, never bytes or passphrases.
- `RequestOptions.client_certificates` becomes `Vec<ResolvedClientCertificate>` with `#[serde(skip)]`. The IPC input can no longer carry certificates, which is already ignored today because the environment is the only source.
- `find_certificate`, `certificate_covers` and `TokenClientProvider::client_for` take the resolved type (they only need `domain`).
- This also removes today's gap where a vault-sourced passphrase sits inside a serializable struct.

## 6. Resolution

One function in `rocket-app` (`client_certificates.rs`, shared by request execution and OAuth2 today) turns persisted entries into resolved ones:

1. Resolve `{{placeholders}}` in domain, file paths and passphrase (existing) and join relative paths onto the collection folder (existing).
2. For each reference, look it up in the RocketVault values map. Both callers already have it (`resolve_external_secrets`). Found: `Inline` bytes. PKCS12 is base64-decoded. Not found or not decodable: `Unavailable { reason }`.
3. **Fail only when selected.** An `Unavailable` entry is an error when it is the certificate chosen for the URL (and only then), naming the reference. No fallback to another source, per the vault spec.
4. A vault that cannot be reached already aborts the request and the OAuth2 call (existing behaviour).

## 7. Loading

`load_identity` (`rocket-infra`) handles `Source::Inline` as well as files:
- PEM: the certificate bytes and the key bytes go through the existing `pem_key::unencrypted_key_pem` (encrypted PKCS#8 works with the passphrase) then `Identity::from_pkcs8_pem`.
- PKCS12: the decoded DER goes to `Identity::from_pkcs12_der` with the passphrase.
- The key still needs to start exactly with `-----BEGIN PRIVATE KEY-----` after decryption (TLS backend requirement).
- `ReqwestTokenClientProvider` uses the same function, so token requests get the same material.

## 8. Security requirements (from the external secrets spec, applied to key material)

- Values are never written to disk in any form: not the environment YAML, not history, not logs, not events, not the audit trail. Only references persist.
- Key bytes and the passphrase are `Zeroizing` from resolution to use. Adding `zeroize` is already done (`dcb695c6`).
- Errors name the file or the reference, never a value.
- A literal PEM in a path or reference field is rejected on save (section 4).
- Redaction: every fetched vault value is already added to `secret_values`. Multi-line values must redact as a whole and by line, which needs a test.
- Verify early that a multi-line PEM stored as a vault secret comes back **byte for byte** (line endings), because RocketVault has had CRLF normalization issues in its CSV path. See section 12.

## 9. UI

In `EnvironmentDialog`, a third tab **Certificates** next to Variables and External Secrets, sharing the dialog's single Save. It follows the `ExternalSecretsTab` prop pattern.

- **Type widening:** add `clientCertificates`, `extends`, `dotEnvFilePath`, `color` and `description` to the frontend `Environment` type. They survive a save today only because the save spreads the loaded object. A test loads an environment with all of them, edits a variable, and asserts they are saved unchanged.
- **Rows:** one per certificate, ordered, because the first match wins (move up and down).
  - Type: PKCS12 or PEM.
  - Domain: a variable-aware single-line field with a hint about wildcards and ports.
  - For each piece: a source selector, **File** or **Vault secret**. File is an editable field plus a Browse button (native picker). Vault secret is a picker listing the environment's bound `alias.secretName` entries from the External Secrets tab.
  - Passphrase: a masked, variable-aware field with the same vault picker. A literal value shows a warning that it will be saved in the file, and the placeholder suggests `{{vault.NAME}}`.
  - Add and remove.
- **Validation on Save:** the rules in section 4 (a blocking error for a missing source, a reference with no matching binding, a pasted PEM, an empty domain, and a relative path with `..`). A literal passphrase is only a warning.
- Project rules: shadcn/ui primitives only, `lucide-react` icons, `SingleLineEditor` for single-line variable-aware fields, narrow Zustand selectors.
- The External Secrets tab and the Certificates tab both edit the same object, so a save from either keeps everything.

## 10. Seam for Plan E: native RocketVault certificates

RocketVault's session is designing a **per-certificate export route** (their answers of 2026-10-01, a design direction until they publish a spec):
- `POST /api/v1/vaults/{vault}/certificates/{id}/export`, formats `pem` and `pkcs12`, PKCS12 with `compat: "legacy"`, leaf then intermediates with no root, an immutable `exportable` flag (an exportable certificate requires an exportable key), a new narrow exporter role, JSON error codes on the export routes, audit, `no-store`. Standalone key export is decided separately and Rocket does not use it.

Rocket's side, once their spec is final, is a separate plan:
- A third source kind for a piece or a whole entry: **vault certificate** (connection, vault, certificate id, format), with the PKCS12 password from a vault secret placeholder.
- A new fetch call on `VaultSecretFetcher` (the trait and `RocketVault` client gain a certificate export method) producing the same `Inline` material as section 5, so sections 7 and 8 apply unchanged.
- The picker lists certificates with their `exportable` and `key_algorithm` metadata and disables non-exportable ones.
- Error codes map to clear messages (not exportable, missing role, not found).

Nothing in Plans B to D blocks this: the resolved-material model is the seam.

## 11. Errors (user-facing)

| Situation | Message |
|---|---|
| Reference not in the vault values | "Client certificate secret vault.clientCertPem was not found. Check the External Secrets binding and fetch the secret names." |
| PKCS12 secret not valid base64 | "Client certificate secret vault.bundle is not valid base64." |
| Key does not start with `BEGIN PRIVATE KEY` after decryption | existing message from the TLS layer, naming the piece |
| Wrong passphrase | "Cannot decrypt the private key ...: wrong passphrase or damaged file." (existing) |
| Vault unreachable | the existing vault error, and the request is not sent |
| Literal PEM in a reference or path field | "Field X must be a file path or a vault secret reference, not key text." (on save) |

## 12. Testing

- **Unit:** persisted-model serde round trip and backward compatibility, validation (all section 4 rules), resolution (found, missing, bad base64, placeholders, paths), redaction of multi-line values, `Debug` and serialization of the resolved type never contain bytes.
- **Executor:** inline PEM, inline encrypted PEM, inline PKCS12 (existing fixtures, read into memory), a missing reference fails only when selected, the real `openssl s_server -Verify` handshake test with inline material.
- **OAuth2:** the capturing-provider tests extended to inline material.
- **Frontend:** `CertificatesTab` (modelled on `ExternalSecretsTab.test.tsx`), dialog tab switch, save payload, preservation of the widened fields, vault picker options, validation, the Browse mock.
- **Live check (when RocketVault is available):** store a PEM and a base64 PKCS12 as secrets, and verify a byte-for-byte round trip and a real handshake. This decides whether any line-ending handling is needed.

## 13. Delivery

Three plans, at most three tasks each, plus an index. Each task is one path-scoped commit. The model is the one for the implementing subagent; every task is reviewed by the orchestrator, and the tasks that touch key material are reviewed by the strongest model.

**Plan B: model and loading**

| Task | Content | Model |
|---|---|---|
| B1 | `ResolvedClientCertificate` in `rocket-http` (not serializable, redacting `Debug`), migrate options, executor, token provider and tests | sonnet |
| B2 | Persisted reference fields, YAML conversion, schema-shape test, save-time validation, read the OpenCollection reference first | sonnet |
| B3 | In-memory loading of inline PEM, encrypted PEM and PKCS12 in the loader and the token provider | sonnet |

**Plan C: resolution and hygiene**

| Task | Content | Model |
|---|---|---|
| C1 | Resolve references for requests and OAuth2, fail only when selected | sonnet |
| C2 | Security pass: redaction of multi-line values, nothing in logs, events, history or serialization, leak tests | sonnet builds, opus reviews |
| C3 | Update the external secrets spec (scope, stale OAuth2 non-goal), crate guides, spec reference | haiku drafts, orchestrator reviews |

**Plan D: UI**

| Task | Content | Model |
|---|---|---|
| D1 | Widen the frontend environment type, preservation test, vault-secret options helper | sonnet |
| D2 | `CertificatesTab` component and tests | sonnet |
| D3 | Dialog wiring, save validation, payload tests, manual check | sonnet, reviewed by the orchestrator |

**Order:** B1 then B2 then B3. C needs B. D needs B2 for the types and can run beside B3 and C. **Plan E** follows RocketVault's final spec and its implementation, and is specced separately.

## 14. Risks and open questions

1. **Multi-line secrets.** A PEM must round-trip through a RocketVault secret exactly. Checked in the live check above, and handled if not.
2. **Secret size.** A PKCS12 bundle as base64 may be tens of KiB. RocketVault has no documented size limit, and Rocket sets none. Add a sanity cap (for example 1 MiB) with a clear error.
3. **Windows and EC keys.** The Windows TLS backend likely fails on EC PKCS#8 keys (existing limitation). The UI hint says so.
4. **Extension keys.** Rocket-only keys in the environment file are invisible to other OpenCollection tools, which would drop them on save. Same trade-off as `externalSecrets`.
5. **Fetch cost.** All vault values of the environment are fetched on every send today. More secrets in a binding means more calls. No change here, but worth watching.
