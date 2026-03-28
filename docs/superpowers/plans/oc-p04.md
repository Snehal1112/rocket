# OC-P04: Domain — Variable System + Environment + Extensions

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the full OpenCollection variable system (typed values, variants, secrets) and extend Environment with all schema fields (color, extends, dotEnvFilePath, clientCertificates).

**Architecture:** Modify `rocket-environment` crate. New `VariableValue` and `VariableValueVariant` types in `rocket-shared`.

**Tech Stack:** Rust, serde

**Prerequisite:** OC-P03 complete.

**Schema types (6):** Variable, SecretVariable, VariableValue, VariableValueVariant, Environment, Extensions

---

## Task 1: VariableValue + VariableValueVariant types

**Files:**
- Create: `crates/rocket-shared/src/variable_value.rs`
- Modify: `crates/rocket-shared/src/lib.rs`
- Test: inline `#[cfg(test)]`

Schema `VariableValue`: `string | { type: "string"|"number"|"boolean"|"null"|"object", data: string }`
Schema `VariableValueVariant`: `{ title, selected, value: VariableValue }`

- [ ] **Step 1: Write failing tests**

Test: `VariableValue::from_str("hello")` → string form. `VariableValue::typed("42", "number")` → object form. Serde roundtrip for both. `VariableValueVariant` with title + selected + value.

- [ ] **Step 2: Implement VariableValue + VariableValueVariant**

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum VariableValue {
    Simple(String),
    Typed { data: String, value_type: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableValueVariant {
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    pub value: VariableValue,
}
```

Custom serde for `VariableValue` (string or object — same pattern as `Description`).

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p rocket-shared -- variable_value::tests
git add crates/rocket-shared/src/
git commit -m "feat(shared): VariableValue + VariableValueVariant for OpenCollection"
```

---

## Task 2: Extend Variable + add SecretVariable

**Files:**
- Modify: `crates/rocket-environment/src/variable.rs`
- Test: inline `#[cfg(test)]`

Schema `Variable`: `{ name, value: VariableValue | VariableValueVariant[], description, disabled }`
Schema `SecretVariable`: `{ secret: true, name, description, disabled, type }`

- [ ] **Step 1: Write failing tests**

Test: Variable with `description: Description`, `disabled: bool`, `value` as `VariableValue` or array of `VariableValueVariant`. SecretVariable with `type` field.

- [ ] **Step 2: Extend Variable struct**

Add to existing `Variable`:
```rust
pub description: Option<Description>,
pub disabled: Option<bool>,  // schema uses disabled, not enabled
pub value_variants: Option<Vec<VariableValueVariant>>,  // when value is array
pub secret: bool,
pub secret_type: Option<String>,  // "string"|"number"|"boolean"|"null"|"object"
```

- [ ] **Step 3: Run tests + fix constructors + commit**

```bash
cargo test --workspace
git add crates/
git commit -m "feat(environment): Variable gains description, disabled, variants, secret type"
```

---

## Task 3: Extend Environment with full schema fields

**Files:**
- Modify: `crates/rocket-environment/src/environment.rs`
- Test: inline `#[cfg(test)]`

Schema `Environment`: `{ name, color, description, variables, clientCertificates, extends, dotEnvFilePath }`
Schema `Extensions`: `{ }` (free-form object)

- [ ] **Step 1: Write failing tests**

Test: Environment with color, description, extends, dotEnvFilePath fields. Extensions as `serde_json::Value`.

- [ ] **Step 2: Add new fields to Environment**

```rust
pub color: Option<String>,
pub description: Option<Description>,
pub extends: Option<String>,
pub dot_env_file_path: Option<String>,
pub client_certificates: Vec<ClientCertificate>,  // from P05
```

For now, `client_certificates` can be `Vec<serde_json::Value>` until P05 defines `ClientCertificate`.

Add `Extensions` type alias:
```rust
pub type Extensions = serde_json::Value;
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test --workspace
git add crates/
git commit -m "feat(environment): Environment gains color, extends, dotEnvFilePath, description"
```

---

## Milestone Checklist — OC-P04

- [ ] `VariableValue` — string | {type, data} with custom serde
- [ ] `VariableValueVariant` — title, selected, value
- [ ] `Variable` — gains description, disabled, value_variants, secret, secret_type
- [ ] `Environment` — gains color, description, extends, dotEnvFilePath, clientCertificates
- [ ] `Extensions` — free-form type alias
- [ ] All 6 schema types covered
- [ ] `cargo test --workspace` — all pass

---

---
