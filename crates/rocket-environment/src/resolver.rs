use std::collections::HashMap;

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
                if let Some(value) = variables.get(&var_name_trimmed) {
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
}
