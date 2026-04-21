use std::collections::HashMap;
use crate::dynamic_vars;

/// Result of variable resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolveResult {
    /// The output string with variables replaced.
    pub output: String,
    /// Names of variables that were referenced but not found.
    pub unresolved: Vec<String>,
}

/// Resolve `{{variable}}` placeholders in a template string.
/// Variables not found in the map are left as-is and reported in `unresolved`.
pub fn resolve(template: &str, variables: &HashMap<String, String>) -> ResolveResult {
    let mut output = String::with_capacity(template.len());
    let mut unresolved = Vec::new();
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second '{'
            let mut var_name = String::new();
            let mut found_closing = false;

            while let Some(inner) = chars.next() {
                if inner == '}' {
                    if chars.peek() == Some(&'}') {
                        chars.next();
                        found_closing = true;
                        break;
                    } else {
                        var_name.push(inner);
                    }
                } else {
                    var_name.push(inner);
                }
            }

            let var_name_trimmed = var_name.trim().to_string();

            if found_closing {
                if let Some(stripped) = var_name_trimmed.strip_prefix('$') {
                    // Dynamic variable — generate fresh value, never falls through to user vars.
                    if let Some(generated) = dynamic_vars::generate(stripped) {
                        output.push_str(&generated);
                    } else {
                        // Unknown $variable — leave as-is, mark unresolved.
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
            } else {
                // Unclosed braces — output as-is.
                output.push_str("{{");
                output.push_str(&var_name);
            }
        } else {
            output.push(ch);
        }
    }

    ResolveResult { output, unresolved }
}

/// Convenience: resolve using an Environment's enabled variables.
pub fn resolve_with_env(
    template: &str,
    env: &crate::environment::Environment,
) -> ResolveResult {
    let vars: HashMap<String, String> = env
        .enabled_variables()
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    resolve(template, &vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_simple_variable() {
        let mut vars = HashMap::new();
        vars.insert("BASE_URL".to_string(), "https://api.example.com".to_string());
        let result = resolve("{{BASE_URL}}/users", &vars);
        assert_eq!(result.output, "https://api.example.com/users");
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn resolve_multiple_variables() {
        let mut vars = HashMap::new();
        vars.insert("HOST".to_string(), "localhost".to_string());
        vars.insert("PORT".to_string(), "8080".to_string());
        let result = resolve("http://{{HOST}}:{{PORT}}/api", &vars);
        assert_eq!(result.output, "http://localhost:8080/api");
    }

    #[test]
    fn resolve_missing_variable_left_as_is() {
        let vars = HashMap::new();
        let result = resolve("{{MISSING}}/path", &vars);
        assert_eq!(result.output, "{{MISSING}}/path");
        assert_eq!(result.unresolved, vec!["MISSING"]);
    }

    #[test]
    fn resolve_no_variables() {
        let vars = HashMap::new();
        let result = resolve("plain text", &vars);
        assert_eq!(result.output, "plain text");
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn resolve_whitespace_in_braces_trimmed() {
        let mut vars = HashMap::new();
        vars.insert("KEY".to_string(), "value".to_string());
        let result = resolve("{{ KEY }}", &vars);
        assert_eq!(result.output, "value");
    }

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
        let uuid_part = result.output.strip_prefix("https://api.test/users/").unwrap();
        assert!(uuid::Uuid::parse_str(uuid_part).is_ok(),
            "UUID portion '{}' is not valid", uuid_part);
    }

    #[test]
    fn resolve_two_dynamic_vars_produce_different_values() {
        let vars = HashMap::new();
        let result = resolve("{{$guid}}-{{$guid}}", &vars);
        // Two 36-char UUIDs separated by a literal '-' = 73 chars total.
        assert_eq!(result.output.len(), 73, "Expected two UUIDs separated by dash, got: {}", result.output);
        // Verify both halves are valid, distinct UUIDs.
        let (first, rest) = result.output.split_at(36);
        let second = &rest[1..]; // skip the separator dash
        assert!(uuid::Uuid::parse_str(first).is_ok(), "first UUID invalid: {}", first);
        assert!(uuid::Uuid::parse_str(second).is_ok(), "second UUID invalid: {}", second);
        assert_ne!(first, second, "two $guid calls should produce different UUIDs");
    }

    #[test]
    fn resolve_dynamic_var_with_whitespace() {
        let vars = HashMap::new();
        let result = resolve("{{ $guid }}", &vars);
        assert!(uuid::Uuid::parse_str(&result.output).is_ok(),
            "Whitespace around $guid should still resolve, got: {}", result.output);
    }
}
