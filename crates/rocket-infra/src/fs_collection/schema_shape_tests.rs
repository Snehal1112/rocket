//! Regression guard for OpenCollection v1.0.0 `additionalProperties: false`.
//! The allow-lists are copied from
//! https://schema.opencollection.com/opencollection/v1.0.0.json (checked 2026-09-26).
//! A failure means Rocket wrote a key that the schema rejects.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use rocket_collection::settings::SandboxMode;
use rocket_collection::{
    CollectionItem, CollectionRepository, CollectionSettings, CollectionVariable, Request,
};
use rocket_shared::oauth2::{
    OAuth2ClientCredentials, OAuth2Flow, OAuth2PKCE, OAuth2ResourceOwner, OAuth2Settings,
};
use rocket_shared::types::{
    Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, HttpMethod, OAuth1Auth,
    OAuth1PrivateKey, PathParam, QueryParam, RequestSettingValue, RequestSettings,
};
use serde_yaml::Value;
use tempfile::TempDir;

use super::FsCollectionRepo;

/// Keys that still break the schema and are tracked in the plan's Deferred
/// section. Remove an entry as soon as its fix lands.
const KNOWN_DEFERRED: &[&str] = &[
    "HttpRequest.uid",
    "GrpcRequest.uid",
    "FolderInfo.uid",
    "HttpRequestSettings.verifySsl",
    "GraphQLRequest.uid",
    "WebSocketRequest.uid",
    "GraphQLRequestSettings.verifySsl",
    "HttpRequestRuntime.auth",
    "Variable.initial",
    "OAuth2Settings.verifySsl",
    "OAuth2Settings.useSystemBrowser",
    "OAuth2AdditionalParameter.enabled",
];

const COLLECTION_INFO: &[&str] = &["name", "summary", "version", "authors"];
const ITEM_INFO: &[&str] = &["name", "description", "type", "seq", "tags"];
const FOLDER: &[&str] = &["info", "items", "request", "docs"];
const REQUEST_DEFAULTS: &[&str] = &[
    "headers",
    "metadata",
    "auth",
    "variables",
    "scripts",
    "settings",
];
const HTTP_REQUEST: &[&str] = &["info", "http", "runtime", "settings", "examples", "docs"];
const HTTP_DETAILS: &[&str] = &["method", "url", "headers", "params", "body", "auth"];
const HTTP_RUNTIME: &[&str] = &["variables", "scripts", "assertions", "actions"];
const HTTP_SETTINGS: &[&str] = &["encodeUrl", "timeout", "followRedirects", "maxRedirects"];
const HEADER: &[&str] = &["name", "value", "description", "disabled"];
const PARAM: &[&str] = &["name", "value", "description", "type", "disabled"];
const BODY: &[&str] = &["type", "data"];
const FORM_FIELD: &[&str] = &["name", "value", "description", "disabled"];
const MULTIPART_PART: &[&str] = &[
    "name",
    "type",
    "value",
    "description",
    "contentType",
    "disabled",
];
const FILE_VARIANT: &[&str] = &["filePath", "contentType", "selected"];
const VARIABLE: &[&str] = &["name", "value", "description", "disabled"];
const SCRIPT: &[&str] = &["type", "code"];
const GRAPHQL_REQUEST: &[&str] = &["info", "graphql", "runtime", "settings", "docs"];
const GRAPHQL_DETAILS: &[&str] = &["method", "url", "headers", "params", "body", "auth"];
const GRAPHQL_BODY: &[&str] = &["query", "variables"];
const GRPC_REQUEST: &[&str] = &["info", "grpc", "runtime", "docs"];
const GRPC_DETAILS: &[&str] = &[
    "url",
    "method",
    "methodType",
    "protoFilePath",
    "metadata",
    "message",
    "auth",
];
const GRPC_RUNTIME: &[&str] = &["variables", "scripts", "assertions"];
const GRPC_METADATA: &[&str] = &["name", "value", "description", "disabled"];
const GRPC_MESSAGE_VARIANT: &[&str] = &["title", "selected", "message"];
const WEBSOCKET_REQUEST: &[&str] = &["info", "websocket", "runtime", "settings", "docs"];
const WEBSOCKET_DETAILS: &[&str] = &["url", "headers", "message", "auth"];
const WEBSOCKET_SETTINGS: &[&str] = &["timeout", "keepAliveInterval"];
const WEBSOCKET_MESSAGE: &[&str] = &["type", "data"];
const AUTH_USER_PASS: &[&str] = &["type", "username", "password"];
const AUTH_BEARER: &[&str] = &["type", "token"];
const AUTH_APIKEY: &[&str] = &["type", "key", "value", "placement"];
const AUTH_NTLM: &[&str] = &["type", "username", "password", "domain"];
const AUTH_OAUTH1: &[&str] = &[
    "type",
    "consumerKey",
    "consumerSecret",
    "accessToken",
    "accessTokenSecret",
    "callbackUrl",
    "verifier",
    "signatureMethod",
    "privateKey",
    "timestamp",
    "nonce",
    "version",
    "realm",
    "placement",
    "includeBodyHash",
];
const AUTH_AWSV4: &[&str] = &[
    "type",
    "accessKeyId",
    "secretAccessKey",
    "sessionToken",
    "service",
    "region",
    "profileName",
];
const OAUTH2_CLIENT_CREDENTIALS_FLOW: &[&str] = &[
    "type",
    "flow",
    "accessTokenUrl",
    "refreshTokenUrl",
    "credentials",
    "scope",
    "additionalParameters",
    "tokenConfig",
    "settings",
];
const OAUTH2_PASSWORD_FLOW: &[&str] = &[
    "type",
    "flow",
    "accessTokenUrl",
    "refreshTokenUrl",
    "credentials",
    "resourceOwner",
    "scope",
    "additionalParameters",
    "tokenConfig",
    "settings",
];
const OAUTH2_AUTH_CODE_FLOW: &[&str] = &[
    "type",
    "flow",
    "authorizationUrl",
    "accessTokenUrl",
    "refreshTokenUrl",
    "callbackUrl",
    "credentials",
    "scope",
    "state",
    "pkce",
    "additionalParameters",
    "tokenConfig",
    "settings",
];
const OAUTH2_IMPLICIT_FLOW: &[&str] = &[
    "type",
    "flow",
    "authorizationUrl",
    "callbackUrl",
    "credentials",
    "scope",
    "state",
    "additionalParameters",
    "tokenConfig",
    "settings",
];
const OAUTH2_CLIENT_CREDENTIALS: &[&str] = &["clientId", "clientSecret", "placement"];
const OAUTH2_IMPLICIT_CREDENTIALS: &[&str] = &["clientId"];
const OAUTH2_RESOURCE_OWNER: &[&str] = &["username", "password"];
const OAUTH2_PKCE: &[&str] = &["disabled", "method"];
const OAUTH2_SETTINGS: &[&str] = &["autoFetchToken", "autoRefreshToken"];
const OAUTH2_TOKEN_CONFIG: &[&str] = &["id", "placement", "source"];
const OAUTH2_ADDITIONAL_PARAMETER: &[&str] = &["name", "value", "placement"];
const OAUTH2_PARAMS_TOKEN_ONLY: &[&str] = &["accessTokenRequest", "refreshTokenRequest"];
const OAUTH2_PARAMS_AUTH_CODE: &[&str] = &[
    "authorizationRequest",
    "accessTokenRequest",
    "refreshTokenRequest",
];
const OAUTH2_PARAMS_IMPLICIT: &[&str] = &["authorizationRequest"];
const CLIENT_CERT_PEM: &[&str] = &[
    "type",
    "domain",
    "certificateFilePath",
    "privateKeyFilePath",
    "passphrase",
];
const CLIENT_CERT_PKCS12: &[&str] = &["type", "domain", "pkcs12FilePath", "passphrase"];
/// Rocket extensions outside the OpenCollection `ClientCertificate` schema, like
/// `externalSecrets`: vault references (`alias.secretName`) for the certificate material.
/// Other OpenCollection tools do not know them.
const ROCKET_CLIENT_CERT_PEM_EXTENSIONS: &[&str] = &["certificateSecret", "privateKeySecret"];
const ROCKET_CLIENT_CERT_PKCS12_EXTENSIONS: &[&str] = &["pkcs12Secret"];
/// Rocket extension outside the OpenCollection schema: a certificate that RocketVault exports
/// when it is selected. The whole entry type is an extension, and it stores names only.
const ROCKET_CLIENT_CERT_VAULT: &[&str] = &["type", "domain", "binding", "certificate", "format"];

#[derive(Default)]
struct Violations(Vec<String>);

impl Violations {
    /// Records every key of `value` that is neither allowed for `ty` nor a known deferred key.
    fn keys(&mut self, ty: &str, at: &str, value: &Value, allowed: &[&str]) {
        let Some(map) = value.as_mapping() else {
            return;
        };
        for (key, _) in map.iter() {
            let key = key.as_str().unwrap_or("<non-string key>");
            let tagged = format!("{ty}.{key}");
            if !allowed.contains(&key) && !KNOWN_DEFERRED.contains(&tagged.as_str()) {
                self.0.push(format!("{at}: `{key}` is not allowed on {ty}"));
            }
        }
    }
}

fn seq<'a>(value: Option<&'a Value>) -> impl Iterator<Item = &'a Value> + 'a {
    value.and_then(Value::as_sequence).into_iter().flatten()
}

fn auth_keys(auth_type: &str, flow: Option<&str>) -> Option<&'static [&'static str]> {
    match (auth_type, flow) {
        ("basic", _) | ("digest", _) | ("wsse", _) => Some(AUTH_USER_PASS),
        ("bearer", _) => Some(AUTH_BEARER),
        ("apikey", _) => Some(AUTH_APIKEY),
        ("ntlm", _) => Some(AUTH_NTLM),
        ("awsv4", _) => Some(AUTH_AWSV4),
        ("oauth1", _) => Some(AUTH_OAUTH1),
        ("oauth2", Some("client_credentials")) => Some(OAUTH2_CLIENT_CREDENTIALS_FLOW),
        ("oauth2", Some("resource_owner_password_credentials")) => Some(OAUTH2_PASSWORD_FLOW),
        ("oauth2", Some("authorization_code")) => Some(OAUTH2_AUTH_CODE_FLOW),
        ("oauth2", Some("implicit")) => Some(OAUTH2_IMPLICIT_FLOW),
        _ => None,
    }
}

fn check_auth(v: &mut Violations, at: &str, auth: &Value) {
    if auth.as_str() == Some("inherit") {
        return;
    }
    let auth_type = auth.get("type").and_then(Value::as_str).unwrap_or("");
    let flow = auth.get("flow").and_then(Value::as_str);
    let Some(allowed) = auth_keys(auth_type, flow) else {
        v.0.push(format!(
            "{at}: auth type `{auth_type}` (flow {flow:?}) is not a spec Auth member"
        ));
        return;
    };
    v.keys(&format!("Auth[{auth_type}]"), at, auth, allowed);
    if auth_type != "oauth2" {
        return;
    }
    let flow = flow.unwrap_or("");
    if let Some(creds) = auth.get("credentials") {
        let allowed = if flow == "implicit" {
            OAUTH2_IMPLICIT_CREDENTIALS
        } else {
            OAUTH2_CLIENT_CREDENTIALS
        };
        v.keys("OAuth2Credentials", at, creds, allowed);
    }
    if let Some(ro) = auth.get("resourceOwner") {
        v.keys("OAuth2ResourceOwner", at, ro, OAUTH2_RESOURCE_OWNER);
    }
    if let Some(pkce) = auth.get("pkce") {
        v.keys("OAuth2PKCE", at, pkce, OAUTH2_PKCE);
    }
    if let Some(settings) = auth.get("settings") {
        v.keys("OAuth2Settings", at, settings, OAUTH2_SETTINGS);
    }
    if let Some(tc) = auth.get("tokenConfig") {
        v.keys("OAuth2TokenConfig", at, tc, OAUTH2_TOKEN_CONFIG);
    }
    if let Some(ap) = auth.get("additionalParameters") {
        let groups = match flow {
            "authorization_code" => OAUTH2_PARAMS_AUTH_CODE,
            "implicit" => OAUTH2_PARAMS_IMPLICIT,
            _ => OAUTH2_PARAMS_TOKEN_ONLY,
        };
        v.keys("OAuth2AdditionalParameters", at, ap, groups);
        if let Some(map) = ap.as_mapping() {
            for (_, list) in map.iter() {
                for p in seq(Some(list)) {
                    v.keys(
                        "OAuth2AdditionalParameter",
                        at,
                        p,
                        OAUTH2_ADDITIONAL_PARAMETER,
                    );
                }
            }
        }
    }
}

fn check_http_body(v: &mut Violations, at: &str, body: &Value) {
    v.keys("HttpRequestBody", at, body, BODY);
    let (ty, allowed) = match body.get("type").and_then(Value::as_str) {
        Some("form-urlencoded") => ("FormUrlEncodedField", FORM_FIELD),
        Some("multipart-form") => ("MultipartFormPart", MULTIPART_PART),
        Some("file") => ("FileBodyVariant", FILE_VARIANT),
        _ => return,
    };
    for entry in seq(body.get("data")) {
        v.keys(ty, at, entry, allowed);
    }
}

fn check_request_defaults(v: &mut Violations, at: &str, req: &Value) {
    v.keys("RequestDefaults", at, req, REQUEST_DEFAULTS);
    for h in seq(req.get("headers")) {
        v.keys("HttpRequestHeader", at, h, HEADER);
    }
    for var in seq(req.get("variables")) {
        v.keys("Variable", at, var, VARIABLE);
    }
    if let Some(auth) = req.get("auth") {
        check_auth(v, at, auth);
    }
}

fn check_collection_root(v: &mut Violations, at: &str, doc: &Value) {
    // The root object allows extra keys (it is where `extensions` lives), but its `info` and `request` do not.
    if let Some(info) = doc.get("info") {
        v.keys("Info", at, info, COLLECTION_INFO);
    }
    if let Some(req) = doc.get("request") {
        check_request_defaults(v, at, req);
    }
}

fn check_folder(v: &mut Violations, at: &str, doc: &Value) {
    v.keys("Folder", at, doc, FOLDER);
    if let Some(info) = doc.get("info") {
        v.keys("FolderInfo", at, info, ITEM_INFO);
    }
    if let Some(req) = doc.get("request") {
        check_request_defaults(v, at, req);
    }
}

fn check_http_request(v: &mut Violations, at: &str, doc: &Value) {
    v.keys("HttpRequest", at, doc, HTTP_REQUEST);
    if let Some(info) = doc.get("info") {
        v.keys("HttpRequestInfo", at, info, ITEM_INFO);
    }
    if let Some(http) = doc.get("http") {
        v.keys("HttpRequestDetails", at, http, HTTP_DETAILS);
        for h in seq(http.get("headers")) {
            v.keys("HttpRequestHeader", at, h, HEADER);
        }
        for p in seq(http.get("params")) {
            v.keys("HttpRequestParam", at, p, PARAM);
        }
        if let Some(body) = http.get("body") {
            check_http_body(v, at, body);
        }
        if let Some(auth) = http.get("auth") {
            check_auth(v, at, auth);
        }
    }
    if let Some(rt) = doc.get("runtime") {
        v.keys("HttpRequestRuntime", at, rt, HTTP_RUNTIME);
        for var in seq(rt.get("variables")) {
            v.keys("Variable", at, var, VARIABLE);
        }
        for s in seq(rt.get("scripts")) {
            v.keys("Script", at, s, SCRIPT);
        }
    }
    if let Some(settings) = doc.get("settings") {
        v.keys("HttpRequestSettings", at, settings, HTTP_SETTINGS);
    }
}

fn check_graphql_request(v: &mut Violations, at: &str, doc: &Value) {
    v.keys("GraphQLRequest", at, doc, GRAPHQL_REQUEST);
    if let Some(info) = doc.get("info") {
        v.keys("GraphQLRequestInfo", at, info, ITEM_INFO);
    }
    if let Some(gql) = doc.get("graphql") {
        v.keys("GraphQLRequestDetails", at, gql, GRAPHQL_DETAILS);
        for h in seq(gql.get("headers")) {
            v.keys("HttpRequestHeader", at, h, HEADER);
        }
        for p in seq(gql.get("params")) {
            v.keys("HttpRequestParam", at, p, PARAM);
        }
        if let Some(body) = gql.get("body").filter(|b| b.is_mapping()) {
            v.keys("GraphQLBody", at, body, GRAPHQL_BODY);
        }
        if let Some(auth) = gql.get("auth") {
            check_auth(v, at, auth);
        }
    }
    if let Some(settings) = doc.get("settings") {
        v.keys("GraphQLRequestSettings", at, settings, HTTP_SETTINGS);
    }
}

fn check_websocket_request(v: &mut Violations, at: &str, doc: &Value) {
    v.keys("WebSocketRequest", at, doc, WEBSOCKET_REQUEST);
    if let Some(info) = doc.get("info") {
        v.keys("WebSocketRequestInfo", at, info, ITEM_INFO);
    }
    if let Some(ws) = doc.get("websocket") {
        v.keys("WebSocketRequestDetails", at, ws, WEBSOCKET_DETAILS);
        for h in seq(ws.get("headers")) {
            v.keys("HttpRequestHeader", at, h, HEADER);
        }
        if let Some(msg) = ws.get("message").filter(|m| m.is_mapping()) {
            v.keys("WebSocketMessage", at, msg, WEBSOCKET_MESSAGE);
        }
        if let Some(auth) = ws.get("auth") {
            check_auth(v, at, auth);
        }
    }
    if let Some(settings) = doc.get("settings") {
        v.keys("WebSocketRequestSettings", at, settings, WEBSOCKET_SETTINGS);
    }
}

fn check_grpc_request(v: &mut Violations, at: &str, doc: &Value) {
    v.keys("GrpcRequest", at, doc, GRPC_REQUEST);
    if let Some(info) = doc.get("info") {
        v.keys("GrpcRequestInfo", at, info, ITEM_INFO);
    }
    if let Some(grpc) = doc.get("grpc") {
        v.keys("GrpcRequestDetails", at, grpc, GRPC_DETAILS);
        for m in seq(grpc.get("metadata")) {
            v.keys("GrpcMetadata", at, m, GRPC_METADATA);
        }
        for m in seq(grpc.get("message")) {
            v.keys("GrpcMessageVariant", at, m, GRPC_MESSAGE_VARIANT);
        }
        if let Some(auth) = grpc.get("auth") {
            check_auth(v, at, auth);
        }
    }
    if let Some(runtime) = doc.get("runtime") {
        v.keys("GrpcRequestRuntime", at, runtime, GRPC_RUNTIME);
        for s in seq(runtime.get("scripts")) {
            v.keys("Script", at, s, SCRIPT);
        }
        for var in seq(runtime.get("variables")) {
            v.keys("Variable", at, var, VARIABLE);
        }
    }
}

fn setup() -> (TempDir, FsCollectionRepo) {
    let dir = TempDir::new().expect("tempdir");
    let repo = FsCollectionRepo::new(dir.path().to_path_buf(), Arc::new(DashMap::new()));
    (dir, repo)
}

fn read_yaml(path: &Path) -> Value {
    serde_yaml::from_str(&fs::read_to_string(path).expect("read yaml file")).expect("valid yaml")
}

fn sample_bodies() -> Vec<(&'static str, Body)> {
    let raw = |mode: BodyMode, content: &str| Body {
        mode,
        content: Some(content.into()),
        form_data: None,
        file_path: None,
    };
    let form = |mode: BodyMode, entry_type: FormDataType, content_type: Option<String>| Body {
        mode,
        content: None,
        form_data: Some(vec![FormDataEntry {
            key: "k".into(),
            value: "v".into(),
            entry_type,
            enabled: false,
            content_type,
            description: None,
        }]),
        file_path: None,
    };
    vec![
        ("json", raw(BodyMode::Json, "{}")),
        ("xml", raw(BodyMode::Xml, "<a/>")),
        ("text", raw(BodyMode::Text, "hi")),
        ("sparql", raw(BodyMode::Sparql, "SELECT * WHERE {}")),
        (
            "form-urlencoded",
            form(BodyMode::FormUrlEncoded, FormDataType::Text, None),
        ),
        (
            "multipart",
            form(
                BodyMode::FormData,
                FormDataType::File,
                Some("image/png".into()),
            ),
        ),
        (
            "file",
            Body {
                mode: BodyMode::Binary,
                content: None,
                form_data: None,
                file_path: Some("/tmp/upload.bin".into()),
            },
        ),
    ]
}

fn sample_auths() -> Vec<(&'static str, Auth)> {
    let creds = || OAuth2ClientCredentials {
        client_id: "cid".into(),
        client_secret: "csecret".into(),
        placement: Some("basic_auth_header".into()),
    };
    let oauth_settings = || {
        Some(OAuth2Settings {
            auto_fetch_token: Some(true),
            auto_refresh_token: Some(false),
            verify_ssl: None,
            use_system_browser: None,
        })
    };
    vec![
        (
            "basic",
            Auth::Basic {
                username: "u".into(),
                password: "p".into(),
            },
        ),
        ("bearer", Auth::Bearer { token: "t".into() }),
        (
            "apikey",
            Auth::ApiKey {
                key: "X-Key".into(),
                value: "v".into(),
                placement: "header".into(),
            },
        ),
        (
            "digest",
            Auth::Digest {
                username: "u".into(),
                password: "p".into(),
            },
        ),
        (
            "ntlm",
            Auth::Ntlm {
                username: "u".into(),
                password: "p".into(),
                domain: "CORP".into(),
            },
        ),
        (
            "wsse",
            Auth::Wsse {
                username: "u".into(),
                password: "p".into(),
            },
        ),
        (
            "oauth1",
            Auth::OAuth1(Box::new(OAuth1Auth {
                consumer_key: Some("ck".into()),
                consumer_secret: Some("cs".into()),
                access_token: Some("at".into()),
                access_token_secret: Some("ats".into()),
                callback_url: Some("oob".into()),
                verifier: Some("v".into()),
                signature_method: Some("RSA-SHA256".into()),
                private_key: Some(OAuth1PrivateKey {
                    key_type: "text".into(),
                    value: "pem".into(),
                }),
                timestamp: Some("1".into()),
                nonce: Some("n".into()),
                version: Some("1.0".into()),
                realm: Some("r".into()),
                placement: Some("header".into()),
                include_body_hash: Some(true),
            })),
        ),
        (
            "awsv4",
            Auth::AwsSigV4 {
                access_key: "AKIA".into(),
                secret_key: "s".into(),
                region: "us-east-1".into(),
                service: "execute-api".into(),
                session_token: Some("st".into()),
                profile_name: None,
            },
        ),
        ("inherit", Auth::Inherit),
        ("none", Auth::None),
        (
            "oauth2-client-credentials",
            Auth::OAuth2(Box::new(OAuth2Flow::ClientCredentials {
                access_token_url: "https://auth.example.com/token".into(),
                refresh_token_url: None,
                credentials: creds(),
                scope: Some("read".into()),
                additional_parameters: None,
                token_config: None,
                settings: oauth_settings(),
            })),
        ),
        (
            "oauth2-password",
            Auth::OAuth2(Box::new(OAuth2Flow::ResourceOwnerPassword {
                access_token_url: "https://auth.example.com/token".into(),
                refresh_token_url: None,
                credentials: creds(),
                resource_owner: Some(OAuth2ResourceOwner {
                    username: "u".into(),
                    password: "p".into(),
                }),
                scope: None,
                additional_parameters: None,
                token_config: None,
                settings: oauth_settings(),
            })),
        ),
        (
            "oauth2-auth-code",
            Auth::OAuth2(Box::new(OAuth2Flow::AuthorizationCode {
                authorization_url: "https://auth.example.com/authorize".into(),
                access_token_url: "https://auth.example.com/token".into(),
                refresh_token_url: None,
                callback_url: Some("http://localhost/cb".into()),
                credentials: creds(),
                scope: Some("openid".into()),
                state: Some("xyz".into()),
                pkce: Some(OAuth2PKCE {
                    disabled: Some(true),
                    method: Some("S256".into()),
                }),
                additional_parameters: None,
                token_config: None,
                settings: oauth_settings(),
            })),
        ),
        (
            "oauth2-implicit",
            Auth::OAuth2(Box::new(OAuth2Flow::Implicit {
                authorization_url: "https://auth.example.com/authorize".into(),
                callback_url: None,
                client_id: "cid".into(),
                scope: None,
                state: None,
                additional_parameters: None,
                token_config: None,
                settings: oauth_settings(),
            })),
        ),
    ]
}

fn full_request(name: &str, body: Body, auth: Auth) -> Request {
    let mut req = Request::new(name, HttpMethod::Post, "https://api.example.com/users/:id");
    req.headers = vec![
        Header::new("Accept", "application/json"),
        Header {
            key: "X-Off".into(),
            value: "1".into(),
            enabled: false,
            description: None,
        },
    ];
    req.query_params = vec![QueryParam {
        key: "page".into(),
        value: "1".into(),
        enabled: true,
        description: None,
    }];
    req.path_params = vec![PathParam {
        name: "id".into(),
        value: "42".into(),
        description: None,
    }];
    req.body = Some(body);
    req.auth = auth;
    req.pre_request_script = Some("console.log('pre')".into());
    req.settings = Some(RequestSettings {
        encode_url: Some(RequestSettingValue::Value(true)),
        timeout: Some(RequestSettingValue::Value(3000.0)),
        follow_redirects: Some(RequestSettingValue::Inherit("inherit".into())),
        max_redirects: Some(RequestSettingValue::Value(5.0)),
        verify_ssl: None,
    });
    req.variables = vec![CollectionVariable {
        key: "rv".into(),
        value: "1".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    }];
    req
}

const OAUTH1_FIXTURE: &str = "info:\n  name: Signed\n  type: http\nhttp:\n  method: GET\n  url: https://api.example.com/me\n  auth:\n    type: oauth1\n    consumerKey: ck\n    consumerSecret: cs\n    signatureMethod: HMAC-SHA1\n    placement: header\n";

const GRAPHQL_FIXTURE: &str = "info:\n  name: List Users\n  type: graphql\n  seq: 3\ngraphql:\n  method: POST\n  url: https://api.example.com/graphql\n  headers:\n  - name: Accept\n    value: application/json\n  body:\n    query: '{ users { id } }'\n    variables: '{\"first\": 10}'\n  auth:\n    type: bearer\n    token: t\nsettings:\n  timeout: 1000\ndocs: GraphQL docs\n";

const WEBSOCKET_FIXTURE: &str = "info:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: wss://chat.example.com/ws\n  headers:\n  - name: Origin\n    value: https://example.com\n  message:\n    type: json\n    data: '{\"hello\": true}'\nsettings:\n  timeout: 5000\n  keepAliveInterval: 30000\ndocs: WS docs\n";

#[test]
fn written_collection_files_only_use_schema_keys() {
    let (dir, repo) = setup();
    repo.create("api").unwrap();
    repo.save_settings(
        "api",
        &CollectionSettings {
            docs: Some("docs".into()),
            auth: Some(Auth::None),
            headers: vec![Header::new("X-Tenant", "acme")],
            variables: vec![CollectionVariable {
                key: "base".into(),
                value: "https://x".into(),
                initial_value: "https://x".into(),
                enabled: true,
                secret: false,
            }],
            sandbox_mode: SandboxMode::Developer,
        },
    )
    .unwrap();
    repo.create_folder("api", "users").unwrap();
    repo.save_folder_variables(
        "api",
        "users",
        vec![CollectionVariable {
            key: "fv".into(),
            value: "x".into(),
            initial_value: String::new(),
            enabled: false,
            secret: false,
        }],
    )
    .unwrap();

    let mut written = Vec::new();
    for (body_name, body) in sample_bodies() {
        for (auth_name, auth) in sample_auths() {
            let req = full_request(&format!("{body_name} {auth_name}"), body.clone(), auth);
            // save_request may normalize the file name, so keep the path it reports.
            let rel = repo
                .save_request("api", &format!("users/{body_name}-{auth_name}.yml"), &req)
                .unwrap();
            written.push(rel);
        }
    }

    let col_dir = dir.path().join("api");
    fs::write(col_dir.join("users/list-users-gql.yml"), GRAPHQL_FIXTURE).unwrap();
    fs::write(col_dir.join("users/chat-ws.yml"), WEBSOCKET_FIXTURE).unwrap();

    let mut v = Violations::default();
    check_collection_root(
        &mut v,
        "opencollection.yml",
        &read_yaml(&col_dir.join("opencollection.yml")),
    );
    check_folder(
        &mut v,
        "users/folder.yml",
        &read_yaml(&col_dir.join("users/folder.yml")),
    );
    for rel in &written {
        check_http_request(&mut v, rel, &read_yaml(&col_dir.join(rel)));
    }

    // Non-HTTP items are never rewritten by the repo. Their raw values are what
    // Rocket would write if it did, so they are checked through the loaded tree.
    let col = repo.get("api").unwrap();
    let users = col.root.find_folder("users").expect("users folder");
    let mut protocols = Vec::new();
    for item in &users.items {
        match item {
            CollectionItem::GraphQl(g) => {
                protocols.push("graphql".to_string());
                let raw = serde_yaml::to_value(crate::conversions::graphql_to_oc(g))
                    .expect("serialize GraphQL request");
                check_graphql_request(&mut v, &g.name, &raw);
            }
            CollectionItem::OpaqueItem(o) => panic!("unexpected opaque protocol {}", o.protocol),
            CollectionItem::WebSocket(ws) => {
                protocols.push("websocket".to_string());
                let raw = serde_yaml::to_value(crate::conversions::websocket_to_oc_websocket(ws))
                    .expect("serialize websocket");
                check_websocket_request(&mut v, &ws.name, &raw);
            }
            _ => {}
        }
    }
    protocols.sort();
    assert_eq!(
        protocols,
        vec!["graphql", "websocket"],
        "non-HTTP fixtures must load"
    );
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
}

#[test]
fn saved_graphql_request_only_uses_schema_keys_besides_deferred() {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    let mut g = rocket_collection::GraphQlRequest::new("Q", "https://x/graphql").with_query("{ a }");
    g.body.variables = Some("{}".into());
    g.settings = Some(RequestSettings {
        encode_url: None,
        timeout: Some(RequestSettingValue::Value(3000.0)),
        follow_redirects: None,
        max_redirects: None,
        verify_ssl: Some(RequestSettingValue::Value(false)),
    });
    g.pre_request_script = Some("console.log(1)".into());
    let rel = repo
        .save_graphql_request("api", "q.yml", &g)
        .expect("save graphql request");

    let mut v = Violations::default();
    check_graphql_request(&mut v, &rel, &read_yaml(&dir.path().join("api").join(&rel)));
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
}

#[test]
fn checker_flags_known_bad_shapes() {
    let mut v = Violations::default();
    let parse = |yaml: &str| -> Value { serde_yaml::from_str(yaml).expect("fixture yaml") };
    check_folder(
        &mut v,
        "legacy folder",
        &parse("name: legacy\ntype: folder\n"),
    );
    check_auth(&mut v, "none auth", &parse("type: none\n"));
    check_auth(
        &mut v,
        "legacy pkce",
        &parse("type: oauth2\nflow: authorization_code\npkce:\n  enabled: true\n"),
    );
    check_auth(
        &mut v,
        "implicit secret",
        &parse("type: oauth2\nflow: implicit\ncredentials:\n  clientId: a\n  clientSecret: ''\n"),
    );
    // Legacy folder: `name` and `type` (2). Plus none auth, legacy pkce and implicit secret (1 each).
    assert_eq!(v.0.len(), 5, "{:#?}", v.0);
}

#[test]
fn oauth1_request_file_loads_instead_of_vanishing() {
    let (dir, repo) = setup();
    repo.create("api").unwrap();
    fs::write(dir.path().join("api/signed.yml"), OAUTH1_FIXTURE).unwrap();

    let tree = repo.get("api").unwrap();
    assert_eq!(tree.root.items.len(), 1, "oauth1 request dropped from tree");

    let loaded = repo.get_request("api", "signed.yml").unwrap();
    let Auth::OAuth1(auth) = &loaded.auth else {
        panic!("expected OAuth1 auth, got {:?}", loaded.auth);
    };
    assert_eq!(auth.consumer_key.as_deref(), Some("ck"));
    assert_eq!(auth.signature_method.as_deref(), Some("HMAC-SHA1"));

    // Saving keeps the oauth1 block intact.
    repo.save_request("api", "signed.yml", &loaded).unwrap();
    let raw = read_yaml(&dir.path().join("api/signed.yml"));
    let auth = &raw["http"]["auth"];
    assert_eq!(auth["type"].as_str(), Some("oauth1"));
    assert_eq!(auth["consumerSecret"].as_str(), Some("cs"));
}

#[test]
fn environment_client_certificates_use_schema_keys_plus_rocket_extensions() {
    use rocket_environment::{Environment, EnvironmentRepository};
    use rocket_shared::certificate::ClientCertificate;

    let dir = TempDir::new().expect("tempdir");
    let repo = crate::fs_environment_repo::FsEnvironmentRepo::new(dir.path().to_path_buf());
    let mut env = Environment::new("prod");
    env.client_certificates = vec![
        ClientCertificate::Pem {
            domain: "a.example.com".into(),
            certificate_file_path: "certs/client.pem".into(),
            private_key_file_path: "certs/client-key.pem".into(),
            certificate_secret: None,
            private_key_secret: None,
            passphrase: Some("{{pass}}".into()),
        },
        ClientCertificate::Pem {
            domain: "b.example.com".into(),
            certificate_file_path: String::new(),
            private_key_file_path: String::new(),
            certificate_secret: Some("vault.clientCertPem".into()),
            private_key_secret: Some("vault.clientKeyPem".into()),
            passphrase: None,
        },
        ClientCertificate::Pkcs12 {
            domain: "c.example.com".into(),
            pkcs12_file_path: "/certs/client.p12".into(),
            pkcs12_secret: None,
            passphrase: None,
        },
        ClientCertificate::Pkcs12 {
            domain: "d.example.com".into(),
            pkcs12_file_path: String::new(),
            pkcs12_secret: Some("vault.clientBundleB64".into()),
            passphrase: Some("{{vault.bundlePass}}".into()),
        },
        ClientCertificate::Vault {
            domain: "e.example.com".into(),
            binding: "vault".into(),
            certificate: "client-e".into(),
            format: rocket_shared::certificate::VaultCertificateFormat::Pem,
        },
    ];
    repo.save(&env).expect("save environment");

    let doc = read_yaml(&dir.path().join("prod.yml"));
    assert_eq!(seq(doc.get("clientCertificates")).count(), 5);
    let mut v = Violations::default();
    for (i, cert) in seq(doc.get("clientCertificates")).enumerate() {
        let at = format!("prod.yml clientCertificates[{i}]");
        let allowed: Vec<&str> = match cert.get("type").and_then(Value::as_str) {
            Some("pem") => [CLIENT_CERT_PEM, ROCKET_CLIENT_CERT_PEM_EXTENSIONS].concat(),
            Some("pkcs12") => [CLIENT_CERT_PKCS12, ROCKET_CLIENT_CERT_PKCS12_EXTENSIONS].concat(),
            Some("vault") => ROCKET_CLIENT_CERT_VAULT.to_vec(),
            other => {
                v.0.push(format!("{at}: unknown certificate type {other:?}"));
                continue;
            }
        };
        v.keys("ClientCertificate", &at, cert, &allowed);
    }
    assert!(v.0.is_empty(), "{:#?}", v.0);
}

#[test]
fn saved_grpc_request_only_uses_schema_keys_besides_deferred() {
    use rocket_collection::{
        GrpcMessage, GrpcMetadataEntry, GrpcMethodType, GrpcRequest, GrpcScript,
    };

    let (dir, repo) = setup();
    repo.create("api").unwrap();
    let mut g = GrpcRequest::new("Say Hello", "localhost:50051");
    g.method = Some("demo.Greeter/SayHello".into());
    g.method_type = GrpcMethodType::BidiStreaming;
    g.proto_file_path = Some("protos/greeter.proto".into());
    g.metadata = vec![GrpcMetadataEntry::new("x-trace", "abc")];
    g.messages = vec![
        GrpcMessage {
            title: "first".into(),
            selected: true,
            content: "{}".into(),
        },
        GrpcMessage {
            title: "second".into(),
            selected: false,
            content: "{}".into(),
        },
    ];
    g.auth = Auth::Bearer { token: "t".into() };
    g.variables = vec![CollectionVariable {
        key: "tenant".into(),
        value: "acme".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    }];
    g.scripts = vec![GrpcScript {
        script_type: "before-request".into(),
        code: "x".into(),
    }];
    g.docs = Some("docs".into());
    let rel = repo.save_grpc_request("api", "say-hello.yml", &g).unwrap();

    let mut v = Violations::default();
    check_grpc_request(&mut v, &rel, &read_yaml(&dir.path().join("api").join(&rel)));
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
}
