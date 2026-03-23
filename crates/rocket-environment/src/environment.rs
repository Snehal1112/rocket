use serde::{Deserialize, Serialize};

use crate::variable::Variable;

/// Environment aggregate root.
/// A named set of key-value variables used for request interpolation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub name: String,
    pub variables: Vec<Variable>,
}

impl Environment {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            variables: Vec::new(),
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
}
