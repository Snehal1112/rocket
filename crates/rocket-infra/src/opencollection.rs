//! OpenCollection YAML file-format structs.
//! These mirror the OpenCollection JSON schema for on-disk YAML serialization.
//! Domain types from rocket-shared are re-used where field names match.

use serde::{Deserialize, Serialize};

// Re-export domain types that map directly to schema types.
pub use rocket_shared::description::{Description as OcDescription, Documentation as OcDocumentation};
pub use rocket_shared::variable_value::{VariableValue as OcVariableValue, VariableValueVariant as OcVariableValueVariant};

/// OpenCollection Variable — schema field names: name, value, description, disabled.
/// Our domain Variable uses `key` instead of `name` and `enabled` instead of `disabled`,
/// so we need a separate YAML struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcVariable {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<OcVariableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// OpenCollection SecretVariable — schema: { secret: true, name, description, disabled, type }.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcSecretVariable {
    pub secret: bool,  // always true
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub secret_type: Option<String>,  // "string"|"number"|"boolean"|"null"|"object"
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::description::Description;
    use rocket_shared::variable_value::VariableValue;

    #[test]
    fn oc_description_yaml_string() {
        let yaml = "\"A simple description\"";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), Some("A simple description"));
    }

    #[test]
    fn oc_description_yaml_object() {
        let yaml = "content: \"# Docs\"\ntype: text/markdown";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), Some("# Docs"));
        assert_eq!(desc.content_type(), Some("text/markdown"));
    }

    #[test]
    fn oc_description_yaml_null() {
        let yaml = "null";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), None);
    }

    #[test]
    fn oc_variable_yaml_simple() {
        let yaml = "name: BASE_URL\nvalue: https://api.example.com";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.name, "BASE_URL");
        assert_eq!(var.value.as_ref().unwrap().data(), "https://api.example.com");
    }

    #[test]
    fn oc_variable_yaml_typed_value() {
        let yaml = "name: COUNT\nvalue:\n  type: number\n  data: \"42\"";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.value.as_ref().unwrap().value_type(), Some("number"));
        assert_eq!(var.value.as_ref().unwrap().data(), "42");
    }

    #[test]
    fn oc_variable_yaml_with_description_and_disabled() {
        let yaml = "name: HOST\nvalue: localhost\ndescription: The API host\ndisabled: true";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.disabled, Some(true));
        assert!(var.description.is_some());
    }

    #[test]
    fn oc_secret_variable_yaml() {
        let yaml = "secret: true\nname: API_KEY\ntype: string\ndisabled: false";
        let sv: OcSecretVariable = serde_yaml::from_str(yaml).unwrap();
        assert!(sv.secret);
        assert_eq!(sv.name, "API_KEY");
        assert_eq!(sv.secret_type, Some("string".into()));
    }

    #[test]
    fn oc_variable_yaml_roundtrip() {
        let var = OcVariable {
            name: "HOST".into(),
            value: Some(VariableValue::simple("localhost")),
            description: Some(Description::text("Server host")),
            disabled: Some(false),
        };
        let yaml = serde_yaml::to_string(&var).unwrap();
        let back: OcVariable = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(var, back);
    }

    #[test]
    fn oc_variable_value_variant_yaml() {
        let yaml = "title: Production\nselected: true\nvalue: https://prod.example.com";
        let variant: OcVariableValueVariant = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(variant.title, "Production");
        assert!(variant.selected);
    }
}
