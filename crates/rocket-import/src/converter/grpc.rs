use rocket_collection::{GrpcMessage, GrpcMetadataEntry, GrpcMethodType, GrpcRequest};
use rocket_shared::types::Auth;

use crate::bru::ast::*;
use crate::report::SkipReason;

use super::request::bru_auth_to_domain;

/// True when the document is a gRPC request: it has a `grpc` block or its `meta` says so.
pub fn is_grpc(doc: &BruDocument) -> bool {
    doc.grpc.is_some() || doc.meta.as_ref().is_some_and(|m| m.request_type == "grpc")
}

/// Converts a gRPC Bruno document to a domain `GrpcRequest`.
///
/// The proto path is copied as written. The importer rewrites it when it can find
/// the file. Unsupported auth is reported and the request still imports with `auth: None`.
pub fn convert(doc: &BruDocument) -> (Option<GrpcRequest>, Vec<SkipReason>) {
    let skipped: Vec<SkipReason> = doc
        .unknown_blocks
        .iter()
        .filter(|b| b.name == "auth")
        .map(|b| SkipReason::UnsupportedAuthType(b.subtype.clone().unwrap_or_default()))
        .collect();

    let section = doc.grpc.clone().unwrap_or_default();
    let name = doc
        .meta
        .as_ref()
        .map(|m| m.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Untitled".into());
    let mut g = GrpcRequest::new(name, section.url.clone().unwrap_or_default());
    g.seq = doc.meta.as_ref().and_then(|m| m.seq);
    // Bruno writes `/package.Service/Method`. Rocket stores it without the slash.
    g.method = section
        .method
        .as_deref()
        .map(|m| m.trim_start_matches('/').to_string())
        .filter(|m| !m.is_empty());
    g.method_type = section
        .method_type
        .as_deref()
        .and_then(GrpcMethodType::parse)
        .unwrap_or_default();
    g.proto_file_path = section.proto_path.clone();

    // Metadata may sit in `metadata {}` or, in some files, in `headers {}`.
    for kv in doc.grpc_metadata.iter().chain(doc.headers.iter()) {
        let mut entry = GrpcMetadataEntry::new(kv.key.clone(), kv.value.clone());
        entry.enabled = !kv.disabled;
        g.metadata.push(entry);
    }

    g.messages = doc
        .grpc_messages
        .iter()
        .enumerate()
        .map(|(i, m)| GrpcMessage {
            title: m.title.clone(),
            selected: i == 0,
            content: m.content.clone(),
        })
        .collect();

    if section.auth_mode.as_deref() == Some("inherit") {
        g.auth = Auth::Inherit;
    } else if skipped.is_empty() {
        if let Some(auth) = &doc.auth {
            g.auth = bru_auth_to_domain(auth);
        }
    }
    (Some(g), skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grpc_doc() -> BruDocument {
        BruDocument {
            meta: Some(BruMeta {
                name: "Say Hello".into(),
                request_type: "grpc".into(),
                seq: Some(2),
            }),
            grpc: Some(BruGrpc {
                url: Some("localhost:50051".into()),
                method: Some("/demo.greeter.v1.Greeter/SayHello".into()),
                method_type: Some("server-streaming".into()),
                proto_path: Some("protos/greeter.proto".into()),
                auth_mode: Some("none".into()),
            }),
            grpc_metadata: vec![
                BruKeyValue {
                    key: "x-trace".into(),
                    value: "abc".into(),
                    disabled: false,
                },
                BruKeyValue {
                    key: "x-off".into(),
                    value: "1".into(),
                    disabled: true,
                },
            ],
            grpc_messages: vec![
                BruGrpcMessage {
                    title: "first".into(),
                    content: "{\"name\": \"a\"}".into(),
                },
                BruGrpcMessage {
                    title: "second".into(),
                    content: "{\"name\": \"b\"}".into(),
                },
            ],
            ..BruDocument::default()
        }
    }

    #[test]
    fn converts_the_call_description() {
        let (g, skipped) = convert(&grpc_doc());
        assert!(skipped.is_empty());
        let g = g.expect("grpc request");
        assert_eq!(g.name, "Say Hello");
        assert_eq!(g.seq, Some(2));
        assert_eq!(g.url, "localhost:50051");
        assert_eq!(
            g.method.as_deref(),
            Some("demo.greeter.v1.Greeter/SayHello"),
            "the leading slash is dropped"
        );
        assert_eq!(g.method_type, GrpcMethodType::ServerStreaming);
        assert_eq!(g.proto_file_path.as_deref(), Some("protos/greeter.proto"));
        assert_eq!(g.auth, Auth::None);
    }

    #[test]
    fn keeps_disabled_metadata_and_marks_the_first_message_selected() {
        let (g, _) = convert(&grpc_doc());
        let g = g.expect("grpc request");
        assert_eq!(g.metadata.len(), 2);
        assert!(g.metadata[0].enabled);
        assert!(!g.metadata[1].enabled);
        assert_eq!(g.messages.len(), 2);
        assert!(g.messages[0].selected);
        assert!(!g.messages[1].selected);
        assert_eq!(g.messages[1].title, "second");
    }

    #[test]
    fn metadata_written_as_headers_is_kept() {
        let mut doc = grpc_doc();
        doc.grpc_metadata.clear();
        doc.headers = vec![BruKeyValue {
            key: "x-h".into(),
            value: "v".into(),
            disabled: false,
        }];
        let g = convert(&doc).0.expect("grpc request");
        assert_eq!(g.metadata.len(), 1);
        assert_eq!(g.metadata[0].key, "x-h");
    }

    #[test]
    fn an_unknown_method_type_falls_back_to_unary() {
        let mut doc = grpc_doc();
        doc.grpc.as_mut().expect("grpc").method_type = Some("duplex".into());
        assert_eq!(
            convert(&doc).0.expect("grpc request").method_type,
            GrpcMethodType::Unary
        );
    }

    #[test]
    fn inherit_and_bearer_auth_are_converted() {
        let mut doc = grpc_doc();
        doc.grpc.as_mut().expect("grpc").auth_mode = Some("inherit".into());
        assert_eq!(convert(&doc).0.expect("grpc request").auth, Auth::Inherit);

        let mut doc = grpc_doc();
        doc.auth = Some(BruAuth::Bearer {
            token: "{{tok}}".into(),
        });
        assert_eq!(
            convert(&doc).0.expect("grpc request").auth,
            Auth::Bearer {
                token: "{{tok}}".into()
            }
        );
    }

    #[test]
    fn unsupported_auth_is_reported_and_the_request_still_imports_without_auth() {
        let mut doc = grpc_doc();
        doc.auth = Some(BruAuth::Bearer { token: "t".into() });
        doc.unknown_blocks.push(BruRawBlock {
            name: "auth".into(),
            subtype: Some("oauth2".into()),
            content: String::new(),
        });
        let (g, skipped) = convert(&doc);
        assert!(
            matches!(skipped.as_slice(), [SkipReason::UnsupportedAuthType(t)] if t == "oauth2")
        );
        assert_eq!(g.expect("still imported").auth, Auth::None);
    }

    #[test]
    fn a_meta_type_of_grpc_is_enough_to_be_a_grpc_document() {
        let doc = BruDocument {
            meta: Some(BruMeta {
                name: "G".into(),
                request_type: "grpc".into(),
                seq: None,
            }),
            ..BruDocument::default()
        };
        assert!(is_grpc(&doc));
        assert!(!is_grpc(&BruDocument::default()));
    }
}
