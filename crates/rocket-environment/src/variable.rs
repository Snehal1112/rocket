use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub key: String,
    pub value: String,
    pub enabled: bool,
    #[serde(default)]
    pub secret: bool,
}

impl Variable {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            secret: false,
        }
    }

    pub fn secret(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            secret: true,
        }
    }

    pub fn disabled(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: false,
            secret: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_variable_enabled_by_default() {
        let v = Variable::new("BASE_URL", "https://api.example.com");
        assert_eq!(v.key, "BASE_URL");
        assert!(v.enabled);
        assert!(!v.secret);
    }

    #[test]
    fn secret_variable() {
        let v = Variable::secret("API_KEY", "sk-12345");
        assert!(v.secret);
        assert!(v.enabled);
    }
}
