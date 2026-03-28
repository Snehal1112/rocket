# OC-P03: Domain — OAuth2 Full 4 Flows

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the full OAuth2 auth model from the OpenCollection schema — 4 separate flow types (client_credentials, resource_owner_password, authorization_code, implicit), each with nested credentials, PKCE, token config, placement, additional parameters, and settings.

**Architecture:** New `oauth2.rs` module in `rocket-shared`. The `Auth::OAuth2` variant wraps an `OAuth2Flow` enum with 4 variants. This replaces the flat OAuth2 struct from SP2.

**Tech Stack:** Rust, serde

**Prerequisite:** OC-P02 complete.

**Schema types covered (14):** AuthOAuth2, OAuth2ClientCredentialsFlow, OAuth2ResourceOwnerPasswordFlow, OAuth2AuthorizationCodeFlow, OAuth2ImplicitFlow, OAuth2ClientCredentials, OAuth2ResourceOwner, OAuth2PKCE, OAuth2AdditionalParameter, OAuth2TokenConfig, OAuth2TokenPlacement, OAuth2TokenPlacedInHeader, OAuth2TokenPlacedInQuery, OAuth2Settings

---

## Task 1: OAuth2 sub-types (Credentials, PKCE, TokenConfig, Settings, AdditionalParameter)

**Files:**
- Create: `crates/rocket-shared/src/oauth2.rs`
- Modify: `crates/rocket-shared/src/lib.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_credentials_serde() {
        let creds = OAuth2ClientCredentials {
            client_id: "id".into(), client_secret: "secret".into(),
            placement: Some("basic_auth_header".into()),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: OAuth2ClientCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(creds, back);
    }

    #[test]
    fn pkce_config() {
        let pkce = OAuth2PKCE { enabled: true, method: Some("S256".into()) };
        let json = serde_json::to_string(&pkce).unwrap();
        let back: OAuth2PKCE = serde_json::from_str(&json).unwrap();
        assert_eq!(pkce, back);
    }

    #[test]
    fn token_placement_header() {
        let p = OAuth2TokenPlacement::Header { header: "Authorization".into() };
        let json = serde_json::to_string(&p).unwrap();
        let back: OAuth2TokenPlacement = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn token_placement_query() {
        let p = OAuth2TokenPlacement::Query { query: "access_token".into() };
        let json = serde_json::to_string(&p).unwrap();
        let back: OAuth2TokenPlacement = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn token_config_with_placement() {
        let tc = OAuth2TokenConfig {
            id: Some("my-token".into()),
            placement: Some(OAuth2TokenPlacement::Header { header: "Authorization".into() }),
        };
        let json = serde_json::to_string(&tc).unwrap();
        let back: OAuth2TokenConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(tc, back);
    }

    #[test]
    fn additional_parameter() {
        let ap = OAuth2AdditionalParameter {
            name: "audience".into(), value: "https://api.example.com".into(),
            placement: Some("body".into()),
        };
        assert_eq!(ap.name, "audience");
    }

    #[test]
    fn settings() {
        let s = OAuth2Settings { auto_fetch_token: Some(true), auto_refresh_token: Some(false) };
        let json = serde_json::to_string(&s).unwrap();
        let back: OAuth2Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
```

- [ ] **Step 2: Implement sub-types**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2ClientCredentials {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,  // "basic_auth_header" | "body"
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OAuth2ResourceOwner {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OAuth2PKCE {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,  // "S256" | "plain"
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OAuth2TokenPlacement {
    Header { header: String },
    Query { query: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OAuth2TokenConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<OAuth2TokenPlacement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OAuth2AdditionalParameter {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,  // "header" | "query" | "body"
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2AdditionalParameters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_request: Option<Vec<OAuth2AdditionalParameter>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token_request: Option<Vec<OAuth2AdditionalParameter>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token_request: Option<Vec<OAuth2AdditionalParameter>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_fetch_token: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_refresh_token: Option<bool>,
}
```

- [ ] **Step 3: Register module + run tests**

```bash
cargo test -p rocket-shared -- oauth2::tests
git add crates/rocket-shared/src/
git commit -m "feat(shared): OAuth2 sub-types — credentials, PKCE, token config, settings"
```

---

## Task 2: OAuth2Flow enum — 4 flow variants

**Files:**
- Modify: `crates/rocket-shared/src/oauth2.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests for each flow**

```rust
#[test]
fn client_credentials_flow_serde() {
    let flow = OAuth2Flow::ClientCredentials {
        access_token_url: "https://auth.example.com/token".into(),
        refresh_token_url: None,
        credentials: OAuth2ClientCredentials { client_id: "id".into(), client_secret: "s".into(), placement: None },
        scope: Some("read".into()),
        additional_parameters: None,
        token_config: None,
        settings: None,
    };
    let json = serde_json::to_string(&flow).unwrap();
    assert!(json.contains("client_credentials"));
    let back: OAuth2Flow = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, OAuth2Flow::ClientCredentials { .. }));
}

#[test]
fn authorization_code_flow_with_pkce() {
    let flow = OAuth2Flow::AuthorizationCode {
        authorization_url: "https://auth.example.com/authorize".into(),
        access_token_url: "https://auth.example.com/token".into(),
        refresh_token_url: None,
        callback_url: Some("http://localhost:3000/callback".into()),
        credentials: OAuth2ClientCredentials { client_id: "id".into(), client_secret: "s".into(), placement: None },
        scope: Some("openid".into()),
        state: Some("random-state".into()),
        pkce: Some(OAuth2PKCE { enabled: true, method: Some("S256".into()) }),
        additional_parameters: None,
        token_config: None,
        settings: None,
    };
    let json = serde_json::to_string(&flow).unwrap();
    assert!(json.contains("authorization_code"));
    let back: OAuth2Flow = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, OAuth2Flow::AuthorizationCode { .. }));
}

#[test]
fn resource_owner_password_flow() {
    let flow = OAuth2Flow::ResourceOwnerPassword {
        access_token_url: "https://auth.example.com/token".into(),
        refresh_token_url: None,
        credentials: OAuth2ClientCredentials { client_id: "id".into(), client_secret: "s".into(), placement: None },
        resource_owner: Some(OAuth2ResourceOwner { username: "user".into(), password: "pass".into() }),
        scope: None,
        additional_parameters: None,
        token_config: None,
        settings: None,
    };
    let json = serde_json::to_string(&flow).unwrap();
    let back: OAuth2Flow = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, OAuth2Flow::ResourceOwnerPassword { .. }));
}

#[test]
fn implicit_flow() {
    let flow = OAuth2Flow::Implicit {
        authorization_url: "https://auth.example.com/authorize".into(),
        callback_url: Some("http://localhost/cb".into()),
        client_id: "id".into(),
        scope: None,
        state: None,
        additional_parameters: None,
        token_config: None,
        settings: None,
    };
    let json = serde_json::to_string(&flow).unwrap();
    let back: OAuth2Flow = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, OAuth2Flow::Implicit { .. }));
}
```

- [ ] **Step 2: Implement OAuth2Flow enum**

```rust
/// OAuth2 flow — discriminated by `flow` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
pub enum OAuth2Flow {
    #[serde(rename = "client_credentials")]
    ClientCredentials {
        #[serde(rename = "accessTokenUrl")]
        access_token_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "refreshTokenUrl")]
        refresh_token_url: Option<String>,
        credentials: OAuth2ClientCredentials,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "additionalParameters")]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "tokenConfig")]
        token_config: Option<OAuth2TokenConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
    #[serde(rename = "resource_owner_password_credentials")]
    ResourceOwnerPassword {
        #[serde(rename = "accessTokenUrl")]
        access_token_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "refreshTokenUrl")]
        refresh_token_url: Option<String>,
        credentials: OAuth2ClientCredentials,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "resourceOwner")]
        resource_owner: Option<OAuth2ResourceOwner>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "additionalParameters")]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "tokenConfig")]
        token_config: Option<OAuth2TokenConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
    #[serde(rename = "authorization_code")]
    AuthorizationCode {
        #[serde(rename = "authorizationUrl")]
        authorization_url: String,
        #[serde(rename = "accessTokenUrl")]
        access_token_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "refreshTokenUrl")]
        refresh_token_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "callbackUrl")]
        callback_url: Option<String>,
        credentials: OAuth2ClientCredentials,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pkce: Option<OAuth2PKCE>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "additionalParameters")]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "tokenConfig")]
        token_config: Option<OAuth2TokenConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
    #[serde(rename = "implicit")]
    Implicit {
        #[serde(rename = "authorizationUrl")]
        authorization_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "callbackUrl")]
        callback_url: Option<String>,
        #[serde(rename = "clientId")]
        client_id: String,  // implicit flow only needs clientId, not full credentials
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "additionalParameters")]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "tokenConfig")]
        token_config: Option<OAuth2TokenConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket-shared -- oauth2::tests
```
Expected: PASS — all 4 flow tests + 7 sub-type tests.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-shared/src/oauth2.rs
git commit -m "feat(shared): OAuth2Flow enum — 4 flows with full sub-types"
```

---

## Task 3: Wire OAuth2Flow into Auth enum

**Files:**
- Modify: `crates/rocket-shared/src/types.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Replace flat OAuth2 variant with OAuth2Flow**

Change the existing `Auth::OAuth2` variant:
```rust
// Before (flat — from SP2):
OAuth2 { grant_type: String, token_url: String, client_id: String, client_secret: String, scope: String, ... }

// After (wraps OAuth2Flow):
OAuth2(OAuth2Flow),
```

The `Auth` enum's serde needs a custom deserializer since the schema's `Auth` is `oneOf[AuthAwsV4, AuthBasic, ..., AuthOAuth2, "inherit"]` — where `AuthOAuth2` is itself a `oneOf` of 4 flows. The discriminant is the `type` field: `"oauth2"` → then check `flow` field.

- [ ] **Step 2: Update Auth serde to handle OAuth2 with flow sub-dispatch**

The custom deserializer for `Auth` checks `type` field:
- `"oauth2"` → deserialize as `OAuth2Flow` (which dispatches on `flow` field)
- `"basic"` → `Auth::Basic { ... }`
- `"inherit"` string → `Auth::Inherit`
- etc.

- [ ] **Step 3: Fix all existing code that constructs Auth::OAuth2**

Search codebase and update to use `Auth::OAuth2(OAuth2Flow::ClientCredentials { ... })`.

- [ ] **Step 4: Run full workspace tests**

```bash
cargo test --workspace
```
Expected: ALL PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/
git commit -m "feat(shared): Auth::OAuth2 wraps OAuth2Flow — full 4-flow support"
```

---

## Milestone Checklist — OC-P03

- [ ] `OAuth2ClientCredentials` — clientId, clientSecret, placement
- [ ] `OAuth2ResourceOwner` — username, password
- [ ] `OAuth2PKCE` — enabled, method (S256|plain)
- [ ] `OAuth2TokenPlacement` — Header {header} | Query {query}
- [ ] `OAuth2TokenConfig` — id, placement
- [ ] `OAuth2AdditionalParameter` — name, value, placement
- [ ] `OAuth2AdditionalParameters` — per-phase parameter arrays
- [ ] `OAuth2Settings` — autoFetchToken, autoRefreshToken
- [ ] `OAuth2Flow::ClientCredentials` — full spec
- [ ] `OAuth2Flow::ResourceOwnerPassword` — full spec with resourceOwner
- [ ] `OAuth2Flow::AuthorizationCode` — full spec with PKCE + state + callbackUrl
- [ ] `OAuth2Flow::Implicit` — full spec with clientId only
- [ ] `Auth::OAuth2(OAuth2Flow)` — wired into Auth enum
- [ ] All 14 schema types covered
- [ ] `cargo test --workspace` — all pass
