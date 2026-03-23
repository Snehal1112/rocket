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
}
