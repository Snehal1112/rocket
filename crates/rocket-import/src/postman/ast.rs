use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanCollection {
    pub info: PostmanInfo,
    #[serde(default)]
    pub item: Vec<PostmanItem>,
    #[serde(default)]
    pub variable: Vec<PostmanVariable>,
    pub auth: Option<PostmanAuth>,
    /// Environments embedded directly in the collection export.
    /// Most real Postman exports include environments here.
    #[serde(default)]
    pub environment: Vec<PostmanEmbeddedEnvironment>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanInfo {
    pub name: String,
    pub schema: String,
}

/// Untagged: Request is tried first because it has a required `request`
/// field. Folder items lack that field and fall through to `Folder`.
/// (Reversing this order would misclassify every request as a folder,
/// since `PostmanFolder` only requires `name` — all other fields default.)
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum PostmanItem {
    Request(PostmanRequestItem),
    Folder(PostmanFolder),
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanFolder {
    pub name: String,
    #[serde(default)]
    pub item: Vec<PostmanItem>,
    pub auth: Option<PostmanAuth>,
    #[serde(default)]
    pub variable: Vec<PostmanVariable>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanRequestItem {
    pub name: String,
    pub request: PostmanRequest,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanRequest {
    pub method: String,
    pub url: PostmanUrl,
    #[serde(default)]
    pub header: Vec<PostmanHeader>,
    pub auth: Option<PostmanAuth>,
    pub body: Option<PostmanBody>,
    pub description: Option<PostmanDescription>,
}

/// Untagged: Object (v2.1) tried first, then plain String (v2.0).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum PostmanUrl {
    Object(PostmanUrlObject),
    String(String),
}

impl PostmanUrl {
    pub(crate) fn raw(&self) -> &str {
        match self {
            PostmanUrl::Object(o) => &o.raw,
            PostmanUrl::String(s) => s.as_str(),
        }
    }

    pub(crate) fn query_params(&self) -> &[PostmanQueryParam] {
        match self {
            PostmanUrl::Object(o) => &o.query,
            PostmanUrl::String(_) => &[],
        }
    }

    pub(crate) fn path_variables(&self) -> &[PostmanPathVariable] {
        match self {
            PostmanUrl::Object(o) => &o.variable,
            PostmanUrl::String(_) => &[],
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanUrlObject {
    pub raw: String,
    #[serde(default)]
    pub query: Vec<PostmanQueryParam>,
    #[serde(default)]
    pub variable: Vec<PostmanPathVariable>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanHeader {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanQueryParam {
    pub key: Option<String>,
    pub value: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanPathVariable {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanVariable {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanAuth {
    #[serde(rename = "type")]
    pub auth_type: String,
    #[serde(default)]
    pub bearer: Vec<PostmanKeyValue>,
    #[serde(default)]
    pub basic: Vec<PostmanKeyValue>,
    #[serde(default)]
    pub apikey: Vec<PostmanKeyValue>,
    #[serde(default)]
    pub oauth2: Vec<PostmanKeyValue>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanKeyValue {
    pub key: String,
    pub value: serde_json::Value,
}

impl PostmanKeyValue {
    /// Extract value as String regardless of JSON type.
    pub(crate) fn as_str_value(&self) -> String {
        match &self.value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanBody {
    pub mode: String,
    pub raw: Option<String>,
    pub options: Option<PostmanBodyOptions>,
    #[serde(default)]
    pub urlencoded: Vec<PostmanFormParam>,
    #[serde(default)]
    pub formdata: Vec<PostmanFormParam>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanBodyOptions {
    pub raw: Option<PostmanRawBodyOptions>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanRawBodyOptions {
    pub language: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanFormParam {
    pub key: String,
    pub value: Option<String>,
    #[serde(rename = "type", default)]
    pub param_type: String,
    #[serde(default)]
    pub disabled: bool,
}

/// An environment embedded directly inside a collection JSON export.
#[derive(Debug, Deserialize)]
pub(crate) struct PostmanEmbeddedEnvironment {
    pub name: String,
    #[serde(default)]
    pub values: Vec<PostmanEnvVar>,
}

// PostmanEnvVar is also defined in env_parser.rs — duplicate here so ast.rs
// remains self-contained. Both share the same shape.
#[derive(Debug, Deserialize)]
pub(crate) struct PostmanEnvVar {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Description is either a plain string or an object with `content` + `type`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum PostmanDescription {
    String(String),
    Object {
        content: String,
        #[serde(rename = "type")]
        content_type: String,
    },
}

impl PostmanDescription {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            PostmanDescription::String(s) => s.as_str(),
            PostmanDescription::Object { content, .. } => content.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_collection() {
        let json = r#"{
            "info": { "name": "My API", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": [
                {
                    "name": "Get Users",
                    "request": {
                        "method": "GET",
                        "url": { "raw": "{{baseUrl}}/users", "query": [], "variable": [] },
                        "header": []
                    }
                }
            ]
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        assert_eq!(col.info.name, "My API");
        assert_eq!(col.item.len(), 1);
    }

    #[test]
    fn parses_v2_0_url_as_plain_string() {
        let json = r#"{
            "info": { "name": "Legacy", "schema": "https://schema.getpostman.com/json/collection/v2.0.0/collection.json" },
            "item": [{
                "name": "Ping",
                "request": { "method": "GET", "url": "https://example.com/ping", "header": [] }
            }]
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        match &col.item[0] {
            PostmanItem::Request(r) => assert_eq!(r.request.url.raw(), "https://example.com/ping"),
            _ => panic!("expected request item"),
        }
    }

    #[test]
    fn parses_folder_with_nested_request() {
        let json = r#"{
            "info": { "name": "API", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": [{
                "name": "Auth",
                "item": [{
                    "name": "Login",
                    "request": { "method": "POST", "url": { "raw": "{{baseUrl}}/login" }, "header": [] }
                }]
            }]
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        match &col.item[0] {
            PostmanItem::Folder(f) => {
                assert_eq!(f.name, "Auth");
                assert_eq!(f.item.len(), 1);
            }
            _ => panic!("expected folder"),
        }
    }

    #[test]
    fn parses_bearer_auth() {
        let json = r#"{
            "info": { "name": "A", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": [],
            "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "{{myToken}}", "type": "string" }] }
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        let auth = col.auth.unwrap();
        assert_eq!(auth.auth_type, "bearer");
        assert_eq!(auth.bearer[0].as_str_value(), "{{myToken}}");
    }

    #[test]
    fn parses_collection_variables() {
        let json = r#"{
            "info": { "name": "A", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": [],
            "variable": [{ "key": "baseUrl", "value": "http://localhost:3000" }]
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        assert_eq!(col.variable[0].key, "baseUrl");
        assert_eq!(col.variable[0].value, "http://localhost:3000");
    }

    #[test]
    fn parses_embedded_environments() {
        let json = r#"{
            "info": { "name": "A", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": [],
            "environment": [
                {
                    "name": "Local",
                    "values": [
                        { "key": "baseUrl", "value": "http://localhost:3000", "enabled": true },
                        { "key": "apiKey", "value": "dev-key", "enabled": false }
                    ]
                },
                {
                    "name": "Staging",
                    "values": [
                        { "key": "baseUrl", "value": "https://staging.example.com", "enabled": true }
                    ]
                }
            ]
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        assert_eq!(col.environment.len(), 2);
        assert_eq!(col.environment[0].name, "Local");
        assert_eq!(col.environment[0].values.len(), 2);
        assert!(col.environment[0].values[0].enabled);
        assert!(!col.environment[0].values[1].enabled);
        assert_eq!(col.environment[1].name, "Staging");
    }

    #[test]
    fn collection_without_environment_field_defaults_to_empty() {
        let json = r#"{
            "info": { "name": "A", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": []
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        assert!(col.environment.is_empty());
    }
}
