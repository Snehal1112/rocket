# Dynamic Variables Plan 01: Rust Resolver Integration

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate `$`-prefixed dynamic variable resolution into the existing `resolver.rs` so `{{$guid}}` generates a fresh UUID at send time.

**Architecture:** Minimal change to the existing `resolve()` function — after extracting the variable name, check for `$` prefix before user variable lookup. Dynamic variables take precedence and cannot be shadowed by user-defined variables.

**Tech Stack:** Rust, `rocket-environment` crate

**Spec:** Before starting, read `docs/superpowers/specs/2026-04-21-dynamic-variables-design.md`.

**Depends on:** Plan 00 (dynamic_vars.rs must exist)

---

### Task 1: Add `$` prefix handling to `resolve()`

**Files:**
- Modify: `crates/rocket-environment/src/resolver.rs`

- [ ] **Step 1: Write the failing tests**

Add these tests to the existing `mod tests` block in `resolver.rs`:

```rust
#[test]
fn resolve_dynamic_var_guid() {
    let vars = HashMap::new();
    let result = resolve("{{$guid}}", &vars);
    assert!(uuid::Uuid::parse_str(&result.output).is_ok(),
        "{{{{$guid}}}} should resolve to a valid UUID, got: {}", result.output);
    assert!(result.unresolved.is_empty());
}

#[test]
fn resolve_dynamic_var_not_shadowed_by_user_var() {
    let mut vars = HashMap::new();
    vars.insert("$guid".to_string(), "user-override".to_string());
    let result = resolve("{{$guid}}", &vars);
    // Dynamic var takes precedence — should NOT be "user-override"
    assert_ne!(result.output, "user-override");
    assert!(uuid::Uuid::parse_str(&result.output).is_ok());
}

#[test]
fn resolve_unknown_dynamic_var_left_as_is() {
    let vars = HashMap::new();
    let result = resolve("{{$unknownThing}}", &vars);
    assert_eq!(result.output, "{{$unknownThing}}");
    assert_eq!(result.unresolved, vec!["$unknownThing"]);
}

#[test]
fn resolve_mixed_dynamic_and_regular_vars() {
    let mut vars = HashMap::new();
    vars.insert("baseUrl".to_string(), "https://api.test".to_string());
    let result = resolve("{{baseUrl}}/users/{{$randomUUID}}", &vars);
    assert!(result.output.starts_with("https://api.test/users/"));
    // The UUID portion should be valid
    let uuid_part = result.output.strip_prefix("https://api.test/users/").unwrap();
    assert!(uuid::Uuid::parse_str(uuid_part).is_ok(),
        "UUID portion '{}' is not valid", uuid_part);
}

#[test]
fn resolve_two_dynamic_vars_produce_different_values() {
    let vars = HashMap::new();
    let result = resolve("{{$guid}}-{{$guid}}", &vars);
    let parts: Vec<&str> = result.output.split('-').collect();
    // UUID v4 has 5 dash-separated groups, so two UUIDs joined by '-' = 11 parts
    // Just verify the full output has two distinct UUIDs
    let uuids: Vec<&str> = result.output.splitn(2, |c: char| {
        // Split on the dash between the two UUIDs (position 36)
        false
    }).collect();
    // Simpler: just check length is 36 + 1 + 36 = 73
    assert_eq!(result.output.len(), 73, "Expected two UUIDs separated by dash");
}

#[test]
fn resolve_dynamic_var_with_whitespace() {
    let vars = HashMap::new();
    let result = resolve("{{ $guid }}", &vars);
    assert!(uuid::Uuid::parse_str(&result.output).is_ok(),
        "Whitespace around $guid should still resolve, got: {}", result.output);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p rocket-environment -- resolver::tests
```

Expected: the new tests FAIL because `resolve()` doesn't handle `$` prefix yet.

- [ ] **Step 3: Modify the `resolve()` function**

In `crates/rocket-environment/src/resolver.rs`, add the import at the top:

```rust
use crate::dynamic_vars;
```

Then find this block inside the `if found_closing` branch:

```rust
if found_closing {
    if let Some(value) = variables.get(&var_name_trimmed) {
        output.push_str(value);
    } else {
        // Leave as-is and record as unresolved.
        output.push_str("{{");
        output.push_str(&var_name);
        output.push_str("}}");
        unresolved.push(var_name_trimmed);
    }
}
```

Replace it with:

```rust
if found_closing {
    if let Some(stripped) = var_name_trimmed.strip_prefix('$') {
        // Dynamic variable — generate fresh value, never falls through to user vars
        if let Some(generated) = dynamic_vars::generate(stripped) {
            output.push_str(&generated);
        } else {
            // Unknown $variable — leave as-is, mark unresolved
            output.push_str("{{");
            output.push_str(&var_name);
            output.push_str("}}");
            unresolved.push(var_name_trimmed);
        }
    } else if let Some(value) = variables.get(&var_name_trimmed) {
        output.push_str(value);
    } else {
        // Leave as-is and record as unresolved.
        output.push_str("{{");
        output.push_str(&var_name);
        output.push_str("}}");
        unresolved.push(var_name_trimmed);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rocket-environment -- resolver::tests
```

Expected: all tests PASS (both old and new).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-environment/src/resolver.rs
git commit -m "feat: integrate dynamic variable resolution into resolver ($-prefix)"
```
