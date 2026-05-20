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
        "eq" => {
            let exp = expected.unwrap_or("");
            Ok(coerce_eq(value, exp))
        }
        "neq" => {
            let exp = expected.unwrap_or("");
            Ok(!coerce_eq(value, exp))
        }
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
        "isString"    => Ok(value.is_string()),
        "isNumber"    => Ok(value.is_number()),
        "isBoolean"   => Ok(value.is_boolean()),
        "isArray"     => Ok(value.is_array()),
        "isNull"      => Ok(value.is_null()),
        "isDefined"   => Ok(!value.is_null()),
        "isUndefined" => Ok(value.is_null()),
        "isEmpty" => Ok(is_empty(value)),
        "isNotEmpty" => Ok(!is_empty(value)),
        "isTruthy" => Ok(is_truthy(value)),
        "isFalsy"  => Ok(!is_truthy(value)),
        "isJson" => {
            let s = value_as_str(value);
            Ok(serde_json::from_str::<Value>(&s).is_ok())
        }
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
    if let (Some(lhs), Ok(rhs)) = (value_as_f64(value), expected.parse::<f64>()) {
        return (lhs - rhs).abs() < f64::EPSILON;
    }
    value_as_str(value) == expected
}

fn values_eq(a: &Value, b: &Value) -> bool {
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

    #[test]
    fn resolves_res_status() { assert_passes("eq", "res.status", Some("200"), "{}", 200); }
    #[test]
    fn resolves_res_response_time() { assert_passes("gt", "res.responseTime", Some("50"), "{}", 200); }
    #[test]
    fn resolves_res_body_string() { assert_passes("contains", "res.body", Some("hello"), "hello world", 200); }
    #[test]
    fn resolves_res_body_json_path() { assert_passes("eq", "res.body.token", Some("abc"), r#"{"token":"abc"}"#, 200); }
    #[test]
    fn resolves_res_body_nested_path() { assert_passes("eq", "res.body.user.id", Some("42"), r#"{"user":{"id":42}}"#, 200); }
    #[test]
    fn resolves_res_headers() { assert_passes("eq", "res.headers.x-request-id", Some("abc123"), "{}", 200); }
    #[test]
    fn resolves_res_headers_case_insensitive() { assert_passes("eq", "res.headers.Content-Type", Some("application/json"), "{}", 200); }
    #[test]
    fn unresolvable_expression_fails() { assert_fails("eq", "req.url", Some("x"), "{}", 200); }
    #[test]
    fn body_path_on_non_json_fails() { assert_fails("eq", "res.body.field", Some("x"), "not json", 200); }
    #[test]
    fn missing_body_path_fails() { assert_fails("eq", "res.body.missing", Some("x"), r#"{"other":"y"}"#, 200); }
    #[test]
    fn disabled_assertion_skipped() {
        let a = Assertion { expression: "res.status".into(), operator: "eq".into(), value: Some("200".into()), disabled: Some(true), description: None };
        let results = evaluate_assertions(&[a], &resp(200, "{}"));
        assert!(results.is_empty());
    }
    #[test]
    fn eq_numeric_pass() { assert_passes("eq", "res.status", Some("200"), "{}", 200); }
    #[test]
    fn eq_numeric_fail() { assert_fails("eq", "res.status", Some("404"), "{}", 200); }
    #[test]
    fn neq_pass() { assert_passes("neq", "res.status", Some("404"), "{}", 200); }
    #[test]
    fn neq_fail() { assert_fails("neq", "res.status", Some("200"), "{}", 200); }
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
    #[test]
    fn in_pass() { assert_passes("in", "res.status", Some("[200,201,204]"), "{}", 200); }
    #[test]
    fn in_fail() { assert_fails("in", "res.status", Some("[404,500]"), "{}", 200); }
    #[test]
    fn not_in_pass() { assert_passes("notIn", "res.status", Some("[404,500]"), "{}", 200); }
    #[test]
    fn between_pass() { assert_passes("between", "res.status", Some("100,299"), "{}", 200); }
    #[test]
    fn between_fail() { assert_fails("between", "res.status", Some("300,599"), "{}", 200); }
    #[test]
    fn length_string_pass() { assert_passes("length", "res.body", Some("5"), "hello", 200); }
    #[test]
    fn length_string_fail() { assert_fails("length", "res.body", Some("3"), "hello", 200); }
    #[test]
    fn length_array_pass() { assert_passes("length", "res.body.items", Some("2"), r#"{"items":[1,2]}"#, 200); }
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
