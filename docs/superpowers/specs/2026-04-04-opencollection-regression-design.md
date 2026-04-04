# OpenCollection Regression Prevention Design (2026-04-04)

## Goal
Prevent regressions in the OpenCollection YAML handling (serialization, deserialization, and field naming) so that Rocket remains compatible with Bruno and the official OpenCollection specification.

## Approach Overview
We will introduce a **contract‑test suite**, **schema validation**, and **CI integration**:

1. **Contract tests** – round‑trip tests for every major OpenCollection struct (`OcCollection`, `OcHttpRequest`, `OcFolder`, etc.).
2. **JSON‑Schema validation** – generate a schema from the Rust types using `schemars` and validate all `.yml` files on load and in CI.
3. **CI gate** – run the contract tests and schema validator on every PR; failures block merges.

## Detailed Design
### 1. Test Fixtures
- Directory: `rocket-infra/tests/fixtures/opencollection/`.
- Contains minimal but complete YAML examples for each top‑level type.
- Fixtures are version‑controlled; when the spec changes new fixtures are added.

### 2. Round‑Trip Contract Test
File: `rocket-infra/tests/opencollection_contract.rs`.
```rust
#[test]
fn round_trip_opencollection_fixtures() {
    let fixtures = std::fs::read_dir("tests/fixtures/opencollection").unwrap();
    for entry in fixtures {
        let path = entry.unwrap().path();
        let yaml = std::fs::read_to_string(&path).unwrap();
        let original: OcCollection = serde_yaml::from_str(&yaml).unwrap();
        let serialized = serde_yaml::to_string(&original).unwrap();
        let roundtrip: OcCollection = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(original, roundtrip, "Round‑trip mismatch in {}", path.display());
    }
}
```
- Mirrors the existing workspace‑config round‑trip test.

### 3. Schema Generation Binary
File: `rocket-infra/src/bin/validate_oc.rs`.
```rust
use schemars::schema_for;
use rocket_infra::opencollection::*;
use serde_yaml;
use jsonschema::{JSONSchema, Draft};
use std::env;
fn main() {
    let schema = schema_for!(OcCollection);
    let schema_json = serde_json::to_value(&schema).unwrap();
    let validator = JSONSchema::options().with_draft(Draft::Draft7).compile(&schema_json).unwrap();
    for arg in env::args().skip(1) {
        let yaml = std::fs::read_to_string(&arg).expect("read file");
        let data: serde_json::Value = serde_yaml::from_str(&yaml).expect("yaml -> json");
        if let Err(errors) = validator.validate(&data) {
            eprintln!("❌ Validation failed for {}", arg);
            for err in errors { eprintln!("  - {}", err); }
            std::process::exit(1);
        }
    }
    println!("✅ All files validated against OpenCollection schema");
}
```
- Produces a JSON‑Schema from the Rust types and validates any supplied file.

### 4. CI Workflow Changes
Update `.github/workflows/ci.yml` (or the project’s CI file) to add a step:
```yaml
- name: OpenCollection contract tests
  run: cargo test -p rocket-infra --test opencollection_contract
- name: Validate OpenCollection YAML
  run: cargo run --bin validate_oc **/*.yml
```
- The job fails on any non‑zero exit, preventing merges that break the contract.

### 5. Documentation
- Add a **How‑to‑run locally** section in the repo README:
  ```bash
  cargo test -p rocket-infra --test opencollection_contract   # run contract tests
  cargo run --bin validate_oc path/to/file.yml               # validate a single file
  ```
- Document the process for adding new fixtures when the spec evolves.

## Ownership & Review
- **Owner:** `rocket-infra` crate maintainer.
- **Review checklist:**
  - No `TODO` placeholders.
  - All file paths are correct and relative to the crate root.
  - Test fixtures are minimal yet cover every variant.
  - CI step names are descriptive.

---
*This design follows the existing workspace‑config test pattern, adds schema validation for field‑name safety (camelCase), and integrates the checks into CI to provide a fast feedback loop.*
