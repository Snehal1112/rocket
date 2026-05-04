use rocket_shared::action::{ActionSetVariable, HttpRequestExample};
use rocket_shared::assertion::Assertion;
use rocket_shared::description::{Description, Documentation};
use rocket_shared::types::{Auth, Body, Header, HttpMethod, PathParam, QueryParam, RequestSettings};
use serde::{Deserialize, Serialize};

/// A saved API request definition.
/// Value object — immutable identity, compared by value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    #[serde(default = "crate::generate_uid")]
    pub uid: String,
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_params: Vec<QueryParam>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_params: Vec<PathParam>,
    pub body: Option<Body>,
    #[serde(default)]
    pub auth: Auth,
    /// The filename on disk (e.g. "New Request.json"). Populated by build_folder_tree.
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<HttpRequestExample>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<Documentation>,
    /// Request-level variables. Typed as Value until rocket-environment is wired as a dependency.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<serde_json::Value>,
    /// Auth override applied at runtime (e.g. runtime.auth in OC YAML).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_auth: Option<Auth>,
    /// Request-level execution settings (timeout, encode URL, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<RequestSettings>,
}

/// Upper bound on the `"{stem} {n}.yml"` collision loop in
/// `FsCollectionRepo::save_request`. In practice the loop never iterates
/// because requests carry a UID, but the bound lives in the domain so infra
/// cannot embed its own arbitrary limit.
pub const MAX_FILENAME_COLLISION_RETRIES: u32 = 9_999;

/// Apply the on-disk extension policy for a request file.
///
/// - `.yml` / `.yaml` → returned as-is.
/// - `.json` (legacy format) → rewritten to `.yml`.
/// - anything else → `.yml` appended.
///
/// The on-disk format is a domain choice (OpenCollection stores requests as
/// `.yml`); persistence layers must call this rather than embedding the rule.
pub fn request_filename_for(path: &str) -> String {
    if path.ends_with(".yml") || path.ends_with(".yaml") {
        path.to_string()
    } else if let Some(stem) = path.strip_suffix(".json") {
        format!("{stem}.yml")
    } else {
        format!("{path}.yml")
    }
}

/// Build a collision-disambiguating candidate filename `"{stem} {n}.yml"`.
///
/// `counter` is the disambiguation index (caller advances on `AlreadyExists`).
/// The format and `.yml` extension are domain rules; persistence layers must
/// call this rather than constructing their own filename strings.
pub fn candidate_filename(stem: &str, counter: u32) -> String {
    format!("{stem} {counter}.yml")
}

impl Request {
    pub fn new(
        name: impl Into<String>,
        method: HttpMethod,
        url: impl Into<String>,
    ) -> Self {
        Self {
            uid: crate::generate_uid(),
            name: name.into(),
            method,
            url: url.into(),
            headers: Vec::new(),
            query_params: Vec::new(),
            path_params: Vec::new(),
            body: None,
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
            examples: Vec::new(),
            docs: None,
            variables: Vec::new(),
            runtime_auth: None,
            settings: None,
        }
    }

    /// Builder method: add an enabled header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push(Header::new(key, value));
        self
    }

    /// Builder method: set body.
    pub fn with_body(mut self, body: Body) -> Self {
        self.body = Some(body);
        self
    }

    /// Builder method: set auth.
    pub fn with_auth(mut self, auth: Auth) -> Self {
        self.auth = auth;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::{Body, BodyMode};

    #[test]
    fn new_request_has_defaults() {
        let req = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        assert_eq!(req.name, "Get Users");
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.url, "https://api.example.com/users");
        assert!(req.headers.is_empty());
        assert!(req.body.is_none());
        assert_eq!(req.auth, Auth::None);
    }

    #[test]
    fn request_with_headers() {
        let req = Request::new("Test", HttpMethod::Post, "https://api.example.com")
            .with_header("Content-Type", "application/json")
            .with_header("Authorization", "Bearer token");
        assert_eq!(req.headers.len(), 2);
        assert!(req.headers[0].enabled);
    }

    #[test]
    fn request_serialization_roundtrip() {
        let req = Request::new("Test", HttpMethod::Post, "https://api.example.com")
            .with_body(Body {
                mode: BodyMode::Json,
                content: Some("{\"key\":\"val\"}".into()),
                form_data: None,
                file_path: None,
            });
        let json = serde_json::to_string_pretty(&req).unwrap();
        let deserialized: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(req, deserialized);
    }

    #[test]
    fn request_with_description_and_scripts() {
        let req = Request::new("Test", HttpMethod::Get, "https://api.example.com");
        // New fields should default to empty/None.
        assert!(req.description.is_none());
        assert!(req.tags.is_empty());
        assert!(req.assertions.is_empty());
        assert!(req.pre_request_script.is_none());
        assert!(req.tests.is_none());
    }

    #[test]
    fn request_serde_backward_compat() {
        // Old JSON without new fields must still deserialize.
        let json = r#"{"uid":"123","name":"Test","method":"GET","url":"/test","headers":[],"body":null,"auth":{"authType":"none"}}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "Test");
        assert!(req.description.is_none());
        assert!(req.tags.is_empty());
    }

    #[test]
    fn request_filename_for_keeps_yml() {
        assert_eq!(request_filename_for("foo.yml"), "foo.yml");
    }

    #[test]
    fn request_filename_for_keeps_yaml() {
        assert_eq!(request_filename_for("foo.yaml"), "foo.yaml");
    }

    #[test]
    fn request_filename_for_migrates_json() {
        assert_eq!(request_filename_for("foo.json"), "foo.yml");
    }

    #[test]
    fn request_filename_for_appends_when_missing() {
        assert_eq!(request_filename_for("foo"), "foo.yml");
    }

    #[test]
    fn request_filename_for_handles_subpath_json() {
        assert_eq!(request_filename_for("auth/login.json"), "auth/login.yml");
    }

    #[test]
    fn candidate_filename_format() {
        assert_eq!(candidate_filename("login", 1), "login 1.yml");
    }

}
