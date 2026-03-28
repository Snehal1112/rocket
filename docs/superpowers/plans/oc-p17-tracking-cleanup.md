# OC-P17: Tracking Items Cleanup

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clean up tracked items: clippy fixes, documentation, Variable field reconciliation, and scope OAuth2 client_credentials execution.

**Architecture:** Small targeted fixes across multiple files. OAuth2 client_credentials is the only substantial task.

**Tech Stack:** Rust, reqwest (for OAuth2 HTTP calls)

**Prerequisite:** OC-P14 complete.

---

## Task 1: Fix Clippy `manual_strip` Warning

**Files:** `crates/rocket-infra/src/fs_collection_repo.rs`

The `save_request()` function at line 295 uses manual slicing `&path[..path.len() - 5]` instead of `strip_suffix(".json")`. Clippy warns about this pattern.

### Steps

- [ ] **1.1** In `crates/rocket-infra/src/fs_collection_repo.rs`, locate the `save_request` method (line 288). Replace the manual-strip block:
  ```rust
  // Current (line 291-298):
  let base = if path.ends_with(".yml") || path.ends_with(".yaml") {
      path.to_string()
  } else if path.ends_with(".json") {
      // Strip .json and add .yml for migration.
      format!("{}.yml", &path[..path.len() - 5])
  } else {
      format!("{}.yml", path)
  };
  ```
  With:
  ```rust
  let base = if path.ends_with(".yml") || path.ends_with(".yaml") {
      path.to_string()
  } else if let Some(stem) = path.strip_suffix(".json") {
      // Replace .json with .yml for migration.
      format!("{}.yml", stem)
  } else {
      format!("{}.yml", path)
  };
  ```

- [ ] **1.2** Run clippy to verify the warning is gone:
  ```bash
  cargo clippy -p rocket-infra -- -W clippy::manual_strip 2>&1 | grep manual_strip
  ```
  Expected: no output (no warnings).

- [ ] **1.3** Run existing tests to confirm no regressions:
  ```bash
  cargo test -p rocket-infra
  ```

**Commit:** `fix(infra): replace manual_strip with strip_suffix in save_request`

---

## Task 2: Document `collection.json` Exception

**Files:** `crates/rocket-infra/src/fs_collection_repo.rs`

The `settings_path()` method (line 96) returns `collection.json`. All other data files migrated to YAML, but collection settings intentionally remain JSON because they are an internal sidecar, not user-authored content.

### Steps

- [ ] **2.1** Add a doc comment above `settings_path()` explaining the JSON exception:
  ```rust
  /// Path to the collection settings sidecar file.
  ///
  /// This intentionally remains as JSON (not YAML) because collection
  /// settings are an internal sidecar managed by the application, not
  /// user-authored content. They are excluded from request file counts
  /// via `is_request_file()`.
  fn settings_path(&self, name: &str) -> PathBuf {
      self.collection_path(name).join("collection.json")
  }
  ```

- [ ] **2.2** Verify the crate still compiles:
  ```bash
  cargo check -p rocket-infra
  ```

**Commit:** `docs(infra): document why collection.json stays as JSON`

---

## Task 3: Variable `enabled`/`disabled` Field Reconciliation

**Files:**
- `crates/rocket-environment/src/variable.rs`
- `crates/rocket-infra/src/oc_conversions.rs` (read-only reference)

The `Variable` struct has both `enabled: bool` and `disabled: Option<bool>`. The OC schema uses `disabled` (inverted logic); the domain model uses `enabled`. Currently the conversion layer in `oc_conversions.rs` maps between them correctly (`enabled: !oc.disabled.unwrap_or(false)`). The problem is that `Variable` itself carries both fields, which can lead to contradictory state.

**Decision:** Make `enabled` the single source of truth. Remove `disabled` from the `Variable` struct. The `disabled` field only exists for OC YAML serialization and is already handled by `oc_conversions.rs`. Domain code should never read `disabled` on `Variable`.

### Steps

- [ ] **3.1** Search for all usages of `Variable::disabled` or `.disabled` on Variable across the codebase:
  ```bash
  cargo grep -e "\.disabled" -- crates/rocket-environment/
  grep -rn "\.disabled" crates/rocket-environment/src/ crates/rocket-infra/src/ src/
  ```
  Confirm that `disabled` on `Variable` is only set in tests and constructors, never read in domain logic (only in `oc_conversions.rs` on the OC types, not on `Variable`).

- [ ] **3.2** In `crates/rocket-environment/src/variable.rs`, remove the `disabled` field from the struct:
  ```rust
  // Remove this field entirely:
  // #[serde(default, skip_serializing_if = "Option::is_none")]
  // pub disabled: Option<bool>,
  ```

- [ ] **3.3** Add a custom deserializer so that incoming JSON/YAML with a `disabled` field is handled gracefully (derives `enabled` from `disabled` if `enabled` is missing). Use `#[serde(deserialize_with = ...)]` or a manual `Deserialize` impl:
  ```rust
  /// Serde helper: when deserializing, if `disabled` is present and
  /// `enabled` is at its default, derive enabled = !disabled.
  #[derive(Deserialize)]
  #[serde(rename_all = "camelCase")]
  struct VariableHelper {
      key: String,
      value: String,
      #[serde(default = "default_true")]
      enabled: bool,
      #[serde(default)]
      disabled: Option<bool>,
      // ... other fields ...
  }

  fn default_true() -> bool { true }
  ```
  Then in `From<VariableHelper> for Variable`, reconcile: if `disabled == Some(true)` then `enabled = false`.

- [ ] **3.4** Update all test code in `variable.rs` that references `disabled: None` or `disabled: Some(...)` to remove the field from struct literals.

- [ ] **3.5** Verify no compile errors:
  ```bash
  cargo check --workspace
  ```

- [ ] **3.6** Run all tests:
  ```bash
  cargo test -p rocket-environment
  ```

- [ ] **3.7** Add a new test that confirms backward-compat deserialization:
  ```rust
  #[test]
  fn deserialize_disabled_field_sets_enabled_false() {
      let json = r#"{"key":"X","value":"1","disabled":true}"#;
      let v: Variable = serde_json::from_str(json).unwrap();
      assert!(!v.enabled);
  }

  #[test]
  fn deserialize_enabled_field_takes_precedence() {
      let json = r#"{"key":"X","value":"1","enabled":true,"disabled":true}"#;
      let v: Variable = serde_json::from_str(json).unwrap();
      // enabled is explicit, takes precedence.
      assert!(v.enabled);
  }
  ```

**Commit:** `refactor(env): remove disabled field from Variable, derive from serde`

---

## Task 4: OAuth2 Client Credentials Flow Execution

**Files:**
- `crates/rocket-infra/src/reqwest_executor.rs`
- `crates/rocket-shared/src/oauth2.rs` (read-only reference)

The `Auth::OAuth2(_)` arm in `apply_auth()` (line 146) is a no-op placeholder. This task implements the `client_credentials` grant type only.

**OAuth2 client_credentials flow:**
1. POST to `access_token_url` with `grant_type=client_credentials`.
2. Credentials sent via Basic Auth header (when `placement` is `"basic_auth_header"` or `None`) or in the POST body (when `placement` is `"body"`).
3. Optionally include `scope`.
4. Parse JSON response for `access_token`.
5. Attach as `Bearer <token>` header to the original request.

**Key consideration:** `apply_auth` is a sync function, but OAuth2 requires an async HTTP call. The function must become async, or the OAuth2 token fetch must happen before `apply_auth` is called.

### Steps

- [ ] **4.1** Make `apply_auth` an `async fn` since it already lives in an async execution path (called from `async fn execute`):
  ```rust
  async fn apply_auth(
      mut builder: reqwest::RequestBuilder,
      auth: &Auth,
      method: &rocket_shared::types::HttpMethod,
  ) -> DomainResult<reqwest::RequestBuilder> {
  ```
  Update the call site in `execute()` to `.await` the result.

- [ ] **4.2** Implement the `Auth::OAuth2(flow)` match arm. Extract a helper function:
  ```rust
  /// Fetch an access token using the OAuth2 client_credentials grant.
  async fn fetch_client_credentials_token(
      access_token_url: &str,
      credentials: &OAuth2ClientCredentials,
      scope: Option<&str>,
  ) -> DomainResult<String> {
      let client = Client::new();
      let mut params = vec![("grant_type", "client_credentials")];
      // scope is optional.
      let scope_owned;
      if let Some(s) = scope {
          scope_owned = s.to_string();
          params.push(("scope", &scope_owned));
      }

      let mut req = client.post(access_token_url).form(&params);

      // Send credentials via Basic Auth or body based on placement.
      let placement = credentials.placement.as_deref().unwrap_or("basic_auth_header");
      match placement {
          "body" => {
              params.push(("client_id", &credentials.client_id));
              params.push(("client_secret", &credentials.client_secret));
              req = client.post(access_token_url).form(&params);
          }
          _ => {
              // Default: Basic Auth header.
              req = req.basic_auth(&credentials.client_id, Some(&credentials.client_secret));
          }
      }

      let resp = req.send().await
          .map_err(|e| DomainError::Http(format!("OAuth2 token request failed: {e}")))?;

      if !resp.status().is_success() {
          let status = resp.status();
          let body = resp.text().await.unwrap_or_default();
          return Err(DomainError::Http(
              format!("OAuth2 token endpoint returned {status}: {body}")
          ));
      }

      let json: serde_json::Value = resp.json().await
          .map_err(|e| DomainError::Http(format!("OAuth2 token response parse error: {e}")))?;

      json["access_token"]
          .as_str()
          .map(|s| s.to_string())
          .ok_or_else(|| DomainError::Http("OAuth2 response missing access_token".into()))
  }
  ```

- [ ] **4.3** Wire the helper into the `Auth::OAuth2` match arm:
  ```rust
  Auth::OAuth2(flow) => {
      match flow {
          OAuth2Flow::ClientCredentials {
              access_token_url,
              credentials,
              scope,
              ..
          } => {
              let token = fetch_client_credentials_token(
                  access_token_url,
                  credentials,
                  scope.as_deref(),
              ).await?;
              builder = builder.bearer_auth(&token);
          }
          _ => {
              // Other OAuth2 flows not yet implemented.
              log::warn!("OAuth2 flow not yet implemented, skipping auth");
          }
      }
  }
  ```

- [ ] **4.4** Add `serde_json` to imports at the top of `reqwest_executor.rs` (it is already a workspace dep).

- [ ] **4.5** Verify compilation:
  ```bash
  cargo check -p rocket-infra
  ```

- [ ] **4.6** Add a test using a mock HTTP server. Add `wiremock` as a dev-dependency in `crates/rocket-infra/Cargo.toml`:
  ```toml
  [dev-dependencies]
  wiremock = "0.6"
  ```
  Then add the test:
  ```rust
  #[cfg(test)]
  mod oauth2_tests {
      use super::*;
      use rocket_shared::oauth2::{OAuth2ClientCredentials, OAuth2Flow};
      use wiremock::{Mock, MockServer, ResponseTemplate};
      use wiremock::matchers::{method, path, header_exists};

      #[tokio::test]
      async fn client_credentials_fetches_token_and_attaches_bearer() {
          let mock_server = MockServer::start().await;

          Mock::given(method("POST"))
              .and(path("/token"))
              .and(header_exists("Authorization"))
              .respond_with(
                  ResponseTemplate::new(200)
                      .set_body_json(serde_json::json!({
                          "access_token": "test-token-abc",
                          "token_type": "bearer",
                          "expires_in": 3600
                      }))
              )
              .mount(&mock_server)
              .await;

          let token_url = format!("{}/token", mock_server.uri());

          let token = fetch_client_credentials_token(
              &token_url,
              &OAuth2ClientCredentials {
                  client_id: "my-client".into(),
                  client_secret: "my-secret".into(),
                  placement: None,
              },
              Some("read write"),
          ).await.unwrap();

          assert_eq!(token, "test-token-abc");
      }

      #[tokio::test]
      async fn client_credentials_body_placement() {
          let mock_server = MockServer::start().await;

          Mock::given(method("POST"))
              .and(path("/token"))
              .respond_with(
                  ResponseTemplate::new(200)
                      .set_body_json(serde_json::json!({
                          "access_token": "body-token-xyz",
                          "token_type": "bearer"
                      }))
              )
              .mount(&mock_server)
              .await;

          let token_url = format!("{}/token", mock_server.uri());

          let token = fetch_client_credentials_token(
              &token_url,
              &OAuth2ClientCredentials {
                  client_id: "cid".into(),
                  client_secret: "csecret".into(),
                  placement: Some("body".into()),
              },
              None,
          ).await.unwrap();

          assert_eq!(token, "body-token-xyz");
      }

      #[tokio::test]
      async fn client_credentials_error_response() {
          let mock_server = MockServer::start().await;

          Mock::given(method("POST"))
              .and(path("/token"))
              .respond_with(
                  ResponseTemplate::new(401)
                      .set_body_json(serde_json::json!({
                          "error": "invalid_client"
                      }))
              )
              .mount(&mock_server)
              .await;

          let token_url = format!("{}/token", mock_server.uri());

          let result = fetch_client_credentials_token(
              &token_url,
              &OAuth2ClientCredentials {
                  client_id: "bad".into(),
                  client_secret: "bad".into(),
                  placement: None,
              },
              None,
          ).await;

          assert!(result.is_err());
          let err = result.unwrap_err().to_string();
          assert!(err.contains("401"), "Error should mention status: {}", err);
      }
  }
  ```

- [ ] **4.7** Run all tests:
  ```bash
  cargo test -p rocket-infra
  ```

**Commit:** `feat(infra): implement OAuth2 client_credentials token fetch in HTTP executor`

---

## Task 5: Frontend P14 Manual Smoke Test Checklist

This is a manual verification task. No code changes. Perform after all code tasks above are complete and the app is running locally (`cargo tauri dev`).

### Checklist

- [ ] **5.1** Create a new collection via the UI. Verify that `opencollection.yml` appears on disk inside the collection directory.
- [ ] **5.2** Create a new request inside that collection. Verify that a `.yml` file (not `.json`) is written to disk.
- [ ] **5.3** Edit the request (change URL, add a header) and save. Verify the `.yml` file on disk is updated with the new values.
- [ ] **5.4** Place a legacy JSON collection directory (with `.json` request files and no `opencollection.yml`) into the collections base directory. Refresh the sidebar. Verify:
  - The collection appears in the sidebar.
  - An `opencollection.yml` file was auto-created.
  - `.json` request files were migrated to `.yml`.
  - The original `.json` files are removed.
- [ ] **5.5** Open the environment panel. Verify that environment files are stored as `.yml` on disk.
- [ ] **5.6** After making changes, run `git diff` in the collections directory. Verify the diff shows readable YAML (not binary/minified JSON).
- [ ] **5.7** Verify that `collection.json` (settings sidecar) still exists as JSON and is not migrated.

---

## Summary

| Task | Type | Risk | Files |
|------|------|------|-------|
| 1. Clippy manual_strip | Fix | Low | `fs_collection_repo.rs` |
| 2. Document collection.json | Docs | None | `fs_collection_repo.rs` |
| 3. Variable field reconciliation | Refactor | Medium | `variable.rs` |
| 4. OAuth2 client_credentials | Feature | Medium | `reqwest_executor.rs`, `Cargo.toml` |
| 5. Manual smoke test | Verification | None | N/A |

**Estimated effort:** ~2-3 hours for tasks 1-4. Task 5 is ~15 minutes of manual testing.

**Order of execution:** Tasks 1, 2 can run in parallel. Task 3 is independent. Task 4 is independent. Task 5 runs last.
