use serde::Deserialize;
use crate::bru::ast::*;
use crate::error::{ImportError, ImportResult};

// ─── Request structs ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BruYmlRequest {
    pub meta: Option<BruYmlMeta>,
    pub http: Option<BruYmlHttp>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlMeta {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub request_type: Option<String>,
    pub seq: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlHttp {
    pub method: Option<String>,
    pub url: Option<String>,
    pub headers: Option<Vec<BruYmlHeader>>,
    pub body: Option<BruYmlBody>,
    pub auth: Option<BruYmlAuth>,
    pub script: Option<BruYmlScript>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlHeader {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlBody {
    pub mode: Option<String>,
    pub json: Option<String>,
    pub text: Option<String>,
    pub xml: Option<String>,
    #[serde(rename = "formUrlEncoded")]
    pub form_url_encoded: Option<Vec<BruYmlFormField>>,
    pub multipart: Option<Vec<BruYmlFormField>>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlFormField {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlAuth {
    pub mode: Option<String>,
    pub bearer: Option<BruYmlBearerAuth>,
    pub basic: Option<BruYmlBasicAuth>,
    pub awsv4: Option<BruYmlAwsV4Auth>,
    pub apikey: Option<BruYmlApiKeyAuth>,
    pub digest: Option<BruYmlBasicAuth>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlBearerAuth {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlBasicAuth {
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlAwsV4Auth {
    #[serde(rename = "accessKeyId")]
    pub access_key_id: Option<String>,
    #[serde(rename = "secretAccessKey")]
    pub secret_access_key: Option<String>,
    #[serde(rename = "sessionToken")]
    pub session_token: Option<String>,
    pub service: Option<String>,
    pub region: Option<String>,
    #[serde(rename = "profileName")]
    pub profile_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlApiKeyAuth {
    pub key: Option<String>,
    pub value: Option<String>,
    pub placement: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlScript {
    pub req: Option<String>,
    pub res: Option<String>,
}

// ─── Environment structs ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BruYmlEnv {
    pub name: Option<String>,
    pub variables: Option<Vec<BruYmlEnvVar>>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlEnvVar {
    pub name: String,
    pub value: Option<String>,
    #[serde(default)]
    pub secret: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }

// ─── Public adapter functions ─────────────────────────────────────────────────

/// Parse a Bruno .yml request file string into a BruDocument.
pub fn bru_document_from_yml_str(input: &str) -> ImportResult<BruDocument> {
    let yml: BruYmlRequest = serde_yaml::from_str(input)
        .map_err(|e| ImportError::ParseError {
            path: std::path::PathBuf::new(),
            message: e.to_string(),
        })?;
    Ok(adapt_request(yml))
}

/// Parse a Bruno .yml environment file string into a BruDocument.
pub fn bru_document_from_yml_env_str(input: &str) -> ImportResult<BruDocument> {
    let yml: BruYmlEnv = serde_yaml::from_str(input)
        .map_err(|e| ImportError::ParseError {
            path: std::path::PathBuf::new(),
            message: e.to_string(),
        })?;
    Ok(adapt_env(yml))
}

fn adapt_request(yml: BruYmlRequest) -> BruDocument {
    let mut doc = BruDocument::default();

    // Meta
    if let Some(m) = yml.meta {
        let request_type = m.request_type.clone().unwrap_or_default();
        // Non-http types go to unknown_blocks immediately.
        if !matches!(request_type.as_str(), "http" | "") {
            doc.unknown_blocks.push(BruRawBlock {
                name: "unsupported_type".into(),
                subtype: Some(request_type.clone()),
                content: String::new(),
            });
        }
        doc.meta = Some(BruMeta {
            name: m.name.unwrap_or_default(),
            request_type,
            seq: m.seq,
        });
    }

    if let Some(http) = yml.http {
        doc.method = http.method.as_deref().and_then(|m| BruMethod::from_block_name(&m.to_lowercase()));
        doc.url = http.url;

        if let Some(headers) = http.headers {
            doc.headers = headers.into_iter().map(|h| BruKeyValue {
                key: h.name,
                value: h.value,
                disabled: h.disabled,
            }).collect();
        }

        if let Some(body) = http.body {
            doc.body = adapt_body(body, &mut doc.unknown_blocks);
        }

        if let Some(auth) = http.auth {
            doc.auth = adapt_auth(auth, &mut doc.unknown_blocks);
        }

        if let Some(script) = http.script {
            if let Some(req) = script.req {
                if !req.is_empty() { doc.pre_request_script = Some(req); }
            }
            if let Some(res) = script.res {
                if !res.is_empty() { doc.post_response_script = Some(res); }
            }
        }
    }

    doc
}

fn adapt_body(body: BruYmlBody, unknown: &mut Vec<BruRawBlock>) -> Option<BruBody> {
    match body.mode.as_deref() {
        Some("json")           => Some(BruBody::Json(body.json.unwrap_or_default())),
        Some("text")           => Some(BruBody::Text(body.text.unwrap_or_default())),
        Some("xml")            => Some(BruBody::Xml(body.xml.unwrap_or_default())),
        Some("formUrlEncoded") => Some(BruBody::FormUrlEncoded(
            body.form_url_encoded.unwrap_or_default().into_iter()
                .map(|f| BruKeyValue { key: f.name, value: f.value, disabled: f.disabled })
                .collect()
        )),
        Some("multipart") => Some(BruBody::Multipart(
            body.multipart.unwrap_or_default().into_iter()
                .map(|f| BruKeyValue { key: f.name, value: f.value, disabled: f.disabled })
                .collect()
        )),
        Some(other) => {
            unknown.push(BruRawBlock {
                name: "body".into(),
                subtype: Some(other.to_string()),
                content: String::new(),
            });
            None
        }
        None => None,
    }
}

fn adapt_auth(auth: BruYmlAuth, unknown: &mut Vec<BruRawBlock>) -> Option<BruAuth> {
    match auth.mode.as_deref() {
        Some("bearer") => {
            let b = auth.bearer.unwrap_or_default_bearer();
            Some(BruAuth::Bearer { token: b.token.unwrap_or_default() })
        }
        Some("basic") => {
            let b = auth.basic.unwrap_or_default_basic();
            Some(BruAuth::Basic {
                username: b.username.unwrap_or_default(),
                password: b.password.unwrap_or_default(),
            })
        }
        Some("awsv4") => {
            let a = auth.awsv4.unwrap_or(BruYmlAwsV4Auth {
                access_key_id: None, secret_access_key: None,
                session_token: None, service: None, region: None, profile_name: None,
            });
            Some(BruAuth::AwsV4 {
                access_key_id: a.access_key_id.unwrap_or_default(),
                secret_access_key: a.secret_access_key.unwrap_or_default(),
                session_token: a.session_token,
                service: a.service,
                region: a.region,
                profile_name: a.profile_name,
            })
        }
        Some("apikey") => {
            let a = auth.apikey.unwrap_or(BruYmlApiKeyAuth {
                key: None, value: None, placement: None,
            });
            Some(BruAuth::ApiKey {
                key: a.key.unwrap_or_default(),
                value: a.value.unwrap_or_default(),
                placement: a.placement.unwrap_or_default(),
            })
        }
        Some("digest") => {
            let d = auth.digest.unwrap_or_default_basic();
            Some(BruAuth::Digest {
                username: d.username.unwrap_or_default(),
                password: d.password.unwrap_or_default(),
            })
        }
        Some(other) => {
            unknown.push(BruRawBlock {
                name: "auth".into(),
                subtype: Some(other.to_string()),
                content: String::new(),
            });
            None
        }
        None => None,
    }
}

// Helper traits to avoid repeated Option::unwrap_or boilerplate.
trait DefaultBearer { fn unwrap_or_default_bearer(self) -> BruYmlBearerAuth; }
impl DefaultBearer for Option<BruYmlBearerAuth> {
    fn unwrap_or_default_bearer(self) -> BruYmlBearerAuth {
        self.unwrap_or(BruYmlBearerAuth { token: None })
    }
}

trait DefaultBasic { fn unwrap_or_default_basic(self) -> BruYmlBasicAuth; }
impl DefaultBasic for Option<BruYmlBasicAuth> {
    fn unwrap_or_default_basic(self) -> BruYmlBasicAuth {
        self.unwrap_or(BruYmlBasicAuth { username: None, password: None })
    }
}

fn adapt_env(yml: BruYmlEnv) -> BruDocument {
    let mut doc = BruDocument::default();
    if let Some(vars) = yml.variables {
        for v in vars {
            if v.secret {
                doc.secret_vars.push(v.name);
            } else {
                doc.vars.push(BruKeyValue {
                    key: v.name,
                    value: v.value.unwrap_or_default(),
                    disabled: !v.enabled,
                });
            }
        }
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapts_yml_request_to_bru_document() {
        let yml = r#"
meta:
  name: Get Users
  type: http
  seq: 1
http:
  method: GET
  url: "{{baseUrl}}/users"
  headers:
    - name: Content-Type
      value: application/json
      disabled: false
  body:
    mode: json
    json: '{"page": 1}'
  auth:
    mode: bearer
    bearer:
      token: "{{authToken}}"
  script:
    req: "bru.setVar('ts', Date.now());"
    res: ""
"#;
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert_eq!(doc.meta.as_ref().unwrap().name, "Get Users");
        assert_eq!(doc.method, Some(BruMethod::Get));
        assert_eq!(doc.url.as_deref(), Some("{{baseUrl}}/users"));
        assert_eq!(doc.headers.len(), 1);
        assert!(matches!(doc.body, Some(BruBody::Json(_))));
        assert!(matches!(doc.auth, Some(BruAuth::Bearer { .. })));
        assert!(doc.pre_request_script.is_some());
    }

    #[test]
    fn unknown_auth_mode_lands_in_unknown_blocks() {
        let yml = r#"
meta:
  name: Test
  type: http
http:
  method: GET
  url: https://example.com
  auth:
    mode: oauth2
"#;
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert_eq!(doc.unknown_blocks.len(), 1);
        assert_eq!(doc.unknown_blocks[0].name, "auth");
        assert_eq!(doc.unknown_blocks[0].subtype.as_deref(), Some("oauth2"));
    }

    #[test]
    fn adapts_yml_env_to_bru_document() {
        let yml = r#"
name: local
variables:
  - name: baseUrl
    value: http://localhost:3000
    enabled: true
  - name: DB_PASSWORD
    value: ""
    secret: true
    enabled: true
"#;
        let doc = bru_document_from_yml_env_str(yml).unwrap();
        assert_eq!(doc.vars.len(), 1);
        assert_eq!(doc.vars[0].key, "baseUrl");
        assert_eq!(doc.secret_vars.len(), 1);
        assert_eq!(doc.secret_vars[0], "DB_PASSWORD");
    }

    #[test]
    fn graphql_request_type_lands_in_unknown_blocks() {
        let yml = r#"
meta:
  name: GQL Query
  type: graphql
http:
  method: POST
  url: https://api.example.com/graphql
"#;
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert_eq!(doc.unknown_blocks.len(), 1);
        assert_eq!(doc.unknown_blocks[0].name, "unsupported_type");
    }
}
