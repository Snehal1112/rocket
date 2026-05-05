//! OpenCollection YAML file-format structs.
//! These mirror the OpenCollection JSON schema for on-disk YAML serialization.
//! Domain types from rocket-shared are re-used where field names match.

pub mod auth;
pub mod collection;
pub mod defaults;
pub mod environment;
pub mod folder;
pub mod graphql;
pub mod grpc;
pub mod http;
pub mod variables;
pub mod websocket;
pub mod workspace;

// Re-export everything so `use crate::oc::*` works just like `use crate::opencollection::*` did.
pub use auth::*;
pub use collection::*;
pub use defaults::*;
pub use environment::*;
pub use folder::*;
#[allow(unused_imports)]
pub use graphql::*;
#[allow(unused_imports)]
pub use grpc::*;
pub use http::*;
pub use variables::*;
#[allow(unused_imports)]
pub use websocket::*;
pub use workspace::*;

// Re-export the type aliases that opencollection.rs re-exported from shared crates.
#[allow(unused_imports)]
pub use rocket_shared::description::{Description as OcDescription, Documentation as OcDocumentation};
#[allow(unused_imports)]
pub use rocket_shared::oauth2::{OAuth2AdditionalParameters, OAuth2Settings, OAuth2TokenConfig};
#[allow(unused_imports)]
pub use rocket_shared::variable_value::{VariableValue as OcVariableValue, VariableValueVariant as OcVariableValueVariant};
#[allow(unused_imports)]
pub use rocket_shared::assertion::Assertion as OcAssertion;
#[allow(unused_imports)]
pub use rocket_shared::certificate::ClientCertificate as OcClientCertificate;

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::description::Description;
    use rocket_shared::variable_value::VariableValue;

    #[test]
    fn oc_description_yaml_string() {
        let yaml = "\"A simple description\"";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), Some("A simple description"));
    }

    #[test]
    fn oc_description_yaml_object() {
        let yaml = "content: \"# Docs\"\ntype: text/markdown";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), Some("# Docs"));
        assert_eq!(desc.content_type(), Some("text/markdown"));
    }

    #[test]
    fn oc_description_yaml_null() {
        let yaml = "null";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), None);
    }

    #[test]
    fn oc_variable_yaml_simple() {
        let yaml = "name: BASE_URL\nvalue: https://api.example.com";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.name, "BASE_URL");
        assert_eq!(var.value.as_ref().unwrap().data(), "https://api.example.com");
    }

    #[test]
    fn oc_variable_yaml_typed_value() {
        let yaml = "name: COUNT\nvalue:\n  type: number\n  data: \"42\"";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.value.as_ref().unwrap().value_type(), Some("number"));
        assert_eq!(var.value.as_ref().unwrap().data(), "42");
    }

    #[test]
    fn oc_variable_yaml_with_description_and_disabled() {
        let yaml = "name: HOST\nvalue: localhost\ndescription: The API host\ndisabled: true";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.disabled, Some(true));
        assert!(var.description.is_some());
    }

    #[test]
    fn oc_secret_variable_yaml() {
        let yaml = "secret: true\nname: API_KEY\ntype: string\ndisabled: false";
        let sv: OcSecretVariable = serde_yaml::from_str(yaml).unwrap();
        assert!(sv.secret);
        assert_eq!(sv.name, "API_KEY");
        assert_eq!(sv.secret_type, Some("string".into()));
    }

    #[test]
    fn oc_variable_yaml_roundtrip() {
        let var = OcVariable {
            name: "HOST".into(),
            value: Some(VariableValue::simple("localhost")),
            initial: Some(VariableValue::simple("127.0.0.1")),
            description: Some(Description::text("Server host")),
            disabled: Some(false),
        };
        let yaml = serde_yaml::to_string(&var).unwrap();
        let back: OcVariable = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(var, back);
    }

    #[test]
    fn oc_variable_value_variant_yaml() {
        let yaml = "title: Production\nselected: true\nvalue: https://prod.example.com";
        let variant: OcVariableValueVariant = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(variant.title, "Production");
        assert!(variant.selected);
    }

    #[test]
    fn oc_auth_inherit_yaml() {
        let yaml = "inherit";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Inherit(s) if s == "inherit"));
    }

    #[test]
    fn oc_auth_basic_yaml() {
        let yaml = "type: basic\nusername: user\npassword: pass";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::Basic { username, password }) => {
                assert_eq!(username, "user");
                assert_eq!(password, "pass");
            }
            _ => panic!("expected Basic"),
        }
    }

    #[test]
    fn oc_auth_bearer_yaml() {
        let yaml = "type: bearer\ntoken: my-token-123";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Typed(OcAuthTyped::Bearer { .. })));
    }

    #[test]
    fn oc_auth_apikey_yaml() {
        let yaml = "type: apikey\nkey: X-API-Key\nvalue: abc123\nplacement: header";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::ApiKey {
                key,
                value,
                placement,
            }) => {
                assert_eq!(key, "X-API-Key");
                assert_eq!(value, "abc123");
                assert_eq!(placement, Some("header".into()));
            }
            _ => panic!("expected ApiKey"),
        }
    }

    #[test]
    fn oc_auth_awsv4_yaml() {
        let yaml =
            "type: awsv4\naccessKeyId: AKIA...\nsecretAccessKey: secret\nregion: us-east-1\nservice: s3";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Typed(OcAuthTyped::AwsV4 { .. })));
    }

    #[test]
    fn oc_auth_digest_yaml() {
        let yaml = "type: digest\nusername: admin\npassword: secret";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Typed(OcAuthTyped::Digest { .. })));
    }

    #[test]
    fn oc_auth_oauth2_client_credentials_yaml() {
        let yaml = "type: oauth2\nflow: client_credentials\naccessTokenUrl: https://auth.example.com/token\ncredentials:\n  clientId: my-id\n  clientSecret: my-secret";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::OAuth2 {
                flow,
                access_token_url,
                credentials,
                ..
            }) => {
                assert_eq!(flow, "client_credentials");
                assert_eq!(
                    access_token_url,
                    Some("https://auth.example.com/token".into())
                );
                assert!(credentials.is_some());
            }
            _ => panic!("expected OAuth2"),
        }
    }

    #[test]
    fn oc_auth_oauth2_authorization_code_yaml() {
        let yaml = "type: oauth2\nflow: authorization_code\nauthorizationUrl: https://auth.example.com/authorize\naccessTokenUrl: https://auth.example.com/token\ncredentials:\n  clientId: id\n  clientSecret: secret\npkce:\n  enabled: true\n  method: S256";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::OAuth2 { flow, pkce, .. }) => {
                assert_eq!(flow, "authorization_code");
                assert!(pkce.is_some());
                assert_eq!(pkce.unwrap().method, Some("S256".into()));
            }
            _ => panic!("expected OAuth2"),
        }
    }

    #[test]
    fn inheritable_boolean_value() {
        let yaml = "true";
        let v: InheritableBoolean = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableBoolean::Value(true));
    }

    #[test]
    fn inheritable_boolean_inherit() {
        let yaml = "inherit";
        let v: InheritableBoolean = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableBoolean::Inherit("inherit".into()));
    }

    #[test]
    fn inheritable_number_value() {
        let yaml = "5000";
        let v: InheritableNumber = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableNumber::Value(5000.0));
    }

    #[test]
    fn inheritable_number_inherit() {
        let yaml = "inherit";
        let v: InheritableNumber = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableNumber::Inherit("inherit".into()));
    }

    #[test]
    fn oc_http_request_settings_yaml() {
        let yaml = "encodeUrl: true\ntimeout: 30000\nfollowRedirects: inherit\nmaxRedirects: 5";
        let settings: OcHttpRequestSettings = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(settings.encode_url, Some(InheritableBoolean::Value(true)));
        assert_eq!(settings.timeout, Some(InheritableNumber::Value(30000.0)));
        assert_eq!(settings.follow_redirects, Some(InheritableBoolean::Inherit("inherit".into())));
        assert_eq!(settings.max_redirects, Some(InheritableNumber::Value(5.0)));
    }

    #[test]
    fn oc_http_request_settings_roundtrip() {
        let settings = OcHttpRequestSettings {
            encode_url: Some(InheritableBoolean::Value(false)),
            timeout: Some(InheritableNumber::Inherit("inherit".into())),
            follow_redirects: None,
            max_redirects: Some(InheritableNumber::Value(10.0)),
            verify_ssl: Some(InheritableBoolean::Value(true)),
        };
        let yaml = serde_yaml::to_string(&settings).unwrap();
        let back: OcHttpRequestSettings = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(settings, back);
    }

    #[test]
    fn oc_graphql_request_settings_yaml() {
        let yaml = "encodeUrl: false\ntimeout: inherit";
        let settings: OcGraphQLRequestSettings = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(settings.encode_url, Some(InheritableBoolean::Value(false)));
        assert_eq!(settings.timeout, Some(InheritableNumber::Inherit("inherit".into())));
    }

    #[test]
    fn oc_http_request_info_yaml() {
        let yaml = "name: Get Users\ntype: http\nseq: 1\ntags:\n  - api\n  - users";
        let info: OcHttpRequestInfo = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(info.name, "Get Users");
        assert_eq!(info.request_type, Some("http".into()));
        assert_eq!(info.seq, Some(1));
        assert_eq!(info.tags, vec!["api", "users"]);
    }

    #[test]
    fn oc_http_request_header_yaml() {
        let yaml = "name: Content-Type\nvalue: application/json\ndisabled: false";
        let header: OcHttpRequestHeader = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(header.name, "Content-Type");
        assert_eq!(header.disabled, Some(false));
    }

    #[test]
    fn oc_http_request_param_yaml() {
        let yaml = "name: page\nvalue: \"1\"\ntype: query";
        let param: OcHttpRequestParam = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(param.name, "page");
        assert_eq!(param.param_type, Some("query".into()));
    }

    #[test]
    fn oc_body_json_yaml() {
        let yaml = "type: json\ndata: '{\"key\": \"value\"}'";
        let body: OcHttpRequestBody = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(body, OcHttpRequestBody::Json { .. }));
    }

    #[test]
    fn oc_body_form_urlencoded_yaml() {
        let yaml = "type: form-urlencoded\ndata:\n  - name: username\n    value: admin\n  - name: password\n    value: secret\n    disabled: true";
        let body: OcHttpRequestBody = serde_yaml::from_str(yaml).unwrap();
        match body {
            OcHttpRequestBody::FormUrlEncoded { data } => {
                assert_eq!(data.len(), 2);
                assert_eq!(data[1].disabled, Some(true));
            }
            _ => panic!("expected FormUrlEncoded"),
        }
    }

    #[test]
    fn oc_body_multipart_yaml() {
        let yaml = "type: multipart-form\ndata:\n  - name: file\n    type: file\n    value:\n      - /path/to/file.txt\n      - /path/to/file2.txt";
        let body: OcHttpRequestBody = serde_yaml::from_str(yaml).unwrap();
        match body {
            OcHttpRequestBody::MultipartForm { data } => {
                assert_eq!(data.len(), 1);
                assert!(matches!(data[0].value, OcMultipartValue::Multiple(_)));
            }
            _ => panic!("expected MultipartForm"),
        }
    }

    #[test]
    fn oc_body_file_yaml() {
        let yaml = "type: file\ndata:\n  - filePath: /uploads/doc.pdf\n    contentType: application/pdf\n    selected: true";
        let body: OcHttpRequestBody = serde_yaml::from_str(yaml).unwrap();
        match body {
            OcHttpRequestBody::File { data } => {
                assert_eq!(data[0].file_path, "/uploads/doc.pdf");
                assert!(data[0].selected);
            }
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn oc_body_variant_yaml() {
        let yaml = "title: Default\nselected: true\nbody:\n  type: json\n  data: '{}'";
        let variant: OcHttpRequestBodyVariant = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(variant.title, "Default");
        assert!(variant.selected);
    }

    #[test]
    fn oc_script_yaml() {
        let yaml = "type: before-request\ncode: \"console.log('hello')\"";
        let script: OcScript = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(script.script_type, "before-request");
        assert_eq!(script.code, "console.log('hello')");
    }

    #[test]
    fn oc_action_set_variable_yaml() {
        let yaml = "type: set-variable\nphase: after-response\nselector:\n  expression: res.body.token\n  method: jsonq\nvariable:\n  name: authToken\n  scope: collection";
        let action: OcAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            OcAction::SetVariable { phase, selector, variable, .. } => {
                assert_eq!(phase, "after-response");
                assert_eq!(selector.expression, "res.body.token");
                assert_eq!(variable.scope, "collection");
            }
        }
    }

    #[test]
    fn oc_runtime_yaml() {
        let yaml = "scripts:\n  - type: before-request\n    code: \"let x = 1;\"\nassertions:\n  - expression: res.status\n    operator: eq\n    value: \"200\"";
        let runtime: OcHttpRequestRuntime = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(runtime.scripts.len(), 1);
        assert_eq!(runtime.assertions.len(), 1);
    }

    #[test]
    fn oc_http_request_example_yaml() {
        let yaml = "name: Success\nrequest:\n  url: https://api.example.com/users\n  method: GET\nresponse:\n  status: 200\n  statusText: OK\n  body:\n    type: json\n    data: '{\"users\": []}'";
        let example: OcHttpRequestExample = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(example.name, "Success");
        assert!(example.request.is_some());
        let resp = example.response.unwrap();
        assert_eq!(resp.status, Some(200));
        assert_eq!(resp.body.unwrap().body_type, "json");
    }

    #[test]
    fn oc_http_request_full_yaml() {
        let yaml = r#"
info:
  name: Create User
  type: http
  seq: 1
  tags:
    - users
    - api
http:
  method: POST
  url: "https://api.example.com/users"
  headers:
    - name: Content-Type
      value: application/json
    - name: Authorization
      value: "Bearer {{token}}"
  params:
    - name: version
      value: "2"
      type: query
  body:
    type: json
    data: '{"name": "John", "email": "john@example.com"}'
  auth:
    type: bearer
    token: "{{authToken}}"
runtime:
  scripts:
    - type: before-request
      code: "bru.setVar('timestamp', Date.now())"
  assertions:
    - expression: res.status
      operator: eq
      value: "201"
  actions:
    - type: set-variable
      phase: after-response
      selector:
        expression: res.body.id
        method: jsonq
      variable:
        name: userId
        scope: collection
settings:
  encodeUrl: true
  timeout: 30000
examples:
  - name: Success
    response:
      status: 201
      body:
        type: json
        data: '{"id": "123", "name": "John"}'
docs: "Creates a new user in the system."
"#;
        let request: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(request.info.name, "Create User");
        assert_eq!(request.info.seq, Some(1));
        assert_eq!(request.info.tags, vec!["users", "api"]);
        assert_eq!(request.http.method, "POST");
        assert_eq!(request.http.headers.len(), 2);
        assert_eq!(request.http.params.len(), 1);
        assert!(request.http.body.is_some());
        assert!(request.http.auth.is_some());
        let runtime = request.runtime.unwrap();
        assert_eq!(runtime.scripts.len(), 1);
        assert_eq!(runtime.assertions.len(), 1);
        assert_eq!(runtime.actions.len(), 1);
        let settings = request.settings.unwrap();
        assert_eq!(settings.encode_url, Some(InheritableBoolean::Value(true)));
        assert_eq!(request.examples.as_ref().unwrap().len(), 1);
        assert_eq!(request.docs, Some("Creates a new user in the system.".into()));
    }

    #[test]
    fn oc_http_request_minimal_yaml() {
        let yaml = "info:\n  name: Simple GET\nhttp:\n  method: GET\n  url: https://example.com";
        let request: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(request.info.name, "Simple GET");
        assert_eq!(request.http.method, "GET");
        assert!(request.runtime.is_none());
        assert!(request.settings.is_none());
        assert!(request.examples.is_none());
    }

    #[test]
    fn oc_graphql_request_yaml() {
        let yaml = r#"
info:
  name: Get Users
  type: graphql
graphql:
  url: "https://api.example.com/graphql"
  body:
    query: "query { users { id name } }"
"#;
        let req: OcGraphQLRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(req.info.name, "Get Users");
        assert_eq!(req.graphql.url, "https://api.example.com/graphql");
    }

    #[test]
    fn oc_grpc_request_yaml() {
        let yaml = r#"
info:
  name: Get User
  type: grpc
grpc:
  url: "localhost:50051"
  method: "users.UserService/GetUser"
  methodType: unary
  protoFilePath: "./protos/users.proto"
  metadata:
    - name: authorization
      value: "Bearer token"
  message: '{"id": "123"}'
"#;
        let req: OcGrpcRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(req.info.name, "Get User");
        assert_eq!(req.grpc.method_type, Some("unary".into()));
        assert_eq!(req.grpc.metadata.len(), 1);
    }

    #[test]
    fn oc_websocket_request_yaml() {
        let yaml = r#"
info:
  name: Chat
  type: websocket
websocket:
  url: "wss://chat.example.com/ws"
  headers:
    - name: Authorization
      value: "Bearer token"
  message:
    type: json
    data: '{"action": "subscribe", "channel": "general"}'
"#;
        let req: OcWebSocketRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(req.info.name, "Chat");
        assert_eq!(req.websocket.url, "wss://chat.example.com/ws");
        assert!(req.websocket.message.is_some());
    }

    #[test]
    fn oc_script_file_yaml() {
        let yaml = "type: script\nscript: ./scripts/auth.js";
        let sf: OcScriptFile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(sf.script_file_type, "script");
        assert_eq!(sf.script, "./scripts/auth.js");
    }

    #[test]
    fn oc_runtime_with_auth_yaml() {
        let yaml = "scripts:\n  - type: before-request\n    code: \"let x = 1;\"\nauth:\n  type: bearer\n  token: my-token";
        let runtime: OcHttpRequestRuntime = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(runtime.scripts.len(), 1);
        assert!(runtime.auth.is_some());
    }

    #[test]
    fn oc_folder_yaml() {
        let yaml = r#"
info:
  name: Users
  type: folder
  seq: 1
items:
  - info:
      name: Get Users
      type: http
    http:
      method: GET
      url: "https://api.example.com/users"
  - info:
      name: Create User
      type: http
    http:
      method: POST
      url: "https://api.example.com/users"
"#;
        let folder: OcFolder = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(folder.info.name, "Users");
        assert_eq!(folder.items.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn oc_item_dispatch_http() {
        let yaml = r#"
info:
  name: Test
  type: http
http:
  method: GET
  url: https://example.com
"#;
        let item: OcItem = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(item, OcItem::Http(_)));
    }

    #[test]
    fn oc_item_dispatch_folder() {
        let yaml = r#"
info:
  name: My Folder
  type: folder
"#;
        let item: OcItem = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(item, OcItem::Folder(_)));
    }

    #[test]
    fn oc_collection_full_yaml() {
        let yaml = r##"
opencollection: "0.1"
info:
  name: My API
  summary: API collection for testing
  version: "1.0.0"
  authors:
    - name: John Doe
      email: john@example.com
config:
  environments:
    - name: production
      color: "#FF0000"
      variables:
        - name: BASE_URL
          value: https://api.example.com
    - name: staging
      variables:
        - name: BASE_URL
          value: https://staging.example.com
  proxy:
    disabled: false
    config:
      protocol: http
      hostname: proxy.example.com
      port: 8080
  protobuf:
    protoFiles:
      - filePath: ./protos/service.proto
    importPaths:
      - path: ./protos
request:
  headers:
    - name: Content-Type
      value: application/json
  auth:
    type: bearer
    token: "{{token}}"
  settings:
    timeout: 30000
    encodeUrl: true
items:
  - info:
      name: Users
      type: folder
    items:
      - info:
          name: Get Users
          type: http
        http:
          method: GET
          url: "{{BASE_URL}}/users"
docs: "Main API collection documentation."
"##;
        let collection: OcCollection = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(collection.opencollection, Some("0.1".into()));
        let info = collection.info.unwrap();
        assert_eq!(info.name, "My API");
        assert_eq!(info.authors.as_ref().unwrap().len(), 1);
        let config = collection.config.unwrap();
        assert_eq!(config.environments.as_ref().unwrap().len(), 2);
        assert!(config.proxy.is_some());
        assert!(config.protobuf.is_some());
        let request = collection.request.unwrap();
        assert!(request.headers.is_some());
        assert!(request.auth.is_some());
        assert!(request.settings.is_some());
        assert_eq!(collection.items.as_ref().unwrap().len(), 1);
        assert_eq!(collection.docs, Some("Main API collection documentation.".into()));
    }

    #[test]
    fn oc_collection_minimal_yaml() {
        let yaml = "opencollection: \"0.1\"\ninfo:\n  name: Simple";
        let collection: OcCollection = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(collection.info.unwrap().name, "Simple");
        assert!(collection.items.is_none());
    }

    #[test]
    fn oc_request_defaults_yaml() {
        let yaml = r#"
headers:
  - name: Accept
    value: application/json
auth:
  type: basic
  username: admin
  password: secret
settings:
  timeout: 5000
"#;
        let defaults: OcRequestDefaults = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(defaults.headers.as_ref().unwrap().len(), 1);
        assert!(defaults.auth.is_some());
        assert!(defaults.settings.is_some());
    }

    #[test]
    fn oc_environment_yaml() {
        let yaml = r##"
name: production
color: "#FF5733"
variables:
  - name: HOST
    value: api.example.com
extends: base
dotEnvFilePath: .env.prod
"##;
        let env: OcEnvironment = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(env.name, "production");
        assert_eq!(env.color, Some("#FF5733".into()));
        assert_eq!(env.extends, Some("base".into()));
        assert_eq!(env.dot_env_file_path, Some(".env.prod".into()));
        assert_eq!(env.variables.len(), 1);
    }

    #[test]
    fn oc_collection_with_uid_yaml() {
        let yaml = "opencollection: \"0.1\"\nuid: \"550e8400-e29b-41d4-a716-446655440000\"\ninfo:\n  name: My API";
        let col: OcCollection = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(col.uid, Some("550e8400-e29b-41d4-a716-446655440000".into()));
    }

    #[test]
    fn oc_folder_info_with_uid_yaml() {
        let yaml = "name: auth\nuid: abcd-1234\ntype: folder";
        let info: OcFolderInfo = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(info.uid, Some("abcd-1234".into()));
    }

    #[test]
    fn oc_workspace_config_roundtrip() {
        let yaml = r#"
opencollection: "1.0.0"
info:
  name: Acme API
  type: workspace
collections:
- name: main-api
  path: collections/main-api
- name: external
  path: /abs/path/to/ext
docs: Project description
environments:
  activeEnvironment: Staging
globalEnvironment: Production
"#;
        let cfg: OcWorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.info.name, "Acme API");
        assert_eq!(cfg.collections.len(), 2);
        assert_eq!(cfg.collections[0].name, "main-api");
        assert_eq!(cfg.docs.as_deref(), Some("Project description"));
        assert_eq!(cfg.environments.as_ref().unwrap().active_environment.as_deref(), Some("Staging"));
        assert_eq!(cfg.global_environment.as_deref(), Some("Production"));
        // Roundtrip
        let back: OcWorkspaceConfig = serde_yaml::from_str(&serde_yaml::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(cfg, back);
        // Verify camelCase output for Bruno interop
        let yaml_out = serde_yaml::to_string(&cfg).unwrap();
        assert!(yaml_out.contains("globalEnvironment:"), "spec requires camelCase, got:\n{yaml_out}");
        assert!(yaml_out.contains("activeEnvironment:"), "spec requires camelCase, got:\n{yaml_out}");
        assert!(!yaml_out.contains("global_environment:"), "snake_case is not spec-compliant");
    }

    #[test]
    fn oc_workspace_config_requires_info_field() {
        // Old format (no `info:` key) must fail to parse as OcWorkspaceConfig.
        let old_yaml = "name: My Workspace\ncollections: []\n";
        let result = serde_yaml::from_str::<OcWorkspaceConfig>(old_yaml);
        assert!(result.is_err(), "old format should not parse as OcWorkspaceConfig");
    }
}
