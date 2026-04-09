# OAuth2 Verify SSL — Persist and Enforce Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make "Verify SSL certificates" in the collection Authorization tab persist across saves and actually control SSL verification when fetching OAuth2 tokens.

**Architecture:** Add `verify_ssl: Option<bool>` to the shared `OAuth2Settings` struct so it propagates through the storage layer automatically; wire it into the reqwest client builder inside `fetch_client_credentials_token`; fix the frontend serialization and deserialization to use the nested `settings.verifySsl` key the Rust structs expect.

**Tech Stack:** Rust (serde, reqwest, wiremock for tests), TypeScript/React (Vitest for tests)

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-shared/src/oauth2.rs` | Add `verify_ssl` field to `OAuth2Settings` struct |
| `crates/rocket-infra/src/reqwest_executor.rs` | Accept `verify_ssl` in `fetch_client_credentials_token`; build SSL-aware client |
| `src/components/collections/CollectionOverviewTab.tsx` | Fix `toAuthState` read path; add `settings.verifySsl` in `authStateToApi` |

No other files need changes. `oc_conversions.rs` and `opencollection.rs` already pass `settings` through unchanged.

---

## Task 1: Add `verify_ssl` to `OAuth2Settings`

**Files:**
- Modify: `crates/rocket-shared/src/oauth2.rs`

- [ ] **Step 1: Write the failing test**

  Open `crates/rocket-shared/src/oauth2.rs`. Inside the `#[cfg(test)]` block, add a test after the existing `settings()` test:

  ```rust
  #[test]
  fn settings_verify_ssl_roundtrip() {
      let s = OAuth2Settings {
          auto_fetch_token: None,
          auto_refresh_token: None,
          verify_ssl: Some(false),
      };
      let json = serde_json::to_string(&s).unwrap();
      assert!(json.contains("verifySsl"), "field must serialize as verifySsl, got: {json}");
      let back: OAuth2Settings = serde_json::from_str(&json).unwrap();
      assert_eq!(back.verify_ssl, Some(false));
  }

  #[test]
  fn settings_verify_ssl_omitted_when_none() {
      let s = OAuth2Settings {
          auto_fetch_token: None,
          auto_refresh_token: None,
          verify_ssl: None,
      };
      let json = serde_json::to_string(&s).unwrap();
      assert!(!json.contains("verifySsl"), "None must be skipped, got: {json}");
  }
  ```

- [ ] **Step 2: Run tests to confirm they fail**

  ```bash
  cargo test -p rocket-shared settings_verify_ssl 2>&1 | tail -20
  ```

  Expected: compile error — `OAuth2Settings` has no field `verify_ssl`.

- [ ] **Step 3: Add the field to `OAuth2Settings`**

  In `crates/rocket-shared/src/oauth2.rs`, update `OAuth2Settings`:

  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct OAuth2Settings {
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub auto_fetch_token: Option<bool>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub auto_refresh_token: Option<bool>,
      #[serde(default, skip_serializing_if = "Option::is_none", rename = "verifySsl")]
      pub verify_ssl: Option<bool>,
  }
  ```

  Note: `rename = "verifySsl"` is required. The struct uses `rename_all = "camelCase"` but `verify_ssl` would camelCase to `verifySsl` anyway — the explicit rename makes the intent clear and guards against any rename_all change in future.

- [ ] **Step 4: Run tests to confirm they pass**

  ```bash
  cargo test -p rocket-shared settings_verify_ssl 2>&1 | tail -20
  ```

  Expected: both tests pass.

- [ ] **Step 5: Check nothing else broke**

  ```bash
  cargo check -p rocket-shared 2>&1 | tail -20
  cargo check -p rocket-infra 2>&1 | tail -20
  ```

  Expected: no errors. `OAuth2Settings` is used as `Option<OAuth2Settings>` everywhere so the new field (with `#[serde(default)]`) is backwards compatible.

- [ ] **Step 6: Commit**

  ```bash
  git add crates/rocket-shared/src/oauth2.rs
  git commit -m "feat(oauth2): add verify_ssl field to OAuth2Settings"
  ```

---

## Task 2: Wire `verify_ssl` into the token-fetch HTTP client

**Files:**
- Modify: `crates/rocket-infra/src/reqwest_executor.rs`

- [ ] **Step 1: Write the failing test**

  In `crates/rocket-infra/src/reqwest_executor.rs`, find the `mod oauth2_tests` block at the bottom. Add a test that calls the token endpoint and passes `verify_ssl: true`. This validates the new parameter without needing a real TLS server.

  Add after the existing OAuth2 tests:

  ```rust
  #[tokio::test]
  async fn client_credentials_respects_verify_ssl_true() {
      use wiremock::matchers::{method, path};
      use wiremock::{Mock, MockServer, ResponseTemplate};
      use rocket_shared::oauth2::OAuth2ClientCredentials;

      let server = MockServer::start().await;
      Mock::given(method("POST"))
          .and(path("/token"))
          .respond_with(
              ResponseTemplate::new(200)
                  .set_body_json(serde_json::json!({ "access_token": "tok123" })),
          )
          .mount(&server)
          .await;

      let creds = OAuth2ClientCredentials {
          client_id: "id".into(),
          client_secret: "secret".into(),
          placement: None,
      };
      let url = format!("{}/token", server.uri());
      let result = fetch_client_credentials_token(&url, &creds, None, true).await;
      assert!(result.is_ok(), "expected Ok, got: {:?}", result);
      assert_eq!(result.unwrap(), "tok123");
  }
  ```

- [ ] **Step 2: Run the test to confirm it fails**

  ```bash
  cargo test -p rocket-infra client_credentials_respects_verify_ssl_true 2>&1 | tail -20
  ```

  Expected: compile error — `fetch_client_credentials_token` does not accept 4 arguments.

- [ ] **Step 3: Update `fetch_client_credentials_token` signature and body**

  Find `fetch_client_credentials_token` (around line 236). Replace it with:

  ```rust
  /// Fetch an access token using the OAuth2 client_credentials grant.
  async fn fetch_client_credentials_token(
      access_token_url: &str,
      credentials: &rocket_shared::oauth2::OAuth2ClientCredentials,
      scope: Option<&str>,
      verify_ssl: bool,
  ) -> DomainResult<String> {
      let client = Client::builder()
          .danger_accept_invalid_certs(!verify_ssl)
          .build()
          .map_err(|e| DomainError::Http(format!("OAuth2 client build failed: {e}")))?;
      let mut params = vec![("grant_type".to_string(), "client_credentials".to_string())];
      if let Some(s) = scope {
          params.push(("scope".to_string(), s.to_string()));
      }

      let placement = credentials.placement.as_deref().unwrap_or("basic_auth_header");
      let req = match placement {
          "body" => {
              params.push(("client_id".to_string(), credentials.client_id.clone()));
              params.push(("client_secret".to_string(), credentials.client_secret.clone()));
              client.post(access_token_url).form(&params)
          }
          _ => {
              // Default: Basic Auth header.
              client
                  .post(access_token_url)
                  .form(&params)
                  .basic_auth(&credentials.client_id, Some(&credentials.client_secret))
          }
      };

      let resp = req
          .send()
          .await
          .map_err(|e| DomainError::Http(format!("OAuth2 token request failed: {e}")))?;

      if !resp.status().is_success() {
          let status = resp.status();
          let body = resp.text().await.unwrap_or_default();
          return Err(DomainError::Http(format!(
              "OAuth2 token endpoint returned {status}: {body}"
          )));
      }

      let json: serde_json::Value = resp
          .json()
          .await
          .map_err(|e| DomainError::Http(format!("OAuth2 token response parse error: {e}")))?;

      json["access_token"]
          .as_str()
          .map(|s| s.to_string())
          .ok_or_else(|| DomainError::Http("OAuth2 response missing access_token".into()))
  }
  ```

- [ ] **Step 4: Update the call site**

  Find the `Auth::OAuth2(flow)` match arm (around line 149). Replace the `ClientCredentials` branch:

  ```rust
  Auth::OAuth2(flow) => {
      match flow {
          rocket_shared::oauth2::OAuth2Flow::ClientCredentials {
              access_token_url,
              credentials,
              scope,
              settings,
              ..
          } => {
              let verify_ssl = settings.as_ref().and_then(|s| s.verify_ssl).unwrap_or(true);
              let token = fetch_client_credentials_token(
                  access_token_url,
                  credentials,
                  scope.as_deref(),
                  verify_ssl,
              )
              .await?;
              builder = builder.bearer_auth(&token);
          }
          _ => {
              // Other OAuth2 flows (authorization_code, implicit, resource_owner_password)
              // require user interaction and are not yet implemented.
          }
      }
  }
  ```

- [ ] **Step 5: Run tests to confirm they pass**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -30
  ```

  Expected: all tests pass including the new one and all pre-existing OAuth2 tests.

- [ ] **Step 6: Commit**

  ```bash
  git add crates/rocket-infra/src/reqwest_executor.rs
  git commit -m "feat(oauth2): pass verify_ssl to token-fetch http client"
  ```

---

## Task 3: Fix frontend serialization and deserialization

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx`

Context: The two functions to change are `toAuthState` (converts API response → UI state, called on load) and `authStateToApi` (converts UI state → API shape, called on save). Both are defined at the top of `CollectionOverviewTab.tsx`.

- [ ] **Step 1: Fix `toAuthState` — read `verifySsl` from nested `settings`**

  Find line ~105 inside `toAuthState` where the `oauth2` branch is built:

  ```ts
  // BEFORE
  verifySsl: (a.verifySsl as boolean) ?? true,
  ```

  Replace with:

  ```ts
  // AFTER — verifySsl lives in settings.verifySsl on the API response
  verifySsl: ((a.settings as Record<string, unknown> | undefined)?.verifySsl as boolean) ?? true,
  ```

- [ ] **Step 2: Fix `authStateToApi` — serialize `verifySsl` into `settings`**

  Find the `case 'oauth2':` block inside `authStateToApi` (around line 176). There are four sub-branches: `implicit`, `authorization_code`, `resource_owner_password_credentials`, and `client_credentials` (the `base` fallback).

  **2a. `base` object** (covers `client_credentials`, `authorization_code`, `resource_owner_password_credentials`):

  ```ts
  // BEFORE
  const base = {
    authType: 'o-auth2' as const,
    flow,
    accessTokenUrl: o?.tokenUrl ?? '',
    credentials: { clientId: o?.clientId ?? '', clientSecret: o?.clientSecret ?? '' },
    scope: o?.scope || undefined,
  };
  ```

  ```ts
  // AFTER
  const base = {
    authType: 'o-auth2' as const,
    flow,
    accessTokenUrl: o?.tokenUrl ?? '',
    credentials: { clientId: o?.clientId ?? '', clientSecret: o?.clientSecret ?? '' },
    scope: o?.scope || undefined,
    settings: { verifySsl: o?.verifySsl ?? true },
  };
  ```

  **2b. `implicit` branch** (has its own flat return, does not use `base`):

  ```ts
  // BEFORE
  if (flow === 'implicit') {
    return {
      authType: 'o-auth2',
      flow: 'implicit',
      authorizationUrl: o?.authorizationUrl ?? '',
      clientId: o?.clientId ?? '',
      callbackUrl: o?.callbackUrl || undefined,
      scope: o?.scope || undefined,
      state: o?.state || undefined,
    } as unknown as Auth;
  }
  ```

  ```ts
  // AFTER
  if (flow === 'implicit') {
    return {
      authType: 'o-auth2',
      flow: 'implicit',
      authorizationUrl: o?.authorizationUrl ?? '',
      clientId: o?.clientId ?? '',
      callbackUrl: o?.callbackUrl || undefined,
      scope: o?.scope || undefined,
      state: o?.state || undefined,
      settings: { verifySsl: o?.verifySsl ?? true },
    } as unknown as Auth;
  }
  ```

- [ ] **Step 3: TypeScript check**

  ```bash
  yarn tsc --noEmit 2>&1 | tail -20
  ```

  Expected: no errors.

- [ ] **Step 4: Lint check**

  ```bash
  yarn check 2>&1 | tail -20
  ```

  Expected: no errors.

- [ ] **Step 5: Commit**

  ```bash
  git add src/components/collections/CollectionOverviewTab.tsx
  git commit -m "fix(collection-auth): persist and read OAuth2 verifySsl via settings"
  ```

---

## Task 4: End-to-end verification

- [ ] **Step 1: Run the full Rust test suite**

  ```bash
  cargo test 2>&1 | tail -30
  ```

  Expected: all tests pass.

- [ ] **Step 2: Run the frontend test suite**

  ```bash
  yarn test 2>&1 | tail -20
  ```

  Expected: all tests pass.

- [ ] **Step 3: Build the frontend**

  ```bash
  yarn build 2>&1 | tail -20
  ```

  Expected: no errors.

- [ ] **Step 4: Manual smoke test**

  Start the app with `yarn tauri dev`. Open a collection → Authorization tab → set auth type to OAuth 2.0 → uncheck "Verify SSL certificates" → click Save. Close the tab and reopen the collection. Confirm the checkbox is still unchecked.
