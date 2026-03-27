use serde::{Deserialize, Serialize};
use rocket_shared::description::Description;
use rocket_shared::variable_value::VariableValueVariant;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub key: String,
    pub value: String,
    pub enabled: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_variants: Option<Vec<VariableValueVariant>>,
    /// Secret type hint: "string" | "number" | "boolean" | "null" | "object".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_type: Option<String>,
}

impl Variable {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            secret: false,
            description: None,
            disabled: None,
            value_variants: None,
            secret_type: None,
        }
    }

    pub fn secret(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            secret: true,
            description: None,
            disabled: None,
            value_variants: None,
            secret_type: None,
        }
    }

    pub fn disabled(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: false,
            secret: false,
            description: None,
            disabled: None,
            value_variants: None,
            secret_type: None,
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

    #[test]
    fn variable_with_description() {
        use rocket_shared::description::Description;
        let v = Variable {
            key: "HOST".into(),
            value: "localhost".into(),
            enabled: true,
            secret: false,
            description: Some(Description::text("The API host")),
            disabled: None,
            value_variants: None,
            secret_type: None,
        };
        assert!(v.description.is_some());
    }

    #[test]
    fn variable_with_variants() {
        use rocket_shared::variable_value::{VariableValue, VariableValueVariant};
        let v = Variable {
            key: "BASE_URL".into(),
            value: "https://prod.example.com".into(),
            enabled: true,
            secret: false,
            description: None,
            disabled: Some(false),
            value_variants: Some(vec![
                VariableValueVariant {
                    title: "Production".into(),
                    selected: true,
                    value: VariableValue::simple("https://prod.example.com"),
                },
                VariableValueVariant {
                    title: "Staging".into(),
                    selected: false,
                    value: VariableValue::simple("https://staging.example.com"),
                },
            ]),
            secret_type: None,
        };
        assert_eq!(v.value_variants.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn secret_variable_with_type() {
        let v = Variable {
            key: "API_KEY".into(),
            value: "sk-12345".into(),
            enabled: true,
            secret: true,
            description: None,
            disabled: None,
            value_variants: None,
            secret_type: Some("string".into()),
        };
        assert_eq!(v.secret_type, Some("string".into()));
    }

    #[test]
    fn variable_serde_roundtrip_with_new_fields() {
        use rocket_shared::description::Description;
        let v = Variable {
            key: "HOST".into(),
            value: "localhost".into(),
            enabled: true,
            secret: false,
            description: Some(Description::text("Server host")),
            disabled: Some(true),
            value_variants: None,
            secret_type: None,
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: Variable = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }
}
