use crate::bru::ast::*;
use crate::error::{ImportError, ImportResult};
use rocket_collection::{GraphQlBody, GraphQlBodyVariant};
use serde::Deserialize;

// ─── Request structs ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BruYmlRequest {
    pub meta: Option<BruYmlMeta>,
    pub http: Option<BruYmlHttp>,
    #[serde(alias = "websocket")]
    pub ws: Option<BruYmlWs>,
    /// OpenCollection-shaped `info:` block, used by GraphQL files.
    pub info: Option<BruYmlMeta>,
    /// OpenCollection-shaped `graphql:` block.
    pub graphql: Option<BruYmlGraphql>,
    /// OpenCollection-shaped `runtime:` block, read for GraphQL scripts.
    pub runtime: Option<BruYmlRuntime>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlRuntime {
    pub scripts: Option<Vec<BruYmlRuntimeScript>>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlRuntimeScript {
    #[serde(rename = "type")]
    pub script_type: Option<String>,
    #[serde(default)]
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlGraphql {
    pub method: Option<String>,
    pub url: Option<String>,
    pub headers: Option<Vec<BruYmlHeader>>,
    /// A `{query, variables}` mapping, or a list of titled variants.
    pub body: Option<serde_yaml::Value>,
    pub auth: Option<serde_yaml::Value>,
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

/// The WebSocket block of a Bruno YAML request. `websocket` is accepted as an alias of `ws`.
#[derive(Debug, Deserialize)]
pub struct BruYmlWs {
    pub url: Option<String>,
    pub headers: Option<Vec<BruYmlHeader>>,
    pub auth: Option<BruYmlAuth>,
    pub messages: Option<Vec<BruYmlWsMessage>>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlWsMessage {
    #[serde(alias = "title")]
    pub name: Option<String>,
    #[serde(rename = "type", alias = "format")]
    pub kind: Option<String>,
    #[serde(alias = "body", alias = "data")]
    pub content: Option<String>,
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
    /// Parsed but unused; env name comes from the file stem, not the YAML.
    #[allow(dead_code)]
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

fn default_true() -> bool {
    true
}

// ─── Public adapter functions ─────────────────────────────────────────────────

/// Parse a Bruno .yml request file string into a BruDocument.
pub fn bru_document_from_yml_str(input: &str) -> ImportResult<BruDocument> {
    let yml: BruYmlRequest = serde_yaml::from_str(input).map_err(|e| ImportError::ParseError {
        path: std::path::PathBuf::new(),
        message: e.to_string(),
    })?;
    Ok(adapt_request(yml))
}

/// Parse a Bruno .yml environment file string into a BruDocument.
pub fn bru_document_from_yml_env_str(input: &str) -> ImportResult<BruDocument> {
    let yml: BruYmlEnv = serde_yaml::from_str(input).map_err(|e| ImportError::ParseError {
        path: std::path::PathBuf::new(),
        message: e.to_string(),
    })?;
    Ok(adapt_env(yml))
}

fn adapt_request(yml: BruYmlRequest) -> BruDocument {
    let BruYmlRequest {
        meta,
        http,
        ws,
        info,
        graphql,
        runtime,
    } = yml;
    if let Some(gql) = graphql {
        return adapt_graphql(info.or(meta), gql, runtime);
    }
    let mut doc = BruDocument::default();

    // Meta
    if let Some(m) = meta {
        let request_type = m.request_type.clone().unwrap_or_default();
        // Non-http types go to unknown_blocks immediately.
        if !matches!(request_type.as_str(), "http" | "" | "graphql" | "ws" | "websocket") {
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

    if let Some(http) = http {
        doc.method = http
            .method
            .as_deref()
            .and_then(|m| BruMethod::from_block_name(&m.to_lowercase()));
        doc.url = http.url;

        if let Some(headers) = http.headers {
            doc.headers = headers
                .into_iter()
                .map(|h| BruKeyValue {
                    key: h.name,
                    value: h.value,
                    disabled: h.disabled,
                })
                .collect();
        }

        if let Some(body) = http.body {
            doc.body = adapt_body(body, &mut doc.unknown_blocks);
        }

        if let Some(auth) = http.auth {
            doc.auth = adapt_auth(auth, &mut doc.unknown_blocks);
        }

        if let Some(script) = http.script {
            if let Some(req) = script.req {
                if !req.is_empty() {
                    doc.pre_request_script = Some(req);
                }
            }
            if let Some(res) = script.res {
                if !res.is_empty() {
                    doc.post_response_script = Some(res);
                }
            }
        }
    }

    if let Some(ws) = ws {
        doc.url = ws.url;
        doc.headers = ws
            .headers
            .unwrap_or_default()
            .into_iter()
            .map(|h| BruKeyValue {
                key: h.name,
                value: h.value,
                disabled: h.disabled,
            })
            .collect();
        if let Some(auth) = ws.auth {
            doc.ws_auth_mode = auth.mode.clone();
            doc.auth = adapt_auth(auth, &mut doc.unknown_blocks);
        }
        doc.ws_messages = ws
            .messages
            .unwrap_or_default()
            .into_iter()
            .map(|m| BruWsMessage {
                name: m.name.unwrap_or_default(),
                kind: m.kind.unwrap_or_else(|| "text".into()),
                content: m.content.unwrap_or_default(),
            })
            .collect();
    }

    doc
}

fn adapt_graphql(
    info: Option<BruYmlMeta>,
    gql: BruYmlGraphql,
    runtime: Option<BruYmlRuntime>,
) -> BruDocument {
    let mut doc = BruDocument::default();
    if let Some(m) = info {
        doc.meta = Some(BruMeta {
            name: m.name.unwrap_or_default(),
            request_type: "graphql".into(),
            seq: m.seq,
        });
    }
    doc.method = gql
        .method
        .as_deref()
        .and_then(|m| BruMethod::from_block_name(&m.to_lowercase()));
    doc.url = gql.url;
    if let Some(headers) = gql.headers {
        doc.headers = headers
            .into_iter()
            .map(|h| BruKeyValue {
                key: h.name,
                value: h.value,
                disabled: h.disabled,
            })
            .collect();
    }
    doc.graphql = gql.body.as_ref().and_then(graphql_body_of);
    // OpenCollection auth is `type:`-tagged, unlike the `mode:`-tagged http block.
    // Bearer and basic convert; anything else is reported.
    if let Some(auth) = gql.auth {
        let text = |key: &str| {
            auth.get(key)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        match auth.get("type").and_then(|t| t.as_str()) {
            Some("bearer") => {
                doc.auth = Some(BruAuth::Bearer {
                    token: text("token"),
                })
            }
            Some("basic") => {
                doc.auth = Some(BruAuth::Basic {
                    username: text("username"),
                    password: text("password"),
                })
            }
            other => doc.unknown_blocks.push(BruRawBlock {
                name: "auth".into(),
                subtype: Some(other.unwrap_or_default().to_string()),
                content: String::new(),
            }),
        }
    }
    if let Some(runtime) = runtime {
        for script in runtime.scripts.unwrap_or_default() {
            match script.script_type.as_deref() {
                Some("before-request") => doc.pre_request_script = Some(script.code),
                Some("after-response") => doc.post_response_script = Some(script.code),
                _ => {}
            }
        }
    }
    doc
}

/// Reads a GraphQL body that is either a `{query, variables}` mapping or a
/// list of titled variants. A variant list keeps every variant, and `query` and
/// `variables` hold the selected one, or the first when none is marked.
fn graphql_body_of(body: &serde_yaml::Value) -> Option<BruGraphQl> {
    fn plain(v: &serde_yaml::Value) -> Option<(String, Option<String>)> {
        let query = v.get("query")?.as_str()?.to_string();
        let variables = v
            .get("variables")
            .and_then(|x| x.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(String::from);
        Some((query, variables))
    }
    match body.as_sequence() {
        Some(items) => {
            let mut variants: Vec<GraphQlBodyVariant> = items
                .iter()
                .filter_map(|v| {
                    let (query, variables) = plain(v.get("body")?)?;
                    Some(GraphQlBodyVariant {
                        title: v.get("title")?.as_str()?.to_string(),
                        selected: v.get("selected").and_then(|s| s.as_bool()).unwrap_or(false),
                        body: GraphQlBody { query, variables },
                    })
                })
                .collect();
            if !variants.iter().any(|v| v.selected) {
                variants.first_mut()?.selected = true;
            }
            let active = variants.iter().find(|v| v.selected)?.body.clone();
            Some(BruGraphQl {
                query: active.query,
                variables: active.variables,
                variants,
            })
        }
        None => {
            let (query, variables) = plain(body)?;
            Some(BruGraphQl {
                query,
                variables,
                variants: Vec::new(),
            })
        }
    }
}

fn adapt_body(body: BruYmlBody, unknown: &mut Vec<BruRawBlock>) -> Option<BruBody> {
    match body.mode.as_deref() {
        Some("json") => Some(BruBody::Json(body.json.unwrap_or_default())),
        Some("text") => Some(BruBody::Text(body.text.unwrap_or_default())),
        Some("xml") => Some(BruBody::Xml(body.xml.unwrap_or_default())),
        Some("formUrlEncoded") => Some(BruBody::FormUrlEncoded(
            body.form_url_encoded
                .unwrap_or_default()
                .into_iter()
                .map(|f| BruKeyValue {
                    key: f.name,
                    value: f.value,
                    disabled: f.disabled,
                })
                .collect(),
        )),
        Some("multipart") => Some(BruBody::Multipart(
            body.multipart
                .unwrap_or_default()
                .into_iter()
                .map(|f| BruKeyValue {
                    key: f.name,
                    value: f.value,
                    disabled: f.disabled,
                })
                .collect(),
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
            Some(BruAuth::Bearer {
                token: b.token.unwrap_or_default(),
            })
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
                access_key_id: None,
                secret_access_key: None,
                session_token: None,
                service: None,
                region: None,
                profile_name: None,
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
                key: None,
                value: None,
                placement: None,
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
trait DefaultBearer {
    fn unwrap_or_default_bearer(self) -> BruYmlBearerAuth;
}
impl DefaultBearer for Option<BruYmlBearerAuth> {
    fn unwrap_or_default_bearer(self) -> BruYmlBearerAuth {
        self.unwrap_or(BruYmlBearerAuth { token: None })
    }
}

trait DefaultBasic {
    fn unwrap_or_default_basic(self) -> BruYmlBasicAuth;
}
impl DefaultBasic for Option<BruYmlBasicAuth> {
    fn unwrap_or_default_basic(self) -> BruYmlBasicAuth {
        self.unwrap_or(BruYmlBasicAuth {
            username: None,
            password: None,
        })
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
    fn opencollection_graphql_request_is_adapted() {
        let yml = r#"
info:
  name: GQL Query
  type: graphql
  seq: 4
graphql:
  method: POST
  url: https://api.example.com/graphql
  headers:
    - name: Accept
      value: application/json
  body:
    query: "{ users { id } }"
    variables: '{"first": 2}'
"#;
        let doc = bru_document_from_yml_str(yml).expect("adapt");
        assert!(doc.unknown_blocks.is_empty(), "{:?}", doc.unknown_blocks);
        let meta = doc.meta.as_ref().expect("meta");
        assert_eq!(meta.name, "GQL Query");
        assert_eq!(meta.request_type, "graphql");
        assert_eq!(meta.seq, Some(4));
        assert_eq!(doc.method, Some(BruMethod::Post));
        assert_eq!(doc.url.as_deref(), Some("https://api.example.com/graphql"));
        assert_eq!(doc.headers.len(), 1);
        let gql = doc.graphql.expect("graphql");
        assert_eq!(gql.query, "{ users { id } }");
        assert_eq!(gql.variables.as_deref(), Some("{\"first\": 2}"));
    }

    #[test]
    fn opencollection_graphql_variants_use_the_selected_body() {
        let yml = r#"
info:
  name: Multi
  type: graphql
graphql:
  url: https://api.example.com/graphql
  body:
    - title: A
      body:
        query: "{ a }"
    - title: B
      selected: true
      body:
        query: "{ b }"
"#;
        let doc = bru_document_from_yml_str(yml).expect("adapt");
        assert_eq!(doc.graphql.expect("graphql").query, "{ b }");
    }

    #[test]
    fn opencollection_graphql_auth_is_reported_not_silently_dropped() {
        let yml = r#"
info:
  name: Authed
  type: graphql
graphql:
  url: https://api.example.com/graphql
  body:
    query: "{ a }"
  auth:
    type: oauth2
"#;
        let doc = bru_document_from_yml_str(yml).expect("adapt");
        assert_eq!(doc.unknown_blocks.len(), 1);
        assert_eq!(doc.unknown_blocks[0].name, "auth");
        assert_eq!(doc.unknown_blocks[0].subtype.as_deref(), Some("oauth2"));
    }

    #[test]
    fn opencollection_graphql_keeps_every_variant() {
        let yml = r#"
info:
  name: Multi
  type: graphql
graphql:
  url: https://api.example.com/graphql
  body:
    - title: A
      body:
        query: "{ a }"
    - title: B
      selected: true
      body:
        query: "{ b }"
"#;
        let gql = bru_document_from_yml_str(yml)
            .expect("adapt")
            .graphql
            .expect("graphql");
        assert_eq!(gql.variants.len(), 2);
        assert!(!gql.variants[0].selected);
        assert!(gql.variants[1].selected);
        assert_eq!(gql.variants[0].body.query, "{ a }");
    }

    #[test]
    fn opencollection_graphql_reads_scripts_and_supported_auth() {
        let yml = r#"
info:
  name: Authed
  type: graphql
graphql:
  url: https://api.example.com/graphql
  body:
    query: "{ a }"
  auth:
    type: bearer
    token: abc
runtime:
  scripts:
    - type: before-request
      code: console.log(1)
    - type: after-response
      code: console.log(2)
"#;
        let doc = bru_document_from_yml_str(yml).expect("adapt");
        assert!(doc.unknown_blocks.is_empty(), "{:?}", doc.unknown_blocks);
        assert!(matches!(&doc.auth, Some(BruAuth::Bearer { token }) if token == "abc"));
        assert_eq!(doc.pre_request_script.as_deref(), Some("console.log(1)"));
        assert_eq!(doc.post_response_script.as_deref(), Some("console.log(2)"));
    }

    #[test]
    fn grpc_request_type_still_lands_in_unknown_blocks() {
        let yml = "meta:\n  name: G\n  type: grpc\nhttp:\n  method: POST\n  url: grpc://x\n";
        let doc = bru_document_from_yml_str(yml).expect("adapt");
        assert_eq!(doc.unknown_blocks.len(), 1);
        assert_eq!(doc.unknown_blocks[0].name, "unsupported_type");
    }

    #[test]
    fn websocket_yml_request_is_parsed_not_flagged_unsupported() {
        let yml = r#"
meta:
  name: Chat
  type: ws
ws:
  url: wss://chat.example.com/ws
  headers:
    - name: Origin
      value: https://example.com
  auth:
    mode: bearer
    bearer:
      token: t0k
  messages:
    - name: hello
      type: json
      body: '{"hi":true}'
    - name: ping
      type: text
      content: ping
"#;
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert!(doc.is_websocket());
        assert!(doc.unknown_blocks.is_empty(), "{:?}", doc.unknown_blocks);
        assert_eq!(doc.url.as_deref(), Some("wss://chat.example.com/ws"));
        assert_eq!(doc.headers.len(), 1);
        assert!(matches!(&doc.auth, Some(BruAuth::Bearer { token }) if token == "t0k"));
        assert_eq!(doc.ws_messages.len(), 2);
        assert_eq!(doc.ws_messages[0].kind, "json");
        assert_eq!(doc.ws_messages[0].content, "{\"hi\":true}");
        assert_eq!(doc.ws_messages[1].content, "ping");
    }

    #[test]
    fn websocket_yml_accepts_the_websocket_key_alias() {
        let yml = "meta:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: ws://x\n";
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert!(doc.is_websocket());
        assert_eq!(doc.url.as_deref(), Some("ws://x"));
    }
}
