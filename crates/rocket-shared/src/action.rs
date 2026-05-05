use serde::{Deserialize, Serialize};

use crate::description::Description;

/// Selector for extracting a value from request/response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionSelector {
    pub expression: String,
    pub method: String, // "jsonq"
}

/// Target variable for an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionVariable {
    pub name: String,
    pub scope: String, // "runtime" | "request" | "folder" | "collection" | "environment"
}

/// Action that sets a variable from a request/response value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionSetVariable {
    pub phase: String, // "before-request" | "after-response"
    pub selector: ActionSelector,
    pub variable: ActionVariable,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
}

/// A saved example of an HTTP request-response pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequestExample {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_yaml::Value>, // Opaque snapshot of the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_yaml::Value>, // Opaque snapshot of the response.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_set_variable_serde() {
        let action = ActionSetVariable {
            phase: "after-response".into(),
            selector: ActionSelector {
                expression: "res.body.token".into(),
                method: "jsonq".into(),
            },
            variable: ActionVariable {
                name: "authToken".into(),
                scope: "collection".into(),
            },
            disabled: None,
            description: None,
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: ActionSetVariable = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn action_with_description() {
        let action = ActionSetVariable {
            phase: "before-request".into(),
            selector: ActionSelector {
                expression: "req.headers.Authorization".into(),
                method: "jsonq".into(),
            },
            variable: ActionVariable {
                name: "token".into(),
                scope: "runtime".into(),
            },
            disabled: Some(true),
            description: Some(Description::text("Extract auth token")),
        };
        assert!(action.description.is_some());
        assert_eq!(action.disabled, Some(true));
        let json = serde_json::to_string(&action).unwrap();
        let back: ActionSetVariable = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn http_request_example_serde() {
        let example = HttpRequestExample {
            name: "Success case".into(),
            description: None,
            request: Some(serde_yaml::from_str("url: https://api.example.com\nmethod: GET").unwrap()),
            response: Some(serde_yaml::from_str("status: 200\nbody:\n  ok: true").unwrap()),
        };
        let yaml = serde_yaml::to_string(&example).unwrap();
        let back: HttpRequestExample = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(example, back);
    }
}
