# Design: OAuth2 "Verify SSL Certificates" — Persist and Enforce

**Date**: 2026-04-09
**Status**: Approved

## Problem

In the collection Authorization tab, unchecking "Verify SSL certificates" under an OAuth2 auth type and clicking Save does not persist the change. On next load the checkbox reverts to checked.

There is a second, deeper gap: even if the value were persisted, the Rust token-fetch function ignores it — it always creates a plain `Client::new()` that verifies SSL regardless.

Root cause is two-layered:

1. **Frontend**: `authStateToApi()` in `CollectionOverviewTab.tsx` converts the UI's `AuthState` to the API shape for saving but never includes `verifySsl` in any OAuth2 branch — the field is silently dropped on every save.

2. **Backend**: `OAuth2Settings` (the shared sub-struct nested on every `OAuth2Flow` variant) has no `verify_ssl` field. Even if the frontend sent the value, serde would discard it. The `fetch_client_credentials_token` function in `reqwest_executor.rs` uses `Client::new()` unconditionally.

## Approach

Add `verify_ssl: Option<bool>` to `OAuth2Settings` in `rocket-shared` (Option B — settings sub-struct). This is the correct DDD placement because `OAuth2Settings` already holds behavioral token-fetch settings (`auto_fetch_token`, `auto_refresh_token`), not credentials. A single field definition covers all four OAuth2 flow variants; no duplication.

`oc_conversions.rs` and `opencollection.rs` require no changes — both already pass `settings` through unchanged, so the new field propagates through the storage layer automatically.

## Changes

### 1. `crates/rocket-shared/src/oauth2.rs`

Add to `OAuth2Settings`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none", rename = "verifySsl")]
pub verify_ssl: Option<bool>,
```

`Option` + `skip_serializing_if` ensures existing stored files without the key deserialize cleanly. `None` means "use default (`true`)" at every call site. `rename = "verifySsl"` matches the camelCase wire format used by the rest of the struct.

### 2. `crates/rocket-infra/src/reqwest_executor.rs`

`fetch_client_credentials_token` currently uses `Client::new()`. Changes:

- Add `verify_ssl: bool` parameter.
- Replace `Client::new()` with:
  ```rust
  Client::builder()
      .danger_accept_invalid_certs(!verify_ssl)
      .build()
      .map_err(|e| DomainError::Http(e.to_string()))?
  ```
- At the call site inside the `Auth::OAuth2(flow)` match arm (`ClientCredentials` branch), extract:
  ```rust
  let verify_ssl = settings.as_ref().and_then(|s| s.verify_ssl).unwrap_or(true);
  ```
  and pass it into `fetch_client_credentials_token`.

Other OAuth2 flows (`authorization_code`, `resource_owner_password_credentials`, `implicit`) are currently stubbed with `_ => {}` and need no call-site changes.

### 3. `src/components/collections/CollectionOverviewTab.tsx`

**`toAuthState` (line ~105)**

Currently reads `a.verifySsl` from the top-level auth object. With `verify_ssl` now nested in `settings`, change to:

```ts
verifySsl: (a.settings as Record<string, unknown> | undefined)?.verifySsl as boolean ?? true,
```

**`authStateToApi` — OAuth2 branches**

Add `settings: { verifySsl: o?.verifySsl ?? true }` to every OAuth2 flow branch:

- The `base` object (shared by `client_credentials`, `authorization_code`, `resource_owner_password_credentials`): add `settings: { verifySsl: o?.verifySsl ?? true }`.
- The `implicit` flat object: add `settings: { verifySsl: o?.verifySsl ?? true }`.

This ensures `verifySsl` is always serialized into the nested `settings` key the Rust structs expect.

## Backwards Compatibility

- Existing collection files with no `verifySsl` in their `settings` block: `Option::None` deserializes cleanly; `unwrap_or(true)` preserves the original behaviour (SSL verified by default).
- No migration needed.

## Files Touched

| File | Change |
|---|---|
| `crates/rocket-shared/src/oauth2.rs` | Add `verify_ssl` field to `OAuth2Settings` |
| `crates/rocket-infra/src/reqwest_executor.rs` | Pass `verify_ssl` into token-fetch client builder |
| `src/components/collections/CollectionOverviewTab.tsx` | Fix `toAuthState` read path; add `settings.verifySsl` in `authStateToApi` |

## Out of Scope

- Request-level OAuth2 `verifySsl` persistence (separate tab, separate issue).
- Other OAuth2 flows that are currently stubbed in the executor.
