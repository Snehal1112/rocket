# OC-P09: Conversions — Shared Types (Header, Param, Body, Auth, Variable, Environment)

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `From`/`Into` conversions between Oc* YAML structs and domain types for all shared types.

**Architecture:** New `oc_conversions.rs` module in rocket-infra. Key mappings: `name`↔`key`, `disabled`↔`!enabled`, auth type dispatch, body type dispatch, variable typed values.

**Tech Stack:** Rust

**Prerequisite:** OC-P08 complete.

---

## Task 1: Header + Param + Description conversions

**Files:**
- Create: `crates/rocket-infra/src/oc_conversions.rs`
- Modify: `crates/rocket-infra/src/lib.rs`

- [ ] **Step 1: Implement + test: OcHttpRequestHeader ↔ Header, OcHttpRequestParam → split into QueryParam[] + PathParam[], OcDescription ↔ Description**

Field mappings: `name`→`key`, `disabled: true`→`enabled: false`, `description`→`description`.

Tests: convert headers with/without description, split params by type, roundtrip.

- [ ] **Step 2: Run tests + commit**

---

## Task 2: Body + Auth conversions

- [ ] **Step 1: Implement + test: OcHttpRequestBody ↔ Body (all 4 body types + file body), OcAuth ↔ Auth (all 10 types + OAuth2 4 flows)**

Auth conversion must handle: string "inherit"/"none" → Auth::Inherit/None, object form → dispatch on type field, OAuth2 → dispatch on flow field → full OAuth2Flow conversion.

Body: RawBody → Body::Json/Text/Xml/Sparql, FormUrlEncodedBody → Body::FormData, MultipartFormBody → Body::MultipartForm, FileBody → Body::Binary.

Tests: convert each auth type, convert each body type, roundtrip.

- [ ] **Step 2: Run tests + commit**

---

## Task 3: Variable + Environment conversions

- [ ] **Step 1: Implement + test: OcVariable/OcSecretVariable ↔ Variable, OcEnvironment ↔ Environment**

Variable: handle typed `VariableValue` (string or {type, data}), variants array, secret flag, description, disabled.

Environment: map name, color, description, extends, dotEnvFilePath, clientCertificates, variables (mixed Variable + SecretVariable).

Tests: convert variable with typed value, convert secret variable, convert environment with all fields.

- [ ] **Step 2: Run tests + commit**

---

## Milestone Checklist — OC-P09

- [ ] Header ↔ conversions (name↔key, disabled↔!enabled, description)
- [ ] Param split/merge (query + path)
- [ ] Body ↔ conversions (all 4 types + file + body variants)
- [ ] Auth ↔ conversions (all 10 types + OAuth2 4 flows)
- [ ] Variable ↔ conversions (typed values, variants, secrets)
- [ ] Environment ↔ conversions (full fields)
- [ ] Description ↔ conversions

---

---
