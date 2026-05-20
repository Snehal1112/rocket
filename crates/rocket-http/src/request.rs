use rocket_shared::types::{Auth, Body, Header, HttpMethod, QueryParam};
use serde::{Deserialize, Serialize};

/// An HTTP request ready for execution (resolved variables, all fields populated).
/// This is different from collection::Request which is a saved definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub query_params: Vec<QueryParam>,
    pub body: Option<Body>,
    pub auth: Auth,
    pub options: RequestOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestOptions {
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_true")]
    pub verify_ssl: bool,
    /// Override the maximum number of redirects to follow. `None` uses the executor default (10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<u32>,
}

fn default_true() -> bool { true }
fn default_timeout() -> u64 { 30_000 }

impl Default for RequestOptions {
    fn default() -> Self {
        Self {
            follow_redirects: true,
            timeout_ms: 30_000,
            verify_ssl: true,
            max_redirects: None,
        }
    }
}

impl HttpRequest {
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            query_params: Vec::new(),
            body: None,
            auth: Auth::None,
            options: RequestOptions::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options() {
        let req = HttpRequest::new(HttpMethod::Get, "https://example.com");
        assert!(req.options.follow_redirects);
        assert_eq!(req.options.timeout_ms, 30_000);
        assert!(req.options.verify_ssl);
    }
}
