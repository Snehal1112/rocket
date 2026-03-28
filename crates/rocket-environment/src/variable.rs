use serde::{Deserialize, Serialize};
use rocket_shared::description::Description;
use rocket_shared::variable_value::VariableValueVariant;

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    pub value_variants: Option<Vec<VariableValueVariant>>,
    /// Secret type hint: "string" | "number" | "boolean" | "null" | "object".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_type: Option<String>,
}

// Custom deserializer that handles backward-compat `disabled` field.
impl<'de> Deserialize<'de> for Variable {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct VariableHelper {
            key: String,
            #[serde(default)]
            value: String,
            #[serde(default = "default_true")]
            enabled: bool,
            #[serde(default)]
            disabled: Option<bool>,
            #[serde(default)]
            secret: bool,
            #[serde(default)]
            description: Option<Description>,
            #[serde(default)]
            value_variants: Option<Vec<VariableValueVariant>>,
            #[serde(default)]
            secret_type: Option<String>,
        }

        fn default_true() -> bool { true }

        let helper = VariableHelper::deserialize(deserializer)?;

        // enabled = helper.enabled AND NOT disabled.
        // This means disabled: true overrides the default enabled: true,
        // and an explicit enabled: false is also respected.
        let enabled = helper.enabled && !helper.disabled.unwrap_or(false);

        Ok(Variable {
            key: helper.key,
            value: helper.value,
            enabled,
            secret: helper.secret,
            description: helper.description,
            value_variants: helper.value_variants,
            secret_type: helper.secret_type,
        })
    }
}

impl Variable {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            secret: false,
            description: None,
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
            value_variants: None,
            secret_type: Some("string".into()),
        };
        assert_eq!(v.secret_type, Some("string".into()));
    }

    #[test]
    fn variable_serde_roundtrip() {
        use rocket_shared::description::Description;
        let v = Variable {
            key: "HOST".into(),
            value: "localhost".into(),
            enabled: true,
            secret: false,
            description: Some(Description::text("Server host")),
            value_variants: None,
            secret_type: None,
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: Variable = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn deserialize_disabled_field_sets_enabled_false() {
        let json = r#"{"key":"X","value":"1","disabled":true}"#;
        let v: Variable = serde_json::from_str(json).unwrap();
        assert!(!v.enabled);
    }

    #[test]
    fn deserialize_without_disabled_defaults_to_enabled() {
        let json = r#"{"key":"X","value":"1"}"#;
        let v: Variable = serde_json::from_str(json).unwrap();
        assert!(v.enabled);
    }

    #[test]
    fn deserialize_disabled_false_keeps_enabled() {
        let json = r#"{"key":"X","value":"1","disabled":false}"#;
        let v: Variable = serde_json::from_str(json).unwrap();
        assert!(v.enabled);
    }

    #[test]
    fn deserialize_explicit_enabled_false_respected() {
        let json = r#"{"key":"X","value":"1","enabled":false}"#;
        let v: Variable = serde_json::from_str(json).unwrap();
        assert!(!v.enabled);
    }

    #[test]
    fn disabled_constructor_sets_enabled_false() {
        let v = Variable::disabled("KEY", "val");
        assert!(!v.enabled);
    }
}
