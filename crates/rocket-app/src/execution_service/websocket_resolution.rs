//! Turns a WebSocket connect request or message into something the client can send:
//! `{{variables}}` resolved, collection defaults merged, auth turned into a header or a
//! query parameter. This is a child module of `execution_service` so it can reuse the
//! private merge and resolve helpers that HTTP sends already use.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rocket_collection::WebSocketMessageKind;
use rocket_environment::resolve;
use rocket_http::websocket::{
    WebSocketConnectRequest, WebSocketFrame, DEFAULT_CONNECT_TIMEOUT_MS,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Auth, Header};
use serde::Deserialize;

use super::{merge_auth, merge_headers, resolve_auth, RequestExecutionService};

/// Where `{{variables}}` come from. Mirrors the scope fields of `ExecuteRequestInput`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketScope {
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub environment_name: Option<String>,
    #[serde(default)]
    pub global_env_name: Option<String>,
    #[serde(default)]
    pub request_path: Option<String>,
}

/// IPC input of `ws_connect`. Built from the open tab, like `ExecuteRequestInput`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketConnectInput {
    pub url: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default)]
    pub auth: Option<Auth>,
    #[serde(default)]
    pub subprotocols: Vec<String>,
    /// Connect timeout in milliseconds. `None` uses the default, `0` waits forever.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Milliseconds between client pings. `None` or `0` sends none.
    #[serde(default)]
    pub keep_alive_ms: Option<u64>,
    #[serde(default)]
    pub verify_ssl: Option<bool>,
    #[serde(flatten)]
    pub scope: WebSocketScope,
}

/// IPC input of `ws_send`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketSendInput {
    pub kind: WebSocketMessageKind,
    pub data: String,
    #[serde(flatten)]
    pub scope: WebSocketScope,
}

fn millis(value: u64) -> Option<Duration> {
    if value == 0 {
        None
    } else {
        Some(Duration::from_millis(value))
    }
}

fn auth_label(auth: &Auth) -> &'static str {
    match auth {
        Auth::OAuth2(_) => "OAuth 2.0",
        Auth::OAuth1(_) => "OAuth 1.0",
        Auth::Digest { .. } => "Digest",
        Auth::Ntlm { .. } => "NTLM",
        Auth::Wsse { .. } => "WSSE",
        Auth::AwsSigV4 { .. } => "AWS Signature",
        _ => "This",
    }
}

fn append_query(url: &str, key: &str, value: &str) -> DomainResult<String> {
    let mut parsed = url::Url::parse(url)
        .map_err(|_| DomainError::InvalidInput("invalid WebSocket URL".into()))?;
    parsed.query_pairs_mut().append_pair(key, value);
    Ok(parsed.to_string())
}

/// Applies a resolved auth value to the handshake. Anything that needs more than a
/// header or a query parameter is refused loudly rather than connecting anonymously.
fn apply_auth(
    auth: &Auth,
    url: &mut String,
    headers: &mut Vec<(String, String)>,
) -> DomainResult<()> {
    match auth {
        Auth::None | Auth::Inherit => Ok(()),
        Auth::Basic { username, password } => {
            let encoded = STANDARD.encode(format!("{username}:{password}"));
            headers.push(("Authorization".into(), format!("Basic {encoded}")));
            Ok(())
        }
        Auth::Bearer { token } => {
            headers.push(("Authorization".into(), format!("Bearer {token}")));
            Ok(())
        }
        Auth::ApiKey { key, value, placement } => match placement.as_str() {
            "header" => {
                headers.push((key.clone(), value.clone()));
                Ok(())
            }
            "query" => {
                *url = append_query(url, key, value)?;
                Ok(())
            }
            _ => Err(DomainError::InvalidInput(
                "API key placement must be header or query".into(),
            )),
        },
        other => Err(DomainError::InvalidInput(format!(
            "{} auth is not supported for WebSocket connections yet",
            auth_label(other)
        ))),
    }
}

impl RequestExecutionService {
    /// Resolves a connect input into a ready-to-send handshake request.
    ///
    /// External secrets use the strict `resolve_external_secrets`: any failing binding fails the
    /// connect. HTTP sends tolerate a failing binding the request never references; this does not.
    pub async fn resolve_websocket(
        &self,
        input: &WebSocketConnectInput,
    ) -> DomainResult<WebSocketConnectRequest> {
        let scope = &input.scope;
        let secrets = self
            .resolve_external_secrets(scope.collection.as_deref(), scope.environment_name.as_deref())
            .await?;
        let vars = self.build_variable_context_with_process_env(
            scope.global_env_name.as_deref(),
            scope.collection.as_deref(),
            scope.environment_name.as_deref(),
            scope.request_path.as_deref(),
            &secrets,
        );

        let request_auth = input.auth.clone().unwrap_or(Auth::None);
        let (auth, headers) = match scope.collection.as_deref() {
            Some(collection) => {
                let settings = self.collection_repo.get_settings(collection).unwrap_or_default();
                (
                    merge_auth(request_auth, settings.auth),
                    merge_headers(&settings.headers, &input.headers),
                )
            }
            None => (request_auth, input.headers.clone()),
        };
        let auth = resolve_auth(auth, &vars);

        let mut url = resolve(&input.url, &vars).output;
        let mut wire_headers: Vec<(String, String)> = headers
            .iter()
            .filter(|h| h.enabled && !h.key.trim().is_empty())
            .map(|h| (resolve(&h.key, &vars).output, resolve(&h.value, &vars).output))
            .collect();
        apply_auth(&auth, &mut url, &mut wire_headers)?;

        Ok(WebSocketConnectRequest {
            url,
            headers: wire_headers,
            subprotocols: input
                .subprotocols
                .iter()
                .map(|s| resolve(s, &vars).output)
                .collect(),
            connect_timeout: millis(input.timeout_ms.unwrap_or(DEFAULT_CONNECT_TIMEOUT_MS)),
            keep_alive_interval: millis(input.keep_alive_ms.unwrap_or(0)),
            verify_ssl: input.verify_ssl.unwrap_or(true),
        })
    }

    /// Resolves one outgoing message. Text kinds get `{{variables}}`; binary data is base64
    /// (whitespace ignored) and is decoded after substitution.
    ///
    /// External secrets are only fetched when the message actually contains a placeholder, so a
    /// plain message never costs a vault round trip.
    pub async fn resolve_websocket_message(
        &self,
        scope: &WebSocketScope,
        kind: WebSocketMessageKind,
        data: &str,
    ) -> DomainResult<WebSocketFrame> {
        let secrets = if data.contains("{{") {
            self.resolve_external_secrets(
                scope.collection.as_deref(),
                scope.environment_name.as_deref(),
            )
            .await?
        } else {
            std::collections::HashMap::new()
        };
        let vars = self.build_variable_context_with_process_env(
            scope.global_env_name.as_deref(),
            scope.collection.as_deref(),
            scope.environment_name.as_deref(),
            scope.request_path.as_deref(),
            &secrets,
        );
        let resolved = resolve(data, &vars).output;
        match kind {
            WebSocketMessageKind::Binary => {
                let compact: String = resolved.split_whitespace().collect();
                STANDARD
                    .decode(compact.as_bytes())
                    .map(WebSocketFrame::Binary)
                    .map_err(|_| {
                        DomainError::InvalidInput("binary message must be valid base64".into())
                    })
            }
            _ => Ok(WebSocketFrame::Text(resolved)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_service::RequestExecutionService;
    use crate::test_doubles::{
        EmptySecretManagerRepo, InMemoryCollectionRepo, InMemoryHistoryRepo, NullCookieRepo,
        RecordingExecutor, SharedCollectionRepo, SharedHistoryRepo, StaticEnvRepo,
    };
    use rocket_collection::{Collection, CollectionSettings};
    use rocket_environment::{Environment, Variable};
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Arc;

    fn service(env: Environment, settings: CollectionSettings) -> RequestExecutionService {
        let mut collection = Collection::new("api");
        collection.settings = settings;
        RequestExecutionService::new(
            Box::new(StaticEnvRepo(env)),
            RecordingExecutor::new(),
            Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
            Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(collection))),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
    }

    fn dev_env() -> Environment {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("host", "chat.example.com"));
        env.set_variable(Variable::new("token", "abc123"));
        env
    }

    fn input(url: &str) -> WebSocketConnectInput {
        serde_json::from_value(serde_json::json!({
            "url": url,
            "collection": "api",
            "environmentName": "dev"
        }))
        .expect("input")
    }

    fn header<'a>(request: &'a WebSocketConnectRequest, name: &str) -> Option<&'a str> {
        request
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[tokio::test]
    async fn variables_are_resolved_in_the_url_and_headers() {
        let svc = service(dev_env(), CollectionSettings::default());
        let mut i = input("wss://{{host}}/ws");
        i.headers = vec![Header::new("X-Token", "{{token}}")];

        let resolved = svc.resolve_websocket(&i).await.expect("resolve");

        assert_eq!(resolved.url, "wss://chat.example.com/ws");
        assert_eq!(header(&resolved, "X-Token"), Some("abc123"));
    }

    #[tokio::test]
    async fn collection_headers_and_auth_apply_and_request_values_win() {
        let settings = CollectionSettings {
            headers: vec![Header::new("X-Team", "core"), Header::new("X-Token", "collection")],
            auth: Some(Auth::Bearer { token: "from-collection".into() }),
            ..CollectionSettings::default()
        };
        let svc = service(dev_env(), settings);
        let mut i = input("wss://h/ws");
        i.headers = vec![Header::new("X-Token", "request")];
        i.auth = Some(Auth::Inherit);

        let resolved = svc.resolve_websocket(&i).await.expect("resolve");

        assert_eq!(header(&resolved, "X-Team"), Some("core"));
        assert_eq!(header(&resolved, "X-Token"), Some("request"));
        assert_eq!(header(&resolved, "Authorization"), Some("Bearer from-collection"));
    }

    #[tokio::test]
    async fn process_env_placeholders_resolve_in_the_url_and_headers() {
        std::env::set_var("ROCKET_WS_TEST_TOKEN", "from-os");
        let svc = service(dev_env(), CollectionSettings::default());
        let mut i = input("wss://h/ws");
        i.headers = vec![Header::new("X-Token", "{{process.env.ROCKET_WS_TEST_TOKEN}}")];

        let resolved = svc.resolve_websocket(&i).await.expect("resolve");

        assert_eq!(header(&resolved, "X-Token"), Some("from-os"));
    }

    #[tokio::test]
    async fn disabled_headers_are_not_sent() {
        let svc = service(dev_env(), CollectionSettings::default());
        let mut i = input("wss://h/ws");
        i.headers = vec![Header::disabled("X-Off", "1"), Header::new("X-On", "1")];

        let resolved = svc.resolve_websocket(&i).await.expect("resolve");

        assert_eq!(header(&resolved, "X-Off"), None);
        assert_eq!(header(&resolved, "X-On"), Some("1"));
    }

    #[tokio::test]
    async fn basic_bearer_and_api_key_auth_become_handshake_credentials() {
        let svc = service(dev_env(), CollectionSettings::default());

        let mut basic = input("wss://h/ws");
        basic.auth = Some(Auth::Basic { username: "u".into(), password: "p".into() });
        let r = svc.resolve_websocket(&basic).await.expect("basic");
        assert_eq!(header(&r, "Authorization"), Some("Basic dTpw"));

        let mut bearer = input("wss://h/ws");
        bearer.auth = Some(Auth::Bearer { token: "{{token}}".into() });
        let r = svc.resolve_websocket(&bearer).await.expect("bearer");
        assert_eq!(header(&r, "Authorization"), Some("Bearer abc123"));

        let mut key_header = input("wss://h/ws");
        key_header.auth = Some(Auth::ApiKey { key: "X-Key".into(), value: "k".into(), placement: "header".into() });
        let r = svc.resolve_websocket(&key_header).await.expect("api key header");
        assert_eq!(header(&r, "X-Key"), Some("k"));

        let mut key_query = input("wss://h/ws?a=1");
        key_query.auth = Some(Auth::ApiKey { key: "api_key".into(), value: "k 1".into(), placement: "query".into() });
        let r = svc.resolve_websocket(&key_query).await.expect("api key query");
        assert_eq!(r.url, "wss://h/ws?a=1&api_key=k+1");
        assert_eq!(header(&r, "api_key"), None);
    }

    #[tokio::test]
    async fn an_unsupported_auth_type_is_an_explicit_error_not_an_anonymous_connect() {
        let svc = service(dev_env(), CollectionSettings::default());
        let mut i = input("wss://h/ws");
        i.auth = Some(Auth::Digest { username: "u".into(), password: "p".into() });

        let err = svc.resolve_websocket(&i).await.expect_err("digest is unsupported");

        assert!(err.to_string().contains("Digest"), "{err}");
        assert!(err.to_string().contains("not supported"), "{err}");
    }

    #[tokio::test]
    async fn timeouts_and_tls_default_sensibly_and_zero_means_off() {
        let svc = service(dev_env(), CollectionSettings::default());

        let defaults = svc.resolve_websocket(&input("wss://h/ws")).await.expect("defaults");
        assert_eq!(defaults.connect_timeout, Some(std::time::Duration::from_millis(30_000)));
        assert_eq!(defaults.keep_alive_interval, None);
        assert!(defaults.verify_ssl);

        let mut custom = input("wss://h/ws");
        custom.timeout_ms = Some(0);
        custom.keep_alive_ms = Some(2500);
        custom.verify_ssl = Some(false);
        let r = svc.resolve_websocket(&custom).await.expect("custom");
        assert_eq!(r.connect_timeout, None);
        assert_eq!(r.keep_alive_interval, Some(std::time::Duration::from_millis(2500)));
        assert!(!r.verify_ssl);
    }

    #[tokio::test]
    async fn message_variables_resolve_and_binary_is_decoded_from_base64() {
        let svc = service(dev_env(), CollectionSettings::default());
        let scope = WebSocketScope {
            collection: Some("api".into()),
            environment_name: Some("dev".into()),
            ..WebSocketScope::default()
        };

        let text = svc
            .resolve_websocket_message(&scope, WebSocketMessageKind::Json, "{\"t\":\"{{token}}\"}")
            .await
            .expect("json");
        assert_eq!(text, WebSocketFrame::Text("{\"t\":\"abc123\"}".into()));

        let bytes = svc
            .resolve_websocket_message(&scope, WebSocketMessageKind::Binary, "AQID\n")
            .await
            .expect("binary");
        assert_eq!(bytes, WebSocketFrame::Binary(vec![1, 2, 3]));

        let err = svc
            .resolve_websocket_message(&scope, WebSocketMessageKind::Binary, "not base64!!")
            .await
            .expect_err("invalid base64");
        assert!(err.to_string().contains("base64"), "{err}");
    }

    #[test]
    fn inputs_deserialize_from_the_camel_case_ipc_shape() {
        let i: WebSocketConnectInput = serde_json::from_value(serde_json::json!({
            "url": "wss://h/ws",
            "headers": [{ "key": "A", "value": "1", "enabled": true }],
            "auth": { "authType": "bearer", "token": "t" },
            "subprotocols": ["graphql-transport-ws"],
            "timeoutMs": 1000,
            "keepAliveMs": 500,
            "verifySsl": false,
            "collection": "api",
            "environmentName": "dev",
            "globalEnvName": "g",
            "requestPath": "chat.yml"
        }))
        .expect("deserialize");
        assert_eq!(i.subprotocols, vec!["graphql-transport-ws".to_string()]);
        assert_eq!(i.timeout_ms, Some(1000));
        assert_eq!(i.scope.request_path.as_deref(), Some("chat.yml"));

        let s: WebSocketSendInput = serde_json::from_value(serde_json::json!({
            "kind": "binary", "data": "AQID", "collection": "api"
        }))
        .expect("send input");
        assert_eq!(s.kind, WebSocketMessageKind::Binary);
        assert_eq!(s.scope.collection.as_deref(), Some("api"));
    }
}
