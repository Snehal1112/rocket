//! Variable structs for the OpenCollection YAML format.

use rocket_shared::description::Description as OcDescription;
use rocket_shared::variable_value::VariableValue as OcVariableValue;
use serde::{Deserialize, Serialize};

/// OpenCollection Variable — schema field names: name, value, initial, description, disabled.
/// Our domain Variable uses `key` instead of `name` and `enabled` instead of `disabled`,
/// so we need a separate YAML struct.
/// The `initial` field stores the initial/default value; `value` stores the current/runtime value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcVariable {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<OcVariableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<OcVariableValue>,
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
