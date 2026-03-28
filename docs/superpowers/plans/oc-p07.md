# OC-P07: YAML Structs — HTTP Request (20 types)

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** YAML structs for the complete HTTP request: info, details, params, headers, body (all 4 body types + variants + file body), runtime (scripts, assertions, actions), examples, and the top-level HttpRequest.

**Architecture:** Continues `opencollection.rs`. Body is `oneOf[RawBody, FormUrlEncodedBody, MultipartFormBody, FileBody]`.

**Tech Stack:** Rust, serde, serde_yaml

**Prerequisite:** OC-P06 complete.

**Schema types (20):** HttpRequest, HttpRequestInfo, HttpRequestDetails, HttpRequestParam, HttpRequestHeader, HttpRequestBody, RawBody, FormUrlEncodedBody, MultipartFormBody, FileBody, FileBodyVariant, HttpRequestBodyVariant, HttpRequestRuntime, HttpRequestSettings, Script, Scripts, ScriptFile, Action, ActionSetVariable, HttpRequestExample, HttpResponseHeader

---

## Task 1: OcHttpRequestInfo + OcHttpRequestParam + OcHttpRequestHeader + OcHttpRequestBody (all variants)

**Files:**
- Modify: `crates/rocket-infra/src/opencollection.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement all HTTP request detail structs**

Key structs:
- `OcHttpRequestInfo` — name, description, type: "http", seq, tags
- `OcHttpRequestParam` — name, value, description, type (query|path), disabled
- `OcHttpRequestHeader` — name, value, description, disabled
- `OcHttpRequestBody` — oneOf[RawBody, FormUrlEncodedBody, MultipartFormBody, FileBody]. Discriminated by `type` field.
- `OcRawBody` — type (json|text|xml|sparql), data (string)
- `OcFormUrlEncodedBody` — type: "form-urlencoded", data (array of {name, value, description, disabled})
- `OcMultipartFormBody` — type: "multipart-form", data (array of {name, type (text|file), value (string|string[]), description, disabled})
- `OcFileBody` — type: "file", data (array of FileBodyVariant)
- `OcFileBodyVariant` — filePath, contentType, selected
- `OcHttpRequestBodyVariant` — title, selected, body

Tests: parse JSON body, form body, multipart body, file body, body variants from YAML.

- [ ] **Step 2: Run tests + commit**

```bash
cargo test -p rocket-infra -- opencollection::tests
git add crates/rocket-infra/src/opencollection.rs
git commit -m "feat(infra): OcHttpRequest detail structs — params, headers, body (4 types + variants)"
```

---

## Task 2: OcHttpRequestRuntime + OcScript + OcAssertion + OcAction + OcHttpRequestExample

**Files:**
- Modify: `crates/rocket-infra/src/opencollection.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement runtime structs**

- `OcScript` — type (before-request|after-response|tests|hooks), code
- `OcScripts` — Vec<OcScript> (type alias)
- `OcAssertion` — expression, operator, value, disabled, description
- `OcAction` — oneOf[OcActionSetVariable] (currently only set-variable)
- `OcActionSetVariable` — type: "set-variable", description, phase, selector: {expression, method: "jsonq"}, variable: {name, scope}, disabled
- `OcScriptFile` — type: "script", script (string)
- `OcHttpRequestRuntime` — variables, scripts, assertions, actions, auth
- `OcHttpRequestExample` — name, description, request: {url, method, headers, params, body}, response: {status, statusText, headers, body: {type, data}}

Tests: parse runtime with scripts + assertions + actions, parse example with request/response.

- [ ] **Step 2: Run tests + commit**

```bash
cargo test -p rocket-infra -- opencollection::tests
git add crates/rocket-infra/src/opencollection.rs
git commit -m "feat(infra): OcRuntime + OcScript + OcAction + OcHttpRequestExample"
```

---

## Task 3: Top-level OcHttpRequest

**Files:**
- Modify: `crates/rocket-infra/src/opencollection.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement OcHttpRequest + parse full request YAML file**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcHttpRequest {
    pub info: OcHttpRequestInfo,
    pub http: OcHttpRequestDetails,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<OcHttpRequestRuntime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<OcHttpRequestSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<OcHttpRequestExample>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}
```

Test: parse a complete HTTP request YAML file with all sections populated. Verify every field.

- [ ] **Step 2: Run tests + commit**

```bash
cargo test -p rocket-infra -- opencollection::tests
git add crates/rocket-infra/src/opencollection.rs
git commit -m "feat(infra): OcHttpRequest — complete HTTP request YAML struct"
```

---

## Milestone Checklist — OC-P07

- [ ] All 20 HTTP request schema types implemented as YAML structs
- [ ] Body dispatch: RawBody | FormUrlEncodedBody | MultipartFormBody | FileBody
- [ ] Body variants: OcHttpRequestBodyVariant (title, selected, body)
- [ ] Scripts: before-request, after-response, tests, hooks
- [ ] Actions: set-variable with selector + variable scope
- [ ] Examples: saved request/response pairs
- [ ] Full OcHttpRequest parses complete YAML files

---

---
