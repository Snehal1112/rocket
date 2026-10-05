use serde::{Deserialize, Serialize};

/// Which protocol a collection item speaks. The serialized form is the
/// frontend's `requestType` discriminator, so the two stay in step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestKind {
    #[default]
    Http,
    GraphQl,
    Grpc,
    WebSocket,
}

impl RequestKind {
    /// Used by serde to keep HTTP summaries byte-identical to older builds.
    pub fn is_http(&self) -> bool {
        matches!(self, RequestKind::Http)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_serialize_to_the_frontend_discriminator() {
        let json = |k: RequestKind| serde_json::to_string(&k).expect("serialize");
        assert_eq!(json(RequestKind::Http), "\"http\"");
        assert_eq!(json(RequestKind::GraphQl), "\"graphql\"");
        assert_eq!(json(RequestKind::Grpc), "\"grpc\"");
        assert_eq!(json(RequestKind::WebSocket), "\"websocket\"");
    }

    #[test]
    fn default_kind_is_http() {
        assert_eq!(RequestKind::default(), RequestKind::Http);
        assert!(RequestKind::Http.is_http());
        assert!(!RequestKind::GraphQl.is_http());
    }
}
