# OC-P02: Domain — Auth Enum (All 10 Types)

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the `Auth` enum to cover all OpenCollection auth types: None, Inherit, Basic, Bearer, ApiKey, Digest, NTLM, WSSE, AwsV4 (+profileName), and OAuth2 (as a placeholder — full OAuth2 flows are P03).

**Architecture:** Modify `Auth` enum in `rocket-shared`. Each variant matches the schema's field names exactly. OAuth2 is a wrapper around a sub-enum (defined in P03).

**Tech Stack:** Rust, serde

**Prerequisite:** OC-P01 complete.

**Schema reference:** `$defs/Auth`, `$defs/AuthBasic`, `$defs/AuthBearer`, `$defs/AuthApiKey`, `$defs/AuthDigest`, `$defs/AuthNTLM`, `$defs/AuthWsse`, `$defs/AuthAwsV4`

---

## Task 1: Add Inherit, Digest, NTLM, WSSE to Auth enum

**Files:**
- Modify: `crates/rocket-shared/src/types.rs` (Auth enum)
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests for all new auth variants**

```rust
#[test]
fn auth_inherit_serde() {
    let auth = Auth::Inherit;
    let json = serde_json::to_string(&auth).unwrap();
    let back: Auth = serde_json::from_str(&json).unwrap();
    assert_eq!(auth, back);
}

#[test]
fn auth_wsse_serde() {
    let auth = Auth::Wsse { username: "user".into(), password: "pass".into() };
    let json = serde_json::to_string(&auth).unwrap();
    let back: Auth = serde_json::from_str(&json).unwrap();
    assert_eq!(auth, back);
}

#[test]
fn auth_digest_serde() {
    let auth = Auth::Digest { username: "admin".into(), password: "secret".into() };
    let json = serde_json::to_string(&auth).unwrap();
    let back: Auth = serde_json::from_str(&json).unwrap();
    assert_eq!(auth, back);
}

#[test]
fn auth_ntlm_serde() {
    let auth = Auth::Ntlm { username: "CORP\\user".into(), password: "p".into(), domain: "CORP".into() };
    let json = serde_json::to_string(&auth).unwrap();
    let back: Auth = serde_json::from_str(&json).unwrap();
    assert_eq!(auth, back);
}
```

- [ ] **Step 2: Add new variants to Auth enum**

Add these variants (keeping all existing ones):
```rust
Inherit,
Wsse { username: String, password: String },
Digest { username: String, password: String },
Ntlm { username: String, password: String, domain: String },
```

- [ ] **Step 3: Run all auth tests + workspace**

```bash
cargo test -p rocket-shared
cargo test --workspace
```
Expected: ALL PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-shared/src/
git commit -m "feat(shared): Auth::Inherit, Wsse, Digest, Ntlm variants"
```

---

## Task 2: Extend AwsV4 with profileName

**Files:**
- Modify: `crates/rocket-shared/src/types.rs`
- Test: inline `#[cfg(test)]`

Schema `AuthAwsV4`: `type, accessKeyId, secretAccessKey, sessionToken, service, region, profileName`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn auth_awsv4_with_profile_name() {
    let auth = Auth::AwsSigV4 {
        access_key: "AK".into(),
        secret_key: "SK".into(),
        region: "us-east-1".into(),
        service: "s3".into(),
        session_token: None,
        profile_name: Some("prod".into()),
    };
    let json = serde_json::to_string(&auth).unwrap();
    assert!(json.contains("profileName") || json.contains("profile_name"));
    let back: Auth = serde_json::from_str(&json).unwrap();
    assert_eq!(auth, back);
}
```

- [ ] **Step 2: Add `profile_name: Option<String>` to AwsSigV4 variant**

Update the existing `AwsSigV4` variant:
```rust
AwsSigV4 {
    access_key: String,
    secret_key: String,
    region: String,
    service: String,
    session_token: Option<String>,
    profile_name: Option<String>,  // NEW
},
```

**Important:** Fix all existing code that constructs `AwsSigV4` — add `profile_name: None`.

- [ ] **Step 3: Run all tests**

```bash
cargo test --workspace
```
Expected: ALL PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/
git commit -m "feat(shared): AwsSigV4 gains profileName field"
```

---

## Task 3: Update ApiKey placement to match schema

**Files:**
- Modify: `crates/rocket-shared/src/types.rs`
- Test: inline `#[cfg(test)]`

Schema `AuthApiKey`: `placement` is `"header" | "query"` (not `"queryparams"`).

- [ ] **Step 1: Verify and fix ApiKey placement values**

Ensure the `ApiKey` variant's `placement` field uses schema values:
```rust
ApiKey {
    key: String,
    value: String,
    placement: String,  // "header" | "query"
},
```

Write test:
```rust
#[test]
fn auth_apikey_placement_values() {
    let auth = Auth::ApiKey { key: "X-Key".into(), value: "123".into(), placement: "header".into() };
    let json = serde_json::to_string(&auth).unwrap();
    let back: Auth = serde_json::from_str(&json).unwrap();
    assert_eq!(auth, back);

    let auth2 = Auth::ApiKey { key: "token".into(), value: "abc".into(), placement: "query".into() };
    let json2 = serde_json::to_string(&auth2).unwrap();
    let back2: Auth = serde_json::from_str(&json2).unwrap();
    assert_eq!(auth2, back2);
}
```

- [ ] **Step 2: Run tests + commit**

```bash
cargo test --workspace
git add crates/rocket-shared/src/
git commit -m "feat(shared): verify ApiKey placement matches OpenCollection schema"
```

---

## Milestone Checklist — OC-P02

- [ ] `Auth::Inherit` variant
- [ ] `Auth::Wsse { username, password }` variant
- [ ] `Auth::Digest { username, password }` variant
- [ ] `Auth::Ntlm { username, password, domain }` variant
- [ ] `Auth::AwsSigV4` gains `profile_name: Option<String>`
- [ ] `Auth::ApiKey` placement is "header" | "query"
- [ ] All serde roundtrip tests pass
- [ ] `cargo test --workspace` — all pass
