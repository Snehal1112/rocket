# RocketVault certificate source for client certificates: design

Date: 2026-10-02. Status: proposed (awaiting review). Issue: #21 (Plan E).

> Before implementing, read `docs/superpowers/specs/opencollection-spec-reference.md` and the parent design `docs/superpowers/specs/2026-10-01-environment-client-certificates-design.md` (this spec fills in its section 10).

## 1. Context

Plans B to D let an environment's client certificate take its material from files or from RocketVault **secrets** (PEM text or base64 PKCS12 stored as ordinary secrets). RocketVault v-4.0.0 adds a per-certificate export route, so Rocket can fetch a certificate and its private key directly, with no hand-copied secret.

RocketVault contract (from the RocketVault session, 2026-10-02; the branch is local and unpushed, so confirm before release):

- `POST /api/v1/vaults/{vault}/certificates/{id}/export` (and the flat route for the default vault). Body `{"format":"pem"}` or `{"format":"pkcs12","password":"...","compat":"modern"|"legacy"}`; the `password` field must be present (empty allowed); optional `version` (0 means current).
- Responses carry `certificate_pem` (leaf first, intermediates, no root) and `private_key_pem` (unencrypted PKCS#8), or `pkcs12_base64`, plus `id`, `name`, `version`, `not_before`, `expires_at`, `key_algorithm`. They always send `Cache-Control: no-store`.
- Errors use `{"error":{"code","message"}}` with 400 `bad_request`, 403 `certificate_not_exportable`, 404 `not_found`, 409 `certificate_disabled`, 500 `internal_error`. A 401 and a missing-role 403 come from middleware with non-JSON bodies and are read by status only.
- The principal needs the Key Vault Certificate Exporter role (or Administrator); only a global admin grants it. The certificate must have been created exportable over an exportable key; the flag is immutable.
- The route takes the certificate UUID. `GET /api/v1/vaults/{vault}/certificates` returns id and name per certificate, has no name filter and is paged (`page` from 0, `per_page` default 60, maximum 200). Names are unique per vault among non-deleted certificates.

## 2. Decisions (made with the user, 2026-10-02)

1. A vault certificate is a **new entry type** `vault`, not a mode of `pem` or `pkcs12`.
2. The **format is chosen by the user** per entry (`pem` default, or `pkcs12`). The picker warns when the certificate is EC and the app runs on Windows.
3. For PKCS12 the **password is random per export, never stored**, used once to open the bundle in memory, then zeroized.
4. The stored **name** is resolved to the id at send time, with the **id cached in memory** and one retry on a 404. Certificate material is never cached.
5. The entry names its RocketVault through an **existing External Secrets binding alias**, not its own connection and vault.

## 3. Goals and non-goals

Goals: use a RocketVault certificate for mTLS on requests and OAuth2 token requests; nothing from RocketVault reaches disk; the rules of the parent design hold (`Zeroizing`, fail only when selected, no fallback).

Non-goals: pinning a certificate version; certificate import or creation from Rocket; standalone key export; a RocketVault write-back (still a spec non-goal); load tests (see 6).

## 4. Persisted model

A new entry type next to `pem` and `pkcs12`:

```yaml
- type: vault
  domain: api.example.com
  binding: prod          # External Secrets binding alias (connection and vault come from it)
  certificate: client-a  # certificate name in that vault
  format: pem            # pem or pkcs12, default pem
```

- No id, version, password, key text or file path is stored.
- Save-time rules (rocket-environment `validate_client_certificates`, mirrored in `src/lib/certificate-validation.ts`): non-empty domain; `binding` matches one of the environment's bindings; non-empty `certificate`; `format` is `pem` or `pkcs12`; a value starting with `-----BEGIN` is rejected in any field.
- Backward compatible: additive; an older build ignores the unknown type. `rocket-shared` `ClientCertificate` gains a `Vault` variant (camelCase on its fields like the others). The OpenCollection spec reference gets a note that `vault` is a Rocket extension outside the schema.

## 5. Runtime flow

1. `environment_client_certificates` (rocket-app) turns a `vault` entry into a `ResolvedClientCertificate` whose material is a new variant `CertificateMaterial::Deferred { binding, certificate, format }` in `rocket-http`. It carries no secret.
2. Before dispatch, rocket-app selects the certificate for the request URL with the existing `find_certificate`. If the selected entry is `Deferred`, it asks the fetcher for the material and replaces it with `Inline`. OAuth2 token requests do the same against the token URL. Entries for other domains cause no network call.
3. The fetcher (`VaultSecretFetcher`, same `connection, client_secret, vault_name` arguments as today) gains:
   - `list_certificates(...)`: id, name, `exportable`, `key_algorithm`, enabled and `expires_at`, for the picker.
   - `fetch_certificate(..., name, format, password)`: returns `Zeroizing` bytes plus format and key algorithm.
   The name-to-id walk, the in-memory id cache and the single retry on a 404 live inside the RocketVault client in rocket-infra, next to its token cache.
4. PKCS12: a fresh random password per call, sent with `compat` set to `legacy`, then zeroized. PEM: the private key is already unencrypted PKCS#8, so the existing `-----BEGIN PRIVATE KEY-----` rule holds.
5. The resulting `Inline` material goes through the unchanged load code, the 1 MiB cap and the redaction forms of Plan C.
6. A `Deferred` entry that reaches the executor is an `InvalidInput` error, never a silent skip. A failed fetch fails only the request that selected the entry, with no fallback to another entry.

## 6. Load tests

`run_load_test` runs with no fetcher, like vault secrets today. A vault certificate there fails with a message saying vault certificates are not available in load tests. Extending this is out of scope, as for Plans B to D.

## 7. UI

- Certificates tab: "Add RocketVault certificate". The entry shows a binding dropdown (from the environment's bindings), a certificate dropdown loaded through a new Tauri command `list_vault_certificates(connection_id, vault_name)`, and a format dropdown (default PEM).
- Non-exportable certificates are listed but disabled; the key algorithm is shown next to each name. An EC certificate with PEM on Windows shows a warning suggesting PKCS12. A stored name missing from the list shows as "not found", like a stale secret reference.
- shadcn primitives, lucide icons, no raw controls, as in the rest of the tab.

## 8. Errors (user-facing)

- 403 with `certificate_not_exportable`: "Certificate is not marked exportable."
- 403 without a JSON body: "The service account lacks the Certificate Exporter role."
- 404: "Certificate not found in this vault." 409: "Certificate is disabled."
- 401: evict the cached token and fail the request. 400 and 500: a generic message naming the status.
- Messages name the certificate and the binding, never key bytes or the password.

## 9. Security

- Same rules as the parent design section 8: material only in memory, `Zeroizing`, never in the environment file, history, logs, events or audit; the resolved type is not `Serialize`; `Debug` prints sizes and source kind only.
- The random password and the exported bytes are never logged or put in an error message. The export response is read with a size cap of 1 MiB.
- Only name-to-id is cached; a cached id is not secret.
- The existing vault write guard (scripts cannot persist vault values) already covers exported material, because it reaches scripts only through the secrets map, which does not include certificate material.

## 10. Testing

- rocket-app: a fake fetcher; only the selected entry is fetched; a failure fails only that request; OAuth2 uses the token URL; load tests get the clear error.
- rocket-infra: `wiremock` for the client: paged list resolution, the id cache, one retry on a 404, each error code, a random password per call, a request body without the password in any log. A real-handshake test against a live RocketVault instance, marked `#[ignore]`, once one is available.
- rocket-environment and frontend: validation for the new type; picker states (disabled non-exportable, stale name, EC on Windows warning).
- Mutation check: a test that fails if `Deferred` ever skips silently in the executor.

## 11. Risks and open questions

1. The RocketVault branch is unpushed; field names or codes could still change. The client isolates the contract in one module so a change is local.
2. The picker needs to know the operating system for the EC warning. Use the Tauri OS information if the app already depends on it, otherwise the user agent; decide in the plan.
3. The id cache has no size limit beyond the number of distinct certificates used in a session; this is acceptable, and the cache is cleared when a connection changes.
4. Interaction with the Flow auth work on another machine: that work changes how auth is inherited in flows, not how certificates are selected. Keep edits in `execution_service.rs` small and pull before starting.

## 12. Delivery

Split into plans of at most three tasks each: (A) model, validation and the `Deferred` material plus the executor guard; (B) the RocketVault client methods and the fetcher trait; (C) rocket-app materialization for requests and OAuth2, and the Tauri command; (D) the Certificates tab UI. Commits are conventional and path-scoped, through the `1-git-commit` skill, with `Relates to: #21`.
