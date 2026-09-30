# Rocket to RocketVault: certificate and key export requirements

Status: request, 2026-10-01. RocketVault answered the open questions the same day (section 9); their answers are a design direction, not final until they write a spec. Author: the Rocket side. Audience: the RocketVault session working on export.

## 1. Purpose

Rocket (a Tauri desktop API client) wants to fetch an mTLS client identity (certificate plus private key) from RocketVault at send time. Nothing is persisted on the Rocket side. The environment file holds only a reference (vault connection, vault, certificate), never material.

How Rocket talks to RocketVault today (`crates/rocket-infra/src/rocketvault/mod.rs`):

- OAuth2 client credentials: `POST /api/v1/oauth2/token` (form: `grant_type`, `client_id`, `client_secret`), then `Authorization: Bearer`. Tokens are cached per connection and evicted on a 401.
- `https` is required unless the host is loopback. Redirects are disabled. Timeout is 30 s.
- Only three calls exist: token, list secrets, get one secret value.

## 2. Why the existing and proposed routes do not work for Rocket

| Route | Problem for Rocket |
|---|---|
| `POST .../certificates/{id}/backup` | Certificate PEM is plaintext, but `private_key` is ciphertext under the instance master key. Unusable outside that instance. |
| `GET .../certificates/{id}` | `CertificateResponse` has metadata only: no PEM, no key. |
| Keys (all routes) | Non-extractable by design (2026-08-25 decision record). |
| Proposed `POST .../certificates/export` (2026-08-25 design) | Bulk (every certificate, tag-filtered), sealed under a human passphrase (argon2id + AES-256-GCM), and gated by the same Officer-level roles as backup. A Rocket service principal would need a role that can export every private key in the vault, and Rocket would have to implement the envelope. |

## 3. Requirements

### R1. Per-certificate export route (MUST)

`POST /api/v1/vaults/{vault_name}/certificates/{certificate_id}/export` (and the flat `/api/v1/certificates/{certificate_id}/export`).

Request body (JSON):

```json
{ "format": "pem", "version": "optional" }
{ "format": "pkcs12", "password": "required for pkcs12", "compat": "legacy" }
```

- POST, not GET, so the password never appears in a URL or access log.
- `version` is optional and defaults to the latest.

### R2. PEM response (MUST)

```json
{
  "id": "…", "name": "client-cert", "version": "…", "format": "pem",
  "certificate_pem": "-----BEGIN CERTIFICATE-----\n…leaf…\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\n…intermediate…",
  "private_key_pem": "-----BEGIN PRIVATE KEY-----\n…\n-----END PRIVATE KEY-----\n",
  "not_before": "…", "expires_at": "…"
}
```

Hard constraints from Rocket's TLS stack (native-tls 0.2.18, checked in source):

- `private_key_pem` must be **unencrypted PKCS#8 and start exactly with `-----BEGIN PRIVATE KEY-----`**. All three backends (OpenSSL, Security.framework, schannel) reject anything else: `BEGIN RSA PRIVATE KEY`, `BEGIN EC PRIVATE KEY`, `BEGIN ENCRYPTED PRIVATE KEY`, a leading BOM or whitespace, or a `Bag Attributes` preamble.
- `certificate_pem` must list the **leaf first, then the intermediates in chain order**. The root is optional.
- RSA keys are fine. EC PKCS#8 keys are expected to work on Linux and macOS and are likely to fail on Windows (schannel uses an RSA provider). Please include the key algorithm in the metadata (see R5) so Rocket can warn.

### R3. PKCS12 response (SHOULD)

```json
{ "id": "…", "name": "…", "version": "…", "format": "pkcs12", "pkcs12_base64": "…" }
```

- Encrypted with the caller-supplied `password`. An empty password is allowed.
- Use **broadly compatible algorithms**: `PBE-SHA1-3DES` for certificate and key with a SHA-1 MAC. OpenSSL 3's default AES-256-CBC + PBKDF2 output is unreadable by older Security.framework and Windows stacks.
- RocketVault's answer: the default stays modern (AES-256-CBC + PBKDF2) and a `compat` parameter selects `"legacy"` (PBE-SHA1-3DES with a SHA-1 MAC). **Rocket always sends `compat: "legacy"`.**
- Rocket will source the password from a RocketVault secret, so it can be any string the vault generates or the caller supplies.

### R4. Authorization (MUST)

- A new data action for single-certificate export, distinct from the bulk `ActionCertificatesExport` and from backup.
- Granted to a **new narrow role** (for example "Key Vault Certificate Exporter") and to Administrator. It must **not** be in Reader, Secrets User, Certificate User, Officer or Crypto roles by default.
- The Rocket service principal then needs only that role plus Secrets User. It must not need Officer or Administrator.
- Vault-scoped role assignments must apply.

### R5. `exportable` flag and metadata (MUST)

- `exportable` (bool) on the certificate, **default false**, set at creation or import (Azure makes it immutable once set, which is acceptable).
- Export of a non-exportable certificate returns 403 with code `certificate_not_exportable`.
- Certificates whose key lives on an HSM (`pkcs11:` handle) are never exportable (same code, with a reason).
- `CertificateResponse` (used by `GET /certificates` and `GET /certificates/{id}`) gains `exportable` and `key_algorithm` (for example `RSA-2048`, `EC-P256`). Rocket uses the list to fill a picker and to disable non-exportable entries.

### R6. Error contract (MUST)

JSON body `{"error": {"code": "…", "message": "…"}}` with these codes:

| HTTP | code | Rocket behaviour |
|---|---|---|
| 401 | `unauthorized` | evict the cached token, fail the request |
| 403 | `forbidden` | "service principal lacks the export role" |
| 403 | `certificate_not_exportable` | "certificate is not marked exportable" |
| 404 | `not_found` | "certificate not found in this vault" |
| 400 | `bad_request` | invalid `format`, or `pkcs12` without a password |
| 409 | `certificate_disabled` | "certificate is disabled" |

No error may contain key material or the password.

### R7. Transport and handling (MUST)

- `Cache-Control: no-store` and `Pragma: no-cache` on the response.
- The response is a normal JSON body. No `Content-Disposition`.
- Key material and the password must never appear in server logs.
- Response size is small (under 64 KiB). Rocket will cap what it reads at 1 MiB.

### R8. Audit (MUST)

Every export attempt, allowed or denied, is recorded with principal, vault, certificate id, name and version, format and outcome. Never the material or the password.

### R9. Documentation (MUST)

Add the route, request and response shapes, error codes, the role and the `exportable` flag to `docs/api-specification.yaml`, `docs/api-developer-guide.md` and `docs/integration-examples.md`. Keep the `backup`/`restore` and bulk sealed-export descriptions clearly separate.

## 4. Keys

Rocket's mTLS does not need a standalone key export: the private key travels with the certificate (R2, R3). Rocket does not plan any other use of raw keys.

If you still want key export:

- It reverses the 2026-08-25 decision record, so it is your call.
- Software-backed keys only. `exportable` fixed at creation. HSM or `pkcs11:` keys never.
- Same shape: `POST .../keys/{key_id}/export` returning unencrypted PKCS#8 PEM (`-----BEGIN PRIVATE KEY-----`), the same role, audit and error contract.

## 5. Alternative that needs no new Rocket code: certificate-linked secret

If you build the Azure-style certificate-to-secret linkage already planned (P5): `GET /api/v1/vaults/{vault}/secrets/{certificate_name}` returns the certificate chain PEM plus the PKCS#8 key (or a PKCS12 base64) **when the certificate is exportable**, under the Secrets User role. Rocket's existing secret fetch then works unchanged, and only its planned "material from vault secrets" feature is needed. This is Rocket's preferred route if the linkage is cheaper for you than R1 to R9. The same R5 to R8 rules apply (exportable flag, audit, no caching, no logging).

## 6. Not needed by Rocket

The bulk sealed export, CSV, the argon2id envelope, and the backup blob.

## 7. What Rocket will do on its side

- Add a "vault certificate" source to an environment's client certificate entry: a reference to (connection, vault, certificate), a format choice, and for PKCS12 a password taken from a vault secret placeholder.
- Fetch at send time with the existing token. Hold the material in memory only, zeroized, redacted in logs, never written to disk or to the environment file. Fail loudly if the fetch fails (no fallback to another source).
- Load the PEM with the native TLS identity API and the PKCS12 with its DER API.
- Show a picker of exportable certificates, using R5 metadata.
- Map R6 codes to clear messages.

## 8. Acceptance checks Rocket will run

1. Export as PEM and as PKCS12, then a real TLS handshake against `openssl s_server -Verify 1` with each identity. Rocket already has this test for files.
2. A non-exportable certificate returns 403 `certificate_not_exportable`.
3. A principal without the role returns 403 `forbidden`, and the audit log shows the denial.
4. An unknown certificate returns 404. PKCS12 without a password returns 400.
5. The response carries `Cache-Control: no-store`, and no material appears in server logs.
6. EC and RSA certificates both export, and `key_algorithm` is reported correctly.

## 9. RocketVault's answers (2026-10-01, design direction, not final until specced)

1. **Route:** the per-item route (R1 to R9), not the certificate-linked secret. It gives a separately audited data action and leaves GET secret unchanged. The linked secret depends on the unbuilt P5 linkage and would expose private keys to Secrets User unless also gated on the exporter role.
2. **PKCS12:** a `compat` parameter. Default modern (AES-256-CBC + PBKDF2). `compat: "legacy"` selects PBE-SHA1-3DES with a SHA-1 MAC. Rocket asks for legacy.
3. **Root:** `certificate_pem` is the leaf first, then intermediates, **no root by default**. An `include_root` flag may come later if needed.
4. **exportable:** immutable after creation, set at create or import only, because flipping it to true later would expose an existing private key.

Also: this is a new design, separate from the bulk sealed export of 2026-08-25, which Rocket does not need. **Standalone key export stays rejected** and the 2026-08-25 decision record is unchanged. Until the route shape is finalised in a spec, Rocket keeps using ordinary vault secrets.
