# OC-P05: Domain — Request Extensions + Proxy + ClientCertificate

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add remaining domain types: Proxy (with connection config + auth), ClientCertificate (PEM + PKCS12), HttpRequestExample, ActionSetVariable. Extend Request with all OpenCollection fields.

**Architecture:** New types in `rocket-shared`. Request extensions in `rocket-collection`.

**Tech Stack:** Rust, serde

**Prerequisite:** OC-P04 complete.

**Schema types (7+):** Proxy, ProxyConnectionConfig, ProxyAuth, ClientCertificate, PemCertificate, Pkcs12Certificate, HttpRequestExample, Action, ActionSetVariable

---

## Task 1: Proxy + ProxyAuth + ProxyConnectionConfig

**Files:**
- Create: `crates/rocket-shared/src/proxy.rs`
- Test: inline `#[cfg(test)]`

Schema `Proxy`: `{ enabled, inherit, config: ProxyConnectionConfig }`
Schema `ProxyConnectionConfig`: `{ protocol, hostname, port, auth: false | ProxyAuth, bypassProxy }`
Schema `ProxyAuth`: `{ username, password }`

- [ ] **Step 1: Write failing tests** — Proxy with full config, auth=false, auth=ProxyAuth, roundtrip.

- [ ] **Step 2: Implement Proxy, ProxyConnectionConfig, ProxyAuth**

`auth` field is polymorphic: `false | ProxyAuth`. Use custom serde or `Option<ProxyAuth>` where `None` means `false`.

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p rocket-shared -- proxy::tests
git add crates/rocket-shared/src/
git commit -m "feat(shared): Proxy + ProxyConnectionConfig + ProxyAuth"
```

---

## Task 2: ClientCertificate (PEM + PKCS12)

**Files:**
- Create: `crates/rocket-shared/src/certificate.rs`
- Test: inline `#[cfg(test)]`

Schema `ClientCertificate`: `oneOf[PemCertificate | Pkcs12Certificate]`
Schema `PemCertificate`: `{ domain, type: "pem", certificateFilePath, privateKeyFilePath, passphrase? }`
Schema `Pkcs12Certificate`: `{ domain, type: "pkcs12", pkcs12FilePath, passphrase? }`

- [ ] **Step 1: Write failing tests** — PEM cert serde, PKCS12 cert serde, ClientCertificate dispatch on type.

- [ ] **Step 2: Implement**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientCertificate {
    #[serde(rename = "pem")]
    Pem {
        domain: String,
        certificate_file_path: String,
        private_key_file_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
    #[serde(rename = "pkcs12")]
    Pkcs12 {
        domain: String,
        pkcs12_file_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
}
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p rocket-shared -- certificate::tests
git add crates/rocket-shared/src/
git commit -m "feat(shared): ClientCertificate — PEM + PKCS12 with passphrase"
```

---

## Task 3: ActionSetVariable + HttpRequestExample + Extend Request

**Files:**
- Create: `crates/rocket-shared/src/action.rs`
- Modify: `crates/rocket-collection/src/request.rs`
- Test: inline `#[cfg(test)]`

Schema `ActionSetVariable`: `{ type: "set-variable", description, phase, selector: {expression, method}, variable: {name, scope}, disabled }`
Schema `HttpRequestExample`: `{ name, description, request: {url, method, headers, params, body}, response: {status, statusText, headers, body} }`

- [ ] **Step 1: Implement ActionSetVariable + HttpRequestExample**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionSetVariable {
    pub phase: String,  // "before-request" | "after-response"
    pub selector: ActionSelector,
    pub variable: ActionVariable,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionSelector {
    pub expression: String,
    pub method: String,  // "jsonq"
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionVariable {
    pub name: String,
    pub scope: String,  // "runtime" | "request" | "folder" | "collection" | "environment"
}
```

- [ ] **Step 2: Add all OpenCollection fields to Request**

```rust
// Add to Request struct:
pub seq: Option<u32>,
pub tags: Vec<String>,
pub description: Option<Description>,
pub pre_request_script: Option<String>,
pub post_response_script: Option<String>,
pub tests: Option<String>,
pub assertions: Vec<Assertion>,
pub actions: Vec<ActionSetVariable>,
pub examples: Vec<HttpRequestExample>,
pub docs: Option<Documentation>,
pub variables: Vec<Variable>,  // request-level runtime variables
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test --workspace
git add crates/
git commit -m "feat: ActionSetVariable, HttpRequestExample, full Request fields"
```

---

## Milestone Checklist — OC-P05

- [ ] `Proxy` + `ProxyConnectionConfig` + `ProxyAuth`
- [ ] `ClientCertificate` — PEM + PKCS12
- [ ] `ActionSetVariable` + `ActionSelector` + `ActionVariable`
- [ ] `HttpRequestExample` with nested request/response
- [ ] `Request` gains: seq, tags, description, scripts, assertions, actions, examples, docs, variables
- [ ] All 7+ schema types covered
- [ ] `cargo test --workspace` — all pass
