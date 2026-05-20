# Runtime Assertions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement declarative `runtime.assertions` evaluation so assertions saved in request YAML run during execution and appear as pass/fail entries in the existing TestsPanel.

**Architecture:** A pure-Rust `evaluate_assertions()` function in `rocket-app` resolves `res.*` expressions against `HttpResponse` and applies the full Bruno operator set, producing `Vec<TestResult>` appended to the existing `all_test_results` accumulator. The frontend adds an `AssertionsTab` component and wires assertions through `RequestState` → IPC → execution.

**Tech Stack:** Rust (`serde_json` for JSON path resolution, `regex` crate for `matches`/`notMatches`), React + TypeScript, shadcn/ui, CodeMirror 6 (`SingleLineEditor`)

---

## File Map

| File | Action | Purpose |
|---|---|---|
| `crates/rocket-app/src/assertion_evaluator.rs` | **Create** | Pure `evaluate_assertions()` function + all operator logic |
| `crates/rocket-app/src/lib.rs` | **Modify** | Add `pub mod assertion_evaluator;` |
| `crates/rocket-app/Cargo.toml` | **Modify** | Add `regex` dependency |
| `crates/rocket-app/src/execution_service.rs` | **Modify** | Add `assertions` to `ExecuteRequestInput`; call `evaluate_assertions` after tests-script phase |
| `src/lib/tauri-api.ts` | **Modify** | Add `AssertionEntry` interface; add `assertions?` to `Request` interface |
| `src/types/pane-types.ts` | **Modify** | Add `assertions: AssertionEntry[]` to `RequestState` |
| `src/lib/pane-utils.ts` | **Modify** | Map `assertions` in `mapApiRequestToState` and `createDefaultRequest` |
| `src/lib/auto-save.ts` | **Modify** | Include `assertions` in `toApiRequest` |
| `src/components/request/SaveRequestButton.tsx` | **Modify** | Include `assertions` in `buildPayloadFromTab` |
| `src/lib/execute-request.ts` | **Modify** | Include `assertions` in `ExecuteRequestInput` payload |
| `src/components/request/AssertionsTab.tsx` | **Create** | Table-style row editor for assertions |
| `src/components/request/RequestPanel.tsx` | **Modify** | Add `'assertions'` tab, render `AssertionsTab` |

---

## Task 1: Add `assertion_evaluator` module skeleton and `regex` dependency

**Files:**
- Create: `crates/rocket-app/src/assertion_evaluator.rs`
- Modify: `crates/rocket-app/src/lib.rs`
- Modify: `crates/rocket-app/Cargo.toml`

- [ ] **Step 1: Add `regex` to `rocket-app/Cargo.toml`**

Open `crates/rocket-app/Cargo.toml`. In the `[dependencies]` section, add after `base64`:

```toml
regex = "1"
```

- [ ] **Step 2: Create `assertion_evaluator.rs` with the module skeleton**

Create `crates/rocket-app/src/assertion_evaluator.rs` with this full content:

```rust
use regex::Regex;
use rocket_http::HttpResponse;
use rocket_scripting::{TestResult, TestStatus};
use rocket_shared::assertion::Assertion;
use serde_json::Value;

/// Evaluate a slice of declarative assertions against a completed HTTP response.
/// Disabled assertions are skipped. Each assertion produces exactly one TestResult.
pub fn evaluate_assertions(assertions: &[Assertion], response: &HttpResponse) -> Vec<TestResult> {
    assertions
        .iter()
        .filter(|a| a.disabled != Some(true))
        .map(|a| evaluate_one(a, response))
        .collect()
}

fn evaluate_one(assertion: &Assertion, response: &HttpResponse) -> TestResult {
    let name = result_name(assertion);
    let resolved = match resolve_expression(&assertion.expression, response) {
        Ok(v) => v,
        Err(e) => {
            return TestResult {
                name,
                status: TestStatus::Failed,
                error: Some(e),
            }
        }
    };

    let passed = match apply_operator(&assertion.operator, &resolved, assertion.value.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            return TestResult {
                name,
                status: TestStatus::Failed,
                error: Some(e),
            }
        }
    };

    TestResult {
        name,
        status: if passed { TestStatus::Passed } else { TestStatus::Failed },
        error: if passed {
            None
        } else {
            Some(format!(
                "expected {} {} {}",
                assertion.expression,
                assertion.operator,
                assertion.value.as_deref().unwrap_or("")
            ))
        },
    }
}

fn result_name(assertion: &Assertion) -> String {
    match &assertion.value {
        Some(v) if !v.is_empty() => {
            format!("{} {} {}", assertion.expression, assertion.operator, v)
        }
        _ => format!("{} {}", assertion.expression, assertion.operator),
    }
}

/// Resolve a `res.*` expression to a serde_json Value.
fn resolve_expression(expr: &str, response: &HttpResponse) -> Result<Value, String> {
    if expr == "res.status" {
        return Ok(Value::Number(response.status.into()));
    }
    if expr == "res.responseTime" {
        return Ok(Value::Number(response.duration_ms.into()));
    }
    if expr == "res.body" {
        return Ok(Value::String(response.body.clone()));
    }
    if let Some(path) = expr.strip_prefix("res.body.") {
        let parsed: Value = serde_json::from_str(&response.body)
            .map_err(|_| format!("response body is not valid JSON (expression: {expr})"))?;
        return resolve_json_path(&parsed, path)
            .ok_or_else(|| format!("path '{path}' not found in response body"));
    }
    if let Some(header_name) = expr.strip_prefix("res.headers.") {
        return match response.header_value(header_name) {
            Some(v) => Ok(Value::String(v.to_string())),
            None => Ok(Value::Null),
        };
    }
    Err(format!("unknown expression '{expr}'; supported: res.status, res.responseTime, res.body, res.body.<path>, res.headers.<name>"))
}

/// Walk a dot-separated path into a serde_json Value.
fn resolve_json_path(root: &Value, path: &str) -> Option<Value> {
    let mut current = root;
    for key in path.split('.') {
        match current {
            Value::Object(map) => current = map.get(key)?,
            Value::Array(arr) => {
                let idx: usize = key.parse().ok()?;
                current = arr.get(idx)?;
            }
            _ => return None,
        }
    }
    Some(current.clone())
}

/// Apply an operator to a resolved value and optional expected string.
fn apply_operator(operator: &str, value: &Value, expected: Option<&str>) -> Result<bool, String> {
    match operator {
        // ── Equality ────────────────────────────────────────────────────────
        "eq" => {
            let exp = expected.unwrap_or("");
            Ok(coerce_eq(value, exp))
        }
        "neq" => {
            let exp = expected.unwrap_or("");
            Ok(!coerce_eq(value, exp))
        }
        // ── Numeric ─────────────────────────────────────────────────────────
        "gt" => {
            let (lhs, rhs) = parse_numeric_pair(value, expected, "gt")?;
            Ok(lhs > rhs)
        }
        "gte" => {
            let (lhs, rhs) = parse_numeric_pair(value, expected, "gte")?;
            Ok(lhs >= rhs)
        }
        "lt" => {
            let (lhs, rhs) = parse_numeric_pair(value, expected, "lt")?;
            Ok(lhs < rhs)
        }
        "lte" => {
            let (lhs, rhs) = parse_numeric_pair(value, expected, "lte")?;
            Ok(lhs <= rhs)
        }
        // ── String ──────────────────────────────────────────────────────────
        "contains" => {
            let s = value_as_str(value);
            Ok(s.contains(expected.unwrap_or("")))
        }
        "notContains" => {
            let s = value_as_str(value);
            Ok(!s.contains(expected.unwrap_or("")))
        }
        "startsWith" => {
            let s = value_as_str(value);
            Ok(s.starts_with(expected.unwrap_or("")))
        }
        "endsWith" => {
            let s = value_as_str(value);
            Ok(s.ends_with(expected.unwrap_or("")))
        }
        "matches" => {
            let pattern = expected.unwrap_or("");
            let re = Regex::new(pattern)
                .map_err(|e| format!("invalid regex '{pattern}': {e}"))?;
            Ok(re.is_match(&value_as_str(value)))
        }
        "notMatches" => {
            let pattern = expected.unwrap_or("");
            let re = Regex::new(pattern)
                .map_err(|e| format!("invalid regex '{pattern}': {e}"))?;
            Ok(!re.is_match(&value_as_str(value)))
        }
        // ── Type checks ─────────────────────────────────────────────────────
        "isString"    => Ok(value.is_string()),
        "isNumber"    => Ok(value.is_number()),
        "isBoolean"   => Ok(value.is_boolean()),
        "isArray"     => Ok(value.is_array()),
        "isNull"      => Ok(value.is_null()),
        "isDefined"   => Ok(!value.is_null()),
        "isUndefined" => Ok(value.is_null()),
        // ── Value checks ────────────────────────────────────────────────────
        "isEmpty" => Ok(is_empty(value)),
        "isNotEmpty" => Ok(!is_empty(value)),
        "isTruthy" => Ok(is_truthy(value)),
        "isFalsy"  => Ok(!is_truthy(value)),
        "isJson" => {
            let s = value_as_str(value);
            Ok(serde_json::from_str::<Value>(&s).is_ok())
        }
        // ── Set ─────────────────────────────────────────────────────────────
        "in" => {
            let arr_str = expected.unwrap_or("[]");
            let arr: Value = serde_json::from_str(arr_str)
                .map_err(|_| format!("'in' value must be a JSON array, got '{arr_str}'"))?;
            let arr = arr.as_array()
                .ok_or_else(|| format!("'in' value must be a JSON array, got '{arr_str}'"))?;
            Ok(arr.iter().any(|item| values_eq(item, value)))
        }
        "notIn" => {
            let arr_str = expected.unwrap_or("[]");
            let arr: Value = serde_json::from_str(arr_str)
                .map_err(|_| format!("'notIn' value must be a JSON array, got '{arr_str}'"))?;
            let arr = arr.as_array()
                .ok_or_else(|| format!("'notIn' value must be a JSON array, got '{arr_str}'"))?;
            Ok(!arr.iter().any(|item| values_eq(item, value)))
        }
        // ── Range ───────────────────────────────────────────────────────────
        "between" => {
            let exp = expected.unwrap_or("");
            let parts: Vec<&str> = exp.splitn(2, ',').collect();
            if parts.len() != 2 {
                return Err(format!("'between' value must be 'min,max', got '{exp}'"));
            }
            let min: f64 = parts[0].trim().parse()
                .map_err(|_| format!("'between' min is not a number: '{}'", parts[0].trim()))?;
            let max: f64 = parts[1].trim().parse()
                .map_err(|_| format!("'between' max is not a number: '{}'", parts[1].trim()))?;
            let lhs = value_as_f64(value)
                .ok_or_else(|| format!("cannot compare '{value}' as a number for 'between'"))?;
            Ok(lhs >= min && lhs <= max)
        }
        // ── Length ──────────────────────────────────────────────────────────
        "length" => {
            let exp = expected.unwrap_or("0");
            let expected_len: usize = exp.trim().parse()
                .map_err(|_| format!("'length' value must be a non-negative integer, got '{exp}'"))?;
            let actual_len = match value {
                Value::String(s) => s.len(),
                Value::Array(a)  => a.len(),
                _ => {
                    return Err(format!("'length' applies to strings and arrays, got '{value}'"));
                }
            };
            Ok(actual_len == expected_len)
        }
        _ => Err(format!("unknown operator '{operator}'")),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn value_as_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b)   => b.to_string(),
        Value::Null      => "null".into(),
        other            => other.to_string(),
    }
}

fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn coerce_eq(value: &Value, expected: &str) -> bool {
    // Try numeric comparison first.
    if let (Some(lhs), Ok(rhs)) = (value_as_f64(value), expected.parse::<f64>()) {
        return (lhs - rhs).abs() < f64::EPSILON;
    }
    // Fall back to string comparison.
    value_as_str(value) == expected
}

fn values_eq(a: &Value, b: &Value) -> bool {
    // Cross-type numeric equality.
    if let (Some(fa), Some(fb)) = (value_as_f64(a), value_as_f64(b)) {
        return (fa - fb).abs() < f64::EPSILON;
    }
    a == b
}

fn parse_numeric_pair(value: &Value, expected: Option<&str>, op: &str) -> Result<(f64, f64), String> {
    let lhs = value_as_f64(value)
        .ok_or_else(|| format!("cannot compare '{value}' as a number for '{op}'"))?;
    let rhs: f64 = expected
        .unwrap_or("0")
        .trim()
        .parse()
        .map_err(|_| format!("'{op}' value is not a number: '{}'", expected.unwrap_or("")))?;
    Ok((lhs, rhs))
}

fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null         => true,
        Value::String(s)    => s.is_empty(),
        Value::Array(a)     => a.is_empty(),
        Value::Object(o)    => o.is_empty(),
        Value::Bool(false)  => true,
        _                   => false,
    }
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null        => false,
        Value::Bool(b)     => *b,
        Value::Number(n)   => n.as_f64().map_or(false, |f| f != 0.0),
        Value::String(s)   => !s.is_empty(),
        Value::Array(a)    => !a.is_empty(),
        Value::Object(o)   => !o.is_empty(),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::Header;

    fn resp(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            status_text: "OK".into(),
            headers: vec![
                Header::new("content-type", "application/json"),
                Header::new("x-request-id", "abc123"),
            ],
            body: body.to_string(),
            duration_ms: 120,
            ttfb_ms: 60,
            size_bytes: body.len(),
        }
    }

    fn assert_passes(operator: &str, expression: &str, value: Option<&str>, body: &str, status: u16) {
        let a = Assertion {
            expression: expression.into(),
            operator: operator.into(),
            value: value.map(str::to_string),
            disabled: None,
            description: None,
        };
        let results = evaluate_assertions(&[a], &resp(status, body));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, TestStatus::Passed,
            "expected pass for {expression} {operator} {:?}, got error: {:?}",
            value, results[0].error);
    }

    fn assert_fails(operator: &str, expression: &str, value: Option<&str>, body: &str, status: u16) {
        let a = Assertion {
            expression: expression.into(),
            operator: operator.into(),
            value: value.map(str::to_string),
            disabled: None,
            description: None,
        };
        let results = evaluate_assertions(&[a], &resp(status, body));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, TestStatus::Failed,
            "expected fail for {expression} {operator} {:?}", value);
    }

    // ── Expression resolution ──────────────────────────────────────────────

    #[test]
    fn resolves_res_status() {
        assert_passes("eq", "res.status", Some("200"), "{}", 200);
    }

    #[test]
    fn resolves_res_response_time() {
        assert_passes("gt", "res.responseTime", Some("50"), "{}", 200);
    }

    #[test]
    fn resolves_res_body_string() {
        assert_passes("contains", "res.body", Some("hello"), "hello world", 200);
    }

    #[test]
    fn resolves_res_body_json_path() {
        assert_passes("eq", "res.body.token", Some("abc"), r#"{"token":"abc"}"#, 200);
    }

    #[test]
    fn resolves_res_body_nested_path() {
        assert_passes("eq", "res.body.user.id", Some("42"), r#"{"user":{"id":42}}"#, 200);
    }

    #[test]
    fn resolves_res_headers() {
        assert_passes("eq", "res.headers.x-request-id", Some("abc123"), "{}", 200);
    }

    #[test]
    fn resolves_res_headers_case_insensitive() {
        assert_passes("eq", "res.headers.Content-Type", Some("application/json"), "{}", 200);
    }

    #[test]
    fn unresolvable_expression_fails() {
        assert_fails("eq", "req.url", Some("x"), "{}", 200);
    }

    #[test]
    fn body_path_on_non_json_fails() {
        assert_fails("eq", "res.body.field", Some("x"), "not json", 200);
    }

    #[test]
    fn missing_body_path_fails() {
        assert_fails("eq", "res.body.missing", Some("x"), r#"{"other":"y"}"#, 200);
    }

    // ── Disabled ───────────────────────────────────────────────────────────

    #[test]
    fn disabled_assertion_skipped() {
        let a = Assertion {
            expression: "res.status".into(),
            operator: "eq".into(),
            value: Some("200".into()),
            disabled: Some(true),
            description: None,
        };
        let results = evaluate_assertions(&[a], &resp(200, "{}"));
        assert!(results.is_empty());
    }

    // ── Equality ───────────────────────────────────────────────────────────

    #[test]
    fn eq_numeric_pass() { assert_passes("eq", "res.status", Some("200"), "{}", 200); }
    #[test]
    fn eq_numeric_fail() { assert_fails("eq", "res.status", Some("404"), "{}", 200); }
    #[test]
    fn neq_pass() { assert_passes("neq", "res.status", Some("404"), "{}", 200); }
    #[test]
    fn neq_fail() { assert_fails("neq", "res.status", Some("200"), "{}", 200); }

    // ── Numeric ────────────────────────────────────────────────────────────

    #[test]
    fn gt_pass() { assert_passes("gt", "res.status", Some("100"), "{}", 200); }
    #[test]
    fn gt_fail() { assert_fails("gt", "res.status", Some("200"), "{}", 200); }
    #[test]
    fn gte_pass() { assert_passes("gte", "res.status", Some("200"), "{}", 200); }
    #[test]
    fn gte_fail() { assert_fails("gte", "res.status", Some("201"), "{}", 200); }
    #[test]
    fn lt_pass() { assert_passes("lt", "res.status", Some("300"), "{}", 200); }
    #[test]
    fn lt_fail() { assert_fails("lt", "res.status", Some("200"), "{}", 200); }
    #[test]
    fn lte_pass() { assert_passes("lte", "res.status", Some("200"), "{}", 200); }
    #[test]
    fn lte_fail() { assert_fails("lte", "res.status", Some("199"), "{}", 200); }

    // ── String ─────────────────────────────────────────────────────────────

    #[test]
    fn contains_pass() { assert_passes("contains", "res.body", Some("hello"), "hello world", 200); }
    #[test]
    fn contains_fail() { assert_fails("contains", "res.body", Some("xyz"), "hello world", 200); }
    #[test]
    fn not_contains_pass() { assert_passes("notContains", "res.body", Some("xyz"), "hello", 200); }
    #[test]
    fn starts_with_pass() { assert_passes("startsWith", "res.body", Some("hel"), "hello", 200); }
    #[test]
    fn starts_with_fail() { assert_fails("startsWith", "res.body", Some("ell"), "hello", 200); }
    #[test]
    fn ends_with_pass() { assert_passes("endsWith", "res.body", Some("rld"), "world", 200); }
    #[test]
    fn ends_with_fail() { assert_fails("endsWith", "res.body", Some("wor"), "world", 200); }
    #[test]
    fn matches_pass() { assert_passes("matches", "res.body", Some("^hel"), "hello", 200); }
    #[test]
    fn matches_fail() { assert_fails("matches", "res.body", Some("^xyz"), "hello", 200); }
    #[test]
    fn not_matches_pass() { assert_passes("notMatches", "res.body", Some("^xyz"), "hello", 200); }

    // ── Type checks ────────────────────────────────────────────────────────

    #[test]
    fn is_string_pass() { assert_passes("isString", "res.body.name", None, r#"{"name":"alice"}"#, 200); }
    #[test]
    fn is_string_fail() { assert_fails("isString", "res.body.count", None, r#"{"count":3}"#, 200); }
    #[test]
    fn is_number_pass() { assert_passes("isNumber", "res.body.count", None, r#"{"count":3}"#, 200); }
    #[test]
    fn is_boolean_pass() { assert_passes("isBoolean", "res.body.ok", None, r#"{"ok":true}"#, 200); }
    #[test]
    fn is_array_pass() { assert_passes("isArray", "res.body.items", None, r#"{"items":[]}"#, 200); }
    #[test]
    fn is_null_pass() { assert_passes("isNull", "res.body.x", None, r#"{"x":null}"#, 200); }
    #[test]
    fn is_defined_pass() { assert_passes("isDefined", "res.body.x", None, r#"{"x":"y"}"#, 200); }
    #[test]
    fn is_undefined_pass() { assert_passes("isUndefined", "res.headers.x-missing", None, "{}", 200); }

    // ── Value checks ───────────────────────────────────────────────────────

    #[test]
    fn is_empty_pass() { assert_passes("isEmpty", "res.body", None, "", 200); }
    #[test]
    fn is_empty_fail() { assert_fails("isEmpty", "res.body", None, "hello", 200); }
    #[test]
    fn is_not_empty_pass() { assert_passes("isNotEmpty", "res.body", None, "hello", 200); }
    #[test]
    fn is_truthy_pass() { assert_passes("isTruthy", "res.body.ok", None, r#"{"ok":true}"#, 200); }
    #[test]
    fn is_falsy_pass() { assert_passes("isFalsy", "res.body.ok", None, r#"{"ok":false}"#, 200); }
    #[test]
    fn is_json_pass() { assert_passes("isJson", "res.body", None, r#"{"a":1}"#, 200); }
    #[test]
    fn is_json_fail() { assert_fails("isJson", "res.body", None, "not json", 200); }

    // ── Set ────────────────────────────────────────────────────────────────

    #[test]
    fn in_pass() { assert_passes("in", "res.status", Some("[200,201,204]"), "{}", 200); }
    #[test]
    fn in_fail() { assert_fails("in", "res.status", Some("[404,500]"), "{}", 200); }
    #[test]
    fn not_in_pass() { assert_passes("notIn", "res.status", Some("[404,500]"), "{}", 200); }

    // ── Range ──────────────────────────────────────────────────────────────

    #[test]
    fn between_pass() { assert_passes("between", "res.status", Some("100,299"), "{}", 200); }
    #[test]
    fn between_fail() { assert_fails("between", "res.status", Some("300,599"), "{}", 200); }

    // ── Length ─────────────────────────────────────────────────────────────

    #[test]
    fn length_string_pass() { assert_passes("length", "res.body", Some("5"), "hello", 200); }
    #[test]
    fn length_string_fail() { assert_fails("length", "res.body", Some("3"), "hello", 200); }
    #[test]
    fn length_array_pass() { assert_passes("length", "res.body.items", Some("2"), r#"{"items":[1,2]}"#, 200); }

    // ── Result name format ─────────────────────────────────────────────────

    #[test]
    fn result_name_binary() {
        let a = Assertion::new("res.status", "eq", Some("200".into()));
        let results = evaluate_assertions(&[a], &resp(200, "{}"));
        assert_eq!(results[0].name, "res.status eq 200");
    }

    #[test]
    fn result_name_unary() {
        let a = Assertion::new("res.body", "isJson", None);
        let results = evaluate_assertions(&[a], &resp(200, "{}"));
        assert_eq!(results[0].name, "res.body isJson");
    }
}
```

- [ ] **Step 3: Register the module in `lib.rs`**

Open `crates/rocket-app/src/lib.rs`. After the last `pub mod` line, add:

```rust
pub mod assertion_evaluator;
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rocket-app assertion_evaluator 2>&1 | tail -15
```

Expected: all tests pass, no compilation errors.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/assertion_evaluator.rs crates/rocket-app/src/lib.rs crates/rocket-app/Cargo.toml
git commit -m "feat(rocket-app): add assertion_evaluator with full Bruno operator set"
```

---

## Task 2: Wire `evaluate_assertions` into the execution service

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`

- [ ] **Step 1: Add `assertions` field to `ExecuteRequestInput`**

Open `crates/rocket-app/src/execution_service.rs`. Find the `ExecuteRequestInput` struct (around line 23). Add after the `global_env_name` field:

```rust
    /// Declarative assertions to evaluate after the tests-script phase.
    #[serde(default)]
    pub assertions: Vec<rocket_shared::Assertion>,
```

- [ ] **Step 2: Update `sample_input` in tests**

Find the `fn sample_input` helper inside the `#[cfg(test)]` block (around line 843). Add the new field:

```rust
        assertions: vec![],
```

- [ ] **Step 3: Call `evaluate_assertions` after the tests-script phase**

Find the comment `// ── Emit events` (around line 534). Insert the following block immediately before it:

```rust
        // ── Declarative assertions ────────────────────────────────────────────
        // Run after tests script so JS test results appear first in TestsPanel.
        let assertion_results = crate::assertion_evaluator::evaluate_assertions(
            &input.assertions,
            &response,
        );
        all_test_results.extend(assertion_results);
```

- [ ] **Step 4: Run the full execution service test suite**

```bash
cargo test -p rocket-app 2>&1 | tail -10
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat(rocket-app): wire evaluate_assertions into execution pipeline"
```

---

## Task 3: Add integration test for assertions through execute()

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`

- [ ] **Step 1: Write the integration test**

Open `crates/rocket-app/src/execution_service.rs`. Find the `#[cfg(test)]` block. Add this test after the `sample_input` helper function:

```rust
    #[tokio::test]
    async fn assertions_run_after_tests_script_and_appear_in_results() {
        use rocket_scripting::TestStatus;
        use rocket_shared::Assertion;

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );

        let mut input = sample_input("https://example.com", None);
        // One passing and one failing assertion.
        input.assertions = vec![
            Assertion::new("res.status", "eq", Some("200".into())),
            Assertion::new("res.status", "eq", Some("404".into())),
        ];

        let output = svc.execute(input).await.expect("execute");
        assert_eq!(output.test_results.len(), 2);
        assert_eq!(output.test_results[0].status, TestStatus::Passed);
        assert_eq!(output.test_results[1].status, TestStatus::Failed);
    }
```

- [ ] **Step 3: Run the new integration test**

```bash
cargo test -p rocket-app assertions_run_after -- --nocapture 2>&1 | tail -15
```

Expected: 1 test passes.

- [ ] **Step 4: Run the full suite**

```bash
cargo test -p rocket-app 2>&1 | tail -5
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "test(rocket-app): integration test for assertions in execute()"
```

---

## Task 4: IPC types — `AssertionEntry` and `Request` interface

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Add `AssertionEntry` interface to `tauri-api.ts`**

Open `src/lib/tauri-api.ts`. Find the `export interface Request {` block (around line 86). Immediately before it, add:

```typescript
export interface AssertionEntry {
  expression: string;
  operator: string;
  value?: string;
  disabled?: boolean;
}
```

- [ ] **Step 2: Add `assertions` to the `Request` interface**

Inside the `Request` interface, after `tests?: string | null;`, add:

```typescript
  assertions?: AssertionEntry[];
```

- [ ] **Step 3: Run TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(frontend): add AssertionEntry type and assertions field to Request IPC interface"
```

---

## Task 5: Frontend state — `RequestState` and mapping utilities

**Files:**
- Modify: `src/types/pane-types.ts`
- Modify: `src/lib/pane-utils.ts`

- [ ] **Step 1: Add `assertions` to `RequestState` in `pane-types.ts`**

Open `src/types/pane-types.ts`. Find the `RequestState` interface. Add after `testsScript?: string;`:

```typescript
  assertions: AssertionEntry[];
```

Also add the import at the top if `AssertionEntry` is not already imported from `tauri-api.ts`. Find the existing import from `@/lib/tauri-api` and add `AssertionEntry` to it.

- [ ] **Step 2: Update `mapApiRequestToState` in `pane-utils.ts`**

Open `src/lib/pane-utils.ts`. Find the `return {` block inside `mapApiRequestToState`. After `testsScript: req.tests ?? undefined,`, add:

```typescript
    assertions: req.assertions ?? [],
```

- [ ] **Step 3: Update `createDefaultRequest` in `pane-utils.ts`**

In `createDefaultRequest`, after `docs: null,`, add:

```typescript
    assertions: [],
```

- [ ] **Step 4: Run TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors (or only the 5 pre-existing Biome errors).

- [ ] **Step 5: Commit**

```bash
git add src/types/pane-types.ts src/lib/pane-utils.ts
git commit -m "feat(frontend): add assertions to RequestState and mapping utilities"
```

---

## Task 6: Wire assertions through save and execute paths

**Files:**
- Modify: `src/lib/auto-save.ts`
- Modify: `src/components/request/SaveRequestButton.tsx`
- Modify: `src/lib/execute-request.ts`

- [ ] **Step 1: Add `assertions` to `auto-save.ts` — `toApiRequest`**

Open `src/lib/auto-save.ts`. In `toApiRequest`, after `tests: request.testsScript ?? null,`, add:

```typescript
    assertions: request.assertions ?? [],
```

- [ ] **Step 2: Add `assertions` to `SaveRequestButton.tsx` — `buildPayloadFromTab`**

Open `src/components/request/SaveRequestButton.tsx`. In `buildPayloadFromTab`, after `tests: tab.request.testsScript ?? null,`, add:

```typescript
    assertions: tab.request.assertions ?? [],
```

- [ ] **Step 3: Add `assertions` to `execute-request.ts` execution payload**

Open `src/lib/execute-request.ts`. Find where `ExecuteRequestInput` is built (around line 465 — where `testsScript` is set). After `testsScript: effectiveRequest.testsScript ?? undefined,`, add:

```typescript
      assertions: effectiveRequest.assertions ?? [],
```

- [ ] **Step 4: Run TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/lib/auto-save.ts src/components/request/SaveRequestButton.tsx src/lib/execute-request.ts
git commit -m "feat(frontend): wire assertions through save, auto-save, and execute paths"
```

---

## Task 7: Build `AssertionsTab` component

**Files:**
- Create: `src/components/request/AssertionsTab.tsx`

- [ ] **Step 1: Create `AssertionsTab.tsx`**

Create `src/components/request/AssertionsTab.tsx` with this full content:

```typescript
import { Plus, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { SingleLineEditor } from '@/components/editor';
import type { AssertionEntry } from '@/lib/tauri-api';

interface AssertionsTabProps {
  assertions: AssertionEntry[];
  onChange: (assertions: AssertionEntry[]) => void;
}

const UNARY_OPERATORS = new Set([
  'isString', 'isNumber', 'isBoolean', 'isArray', 'isNull',
  'isDefined', 'isUndefined', 'isEmpty', 'isNotEmpty',
  'isTruthy', 'isFalsy', 'isJson',
]);

const OPERATOR_GROUPS = [
  {
    label: 'Equality',
    operators: [
      { value: 'eq', label: 'eq (equals)' },
      { value: 'neq', label: 'neq (not equals)' },
    ],
  },
  {
    label: 'Numeric',
    operators: [
      { value: 'gt', label: 'gt (greater than)' },
      { value: 'gte', label: 'gte (≥)' },
      { value: 'lt', label: 'lt (less than)' },
      { value: 'lte', label: 'lte (≤)' },
    ],
  },
  {
    label: 'String',
    operators: [
      { value: 'contains', label: 'contains' },
      { value: 'notContains', label: 'notContains' },
      { value: 'startsWith', label: 'startsWith' },
      { value: 'endsWith', label: 'endsWith' },
      { value: 'matches', label: 'matches (regex)' },
      { value: 'notMatches', label: 'notMatches (regex)' },
    ],
  },
  {
    label: 'Type',
    operators: [
      { value: 'isString', label: 'isString' },
      { value: 'isNumber', label: 'isNumber' },
      { value: 'isBoolean', label: 'isBoolean' },
      { value: 'isArray', label: 'isArray' },
      { value: 'isNull', label: 'isNull' },
      { value: 'isDefined', label: 'isDefined' },
      { value: 'isUndefined', label: 'isUndefined' },
    ],
  },
  {
    label: 'Value',
    operators: [
      { value: 'isEmpty', label: 'isEmpty' },
      { value: 'isNotEmpty', label: 'isNotEmpty' },
      { value: 'isTruthy', label: 'isTruthy' },
      { value: 'isFalsy', label: 'isFalsy' },
      { value: 'isJson', label: 'isJson' },
    ],
  },
  {
    label: 'Set',
    operators: [
      { value: 'in', label: 'in (JSON array)' },
      { value: 'notIn', label: 'notIn (JSON array)' },
    ],
  },
  {
    label: 'Range & Length',
    operators: [
      { value: 'between', label: 'between (min,max)' },
      { value: 'length', label: 'length' },
    ],
  },
];

export function AssertionsTab({ assertions, onChange }: AssertionsTabProps) {
  function update(index: number, patch: Partial<AssertionEntry>) {
    const next = assertions.map((a, i) => (i === index ? { ...a, ...patch } : a));
    onChange(next);
  }

  function remove(index: number) {
    onChange(assertions.filter((_, i) => i !== index));
  }

  function add() {
    onChange([...assertions, { expression: '', operator: 'eq', value: '' }]);
  }

  if (assertions.length === 0) {
    return (
      <div className='flex flex-col h-full'>
        <div className='flex-1 flex flex-col items-center justify-center gap-3 text-muted-foreground'>
          <p className='text-sm'>No assertions yet.</p>
          <Button size='sm' variant='outline' onClick={add}>
            <Plus className='mr-1.5 h-3.5 w-3.5' />
            Add Assertion
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className='flex flex-col h-full overflow-hidden'>
      <div className='flex-1 overflow-auto'>
        <table className='w-full text-sm border-collapse'>
          <thead>
            <tr className='border-b bg-muted/40'>
              <th className='w-10 px-2 py-1.5 text-left font-medium text-muted-foreground'>On</th>
              <th className='px-2 py-1.5 text-left font-medium text-muted-foreground'>Expression</th>
              <th className='w-44 px-2 py-1.5 text-left font-medium text-muted-foreground'>Operator</th>
              <th className='px-2 py-1.5 text-left font-medium text-muted-foreground'>Value</th>
              <th className='w-8' />
            </tr>
          </thead>
          <tbody>
            {assertions.map((assertion, i) => {
              const isUnary = UNARY_OPERATORS.has(assertion.operator);
              return (
                <tr key={i} className='border-b hover:bg-muted/20'>
                  <td className='px-2 py-1'>
                    <Switch
                      checked={!assertion.disabled}
                      onCheckedChange={(checked) => update(i, { disabled: !checked })}
                      className='scale-75'
                    />
                  </td>
                  <td className='px-1 py-1'>
                    <SingleLineEditor
                      value={assertion.expression}
                      onChange={(v) => update(i, { expression: v })}
                      placeholder='res.status'
                      className='h-7 text-xs'
                    />
                  </td>
                  <td className='px-1 py-1'>
                    <Select
                      value={assertion.operator}
                      onValueChange={(v) => update(i, { operator: v, value: isUnary && !UNARY_OPERATORS.has(v) ? '' : assertion.value })}
                    >
                      <SelectTrigger className='h-7 text-xs'>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {OPERATOR_GROUPS.map((group) => (
                          <SelectGroup key={group.label}>
                            <SelectLabel className='text-xs'>{group.label}</SelectLabel>
                            {group.operators.map((op) => (
                              <SelectItem key={op.value} value={op.value} className='text-xs'>
                                {op.label}
                              </SelectItem>
                            ))}
                          </SelectGroup>
                        ))}
                      </SelectContent>
                    </Select>
                  </td>
                  <td className='px-1 py-1'>
                    {isUnary ? (
                      <span className='text-xs text-muted-foreground italic px-2'>—</span>
                    ) : (
                      <SingleLineEditor
                        value={assertion.value ?? ''}
                        onChange={(v) => update(i, { value: v })}
                        placeholder='expected value'
                        className='h-7 text-xs'
                      />
                    )}
                  </td>
                  <td className='px-1 py-1'>
                    <Button
                      size='icon'
                      variant='ghost'
                      className='h-6 w-6'
                      onClick={() => remove(i)}
                    >
                      <Trash2 className='h-3.5 w-3.5 text-muted-foreground' />
                    </Button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <div className='shrink-0 border-t px-3 py-2'>
        <Button size='sm' variant='ghost' onClick={add}>
          <Plus className='mr-1.5 h-3.5 w-3.5' />
          Add Assertion
        </Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Run TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/request/AssertionsTab.tsx
git commit -m "feat(frontend): add AssertionsTab component"
```

---

## Task 8: Wire `AssertionsTab` into `RequestPanel`

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

- [ ] **Step 1: Add `'assertions'` to the `SectionTab` type**

Open `src/components/request/RequestPanel.tsx`. Find the `SectionTab` type union (around line 107). Add `'assertions'` to the union:

```typescript
  | 'assertions'
  | 'scripts';
```

- [ ] **Step 2: Import `AssertionsTab`**

Add the import near the other tab imports:

```typescript
import { AssertionsTab } from './AssertionsTab';
```

- [ ] **Step 3: Add the tab definition in `tabDefs`**

Find the `tabDefs` `useMemo` array. Find the `scripts` tab entry (around line 738). After the closing brace of the `scripts` entry, add a new tab entry:

```typescript
      {
        value: 'assertions',
        label: (
          <>
            Assertions
            {request.assertions.some((a) => !a.disabled) && (
              <span className='ml-1.5 inline-block w-1.5 h-1.5 rounded-full bg-primary' />
            )}
          </>
        ),
        isActive: activeSection === 'assertions',
        onClick: () => setActiveSection('assertions'),
      },
```

Also add `request.assertions` to the `useMemo` dependency array.

- [ ] **Step 4: Render `AssertionsTab` in the panel body**

Find the section where `activeSection === 'scripts'` renders `<ScriptsTab>` (around line 968). Add an `else if` branch for assertions in the same pattern:

```typescript
      {activeSection === 'assertions' ? (
        <AssertionsTab
          assertions={request.assertions}
          onChange={(newAssertions) => updateRequest(tab.id, { assertions: newAssertions })}
        />
      ) : activeSection === 'scripts' ? (
```

Ensure the closing of the existing ternary chain is adjusted correctly.

- [ ] **Step 5: Add `'assertions'` to the full-panel layout check**

Find the boolean that controls whether to show the full-panel layout (no request/response split). It looks like:

```typescript
activeSection === 'docs' || activeSection === 'load-test' || activeSection === 'scripts'
```

Add `activeSection === 'assertions'` to this expression.

- [ ] **Step 6: Run TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 7: Run Biome check — confirm no new errors beyond the 5 pre-existing ones**

```bash
yarn check 2>&1 | grep "^Found"
```

Expected: `Found 5 errors.`

- [ ] **Step 8: Commit**

```bash
git add src/components/request/RequestPanel.tsx
git commit -m "feat(frontend): add Assertions tab to RequestPanel"
```

---

## Task 9: Final verification

- [ ] **Step 1: Run full Rust test suite**

```bash
cargo check && cargo test -p rocket-app 2>&1 | tail -8
```

Expected: all tests pass, no warnings about unused imports.

- [ ] **Step 2: Run full frontend checks**

```bash
yarn tsc --noEmit && yarn check 2>&1 | tail -5
```

Expected: TypeScript passes; Biome shows the same 5 pre-existing errors, no new ones.

- [ ] **Step 3: Smoke-test checklist (manual)**

Open the app (`yarn tauri dev`):
1. Open a request from the sidebar — confirm the "Assertions" tab is visible.
2. Click "Add Assertion" — confirm a row appears.
3. Set expression to `res.status`, operator `eq`, value `200`.
4. Save (Cmd/Ctrl+S) and close the tab, reopen — confirm the assertion is still there.
5. Run the request — confirm the assertion appears in the Tests panel with pass/fail status.
6. Change the operator to `isString` — confirm the Value field shows a dash (unary).
7. Disable the assertion via the Switch — confirm the dot indicator on the tab disappears.

- [ ] **Step 4: Final commit if any cleanup was needed**

```bash
git add -p
git commit -m "chore: post-review cleanup for runtime assertions"
```
