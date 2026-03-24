use rocket_shared::types::{Auth, Body, Header, HttpMethod};
use serde::{Deserialize, Serialize};

/// A saved API request definition.
/// Value object — immutable identity, compared by value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Option<Body>,
    #[serde(default)]
    pub auth: Auth,
    /// The filename on disk (e.g. "New Request.json"). Populated by build_folder_tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
}

impl Request {
    pub fn new(
        name: impl Into<String>,
        method: HttpMethod,
        url: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            method,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            auth: Auth::None,
            file_name: None,
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
}
