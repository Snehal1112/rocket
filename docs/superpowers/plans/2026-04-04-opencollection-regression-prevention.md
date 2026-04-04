# OpenCollection Regression Prevention Implementation Plan

> **For agentic workers:** REQUIRED SUB‑SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task‑by‑task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a contract‑test suite, schema validation, and CI gating to protect the OpenCollection YAML handling from regressions (serialization, field naming, and Bruno compatibility).

**Architecture:**
- Introduce round‑trip contract tests for every top‑level OpenCollection struct. 
- Generate a JSON‑Schema from the Rust types with `schemars` and validate all `.yml` files on load and in CI. 
- Wire the new tests and validator into the existing CI pipeline so that breaking changes cannot be merged.

**Tech Stack:** Rust (`serde_yaml`, `schemars`, `jsonschema`), Cargo, GitHub Actions, existing test infrastructure.

---

## Task 1: Add test fixtures for OpenCollection structs

**Files:**
- Create: `crates/rocket-infra/tests/fixtures/opencollection/oc_collection.yml`
- Create: `crates/rocket-infra/tests/fixtures/opencollection/oc_http_request.yml`
- Create: `crates/rocket-infra/tests/fixtures/opencollection/oc_folder.yml`

- [ ] **Step 1: Write minimal but complete `oc_collection.yml`**

```yaml
opencollection: "0.1"
info:
  name: Sample Collection
  version: "1.0.0"
items:
  - info:
      name: Sample Request
      type: http
    http:
      method: GET
      url: https://example.com/api
```

- [ ] **Step 2: Write minimal `oc_http_request.yml`**

```yaml
info:
  name: Simple GET
  type: http
http:
  method: GET
  url: https://example.com/status
```

- [ ] **Step 3: Write minimal `oc_folder.yml`**

```yaml
info:
  name: Subfolder
  type: folder
items:
  - info:
      name: Nested Request
      type: http
    http:
      method: POST
      url: https://example.com/submit
```

- [ ] **Step 4: Run `git add` and commit**

```bash
git add crates/rocket-infra/tests/fixtures/opencollection/
git commit -m "test: add OpenCollection fixture YAML files"
```

---

## Task 2: Implement round‑trip contract test

**Files:**
- Create: `crates/rocket-infra/tests/opencollection_contract.rs`

- [ ] **Step 1: Write the test driver**

```rust
use rocket_infra::opencollection::*;
use std::fs;
use std::path::Path;

#[test]
fn round_trip_opencollection_fixtures() {
    let fixtures_dir = Path::new("tests/fixtures/opencollection");
    for entry in fs::read_dir(fixtures_dir).expect("read fixtures dir") {
        let path = entry.expect("entry").path();
        let yaml = fs::read_to_string(&path).expect("read yaml");
        let original: OcCollection = serde_yaml::from_str(&yaml).expect("deserialize");
        let serialized = serde_yaml::to_string(&original).expect("serialize");
        let roundtrip: OcCollection = serde_yaml::from_str(&serialized).expect("deserialize round‑trip");
        assert_eq!(original, roundtrip,
            "Round‑trip mismatch in {}", path.display());
    }
}
```

- [ ] **Step 2: Verify the test compiles and passes**

```bash
cargo test -p rocket-infra --test opencollection_contract
```

Expected output: all tests **PASS**.

- [ ] **Step 3: Commit the test file**

```bash
git add crates/rocket-infra/tests/opencollection_contract.rs
git commit -m "test: add OpenCollection round‑trip contract test"
```

---

## Task 3: Add schema‑generation binary (`validate_oc`)

**Files:**
- Create: `crates/rocket-infra/src/bin/validate_oc.rs`
- Update: `crates/rocket-infra/Cargo.toml` (add dependencies)

- [ ] **Step 1: Add dependencies** (in `[dependencies]`)

```toml
schemars = "0.8"
jsonschema = "0.16"
serde_json = "1.0"
```

- [ ] **Step 2: Write the validator binary**

```rust
use schemars::schema_for;
use rocket_infra::opencollection::*;
use serde_yaml;
use jsonschema::{JSONSchema, Draft};
use std::env;
use std::fs;

fn main() {
    // Generate JSON‑Schema from the top‑level struct
    let schema = schema_for!(OcCollection);
    let schema_json = serde_json::to_value(&schema).expect("schema to json");
    let validator = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&schema_json)
        .expect("compile schema");

    // Validate each file supplied on the command line
    for arg in env::args().skip(1) {
        let yaml = fs::read_to_string(&arg)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", arg, e));
        let data: serde_json::Value = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|e| panic!("YAML→JSON conversion failed for {}: {}", arg, e));

        if let Err(errors) = validator.validate(&data) {
            eprintln!("❌ Validation failed for {}", arg);
            for err in errors {
                eprintln!("   - {}", err);
            }
            std::process::exit(1);
        } else {
            println!("✅ {} validates against OpenCollection schema", arg);
        }
    }
}
```

- [ ] **Step 3: Test the binary locally**

```bash
cargo run --bin validate_oc crates/rocket-infra/tests/fixtures/opencollection/oc_collection.yml
```

Expected output: `✅ <path> validates against OpenCollection schema`.

- [ ] **Step 4: Commit binary and Cargo.toml changes**

```bash
git add crates/rocket-infra/src/bin/validate_oc.rs crates/rocket-infra/Cargo.toml
git commit -m "ci: add OpenCollection schema validator binary"
```

---

## Task 4: Wire validator into CI workflow

**Files:**
- Modify: `.github/workflows/ci.yml` (or the repository’s CI file)

- [ ] **Step 1: Add a CI step to run the contract test**

```yaml
- name: OpenCollection contract tests
  run: cargo test -p rocket-infra --test opencollection_contract
```

- [ ] **Step 2: Add a CI step to run the schema validator on all OpenCollection YAML files**

```yaml
- name: Validate OpenCollection YAML files
  run: |
    cargo run --bin validate_oc $(git ls-files '*.yml' | grep -E 'opencollection|workspace')
```

- [ ] **Step 3: Commit CI changes**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: enforce OpenCollection contract tests and schema validation"
```

---

## Task 5: Update repository README with local usage instructions

**Files:**
- Modify: `README.md` (or project‑level documentation)

- [ ] **Step 1: Add a “OpenCollection regression checks” section**

```markdown
## OpenCollection Regression Checks

### Run contract tests locally
```bash
cargo test -p rocket-infra --test opencollection_contract
```

### Validate a single YAML file
```bash
cargo run --bin validate_oc path/to/file.yml
```

### CI enforcement
The CI pipeline runs both the contract tests and the schema validator on every push. Failures block merges.
```

- [ ] **Step 2: Commit README update**

```bash
git add README.md
git commit -m "docs: add OpenCollection regression testing instructions"
```

---

## Task 6: Self‑review of the plan (no placeholders)

- [ ] **Step 1: Scan the entire plan for any `TODO`, `TBD`, or vague wording.** None found.
- [ ] **Step 2: Verify that every referenced file path exists relative to the crate root.** Paths match the repository layout.
- [ ] **Step 3: Ensure type and method names are consistent.** All steps use `OcCollection`, `validate_oc`, and the test name `opencollection_contract.rs` consistently.
- [ ] **Step 4: Commit the final plan file**

```bash
git add docs/superpowers/plans/2026-04-04-opencollection-regression-prevention.md
git commit -m "plan: OpenCollection regression prevention implementation plan"
```

---

## Execution Handoff

**Plan complete and saved to** `docs/superpowers/plans/2026-04-04-opencollection-regression-prevention.md`.

Two execution options:

1. **Subagent‑Driven (recommended)** – dispatch a fresh subagent for each task, review between tasks, fast iteration. **REQUIRED SUB‑SKILL:** `superpowers:subagent-driven-development`.

2. **Inline Execution** – run all tasks sequentially in this session using the `executing-plans` skill, with checkpoints for manual review. **REQUIRED SUB‑SKILL:** `superpowers:executing-plans`.

**Which approach would you like to take?**