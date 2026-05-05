use super::*;
use crate::oc::*;
use rocket_collection::collection::Collection;
use rocket_collection::folder::CollectionItem;
use rocket_collection::settings::CollectionVariable;
use rocket_collection::Request;
use rocket_environment::environment::Environment;
use rocket_environment::variable::Variable;
use rocket_shared::description::Description;
use rocket_shared::types::{Auth, Body, BodyMode, Header, HttpMethod, PathParam, QueryParam, RequestSettingValue};
use rocket_shared::variable_value::VariableValue;

#[test]
fn header_oc_to_domain() {
    let oc = OcHttpRequestHeader {
        name: "Content-Type".into(),
        value: "application/json".into(),
        description: Some(Description::text("Content type")),
        disabled: Some(true),
    };
    let h: Header = oc.into();
    assert_eq!(h.key, "Content-Type");
    assert!(!h.enabled);
    assert!(h.description.is_some());
}

#[test]
fn header_domain_to_oc() {
    let h = Header {
        key: "Accept".into(),
        value: "text/html".into(),
        enabled: true,
        description: None,
    };
    let oc: OcHttpRequestHeader = h.into();
    assert_eq!(oc.name, "Accept");
    assert_eq!(oc.disabled, None);  // Enabled → no disabled field.
}

#[test]
fn header_roundtrip() {
    let original = Header {
        key: "X-Custom".into(),
        value: "val".into(),
        enabled: false,
        description: Some(Description::text("Custom header")),
    };
    let oc: OcHttpRequestHeader = original.clone().into();
    let back: Header = oc.into();
    assert_eq!(original, back);
}

#[test]
fn param_split_by_type() {
    let params = vec![
        OcHttpRequestParam {
            name: "page".into(),
            value: "1".into(),
            description: None,
            param_type: Some("query".into()),
            disabled: None,
        },
        OcHttpRequestParam {
            name: "id".into(),
            value: "42".into(),
            description: None,
            param_type: Some("path".into()),
            disabled: None,
        },
        OcHttpRequestParam {
            name: "limit".into(),
            value: "10".into(),
            description: None,
            param_type: Some("query".into()),
            disabled: Some(true),
        },
    ];
    let (query, path) = split_params(params);
    assert_eq!(query.len(), 2);
    assert_eq!(path.len(), 1);
    assert_eq!(query[0].key, "page");
    assert!(query[0].enabled);
    assert!(!query[1].enabled);  // disabled: true → enabled: false.
    assert_eq!(path[0].name, "id");
}

#[test]
fn param_merge_roundtrip() {
    let query = vec![QueryParam {
        key: "q".into(),
        value: "search".into(),
        enabled: true,
        description: None,
    }];
    let path = vec![PathParam {
        name: "id".into(),
        value: "1".into(),
        description: None,
    }];
    let merged = merge_params(&query, &path);
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].param_type, Some("query".into()));
    assert_eq!(merged[1].param_type, Some("path".into()));
}

#[test]
fn param_default_type_is_query() {
    let params = vec![OcHttpRequestParam {
        name: "x".into(),
        value: "1".into(),
        description: None,
        param_type: None,
        disabled: None,
    }];
    let (query, path) = split_params(params);
    assert_eq!(query.len(), 1);
    assert_eq!(path.len(), 0);
}

// ---- Body tests ----

#[test]
fn body_json_oc_to_domain() {
    let oc = OcHttpRequestBody::Json { data: r#"{"key":"val"}"#.into() };
    let body: Body = oc.into();
    assert_eq!(body.mode, BodyMode::Json);
    assert_eq!(body.content.unwrap(), r#"{"key":"val"}"#);
}

#[test]
fn body_sparql_roundtrip() {
    let oc = OcHttpRequestBody::Sparql { data: "SELECT ?s WHERE { ?s ?p ?o }".into() };
    let body: Body = oc.into();
    assert_eq!(body.mode, BodyMode::Sparql);
    assert_eq!(body.content.as_deref(), Some("SELECT ?s WHERE { ?s ?p ?o }"));
    let back: OcHttpRequestBody = body.into();
    assert!(matches!(back, OcHttpRequestBody::Sparql { ref data } if data == "SELECT ?s WHERE { ?s ?p ?o }"));
}

#[test]
fn body_form_urlencoded_oc_to_domain() {
    let oc = OcHttpRequestBody::FormUrlEncoded { data: vec![
        OcFormField { name: "user".into(), value: "admin".into(), description: None, disabled: None },
        OcFormField { name: "pass".into(), value: "secret".into(), description: None, disabled: Some(true) },
    ]};
    let body: Body = oc.into();
    assert_eq!(body.mode, BodyMode::FormUrlEncoded);
    let fd = body.form_data.unwrap();
    assert_eq!(fd.len(), 2);
    assert_eq!(fd[0].key, "user");
    assert!(fd[0].enabled);
    assert!(!fd[1].enabled);
}

// ---- Auth tests ----

#[test]
fn auth_basic_oc_to_domain() {
    let oc = OcAuth::Typed(OcAuthTyped::Basic { username: "u".into(), password: "p".into() });
    let auth: Auth = oc.into();
    assert_eq!(auth, Auth::Basic { username: "u".into(), password: "p".into() });
}

#[test]
fn auth_inherit_oc_to_domain() {
    let oc = OcAuth::Inherit("inherit".into());
    let auth: Auth = oc.into();
    assert_eq!(auth, Auth::Inherit);
}

#[test]
fn auth_awsv4_oc_to_domain() {
    let oc = OcAuth::Typed(OcAuthTyped::AwsV4 {
        access_key_id: "AK".into(), secret_access_key: "SK".into(),
        region: Some("us-east-1".into()), service: Some("s3".into()),
        session_token: None, profile_name: None,
    });
    let auth: Auth = oc.into();
    match auth {
        Auth::AwsSigV4 { access_key, secret_key, region, service, .. } => {
            assert_eq!(access_key, "AK");
            assert_eq!(secret_key, "SK");
            assert_eq!(region, "us-east-1");
            assert_eq!(service, "s3");
        }
        _ => panic!("expected AwsSigV4"),
    }
}

#[test]
fn auth_oauth2_client_credentials_oc_to_domain() {
    let oc = OcAuth::Typed(OcAuthTyped::OAuth2 {
        flow: "client_credentials".into(),
        access_token_url: Some("https://auth.example.com/token".into()),
        refresh_token_url: None,
        authorization_url: None,
        callback_url: None,
        credentials: Some(OcOAuth2Credentials { client_id: "id".into(), client_secret: "s".into(), placement: None }),
        resource_owner: None,
        scope: Some("read".into()),
        state: None,
        pkce: None,
        additional_parameters: None,
        token_config: None,
        settings: None,
    });
    let auth: Auth = oc.into();
    match auth {
        Auth::OAuth2(flow) => {
            assert!(matches!(flow, rocket_shared::oauth2::OAuth2Flow::ClientCredentials { .. }));
        }
        _ => panic!("expected OAuth2"),
    }
}

// ---- CollectionVariable tests ----

#[test]
fn collection_variable_save_persists_current_value_in_value_field() {
    // Scenario: user has both a current value and an initial value set.
    // Both are saved separately: value → YAML `value`, initial_value → YAML `initial`.
    let cv = CollectionVariable {
        key: "HOST".into(),
        value: "http://production.com".into(),
        initial_value: "http://localhost".into(),
        enabled: true,
        secret: false,
    };
    let oc = OcVariable::from(cv);
    assert_eq!(oc.value.as_ref().map(|v| v.data()), Some("http://production.com"),
        "YAML `value` field should store the current value.");
    assert_eq!(oc.initial.as_ref().map(|v| v.data()), Some("http://localhost"),
        "YAML `initial` field should store the initial value.");
}

#[test]
fn collection_variable_save_omits_value_when_empty() {
    // Scenario: user only filled in the initial value, leaving current value blank.
    // The OcVariable struct should have value=None and initial=Some(...).
    let cv = CollectionVariable {
        key: "HOST".into(),
        value: "".into(),
        initial_value: "http://localhost".into(),
        enabled: true,
        secret: false,
    };
    let oc = OcVariable::from(cv);
    assert_eq!(oc.value, None,
        "YAML `value` should be absent when current value is empty.");
    assert_eq!(oc.initial.as_ref().map(|v| v.data()), Some("http://localhost"),
        "YAML `initial` should be set from initial_value.");
}

#[test]
fn collection_variable_roundtrip_preserves_both_fields_distinct() {
    // After save/load both initial_value and value must be distinct and correct.
    let cv = CollectionVariable {
        key: "BASE_URL".into(),
        value: "http://production.com".into(),
        initial_value: "http://localhost:8080".into(),
        enabled: true,
        secret: false,
    };
    let oc = OcVariable::from(cv);
    let back = CollectionVariable::from(oc);
    assert_eq!(back.value, "http://production.com",
        "current value must round-trip correctly.");
    assert_eq!(back.initial_value, "http://localhost:8080",
        "initial value must round-trip correctly.");
}

#[test]
fn collection_variable_backward_compat_old_yaml_without_initial() {
    // Old YAML files only have `value`; `initial` is absent.
    // On load, initial_value should fall back to the value field.
    let oc = OcVariable {
        name: "status".into(),
        value: Some(VariableValue::simple("2")),
        initial: None,
        description: None,
        disabled: None,
    };
    let cv = CollectionVariable::from(oc);
    assert_eq!(cv.value, "2", "current value should be loaded from YAML `value`.");
    assert_eq!(cv.initial_value, "2",
        "initial_value should fall back to `value` when `initial` is absent.");
}

#[test]
fn collection_variable_both_values_distinct_yaml_roundtrip() {
    // Full roundtrip: initial="default", current="override" →
    // YAML has both fields → loaded back with both fields distinct.
    let cv = CollectionVariable {
        key: "status".into(),
        value: "override".into(),
        initial_value: "default".into(),
        enabled: true,
        secret: false,
    };
    let oc = OcVariable::from(cv);
    // Verify YAML struct has both fields.
    assert_eq!(oc.value.as_ref().map(|v| v.data()), Some("override"));
    assert_eq!(oc.initial.as_ref().map(|v| v.data()), Some("default"));
    // Verify YAML string contains both fields.
    let yaml_str = serde_yaml::to_string(&oc).unwrap();
    assert!(yaml_str.contains("value:"), "YAML must contain `value` field.");
    assert!(yaml_str.contains("initial:"), "YAML must contain `initial` field.");
    // Verify loading back produces correct distinct values.
    let back = CollectionVariable::from(oc);
    assert_eq!(back.value, "override");
    assert_eq!(back.initial_value, "default");
}

// ---- Variable tests ----

#[test]
fn variable_oc_to_domain() {
    let oc = OcVariable {
        name: "HOST".into(),
        value: Some(VariableValue::simple("localhost")),
        initial: None,
        description: Some(Description::text("Server host")),
        disabled: Some(true),
    };
    let v: Variable = oc.into();
    assert_eq!(v.key, "HOST");
    assert_eq!(v.value, "localhost");
    assert!(!v.enabled);
    assert!(!v.secret);
    assert!(v.description.is_some());
}

#[test]
fn variable_domain_to_oc() {
    let v = Variable::new("BASE_URL", "https://api.example.com");
    let oc: OcVariable = v.into();
    assert_eq!(oc.name, "BASE_URL");
    assert!(oc.value.is_some());
    assert_eq!(oc.disabled, None);
}

#[test]
fn secret_variable_oc_to_domain() {
    let oc = OcSecretVariable {
        secret: true,
        name: "API_KEY".into(),
        description: None,
        disabled: None,
        secret_type: Some("string".into()),
    };
    let v: Variable = oc.into();
    assert_eq!(v.key, "API_KEY");
    assert!(v.secret);
    assert!(v.enabled);
    assert_eq!(v.secret_type, Some("string".into()));
}

#[test]
fn environment_oc_to_domain() {
    let oc = OcEnvironment {
        name: "production".into(),
        color: Some("#FF0000".into()),
        description: Some(Description::text("Prod env")),
        variables: vec![
            OcVariable { name: "HOST".into(), value: Some(VariableValue::simple("api.prod.com")), initial: None, description: None, disabled: None },
        ],
        client_certificates: Vec::new(),
        extends: Some("base".into()),
        dot_env_file_path: Some(".env.prod".into()),
    };
    let env: Environment = oc.into();
    assert_eq!(env.name, "production");
    assert_eq!(env.color, Some("#FF0000".into()));
    assert_eq!(env.variables.len(), 1);
    assert_eq!(env.variables[0].key, "HOST");
    assert_eq!(env.extends, Some("base".into()));
}

#[test]
fn environment_roundtrip() {
    let original = Environment {
        name: "staging".into(),
        variables: vec![Variable::new("URL", "https://staging.example.com")],
        color: Some("#00FF00".into()),
        description: None,
        extends: None,
        dot_env_file_path: None,
        client_certificates: Vec::new(),
    };
    let oc: OcEnvironment = original.clone().into();
    let back: Environment = oc.into();
    assert_eq!(original.name, back.name);
    assert_eq!(original.color, back.color);
    assert_eq!(original.variables.len(), back.variables.len());
    assert_eq!(original.variables[0].key, back.variables[0].key);
}

// ---- OcHttpRequest ↔ Request tests ----

#[test]
fn oc_http_request_to_domain_basic() {
    let yaml = r#"
info:
  name: Get Users
  type: http
  seq: 1
  tags:
    - api
http:
  method: GET
  url: "https://api.example.com/users"
  headers:
    - name: Accept
      value: application/json
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    assert_eq!(req.name, "Get Users");
    assert_eq!(req.method, HttpMethod::Get);
    assert_eq!(req.url, "https://api.example.com/users");
    assert_eq!(req.headers.len(), 1);
    assert_eq!(req.headers[0].key, "Accept");
    assert_eq!(req.seq, Some(1));
    assert_eq!(req.tags, vec!["api"]);
}

#[test]
fn oc_http_request_with_runtime() {
    let yaml = r#"
info:
  name: Test
  type: http
http:
  method: POST
  url: "https://api.example.com"
runtime:
  scripts:
    - type: before-request
      code: "let x = 1;"
    - type: after-response
      code: "console.log(res.status);"
    - type: tests
      code: "expect(res.status).to.equal(200);"
  assertions:
    - expression: res.status
      operator: eq
      value: "200"
  actions:
    - type: set-variable
      phase: after-response
      selector:
        expression: res.body.token
        method: jsonq
      variable:
        name: authToken
        scope: collection
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    assert_eq!(req.pre_request_script, Some("let x = 1;".into()));
    assert_eq!(req.post_response_script, Some("console.log(res.status);".into()));
    assert_eq!(req.tests, Some("expect(res.status).to.equal(200);".into()));
    assert_eq!(req.assertions.len(), 1);
    assert_eq!(req.actions.len(), 1);
    assert_eq!(req.actions[0].variable.scope, "collection");
}

// ---- ProtocolRequest lossless roundtrip tests ----

#[test]
fn graphql_lossless_roundtrip() {
    let yaml = r#"
info:
  name: Get Users
  type: graphql
graphql:
  url: "https://api.example.com/graphql"
  body:
    query: "query { users { id name } }"
"#;
    let oc_item: OcItem = serde_yaml::from_str(yaml).unwrap();
    let pr = oc_item_to_protocol_request(oc_item).unwrap();
    assert!(matches!(pr, ProtocolRequest::GraphQL(_)));
    let back = protocol_request_to_oc_item(pr).unwrap();
    match back {
        OcItem::GraphQL(gql) => {
            assert_eq!(gql.info.name, "Get Users");
            assert_eq!(gql.graphql.url, "https://api.example.com/graphql");
        }
        _ => panic!("expected GraphQL"),
    }
}

#[test]
fn grpc_lossless_roundtrip() {
    let yaml = r#"
info:
  name: Get User
  type: grpc
grpc:
  url: "localhost:50051"
  method: "users.UserService/GetUser"
  methodType: unary
"#;
    let oc_item: OcItem = serde_yaml::from_str(yaml).unwrap();
    let pr = oc_item_to_protocol_request(oc_item).unwrap();
    assert!(matches!(pr, ProtocolRequest::Grpc(_)));
    let back = protocol_request_to_oc_item(pr).unwrap();
    assert!(matches!(back, OcItem::Grpc(_)));
}

#[test]
fn websocket_lossless_roundtrip() {
    let yaml = r#"
info:
  name: Chat
  type: websocket
websocket:
  url: "wss://chat.example.com/ws"
  message:
    type: json
    data: '{"action": "subscribe"}'
"#;
    let oc_item: OcItem = serde_yaml::from_str(yaml).unwrap();
    let pr = oc_item_to_protocol_request(oc_item).unwrap();
    assert!(matches!(pr, ProtocolRequest::WebSocket(_)));
    let back = protocol_request_to_oc_item(pr).unwrap();
    assert!(matches!(back, OcItem::WebSocket(_)));
}

#[test]
fn http_item_converts_to_domain() {
    let yaml = r#"
info:
  name: Simple GET
  type: http
http:
  method: GET
  url: https://example.com
"#;
    let oc_item: OcItem = serde_yaml::from_str(yaml).unwrap();
    let pr = oc_item_to_protocol_request(oc_item).unwrap();
    match pr {
        ProtocolRequest::Http(req) => {
            assert_eq!(req.name, "Simple GET");
            assert_eq!(req.method, HttpMethod::Get);
        }
        _ => panic!("expected Http"),
    }
}

#[test]
fn domain_request_to_oc_roundtrip() {
    let yaml = r#"
info:
  name: Create User
  type: http
  seq: 5
http:
  method: POST
  url: "https://api.example.com/users"
  headers:
    - name: Content-Type
      value: application/json
  body:
    type: json
    data: '{"name": "John"}'
  auth:
    type: bearer
    token: my-token
runtime:
  scripts:
    - type: before-request
      code: "let x = 1;"
  assertions:
    - expression: res.status
      operator: eq
      value: "201"
docs: "Creates a user."
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    let back = request_to_oc_http_request(&req);
    assert_eq!(back.info.name, "Create User");
    assert_eq!(back.info.seq, Some(5));
    assert_eq!(back.http.method, "POST");
    assert!(back.http.body.is_some());
    assert!(back.http.auth.is_some());
    assert!(back.runtime.is_some());
    let rt = back.runtime.unwrap();
    assert_eq!(rt.scripts.len(), 1);
    assert_eq!(rt.assertions.len(), 1);
    assert_eq!(back.docs, Some("Creates a user.".into()));
}

// ---- Folder + Collection tests ----

#[test]
fn oc_folder_to_domain() {
    let yaml = r#"
info:
  name: Users
  type: folder
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
    let oc: OcFolder = serde_yaml::from_str(yaml).unwrap();
    let folder = oc_folder_to_folder(oc);
    assert_eq!(folder.name, "Users");
    assert_eq!(folder.items.len(), 2);
    assert!(matches!(&folder.items[0], CollectionItem::Request(r) if r.name == "Get Users"));
    assert!(matches!(&folder.items[1], CollectionItem::Request(r) if r.name == "Create User"));
}

#[test]
fn oc_collection_to_domain() {
    let yaml = r#"
opencollection: "0.1"
info:
  name: My API
request:
  headers:
    - name: Accept
      value: application/json
  auth:
    type: bearer
    token: "{{token}}"
items:
  - info:
      name: Health Check
      type: http
    http:
      method: GET
      url: /health
"#;
    let oc: OcCollection = serde_yaml::from_str(yaml).unwrap();
    let col = oc_collection_to_collection(oc);
    assert_eq!(col.name, "My API");
    assert_eq!(col.root.items.len(), 1);
    assert_eq!(col.settings.headers.len(), 1);
    assert_eq!(col.settings.headers[0].key, "Accept");
    assert!(col.settings.auth.is_some());
}

#[test]
fn collection_roundtrip() {
    use rocket_shared::types::HttpMethod;

    let mut col = Collection::new("Test API");
    col.root
        .add_request(Request::new("Get Users", HttpMethod::Get, "/users"));
    col.settings
        .headers
        .push(Header::new("Accept", "application/json"));

    let oc = collection_to_oc_collection(col);
    assert_eq!(oc.info.as_ref().unwrap().name, "Test API");
    assert!(oc.items.is_some());
    assert_eq!(oc.items.as_ref().unwrap().len(), 1);
    assert!(oc.request.is_some());

    let back = oc_collection_to_collection(oc);
    assert_eq!(back.name, "Test API");
    assert_eq!(back.root.items.len(), 1);
    assert_eq!(back.settings.headers.len(), 1);
}

#[test]
fn params_survive_roundtrip() {
    let yaml = r#"
info:
  name: Parameterised
  type: http
http:
  method: GET
  url: "https://api.example.com/users/:id"
  params:
    - name: page
      value: "1"
      type: query
      description: Page number
    - name: id
      value: "42"
      type: path
    - name: limit
      value: "10"
      type: query
      disabled: true
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    assert_eq!(req.query_params.len(), 2);
    assert_eq!(req.path_params.len(), 1);
    assert_eq!(req.query_params[0].key, "page");
    assert!(req.query_params[0].description.is_some());
    assert!(!req.query_params[1].enabled);
    assert_eq!(req.path_params[0].name, "id");

    let back = request_to_oc_http_request(&req);
    assert_eq!(back.http.params.len(), 3);
    assert_eq!(back.http.params[0].param_type, Some("query".into()));
    assert_eq!(back.http.params[2].param_type, Some("path".into()));
}

#[test]
fn runtime_auth_survives_roundtrip() {
    let yaml = r#"
info:
  name: Runtime Auth
  type: http
http:
  method: GET
  url: "https://api.example.com"
runtime:
  auth:
    type: bearer
    token: runtime-token
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    assert!(req.runtime_auth.is_some());
    match req.runtime_auth.as_ref().unwrap() {
        Auth::Bearer { token } => assert_eq!(token, "runtime-token"),
        _ => panic!("expected Bearer"),
    }

    let back = request_to_oc_http_request(&req);
    let rt = back.runtime.unwrap();
    assert!(rt.auth.is_some());
}

#[test]
fn oauth2_typed_subfields_roundtrip() {
    let yaml = r#"
info:
  name: OAuth2 Test
  type: http
http:
  method: GET
  url: "https://api.example.com"
  auth:
    type: oauth2
    flow: client_credentials
    accessTokenUrl: "https://auth.example.com/token"
    credentials:
      clientId: my-id
      clientSecret: my-secret
    additionalParameters:
      accessTokenRequest:
        - name: audience
          value: "https://api.example.com"
    tokenConfig:
      id: my-token
      placement:
        header: Authorization
    settings:
      autoFetchToken: true
      autoRefreshToken: false
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    match &req.auth {
        Auth::OAuth2(flow) => {
            assert!(matches!(flow, rocket_shared::oauth2::OAuth2Flow::ClientCredentials { .. }));
        }
        _ => panic!("expected OAuth2"),
    }
    let back = request_to_oc_http_request(&req);
    let auth = back.http.auth.unwrap();
    match auth {
        OcAuth::Typed(OcAuthTyped::OAuth2 { additional_parameters, token_config, settings, .. }) => {
            assert!(additional_parameters.is_some());
            assert!(token_config.is_some());
            assert!(settings.is_some());
        }
        _ => panic!("expected OAuth2"),
    }
}

#[test]
fn oauth2_auth_code_full_roundtrip() {
    use rocket_shared::oauth2::{
        OAuth2AdditionalParameter, OAuth2AdditionalParameters, OAuth2ClientCredentials,
        OAuth2Flow, OAuth2PKCE, OAuth2Settings, OAuth2TokenConfig, OAuth2TokenPlacement,
    };

    let original = Auth::OAuth2(OAuth2Flow::AuthorizationCode {
        authorization_url: "https://auth.example.com/authorize".into(),
        access_token_url: "https://auth.example.com/token".into(),
        refresh_token_url: Some("https://auth.example.com/refresh".into()),
        callback_url: Some("https://jwt.io/".into()),
        credentials: OAuth2ClientCredentials {
            client_id: "my-client".into(),
            client_secret: "my-secret".into(),
            placement: Some("basic_auth_header".into()),
        },
        scope: Some("openid email".into()),
        state: Some("random-state".into()),
        pkce: Some(OAuth2PKCE {
            enabled: true,
            method: Some("S256".into()),
        }),
        additional_parameters: Some(OAuth2AdditionalParameters {
            authorization_request: Some(vec![OAuth2AdditionalParameter {
                name: "nonce".into(),
                value: "abc123".into(),
                placement: Some("query".into()),
            }]),
            access_token_request: Some(vec![OAuth2AdditionalParameter {
                name: "audience".into(),
                value: "api/v1".into(),
                placement: Some("body".into()),
            }]),
            refresh_token_request: None,
        }),
        token_config: Some(OAuth2TokenConfig {
            id: Some("my-token".into()),
            source: None,
            placement: Some(OAuth2TokenPlacement::Header {
                header: "Authorization".into(),
            }),
        }),
        settings: Some(OAuth2Settings {
            auto_fetch_token: Some(true),
            auto_refresh_token: Some(false),
            verify_ssl: Some(true),
            use_system_browser: None,
        }),
    });

    let oc: OcAuth = original.clone().into();
    let back: Auth = oc.into();
    assert_eq!(original, back);
}

#[test]
fn settings_survive_roundtrip() {
    let yaml = r#"
info:
  name: With Settings
  type: http
http:
  method: GET
  url: "https://api.example.com"
settings:
  encodeUrl: true
  timeout: 30000
  followRedirects: inherit
  maxRedirects: 5
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    let s = req.settings.as_ref().unwrap();
    assert!(matches!(s.encode_url, Some(RequestSettingValue::Value(true))));
    assert!(matches!(s.follow_redirects, Some(RequestSettingValue::Inherit(_))));

    let back = request_to_oc_http_request(&req);
    let os = back.settings.unwrap();
    assert_eq!(os.encode_url, Some(InheritableBoolean::Value(true)));
    assert_eq!(os.timeout, Some(InheritableNumber::Value(30000.0)));
    assert_eq!(os.follow_redirects, Some(InheritableBoolean::Inherit("inherit".into())));
    assert_eq!(os.max_redirects, Some(InheritableNumber::Value(5.0)));
}

#[test]
fn multipart_metadata_preserved_in_roundtrip() {
    let part = OcMultipartFormPart {
        name: "avatar".into(),
        part_type: "file".into(),
        value: OcMultipartValue::Single("/tmp/avatar.png".into()),
        description: Some(Description::text("User avatar")),
        content_type: Some("image/png".into()),
        disabled: None,
    };
    // Call through the body module's private functions via the From impls.
    let body = Body {
        mode: BodyMode::FormData,
        content: None,
        form_data: Some(vec![{
            use rocket_shared::types::{FormDataEntry, FormDataType};
            FormDataEntry {
                key: "avatar".into(),
                value: "/tmp/avatar.png".into(),
                entry_type: FormDataType::File,
                enabled: true,
                content_type: Some("image/png".into()),
                description: Some(Description::text("User avatar")),
            }
        }]),
        file_path: None,
    };
    let oc_body: OcHttpRequestBody = body.into();
    match oc_body {
        OcHttpRequestBody::MultipartForm { data } => {
            assert_eq!(data[0].content_type, Some("image/png".into()));
            assert_eq!(data[0].description, Some(Description::text("User avatar")));
            let _ = part; // Suppress unused warning.
        }
        _ => panic!("expected MultipartForm"),
    }
}

#[test]
fn non_http_items_preserved_in_folder_roundtrip() {
    let yaml = r#"
info:
  name: Mixed
  type: folder
items:
  - info:
      name: Get Users
      type: http
    http:
      method: GET
      url: "https://api.example.com/users"
  - info:
      name: GQL Query
      type: graphql
    graphql:
      url: "https://api.example.com/graphql"
      body:
        query: "query { users { id } }"
"#;
    let oc: OcFolder = serde_yaml::from_str(yaml).unwrap();
    let folder = oc_folder_to_folder(oc);
    assert_eq!(folder.items.len(), 2);
    assert!(matches!(&folder.items[0], CollectionItem::Request(_)));
    assert!(matches!(&folder.items[1], CollectionItem::OpaqueItem(o) if o.protocol == "graphql"));

    let back = folder_to_oc_folder(folder);
    let items = back.items.unwrap();
    assert_eq!(items.len(), 2);
    assert!(matches!(&items[0], OcItem::Http(_)));
    assert!(matches!(&items[1], OcItem::GraphQL(_)));
}

#[test]
fn body_multipart_form_uses_formdata_mode() {
    let oc = OcHttpRequestBody::MultipartForm { data: vec![
        OcMultipartFormPart {
            name: "file".into(),
            part_type: "file".into(),
            value: OcMultipartValue::Single("/tmp/test.txt".into()),
            description: None,
            content_type: Some("text/plain".into()),
            disabled: None,
        },
    ]};
    let body: Body = oc.into();
    assert_eq!(body.mode, BodyMode::FormData);
}

#[test]
fn body_formurlencoded_roundtrip() {
    use rocket_shared::types::{FormDataEntry, FormDataType};
    let body = Body {
        mode: BodyMode::FormUrlEncoded,
        content: None,
        form_data: Some(vec![FormDataEntry {
            key: "user".into(),
            value: "admin".into(),
            entry_type: FormDataType::Text,
            enabled: true,
            content_type: None,
            description: None,
        }]),
        file_path: None,
    };
    let oc: OcHttpRequestBody = body.into();
    assert!(matches!(oc, OcHttpRequestBody::FormUrlEncoded { .. }));
}

#[test]
fn body_formdata_roundtrip_emits_multipart() {
    use rocket_shared::types::{FormDataEntry, FormDataType};
    let body = Body {
        mode: BodyMode::FormData,
        content: None,
        form_data: Some(vec![FormDataEntry {
            key: "name".into(),
            value: "test".into(),
            entry_type: FormDataType::Text,
            enabled: true,
            content_type: None,
            description: None,
        }]),
        file_path: None,
    };
    let oc: OcHttpRequestBody = body.into();
    assert!(matches!(oc, OcHttpRequestBody::MultipartForm { .. }));
}

#[test]
fn collection_to_oc_has_correct_version() {
    use rocket_collection::Collection;
    use rocket_collection::CollectionSettings;
    use rocket_collection::Folder;
    let col = Collection {
        name: "Test".into(),
        root: Folder { uid: "uid".into(), name: "Test".into(), dir_name: None, items: vec![] },
        settings: CollectionSettings::default(),
    };
    let oc = super::collection_to_oc_collection(col);
    assert_eq!(oc.opencollection.as_deref(), Some("1.0.0"));
}

// ---- docs_roundtrip_tests ----

#[test]
fn collection_docs_roundtrips_through_docs_field() {
    use crate::oc::OcCollection;

    let oc = OcCollection {
        opencollection: Some("1.0.0".into()),
        uid: None,
        info: None,
        config: None,
        items: None,
        request: None,
        docs: Some("# Hello\nWorld".into()),
        bundled: None,
        extensions: None,
    };

    let col = oc_collection_to_collection(oc);
    assert_eq!(col.settings.docs, Some("# Hello\nWorld".into()));

    let oc2 = collection_to_oc_collection(col);
    assert_eq!(oc2.docs, Some("# Hello\nWorld".into()));
}

// ---- workspace_conversion_tests ----

#[test]
fn workspace_config_to_oc_workspace_config() {
    use rocket_workspace::WorkspaceConfig;
    use std::path::PathBuf;

    let mut cfg = WorkspaceConfig::new("My API");
    cfg.description = Some("A great API".into());
    cfg.add_embedded_collection("users");
    cfg.add_external_collection("shared", PathBuf::from("/abs/path/shared"));
    cfg.environments.active_environment = Some("Production".into());
    cfg.global_environment = Some("Prod Global".into());

    let oc = OcWorkspaceConfig::from(cfg);
    assert_eq!(oc.opencollection.as_deref(), Some("1.0.0"));
    assert_eq!(oc.info.name, "My API");
    assert_eq!(oc.info.workspace_type.as_deref(), Some("workspace"));
    assert_eq!(oc.docs.as_deref(), Some("A great API"));
    assert_eq!(oc.collections.len(), 2);
    // Embedded → relative path collections/<name>
    assert_eq!(oc.collections[0].path, Some(PathBuf::from("collections/users")));
    // External → absolute path preserved
    assert_eq!(oc.collections[1].path, Some(PathBuf::from("/abs/path/shared")));
    assert_eq!(oc.environments.as_ref().unwrap().active_environment.as_deref(), Some("Production"));
    assert_eq!(oc.global_environment.as_deref(), Some("Prod Global"));
}

#[test]
fn oc_workspace_config_to_workspace_config() {
    use rocket_workspace::{CollectionRefType, WorkspaceConfig};
    use std::path::PathBuf;

    let oc = OcWorkspaceConfig {
        opencollection: Some("1.0.0".into()),
        info: OcWorkspaceInfo { name: "Acme".into(), workspace_type: Some("workspace".into()) },
        collections: vec![
            OcWorkspaceCollectionRef { name: "api".into(), path: Some(PathBuf::from("collections/api")) },
            OcWorkspaceCollectionRef { name: "ext".into(), path: Some(PathBuf::from("/abs/ext")) },
        ],
        docs: Some("Docs here".into()),
        environments: Some(OcWorkspaceEnvironments { active_environment: Some("Staging".into()) }),
        global_environment: Some("Global".into()),
    };
    let cfg = WorkspaceConfig::from(oc);
    assert_eq!(cfg.name, "Acme");
    assert_eq!(cfg.description.as_deref(), Some("Docs here"));
    assert_eq!(cfg.collections.len(), 2);
    // Relative path → Embedded
    assert_eq!(cfg.collections[0].ref_type, CollectionRefType::Embedded);
    // Absolute path → External
    assert_eq!(cfg.collections[1].ref_type, CollectionRefType::External);
    assert_eq!(cfg.environments.active_environment.as_deref(), Some("Staging"));
    assert_eq!(cfg.global_environment.as_deref(), Some("Global"));
}

#[test]
fn oc_request_missing_uid_gets_empty_not_minted() {
    use crate::oc::{OcHttpRequestDetails, OcHttpRequestInfo};

    let make_oc = || crate::oc::OcHttpRequest {
        uid: None,
        info: OcHttpRequestInfo {
            name: "No UID".into(),
            description: None,
            request_type: Some("http".into()),
            seq: None,
            tags: vec![],
        },
        http: OcHttpRequestDetails {
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: vec![],
            params: vec![],
            body: None,
            auth: None,
        },
        runtime: None,
        settings: None,
        examples: None,
        docs: None,
    };

    let req1 = oc_http_request_to_request(make_oc());
    let req2 = oc_http_request_to_request(make_oc());

    // Both calls must return the same (empty) uid — not two different minted uids.
    assert_eq!(req1.uid, req2.uid, "uid must be stable across loads");
    // The uid must be empty — not a freshly-minted UUID.
    assert!(req1.uid.is_empty(), "expected empty uid for missing uid field, got: {}", req1.uid);
}

#[test]
fn request_variables_survive_oc_roundtrip() {
    use rocket_collection::settings::CollectionVariable;
    let mut req = Request::new("Vars", HttpMethod::Get, "https://example.com");
    req.variables = vec![
        CollectionVariable {
            key: "token".to_string(),
            value: "abc".to_string(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        },
        CollectionVariable {
            key: "disabled_var".to_string(),
            value: "nope".to_string(),
            initial_value: String::new(),
            enabled: false,
            secret: false,
        },
    ];
    let oc = request_to_oc_http_request(&req);
    let back = oc_http_request_to_request(oc);
    assert_eq!(back.variables.len(), 2);
    assert_eq!(back.variables[0].key, "token");
    assert_eq!(back.variables[0].value, "abc");
    assert_eq!(back.variables[1].key, "disabled_var");
    assert!(!back.variables[1].enabled);
}
