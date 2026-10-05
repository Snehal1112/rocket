//! WebSocket request definition. Pure domain type, no I/O.

use rocket_shared::description::Description;
use rocket_shared::types::{Auth, Header, RequestSettingValue};
use serde::{Deserialize, Serialize};

use crate::settings::CollectionVariable;

/// How the composer treats a message. `Json` and `Xml` are editor hints; all
/// three text kinds go on the wire as text frames. `Binary` data is base64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSocketMessageKind {
    #[default]
    Text,
    Json,
    Xml,
    Binary,
}

impl WebSocketMessageKind {
    /// The OpenCollection `type` string for this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Binary => "binary",
        }
    }

    /// Parses an OpenCollection `type` string. Unknown values return `None`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            "xml" => Some(Self::Xml),
            "binary" => Some(Self::Binary),
            _ => None,
        }
    }
}

/// One saved message. A request keeps several; `selected` marks the one the
/// composer sends. An empty `title` is how a lone, untitled message is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketMessage {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    #[serde(default)]
    pub kind: WebSocketMessageKind,
    #[serde(default)]
    pub data: String,
}

/// A script block carried through unchanged. WebSocket requests do not run
/// scripts yet, but a load and save must not drop them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketScript {
    pub script_type: String,
    pub code: String,
}

/// Per-request settings. Both values are milliseconds or `"inherit"`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<RequestSettingValue<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_alive_interval: Option<RequestSettingValue<f64>>,
}

fn default_auth() -> Auth {
    Auth::None
}

/// A saved WebSocket request definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketRequest {
    #[serde(default = "crate::generate_uid")]
    pub uid: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default)]
    pub messages: Vec<WebSocketMessage>,
    #[serde(default = "default_auth")]
    pub auth: Auth,
    /// `runtime.auth` from the file, kept apart from `auth` like `Request` does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_auth: Option<Auth>,
    #[serde(default)]
    pub variables: Vec<CollectionVariable>,
    #[serde(default)]
    pub scripts: Vec<WebSocketScript>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<WebSocketSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    /// On-disk filename. `None` until loaded from or saved to disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
}

impl WebSocketRequest {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            uid: crate::generate_uid(),
            name: name.into(),
            description: None,
            seq: None,
            tags: Vec::new(),
            url: url.into(),
            headers: Vec::new(),
            messages: Vec::new(),
            auth: Auth::None,
            runtime_auth: None,
            variables: Vec::new(),
            scripts: Vec::new(),
            settings: None,
            docs: None,
            file_name: None,
        }
    }

    /// The message the composer sends: the first one marked `selected`, else the first.
    pub fn selected_message(&self) -> Option<&WebSocketMessage> {
        self.messages
            .iter()
            .find(|m| m.selected)
            .or_else(|| self.messages.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::folder::{CollectionItem, Folder};

    #[test]
    fn websocket_item_serializes_with_the_websocket_type_tag() {
        let mut ws = WebSocketRequest::new("Chat", "wss://chat.example.com/ws");
        ws.messages.push(WebSocketMessage {
            title: "hello".into(),
            selected: true,
            kind: WebSocketMessageKind::Json,
            data: "{}".into(),
        });
        let item = CollectionItem::WebSocket(Box::new(ws));

        let value = serde_json::to_value(&item).expect("serialize");
        assert_eq!(value["type"], "websocket");
        assert_eq!(value["name"], "Chat");
        assert_eq!(value["url"], "wss://chat.example.com/ws");
        assert_eq!(value["messages"][0]["kind"], "json");

        let back: CollectionItem = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, item);
    }

    #[test]
    fn a_frontend_payload_with_only_the_required_fields_deserializes() {
        let json = serde_json::json!({
            "uid": "u1",
            "name": "Chat",
            "url": "wss://chat.example.com/ws"
        });
        let ws: WebSocketRequest = serde_json::from_value(json).expect("deserialize");
        assert!(ws.headers.is_empty());
        assert!(ws.messages.is_empty());
        assert_eq!(ws.auth, rocket_shared::types::Auth::None);
        assert!(ws.settings.is_none());
    }

    #[test]
    fn message_kind_parses_the_opencollection_type_strings() {
        assert_eq!(
            WebSocketMessageKind::parse("json"),
            Some(WebSocketMessageKind::Json)
        );
        assert_eq!(
            WebSocketMessageKind::parse(" XML "),
            Some(WebSocketMessageKind::Xml)
        );
        assert_eq!(
            WebSocketMessageKind::parse("binary"),
            Some(WebSocketMessageKind::Binary)
        );
        assert_eq!(WebSocketMessageKind::parse("graphql"), None);
        assert_eq!(WebSocketMessageKind::Text.as_str(), "text");
    }

    #[test]
    fn websocket_items_count_as_requests_like_graphql_items() {
        let mut folder = Folder::new("root");
        folder
            .items
            .push(CollectionItem::WebSocket(Box::new(WebSocketRequest::new(
                "Chat", "ws://x",
            ))));
        assert_eq!(folder.request_count(), 1);
    }
}
