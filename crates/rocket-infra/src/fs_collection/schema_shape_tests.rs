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
    resolve_folder_auth, CollectionItem, CollectionRepository, CollectionSettings,
    CollectionVariable, FolderSettings, Request, ScriptFlow,
};
use rocket_shared::description::Description;
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
/// `Script.type` values in the spec. `hooks` entries are kept untouched by Rocket.
const SCRIPT_TYPES: &[&str] = &["before-request", "after-response", "tests", "hooks"];
/// A `docs` object has exactly these keys. A plain string is also legal.
const DOCS_OBJECT: &[&str] = &["content", "type"];
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

fn check_scripts(v: &mut Violations, at: &str, scripts: Option<&Value>) {
    for script in seq(scripts) {
        v.keys("Script", at, script, SCRIPT);
        let ty = script.get("type").and_then(Value::as_str).unwrap_or("");
        if !SCRIPT_TYPES.contains(&ty) {
            v.0.push(format!(
                "{at}: script type `{ty}` is not a spec Script type"
            ));
        }
    }
}

fn check_docs(v: &mut Violations, at: &str, docs: Option<&Value>) {
    if let Some(docs) = docs {
        // A string has no keys, so `keys` skips it.
        v.keys("Docs", at, docs, DOCS_OBJECT);
    }
}

fn check_request_defaults(v: &mut Violations, at: &str, req: &Value) {
    v.keys("RequestDefaults", at, req, REQUEST_DEFAULTS);
    for h in seq(req.get("headers")) {
        v.keys("HttpRequestHeader", at, h, HEADER);
    }
    for m in seq(req.get("metadata")) {
        v.keys("GrpcMetadata", at, m, GRPC_METADATA);
    }
    for var in seq(req.get("variables")) {
        v.keys("Variable", at, var, VARIABLE);
    }
    check_scripts(v, at, req.get("scripts"));
    if let Some(settings) = req.get("settings") {
        v.keys("RequestSettings", at, settings, HTTP_SETTINGS);
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
    check_docs(v, at, doc.get("docs"));
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
            ..Default::default()
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
    check_folder(
        &mut v,
        "folder script extra key",
        &parse("info:\n  name: f\n  type: folder\nrequest:\n  scripts:\n  - type: tests\n    code: x\n    enabled: true\n"),
    );
    // Legacy folder: `name` and `type` (2). Plus none auth, legacy pkce, implicit secret
    // and the folder script's `enabled` (1 each).
    assert_eq!(v.0.len(), 6, "{:#?}", v.0);
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

#[test]
fn fully_populated_folder_yml_only_uses_schema_keys() {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    repo.create_folder("api", "users").expect("create folder");
    let folder_yml = dir.path().join("api/users/folder.yml");
    // Sections a Bruno user may have written, which a save must keep valid.
    fs::write(
        &folder_yml,
        "info:\n  name: users\n  type: folder\nrequest:\n  metadata:\n  - name: x-trace\n    value: '1'\n  settings:\n    timeout: 5000\n  scripts:\n  - type: hooks\n    code: onStart()\n",
    )
    .expect("write fixture");

    for (auth_name, auth) in sample_auths() {
        repo.save_folder_settings(
            "api",
            "users",
            &FolderSettings {
                headers: vec![
                    Header::new("X-Tenant", "acme"),
                    Header::disabled("X-Debug", "1"),
                ],
                auth: Some(auth),
                variables: vec![CollectionVariable {
                    key: "fv".into(),
                    value: "x".into(),
                    initial_value: "x".into(),
                    enabled: false,
                    secret: false,
                }],
                pre_request_script: Some("console.log('pre');".into()),
                post_response_script: Some("console.log('post');".into()),
                tests_script: Some("test('ok', () => {});".into()),
                docs: Some("# Users".into()),
            },
        )
        .expect("save folder settings");

        let doc = read_yaml(&folder_yml);
        let mut v = Violations::default();
        check_folder(&mut v, &format!("users/folder.yml [{auth_name}]"), &doc);
        assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
        assert!(doc["request"]["headers"].is_sequence(), "{doc:?}");
        assert!(doc["request"]["variables"].is_sequence(), "{doc:?}");
        assert_eq!(
            doc["request"]["scripts"].as_sequence().map(Vec::len),
            Some(4),
            "{doc:?}"
        );
        assert_eq!(doc["docs"].as_str(), Some("# Users"), "{doc:?}");
    }
}

#[test]
fn script_flow_is_written_only_under_bruno_extensions() {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    let path = dir.path().join("api/opencollection.yml");
    let mut settings = CollectionSettings {
        headers: vec![Header::new("X-Tenant", "acme")],
        sandbox_mode: SandboxMode::Developer,
        script_context_roots: vec!["../shared".into()],
        script_flow: ScriptFlow::Sequential,
        ..Default::default()
    };
    repo.save_settings("api", &settings)
        .expect("save sequential");

    let doc = read_yaml(&path);
    let mut v = Violations::default();
    check_collection_root(&mut v, "opencollection.yml", &doc);
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));

    // The flow lives only under Bruno's namespace, and Rocket adds nothing else there.
    let bruno = doc
        .get("extensions")
        .and_then(|e| e.get("bruno"))
        .expect("extensions.bruno written for sequential");
    let expected: Value =
        serde_yaml::from_str("scripts:\n  flow: sequential\n").expect("parse expected yaml");
    assert_eq!(bruno, &expected);
    let root = doc.as_mapping().expect("root mapping");
    for key in ["flow", "scriptFlow", "script_flow", "bruno"] {
        assert!(
            !root.contains_key(Value::String(key.into())),
            "`{key}` must not be a root key"
        );
    }
    let request = doc.get("request").expect("request defaults written for headers");
    assert!(request.get("scripts").is_none(), "flow is not a RequestDefaults key");
    assert!(
        doc.get("extensions")
            .and_then(|e| e.get("rocketapi"))
            .and_then(|r| r.get("sandboxMode"))
            .is_some(),
        "rocketapi extensions are still written"
    );

    // Back to the default: the Bruno namespace disappears again.
    settings.script_flow = ScriptFlow::Sandwich;
    repo.save_settings("api", &settings).expect("save sandwich");
    let doc = read_yaml(&path);
    assert!(
        doc.get("extensions").and_then(|e| e.get("bruno")).is_none(),
        "no bruno key for sandwich"
    );
}

#[test]
fn checker_flags_bad_folder_scripts_docs_and_rocket_keys() {
    let mut v = Violations::default();
    let doc: Value = serde_yaml::from_str(
        "info:\n  name: f\n  type: folder\nrequest:\n  scripts:\n  - type: pre-request\n    code: x\n  - type: tests\n    code: y\n    extra: z\n  scriptFlow: sequential\ndocs:\n  content: a\n  format: md\n",
    )
    .expect("fixture yaml");
    check_folder(&mut v, "bad folder", &doc);
    // `pre-request` is not a script type (1), `extra` on a script (1), `scriptFlow` on
    // RequestDefaults (1) and `format` on docs (1).
    assert_eq!(v.0.len(), 4, "{:#?}", v.0);
}

fn populated_settings(auth: Auth) -> FolderSettings {
    FolderSettings {
        headers: vec![
            Header::new("X-Team", "billing"),
            Header {
                key: "X-Off".into(),
                value: "1".into(),
                enabled: false,
                description: Some(Description::text("Debug only")),
            },
        ],
        auth: Some(auth),
        variables: vec![
            CollectionVariable {
                key: "region".into(),
                value: "eu".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            },
            CollectionVariable {
                key: "legacy".into(),
                value: "old".into(),
                initial_value: String::new(),
                enabled: false,
                secret: false,
            },
        ],
        pre_request_script: Some("console.log('pre');".into()),
        post_response_script: Some("console.log('post');".into()),
        tests_script: Some("test('ok', function () {});".into()),
        docs: Some("# Billing\n\nNotes.".into()),
    }
}

/// Asserts the `folder.yml` has only spec sections and none of the names Rocket uses in memory.
fn assert_only_folder_sections(raw: &Value, at: &str) {
    let top = raw.as_mapping().expect("folder.yml is a mapping");
    for key in top.keys().filter_map(Value::as_str) {
        assert!(
            ["info", "request", "docs"].contains(&key),
            "{at}: unexpected top-level key `{key}`"
        );
    }
    let request = raw
        .get("request")
        .and_then(Value::as_mapping)
        .expect("request block");
    for key in request.keys().filter_map(Value::as_str) {
        assert!(
            REQUEST_DEFAULTS.contains(&key),
            "{at}: unexpected request key `{key}`"
        );
    }
    let text = serde_yaml::to_string(raw).expect("yaml text");
    for banned in [
        "scriptFlow",
        "script_flow",
        "preRequestScript",
        "postResponseScript",
        "testsScript",
        "initialValue",
    ] {
        assert!(!text.contains(banned), "{at}: Rocket-only name `{banned}`");
    }
}

#[test]
fn populated_folder_yml_only_uses_schema_keys() {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    let mut v = Violations::default();
    for (name, auth) in sample_auths() {
        let folder = format!("f-{name}");
        repo.create_folder("api", &folder).expect("create folder");
        repo.save_folder_settings("api", &folder, &populated_settings(auth))
            .expect("save folder settings");
        let rel = format!("{folder}/folder.yml");
        let raw = read_yaml(&dir.path().join("api").join(&rel));
        check_folder(&mut v, &rel, &raw);
        assert_only_folder_sections(&raw, &rel);
        let mut types: Vec<&str> = raw["request"]["scripts"]
            .as_sequence()
            .expect("scripts written")
            .iter()
            .filter_map(|s| s["type"].as_str())
            .collect();
        types.sort_unstable();
        assert_eq!(
            types,
            vec!["after-response", "before-request", "tests"],
            "{rel}"
        );
    }
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
}

#[test]
fn empty_folder_sections_are_omitted() {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    repo.create_folder("api", "empty").expect("create folder");
    repo.save_folder_settings("api", "empty", &FolderSettings::default())
        .expect("save empty settings");
    let raw = read_yaml(&dir.path().join("api/empty/folder.yml"));
    let request = raw.get("request");
    for section in ["headers", "auth", "variables", "scripts"] {
        assert!(
            request.and_then(|r| r.get(section)).is_none(),
            "empty `{section}` must be omitted: {raw:?}"
        );
    }
    assert!(
        raw.get("docs").is_none(),
        "empty docs must be omitted: {raw:?}"
    );
}

/// A `folder.yml` as Bruno writes it: every section, a disabled header and variable, descriptions,
/// all three script types plus `hooks`, untyped `metadata` and `settings`, and typed `docs`.
const BRUNO_FOLDER: &str = r#"info:
  name: Billing
  type: folder
  seq: 2
request:
  headers:
  - name: X-Team
    value: billing
    description: Owning team
  - name: X-Debug
    value: '1'
    disabled: true
  auth:
    type: bearer
    token: '{{billingToken}}'
  variables:
  - name: region
    value: eu
    description: Deployment region
  - name: legacy
    value: old
    disabled: true
  scripts:
  - type: before-request
    code: |-
      console.log('folder pre');
  - type: after-response
    code: |-
      console.log('folder post');
  - type: tests
    code: |-
      test('ok', function () {});
  - type: hooks
    code: |-
      // hook body kept as written
  metadata:
  - name: x-trace
    value: '1'
  settings:
    timeout: 5000
    followRedirects: false
docs:
  content: |-
    # Billing

    Folder notes.
  type: text/markdown
"#;

/// A Bruno folder that inherits auth and uses the plain-string form of `docs`.
const BRUNO_INHERIT_FOLDER: &str = "info:\n  name: Inner\n  type: folder\nrequest:\n  auth: inherit\n  headers:\n  - name: X-Inner\n    value: '1'\ndocs: Inner notes\n";

/// Creates `api` with one folder whose `folder.yml` is the given text, and returns its path.
fn write_folder_fixture(
    folder: &str,
    yaml: &str,
) -> (TempDir, FsCollectionRepo, std::path::PathBuf) {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    repo.create_folder("api", folder).expect("create folder");
    let path = dir.path().join("api").join(folder).join("folder.yml");
    fs::write(&path, yaml).expect("write fixture");
    (dir, repo, path)
}

#[test]
fn bruno_authored_folder_loads_into_folder_settings() {
    let (_dir, repo, _path) = write_folder_fixture("billing", BRUNO_FOLDER);
    let loaded = repo
        .get_folder_settings("api", "billing")
        .expect("a Bruno folder.yml loads");

    assert_eq!(loaded.headers.len(), 2);
    assert_eq!(loaded.headers[0].key, "X-Team");
    assert_eq!(
        loaded.headers[0]
            .description
            .as_ref()
            .and_then(Description::content),
        Some("Owning team")
    );
    assert!(
        !loaded.headers[1].enabled,
        "a disabled header stays disabled"
    );
    assert_eq!(
        loaded.auth,
        Some(Auth::Bearer {
            token: "{{billingToken}}".into()
        })
    );
    assert_eq!(loaded.variables.len(), 2);
    assert_eq!(loaded.variables[0].key, "region");
    assert!(!loaded.variables[1].enabled);
    assert_eq!(
        loaded.pre_request_script.as_deref(),
        Some("console.log('folder pre');")
    );
    assert_eq!(
        loaded.post_response_script.as_deref(),
        Some("console.log('folder post');")
    );
    assert_eq!(
        loaded.tests_script.as_deref(),
        Some("test('ok', function () {});")
    );
    assert_eq!(loaded.docs.as_deref(), Some("# Billing\n\nFolder notes."));
}

#[test]
fn bruno_folder_keeps_untyped_sections_across_a_save() {
    let (_dir, repo, path) = write_folder_fixture("billing", BRUNO_FOLDER);
    let loaded = repo.get_folder_settings("api", "billing").expect("load");
    repo.save_folder_settings("api", "billing", &loaded)
        .expect("save");

    let raw = read_yaml(&path);
    let req = &raw["request"];
    assert_eq!(raw["info"]["seq"].as_u64(), Some(2));
    assert!(
        raw["info"].get("uid").is_none(),
        "a save must not stamp a uid on a Bruno folder: {raw:?}"
    );
    // Rocket writes settings numbers as floats (`5000.0`), which is still a schema `number`.
    assert_eq!(req["settings"]["timeout"].as_f64(), Some(5000.0));
    assert_eq!(req["settings"]["followRedirects"].as_bool(), Some(false));
    assert_eq!(req["metadata"][0]["name"].as_str(), Some("x-trace"));

    let scripts = req["scripts"].as_sequence().expect("scripts");
    assert_eq!(scripts.len(), 4, "three typed scripts plus the hooks entry");
    let hooks = scripts
        .iter()
        .find(|s| s["type"].as_str() == Some("hooks"))
        .expect("hooks entry kept");
    assert_eq!(hooks["code"].as_str(), Some("// hook body kept as written"));

    assert_eq!(
        req["headers"][0]["description"].as_str(),
        Some("Owning team")
    );
    assert_eq!(req["headers"][1]["disabled"].as_bool(), Some(true));
    assert_eq!(req["auth"]["type"].as_str(), Some("bearer"));
    let vars = req["variables"].as_sequence().expect("variables");
    assert_eq!(vars[0]["description"].as_str(), Some("Deployment region"));
    assert_eq!(vars[1]["disabled"].as_bool(), Some(true));
    assert!(
        vars.iter().all(|var| var.get("initial").is_none()),
        "folder variables must not gain the deferred `initial` key: {vars:?}"
    );
    assert_eq!(
        raw["docs"]
            .as_str()
            .or_else(|| raw["docs"]["content"].as_str()),
        Some("# Billing\n\nFolder notes.")
    );

    let mut v = Violations::default();
    check_folder(&mut v, "billing/folder.yml", &raw);
    assert_only_folder_sections(&raw, "billing/folder.yml");
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
}

#[test]
fn second_folder_save_is_byte_stable() {
    let (_dir, repo, path) = write_folder_fixture("billing", BRUNO_FOLDER);
    let loaded = repo.get_folder_settings("api", "billing").expect("load");
    repo.save_folder_settings("api", "billing", &loaded)
        .expect("first save");
    let first = fs::read_to_string(&path).expect("read first save");

    let reloaded = repo.get_folder_settings("api", "billing").expect("reload");
    assert_eq!(reloaded, loaded, "a save must not change what loads");
    repo.save_folder_settings("api", "billing", &reloaded)
        .expect("second save");
    let second = fs::read_to_string(&path).expect("read second save");
    assert_eq!(second, first, "the second save must be byte-identical");
}

#[test]
fn bruno_inherit_folder_resolves_to_no_folder_auth() {
    let (_dir, repo, path) = write_folder_fixture("inner", BRUNO_INHERIT_FOLDER);
    let loaded = repo.get_folder_settings("api", "inner").expect("load");
    assert!(
        matches!(loaded.auth, None | Some(Auth::Inherit)),
        "auth: inherit is no folder auth, got {:?}",
        loaded.auth
    );
    assert_eq!(resolve_folder_auth(std::slice::from_ref(&loaded)), None);
    assert_eq!(loaded.headers.len(), 1);
    assert_eq!(loaded.docs.as_deref(), Some("Inner notes"));

    repo.save_folder_settings("api", "inner", &loaded)
        .expect("save");
    let raw = read_yaml(&path);
    let auth = &raw["request"]["auth"];
    assert!(
        auth.is_null() || auth.as_str() == Some("inherit"),
        "auth must stay absent or `inherit`, got {auth:?}"
    );
    assert_eq!(raw["docs"].as_str(), Some("Inner notes"));
}

#[test]
fn script_flow_survives_settings_and_folder_saves() {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    let oc_path = dir.path().join("api/opencollection.yml");

    // Author the Bruno extension by hand, with a sibling key that Rocket does not own.
    let mut doc = read_yaml(&oc_path);
    let root = doc.as_mapping_mut().expect("root mapping");
    let ext_key = Value::String("extensions".into());
    let mut ext = match root.get(&ext_key) {
        Some(Value::Mapping(map)) => map.clone(),
        _ => serde_yaml::Mapping::new(),
    };
    ext.insert(
        Value::String("bruno".into()),
        serde_yaml::from_str("scripts:\n  flow: sequential\nother: keep\n").expect("bruno ext"),
    );
    root.insert(ext_key, Value::Mapping(ext));
    fs::write(&oc_path, serde_yaml::to_string(&doc).expect("yaml")).expect("write opencollection");

    let mut settings = repo.get_settings("api").expect("settings");
    assert_eq!(settings.script_flow, ScriptFlow::Sequential);

    // A collection settings save keeps the flow and the sibling key.
    settings.docs = Some("changed".into());
    repo.save_settings("api", &settings).expect("save settings");
    let after = read_yaml(&oc_path);
    assert_eq!(
        after["extensions"]["bruno"]["scripts"]["flow"].as_str(),
        Some("sequential")
    );
    assert_eq!(after["extensions"]["bruno"]["other"].as_str(), Some("keep"));

    // A folder settings save never touches opencollection.yml.
    let before = fs::read_to_string(&oc_path).expect("read before folder save");
    repo.create_folder("api", "users").expect("create folder");
    repo.save_folder_settings("api", "users", &populated_settings(Auth::None))
        .expect("save folder settings");
    assert_eq!(
        fs::read_to_string(&oc_path).expect("read after folder save"),
        before
    );
    assert_eq!(
        repo.get_settings("api").expect("settings").script_flow,
        ScriptFlow::Sequential
    );

    // Switching back to the default is read back, and the sibling key still survives.
    settings.script_flow = ScriptFlow::Sandwich;
    repo.save_settings("api", &settings).expect("save sandwich");
    assert_eq!(
        repo.get_settings("api").expect("settings").script_flow,
        ScriptFlow::Sandwich
    );
    let last = read_yaml(&oc_path);
    assert_eq!(last["extensions"]["bruno"]["other"].as_str(), Some("keep"));
}

#[test]
fn folder_variables_save_keeps_descriptions_without_initial() {
    let (_dir, repo, path) = write_folder_fixture("billing", BRUNO_FOLDER);
    let vars = repo.get_folder_variables("api", "billing").expect("load");
    repo.save_folder_variables("api", "billing", vars)
        .expect("save variables");

    let raw = read_yaml(&path);
    let vars = raw["request"]["variables"]
        .as_sequence()
        .expect("variables");
    assert_eq!(vars[0]["description"].as_str(), Some("Deployment region"));
    assert!(
        vars.iter().all(|var| var.get("initial").is_none()),
        "{vars:?}"
    );
    let mut v = Violations::default();
    check_folder(&mut v, "billing/folder.yml", &raw);
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
}
