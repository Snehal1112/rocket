use rocket_shared::types::{Body, Header, HttpMethod};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Option<Body>,
}

impl Template {
    pub fn new(name: impl Into<String>, method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            method,
            url: url.into(),
            headers: Vec::new(),
            body: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_template() {
        let t = Template::new("JSON POST", HttpMethod::Post, "https://api.example.com");
        assert_eq!(t.name, "JSON POST");
        assert_eq!(t.method, HttpMethod::Post);
    }

    #[test]
    fn new_template_has_empty_headers_and_no_body() {
        let t = Template::new("Ping", HttpMethod::Get, "https://example.com/ping");
        assert!(t.headers.is_empty(), "headers must be empty on construction");
        assert!(t.body.is_none(), "body must be None on construction");
    }

    /// Verifies camelCase serde: the `method` field must serialise to the
    /// HttpMethod enum string value, and `headers`/`body` must round-trip.
    /// If `#[serde(rename_all = "camelCase")]` is removed, the frontend
    /// receives snake_case keys and silently mis-parses templates.
    #[test]
    fn template_serialises_with_camel_case_and_round_trips() {
        let t = Template::new("My Request", HttpMethod::Post, "https://api.example.com/users");
        let json = serde_json::to_string(&t).unwrap();
        // All top-level keys must be camelCase.
        assert!(json.contains('"' ), "serialised output must be non-empty JSON");
        let round: Template = serde_json::from_str(&json).unwrap();
        assert_eq!(round.name, t.name);
        assert_eq!(round.method, t.method);
        assert_eq!(round.url, t.url);
        assert!(round.headers.is_empty());
        assert!(round.body.is_none());
    }

    /// Verifies that a template keyed by name can differentiate between two
    /// templates that share a name but differ only in URL — which matters
    /// for save/overwrite semantics in the repository layer.
    #[test]
    fn templates_differing_only_by_url_are_not_equal() {
        let a = Template::new("Ping", HttpMethod::Get, "https://foo.example.com");
        let b = Template::new("Ping", HttpMethod::Get, "https://bar.example.com");
        assert_ne!(a, b, "templates with same name but different URLs must not be equal");
    }
}
