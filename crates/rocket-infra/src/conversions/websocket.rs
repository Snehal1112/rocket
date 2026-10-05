use rocket_collection::settings::CollectionVariable;
use rocket_collection::websocket::{
    WebSocketMessage, WebSocketMessageKind, WebSocketRequest, WebSocketScript, WebSocketSettings,
};
use rocket_shared::types::{Auth, Header, RequestSettingValue};

use super::auth::persisted_oc_auth;
use crate::oc::*;

/// Converts a parsed OpenCollection WebSocket request to the domain type.
pub fn oc_websocket_to_request(oc: OcWebSocketRequest) -> WebSocketRequest {
    let (variables, scripts, runtime_auth) = match oc.runtime {
        Some(runtime) => (
            runtime
                .variables
                .into_iter()
                .map(CollectionVariable::from)
                .collect(),
            runtime
                .scripts
                .into_iter()
                .map(|s| WebSocketScript {
                    script_type: s.script_type,
                    code: s.code,
                })
                .collect(),
            runtime.auth.map(Auth::from),
        ),
        None => (Vec::new(), Vec::new(), None),
    };

    WebSocketRequest {
        uid: oc.uid.unwrap_or_default(),
        name: oc.info.name,
        description: oc.info.description,
        seq: oc.info.seq,
        tags: oc.info.tags,
        url: oc.websocket.url,
        headers: oc.websocket.headers.into_iter().map(Header::from).collect(),
        messages: messages_from_oc(oc.websocket.message),
        auth: oc.websocket.auth.map(Auth::from).unwrap_or(Auth::None),
        runtime_auth,
        variables,
        scripts,
        settings: oc.settings.map(settings_from_oc),
        docs: oc.docs,
        file_name: None,
    }
}

/// Converts a domain WebSocket request back to the OpenCollection structs.
pub fn websocket_to_oc_websocket(ws: &WebSocketRequest) -> OcWebSocketRequest {
    let runtime_auth = ws.runtime_auth.clone().map(OcAuth::from);
    let runtime = if ws.variables.is_empty() && ws.scripts.is_empty() && runtime_auth.is_none() {
        None
    } else {
        Some(OcWebSocketRequestRuntime {
            variables: ws.variables.iter().cloned().map(OcVariable::from).collect(),
            scripts: ws
                .scripts
                .iter()
                .map(|s| OcScript {
                    script_type: s.script_type.clone(),
                    code: s.code.clone(),
                })
                .collect(),
            auth: runtime_auth,
        })
    };

    OcWebSocketRequest {
        uid: if ws.uid.is_empty() {
            None
        } else {
            Some(ws.uid.clone())
        },
        info: OcWebSocketRequestInfo {
            name: ws.name.clone(),
            description: ws.description.clone(),
            request_type: Some("websocket".into()),
            seq: ws.seq,
            tags: ws.tags.clone(),
        },
        websocket: OcWebSocketRequestDetails {
            url: ws.url.clone(),
            headers: ws
                .headers
                .iter()
                .cloned()
                .map(OcHttpRequestHeader::from)
                .collect(),
            message: messages_to_oc(&ws.messages),
            auth: persisted_oc_auth(ws.auth.clone()),
        },
        runtime,
        settings: ws.settings.clone().map(settings_to_oc),
        docs: ws.docs.clone(),
    }
}

/// Sets the on-disk file name and, for a file that has no `uid`, a uid derived
/// from that name. A derived uid is stable across loads, so tab identity does
/// not change until the next save writes a uid into the file.
pub fn with_file_identity(ws: &mut WebSocketRequest, file_name: &str) {
    ws.file_name = Some(file_name.to_string());
    if ws.uid.is_empty() {
        ws.uid = derived_websocket_uid(file_name);
    }
}

/// The uid a file with no `uid` key gets. Stable across loads, so tab identity survives a reload.
pub fn derived_websocket_uid(file_name: &str) -> String {
    format!("ws-{file_name}")
}

fn kind_from_oc(message_type: &str) -> WebSocketMessageKind {
    WebSocketMessageKind::parse(message_type).unwrap_or_else(|| {
        tracing::warn!(
            message_type = %message_type,
            "unknown WebSocket message type, loading as text"
        );
        WebSocketMessageKind::Text
    })
}

fn messages_from_oc(message: Option<OcWebSocketMessageOrVariants>) -> Vec<WebSocketMessage> {
    match message {
        None => Vec::new(),
        Some(OcWebSocketMessageOrVariants::Single(m)) => vec![WebSocketMessage {
            title: String::new(),
            selected: true,
            kind: kind_from_oc(&m.message_type),
            data: m.data,
        }],
        Some(OcWebSocketMessageOrVariants::Variants(variants)) => variants
            .into_iter()
            .map(|v| WebSocketMessage {
                title: v.title,
                selected: v.selected,
                kind: kind_from_oc(&v.message.message_type),
                data: v.message.data,
            })
            .collect(),
    }
}

fn messages_to_oc(messages: &[WebSocketMessage]) -> Option<OcWebSocketMessageOrVariants> {
    let to_oc = |m: &WebSocketMessage| OcWebSocketMessage {
        message_type: m.kind.as_str().to_string(),
        data: m.data.clone(),
    };
    match messages {
        [] => None,
        [only] if only.title.is_empty() => Some(OcWebSocketMessageOrVariants::Single(to_oc(only))),
        many => Some(OcWebSocketMessageOrVariants::Variants(
            many.iter()
                .map(|m| OcWebSocketMessageVariant {
                    title: m.title.clone(),
                    selected: m.selected,
                    message: to_oc(m),
                })
                .collect(),
        )),
    }
}

fn setting_from_oc(value: InheritableNumber) -> RequestSettingValue<f64> {
    match value {
        InheritableNumber::Value(v) => RequestSettingValue::Value(v),
        InheritableNumber::Inherit(s) => RequestSettingValue::Inherit(s),
    }
}

fn setting_to_oc(value: RequestSettingValue<f64>) -> InheritableNumber {
    match value {
        RequestSettingValue::Value(v) => InheritableNumber::Value(v),
        RequestSettingValue::Inherit(s) => InheritableNumber::Inherit(s),
    }
}

fn settings_from_oc(oc: OcWebSocketRequestSettings) -> WebSocketSettings {
    WebSocketSettings {
        timeout: oc.timeout.map(setting_from_oc),
        keep_alive_interval: oc.keep_alive_interval.map(setting_from_oc),
    }
}

fn settings_to_oc(s: WebSocketSettings) -> OcWebSocketRequestSettings {
    OcWebSocketRequestSettings {
        timeout: s.timeout.map(setting_to_oc),
        keep_alive_interval: s.keep_alive_interval.map(setting_to_oc),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::settings::CollectionVariable;
    use rocket_shared::description::Description;
    use rocket_shared::types::{Auth, Header, RequestSettingValue};

    fn full_request() -> WebSocketRequest {
        let mut ws = WebSocketRequest::new("Chat", "wss://chat.example.com/ws");
        ws.uid = "ws-uid-1".into();
        ws.description = Some(Description::text("Team chat"));
        ws.seq = Some(3);
        ws.tags = vec!["realtime".into()];
        ws.headers = vec![
            Header::new("Origin", "https://example.com"),
            Header::disabled("X-Off", "1"),
        ];
        ws.messages = vec![
            WebSocketMessage {
                title: "ping".into(),
                selected: false,
                kind: WebSocketMessageKind::Text,
                data: "ping".into(),
            },
            WebSocketMessage {
                title: "bytes".into(),
                selected: true,
                kind: WebSocketMessageKind::Binary,
                data: "AQID".into(),
            },
        ];
        ws.auth = Auth::Bearer {
            token: "{{token}}".into(),
        };
        ws.runtime_auth = Some(Auth::Basic {
            username: "u".into(),
            password: "p".into(),
        });
        ws.variables = vec![CollectionVariable {
            key: "room".into(),
            value: "general".into(),
            initial_value: "general".into(),
            enabled: true,
            secret: false,
        }];
        ws.scripts = vec![WebSocketScript {
            script_type: "before-request".into(),
            code: "// pre".into(),
        }];
        ws.settings = Some(WebSocketSettings {
            timeout: Some(RequestSettingValue::Value(5000.0)),
            keep_alive_interval: Some(RequestSettingValue::Inherit("inherit".into())),
        });
        ws.docs = Some("# Chat".into());
        ws
    }

    #[test]
    fn every_field_survives_a_domain_oc_domain_roundtrip() {
        let original = full_request();
        let oc = websocket_to_oc_websocket(&original);
        let back = oc_websocket_to_request(oc);
        assert_eq!(back, original);
    }

    #[test]
    fn single_untitled_message_is_written_in_the_single_form() {
        let mut ws = WebSocketRequest::new("Chat", "ws://x");
        ws.messages = vec![WebSocketMessage {
            title: String::new(),
            selected: true,
            kind: WebSocketMessageKind::Json,
            data: "{\"a\":1}".into(),
        }];
        let oc = websocket_to_oc_websocket(&ws);
        match oc.websocket.message {
            Some(OcWebSocketMessageOrVariants::Single(m)) => {
                assert_eq!(m.message_type, "json");
                assert_eq!(m.data, "{\"a\":1}");
            }
            other => panic!("expected the single form, got {other:?}"),
        }
    }

    #[test]
    fn several_messages_are_written_as_variants() {
        let mut ws = WebSocketRequest::new("Chat", "ws://x");
        ws.messages = vec![
            WebSocketMessage {
                title: "a".into(),
                selected: true,
                kind: WebSocketMessageKind::Text,
                data: "1".into(),
            },
            WebSocketMessage {
                title: "b".into(),
                selected: false,
                kind: WebSocketMessageKind::Xml,
                data: "<x/>".into(),
            },
        ];
        let oc = websocket_to_oc_websocket(&ws);
        match oc.websocket.message {
            Some(OcWebSocketMessageOrVariants::Variants(v)) => {
                assert_eq!(v.len(), 2);
                assert_eq!(v[1].title, "b");
                assert_eq!(v[1].message.message_type, "xml");
            }
            other => panic!("expected variants, got {other:?}"),
        }
    }

    #[test]
    fn a_single_titled_message_stays_a_variant_so_the_title_is_kept() {
        let mut ws = WebSocketRequest::new("Chat", "ws://x");
        ws.messages = vec![WebSocketMessage {
            title: "only".into(),
            selected: true,
            kind: WebSocketMessageKind::Text,
            data: "x".into(),
        }];
        let back = oc_websocket_to_request(websocket_to_oc_websocket(&ws));
        assert_eq!(back.messages[0].title, "only");
    }

    #[test]
    fn an_unknown_message_type_loads_as_text() {
        let yaml = "info:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: ws://x\n  message:\n    type: graphql\n    data: hi\n";
        let oc: OcWebSocketRequest = serde_yaml::from_str(yaml).expect("parse");
        let ws = oc_websocket_to_request(oc);
        assert_eq!(ws.messages.len(), 1);
        assert_eq!(ws.messages[0].kind, WebSocketMessageKind::Text);
        assert_eq!(ws.messages[0].data, "hi");
    }

    #[test]
    fn empty_runtime_is_not_written() {
        let ws = WebSocketRequest::new("Chat", "ws://x");
        let oc = websocket_to_oc_websocket(&ws);
        assert!(oc.runtime.is_none());
        assert!(oc.settings.is_none());
        assert_eq!(oc.info.request_type.as_deref(), Some("websocket"));
    }

    #[test]
    fn a_file_without_uid_gets_a_stable_uid_from_its_file_name() {
        let mut a = WebSocketRequest::new("Chat", "ws://x");
        a.uid = String::new();
        with_file_identity(&mut a, "chat.yml");
        let mut b = WebSocketRequest::new("Chat", "ws://x");
        b.uid = String::new();
        with_file_identity(&mut b, "chat.yml");
        assert_eq!(a.uid, "ws-chat.yml");
        assert_eq!(a.uid, b.uid);
        assert_eq!(a.file_name.as_deref(), Some("chat.yml"));

        let mut c = WebSocketRequest::new("Chat", "ws://x");
        c.uid = "keep-me".into();
        with_file_identity(&mut c, "chat.yml");
        assert_eq!(c.uid, "keep-me");
    }
}
