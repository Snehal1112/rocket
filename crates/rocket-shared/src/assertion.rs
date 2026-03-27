use serde::{Deserialize, Serialize};
use crate::description::Description;

/// OpenCollection Assertion — full spec with disabled + description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assertion {
    pub expression: String,
    pub operator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
}

impl Assertion {
    pub fn new(expression: impl Into<String>, operator: impl Into<String>, value: Option<String>) -> Self {
        Self {
            expression: expression.into(),
            operator: operator.into(),
            value,
            disabled: None,
            description: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::description::Description;

    #[test]
    fn assertion_basic() {
        let a = Assertion::new("res.status", "eq", Some("200".to_string()));
        assert_eq!(a.expression, "res.status");
        assert_eq!(a.operator, "eq");
        assert_eq!(a.value, Some("200".to_string()));
        assert_eq!(a.disabled, None);
        assert!(a.description.is_none());
    }

    #[test]
    fn assertion_with_disabled_and_description() {
        let a = Assertion {
            expression: "res.body.name".to_string(),
            operator: "isString".to_string(),
            value: None,
            disabled: Some(true),
            description: Some(Description::text("Check name is string")),
        };
        assert_eq!(a.disabled, Some(true));
        assert_eq!(a.description.as_ref().unwrap().content(), Some("Check name is string"));
    }

    #[test]
    fn assertion_serde_roundtrip() {
        let a = Assertion {
            expression: "res.status".to_string(),
            operator: "eq".to_string(),
            value: Some("200".to_string()),
            disabled: Some(false),
            description: Some(Description::text("Status check")),
        };
        let json = serde_json::to_string(&a).unwrap();
        let back: Assertion = serde_json::from_str(&json).unwrap();
        assert_eq!(a.expression, back.expression);
        assert_eq!(a.disabled, back.disabled);
    }
}
