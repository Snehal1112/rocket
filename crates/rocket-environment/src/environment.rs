use serde::{Deserialize, Serialize};

use crate::variable::Variable;
use rocket_shared::description::Description;

/// OpenCollection Extensions — free-form object for custom metadata.
pub type Extensions = serde_json::Value;

/// Environment aggregate root.
/// A named set of key-value variables used for request interpolation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub name: String,
    pub variables: Vec<Variable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dot_env_file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_certificates: Vec<serde_json::Value>,
}

impl Environment {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            variables: Vec::new(),
            color: None,
            description: None,
            extends: None,
            dot_env_file_path: None,
            client_certificates: Vec::new(),
        }
    }

    /// Add or update a variable. If a variable with the same key exists, replace it.
    pub fn set_variable(&mut self, variable: Variable) {
        if let Some(existing) = self.variables.iter_mut().find(|v| v.key == variable.key) {
            *existing = variable;
        } else {
            self.variables.push(variable);
        }
    }

    /// Remove a variable by key.
    pub fn remove_variable(&mut self, key: &str) {
        self.variables.retain(|v| v.key != key);
    }

    /// Get the value of an enabled variable by key.
    pub fn get_value(&self, key: &str) -> Option<&str> {
        self.variables
            .iter()
            .find(|v| v.key == key && v.enabled)
            .map(|v| v.value.as_str())
    }

    /// Get all enabled variables as key-value pairs.
    pub fn enabled_variables(&self) -> Vec<(&str, &str)> {
        self.variables
            .iter()
            .filter(|v| v.enabled)
            .map(|v| (v.key.as_str(), v.value.as_str()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variable::Variable;

    #[test]
    fn new_environment_is_empty() {
        let env = Environment::new("production");
        assert_eq!(env.name, "production");
        assert!(env.variables.is_empty());
    }

    #[test]
    fn set_variable_adds_new() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("HOST", "localhost"));
        assert_eq!(env.variables.len(), 1);
    }

    #[test]
    fn set_variable_updates_existing() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("HOST", "localhost"));
        env.set_variable(Variable::new("HOST", "127.0.0.1"));
        assert_eq!(env.variables.len(), 1);
        assert_eq!(env.variables[0].value, "127.0.0.1");
    }

    #[test]
    fn remove_variable() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("HOST", "localhost"));
        env.remove_variable("HOST");
        assert!(env.variables.is_empty());
    }

    #[test]
    fn get_value_returns_enabled_only() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("ENABLED", "yes"));
        env.set_variable(Variable::disabled("DISABLED", "no"));
        assert_eq!(env.get_value("ENABLED"), Some("yes"));
        assert_eq!(env.get_value("DISABLED"), None);
    }

    #[test]
    fn environment_with_color_and_description() {
        use rocket_shared::description::Description;
        let env = Environment {
            name: "production".into(),
            variables: Vec::new(),
            color: Some("#FF5733".into()),
            description: Some(Description::text("Production environment")),
            extends: None,
            dot_env_file_path: None,
            client_certificates: Vec::new(),
        };
        assert_eq!(env.color, Some("#FF5733".into()));
        assert!(env.description.is_some());
    }

    #[test]
    fn environment_with_extends() {
        let env = Environment {
            name: "staging".into(),
            variables: Vec::new(),
            color: None,
            description: None,
            extends: Some("production".into()),
            dot_env_file_path: Some(".env.staging".into()),
            client_certificates: Vec::new(),
        };
        assert_eq!(env.extends, Some("production".into()));
        assert_eq!(env.dot_env_file_path, Some(".env.staging".into()));
    }

    #[test]
    fn environment_serde_backward_compat() {
        // Old JSON without new fields must still deserialize.
        let json = r#"{"name":"test","variables":[]}"#;
        let env: Environment = serde_json::from_str(json).unwrap();
        assert_eq!(env.name, "test");
        assert!(env.color.is_none());
        assert!(env.description.is_none());
        assert!(env.extends.is_none());
        assert!(env.dot_env_file_path.is_none());
        assert!(env.client_certificates.is_empty());
    }

    #[test]
    fn environment_serde_roundtrip_with_new_fields() {
        use rocket_shared::description::Description;
        let env = Environment {
            name: "dev".into(),
            variables: Vec::new(),
            color: Some("#00FF00".into()),
            description: Some(Description::text("Dev env")),
            extends: Some("base".into()),
            dot_env_file_path: Some(".env.dev".into()),
            client_certificates: Vec::new(),
        };
        let json = serde_json::to_string(&env).unwrap();
        let back: Environment = serde_json::from_str(&json).unwrap();
        assert_eq!(env, back);
    }
}
