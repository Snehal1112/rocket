use rocket_shared::action::ActionSetVariable;
use rocket_shared::assertion::Assertion;
use rocket_shared::description::{Description, Documentation};
use rocket_shared::types::{Auth, Header, HttpMethod, PathParam, QueryParam, RequestSettings};
use serde::{Deserialize, Serialize};

use crate::settings::CollectionVariable;

/// The query and the variables of a GraphQL request. `variables` is a JSON
/// string, not a parsed object, because the spec stores it that way and a user
/// may leave `{{placeholders}}` in it that are not valid JSON yet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlBody {
    #[serde(default)]
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<String>,
}

/// One named body of a request that stores several (spec `GraphQLBodyVariant`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlBodyVariant {
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    pub body: GraphQlBody,
}

/// A saved GraphQL request. It mirrors `Request` where the fields mean the
/// same thing, so the headers, auth, scripts and settings panels work for both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlRequest {
    #[serde(default = "crate::generate_uid")]
    pub uid: String,
    pub name: String,
    /// `POST` (JSON body) or `GET` (query string). Defaults to `POST`.
    pub method: HttpMethod,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_params: Vec<QueryParam>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_params: Vec<PathParam>,
    /// The body that is sent. When `body_variants` is not empty this is the
    /// selected variant's body.
    #[serde(default)]
    pub body: GraphQlBody,
    /// Every stored variant, so a save does not lose the unselected ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_variants: Vec<GraphQlBodyVariant>,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_request_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_response_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<Assertion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionSetVariable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<Documentation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<CollectionVariable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_auth: Option<Auth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<RequestSettings>,
}

impl GraphQlRequest {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            uid: crate::generate_uid(),
            name: name.into(),
            method: HttpMethod::Post,
            url: url.into(),
            headers: Vec::new(),
            query_params: Vec::new(),
            path_params: Vec::new(),
            body: GraphQlBody::default(),
            body_variants: Vec::new(),
            auth: Auth::None,
            file_name: None,
            seq: None,
            tags: Vec::new(),
            description: None,
            pre_request_script: None,
            post_response_script: None,
            tests: None,
            assertions: Vec::new(),
            actions: Vec::new(),
            docs: None,
            variables: Vec::new(),
            runtime_auth: None,
            settings: None,
        }
    }

    /// Builder method: set the query text.
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.body.query = query.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_graphql_request_posts_by_default() {
        let g = GraphQlRequest::new("List Users", "https://api.example.com/graphql");
        assert_eq!(g.method, HttpMethod::Post);
        assert!(!g.uid.is_empty());
        assert_eq!(g.body, GraphQlBody::default());
        assert!(g.body_variants.is_empty());
    }

    #[test]
    fn json_is_camel_case_and_skips_empty_fields() {
        let g = GraphQlRequest::new("A", "https://x/graphql").with_query("{ a }");
        let v = serde_json::to_value(&g).expect("serialize");
        assert_eq!(v["body"]["query"], "{ a }");
        assert_eq!(v["method"], "POST");
        assert!(v.get("bodyVariants").is_none());
        assert!(v.get("preRequestScript").is_none());
    }

    #[test]
    fn json_without_optional_fields_still_loads() {
        let json = r#"{"uid":"u1","name":"A","method":"POST","url":"https://x","body":{"query":"{ a }"}}"#;
        let g: GraphQlRequest = serde_json::from_str(json).expect("old shape loads");
        assert_eq!(g.body.query, "{ a }");
        assert!(g.body.variables.is_none());
        assert!(g.headers.is_empty());
    }
}
