# OC-P06: YAML Structs — Shared Types (Description, Variable, Auth, Settings)

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create `opencollection.rs` in rocket-infra with YAML file format structs for all shared types that appear across multiple request types.

**Architecture:** New module `opencollection.rs` in rocket-infra. Add `serde_yaml` dependency. These structs directly mirror the JSON schema for YAML serialization. The domain types (P01-P05) are the internal model; these are the file-format model.

**Tech Stack:** Rust, serde, serde_yaml

**Prerequisite:** OC-P05 complete.

---

## Task 1: Module scaffold + OcDescription + OcVariable + OcSecretVariable

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml` (add `serde_yaml = "0.9"`)
- Create: `crates/rocket-infra/src/opencollection.rs`
- Modify: `crates/rocket-infra/src/lib.rs`
- Test: inline `#[cfg(test)]`

**Note:** Since the domain types already have custom serde (Description, VariableValue, etc.), the OC structs can often just re-use the domain types directly via serde_yaml. However, we need separate structs when field names differ between schema and domain (e.g., `name` vs `key`, `disabled` vs `enabled`).

- [ ] **Step 1: Create module, add dep, implement OcDescription, OcVariable, OcSecretVariable, OcVariableValue, OcVariableValueVariant**

Each struct mirrors the exact JSON schema field names and types. All use `serde_yaml` for parsing.

Tests: parse YAML strings for each type, verify fields, roundtrip.

- [ ] **Step 2: Run tests + commit**

```bash
cargo test -p rocket-infra -- opencollection::tests
git add crates/rocket-infra/
git commit -m "feat(infra): opencollection module — OcVariable, OcDescription YAML structs"
```

---

## Task 2: OcAuth (full — all 10 types including OAuth2 4 flows)

**Files:**
- Modify: `crates/rocket-infra/src/opencollection.rs`
- Test: inline `#[cfg(test)]`

The schema `Auth` is: `oneOf[AuthAwsV4, AuthBasic, AuthWsse, AuthBearer, AuthDigest, AuthNTLM, AuthApiKey, AuthOAuth2, "inherit"]`. AuthOAuth2 is itself `oneOf[4 flows]`.

- [ ] **Step 1: Implement OcAuth with custom deserializer**

String shorthand: `"inherit"`, `"none"`. Object form: dispatch on `type` field → then for `oauth2`, dispatch on `flow` field.

Full OAuth2 struct must include: flow, accessTokenUrl, refreshTokenUrl, credentials, resourceOwner, scope, state, pkce, callbackUrl, authorizationUrl, additionalParameters, tokenConfig, settings.

Tests: parse each auth type from YAML, verify fields, roundtrip.

- [ ] **Step 2: Run tests + commit**

```bash
cargo test -p rocket-infra -- opencollection::tests
git add crates/rocket-infra/src/opencollection.rs
git commit -m "feat(infra): OcAuth — all 10 auth types + OAuth2 4 flows YAML structs"
```

---

## Task 3: OcRequestSettings + OcHttpRequestSettings + OcGraphQLRequestSettings

**Files:**
- Modify: `crates/rocket-infra/src/opencollection.rs`
- Test: inline `#[cfg(test)]`

Schema settings values can be `bool | "inherit"` or `number | "inherit"`. Need polymorphic serde.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InheritableBoolean {
    Value(bool),
    Inherit(String),  // "inherit"
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InheritableNumber {
    Value(f64),
    Inherit(String),  // "inherit"
}
```

- [ ] **Step 1: Implement settings structs with inheritable values**

Tests: parse settings with `true`, `false`, `"inherit"`, `5000`, `"inherit"` for timeout.

- [ ] **Step 2: Run tests + commit**

```bash
cargo test -p rocket-infra -- opencollection::tests
git add crates/rocket-infra/src/opencollection.rs
git commit -m "feat(infra): OcRequestSettings with inheritable bool/number values"
```

---

## Milestone Checklist — OC-P06

- [ ] `opencollection.rs` module created with `serde_yaml` dep
- [ ] `OcDescription` / `OcDocumentation` — polymorphic YAML
- [ ] `OcVariable` + `OcSecretVariable` + `OcVariableValue` + `OcVariableValueVariant`
- [ ] `OcAuth` — custom deserializer for all 10 types + OAuth2 4 flows
- [ ] `OcHttpRequestSettings` / `OcGraphQLRequestSettings` with `InheritableBoolean` / `InheritableNumber`
- [ ] All structs parse from YAML and roundtrip

---

---
