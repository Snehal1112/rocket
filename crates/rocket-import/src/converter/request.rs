use rocket_collection::{GraphQlBody, GraphQlRequest, Request};
use rocket_shared::types::{Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, HttpMethod};

use crate::bru::ast::*;
use crate::report::SkipReason;

/// What a Bruno file turns into.
#[derive(Debug, Clone, PartialEq)]
pub enum Converted {
    Http(Request),
    GraphQl(GraphQlRequest),
}

/// True when the document is a GraphQL request: its `meta` says so or it carries a GraphQL body.
fn is_graphql(doc: &BruDocument) -> bool {
    doc.graphql.is_some()
        || doc
            .meta
            .as_ref()
            .is_some_and(|m| m.request_type == "graphql")
}

/// Converts a Bruno document to whichever domain item it describes.
/// Unsupported request types (gRPC, WebSocket) still produce `(None, [skip])`.
pub fn convert_item(doc: &BruDocument) -> (Option<Converted>, Vec<SkipReason>) {
    if is_graphql(doc) {
        let (g, skipped) = convert_graphql(doc);
        return (g.map(Converted::GraphQl), skipped);
    }
    let (req, skipped) = convert(doc);
    (req.map(Converted::Http), skipped)
}

/// Unsupported-auth reasons recorded in the document.
fn auth_skips(doc: &BruDocument) -> Vec<SkipReason> {
    doc.unknown_blocks
        .iter()
        .filter(|b| b.name == "auth")
        .map(|b| SkipReason::UnsupportedAuthType(b.subtype.clone().unwrap_or_default()))
        .collect()
}

/// Converts a GraphQL Bruno document to a domain `GraphQlRequest`.
/// Unsupported auth is reported and the request still imports with `auth: None`.
pub fn convert_graphql(doc: &BruDocument) -> (Option<GraphQlRequest>, Vec<SkipReason>) {
    let skipped = auth_skips(doc);
    let name = doc
        .meta
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "Untitled".into());
    let mut g = GraphQlRequest::new(name, doc.url.clone().unwrap_or_default());
    g.method = doc
        .method
        .as_ref()
        .map(bru_method_to_domain)
        .unwrap_or(HttpMethod::Post);
    g.seq = doc.meta.as_ref().and_then(|m| m.seq);

    for h in &doc.headers {
        g.headers.push(if h.disabled {
            Header::disabled(h.key.clone(), h.value.clone())
        } else {
            Header::new(h.key.clone(), h.value.clone())
        });
    }

    let gql = doc.graphql.clone().unwrap_or_default();
    g.body = GraphQlBody {
        query: gql.query,
        variables: gql.variables.filter(|v| !v.trim().is_empty()),
    };
    g.body_variants = gql.variants;

    if skipped.is_empty() {
        if let Some(auth) = &doc.auth {
            g.auth = bru_auth_to_domain(auth);
        }
    }
    g.pre_request_script = doc.pre_request_script.clone();
    g.post_response_script = doc.post_response_script.clone();
    (Some(g), skipped)
}

/// Convert a BruDocument to a domain Request.
/// Returns `(Option<Request>, Vec<SkipReason>)`.
/// `None` if the request type is unsupported (e.g. graphql).
/// Non-fatal skips (unsupported auth) still return `Some(Request)` with auth omitted.
pub fn convert(doc: &BruDocument) -> (Option<Request>, Vec<SkipReason>) {
    let mut skipped: Vec<SkipReason> = Vec::new();

    // Collect unknown block reasons before deciding whether to bail.
    for block in &doc.unknown_blocks {
        if block.name == "unsupported_type" {
            let t = block.subtype.clone().unwrap_or_default();
            skipped.push(SkipReason::UnsupportedRequestType(t));
        }
        if block.name == "auth" {
            let t = block.subtype.clone().unwrap_or_default();
            skipped.push(SkipReason::UnsupportedAuthType(t));
        }
    }

    // Unsupported type: bail entirely, no Request produced.
    if skipped
        .iter()
        .any(|s| matches!(s, SkipReason::UnsupportedRequestType(_)))
    {
        return (None, skipped);
    }

    let name = doc
        .meta
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "Untitled".into());

    let method = doc
        .method
        .as_ref()
        .map(bru_method_to_domain)
        .unwrap_or(HttpMethod::Get);
    let url = doc.url.clone().unwrap_or_default();

    let mut req = Request::new(name, method, url);

    // seq
    if let Some(meta) = &doc.meta {
        req.seq = meta.seq;
    }

    // Headers — include both enabled and disabled.
    for h in &doc.headers {
        if h.disabled {
            req.headers
                .push(Header::disabled(h.key.clone(), h.value.clone()));
        } else {
            req.headers
                .push(Header::new(h.key.clone(), h.value.clone()));
        }
    }

    // Body
    if let Some(body) = &doc.body {
        req.body = Some(bru_body_to_domain(body));
    }

    // Auth — only set if no unsupported-auth skip was recorded.
    let has_auth_skip = skipped
        .iter()
        .any(|s| matches!(s, SkipReason::UnsupportedAuthType(_)));
    if !has_auth_skip {
        if let Some(auth) = &doc.auth {
            req.auth = bru_auth_to_domain(auth);
        }
    }

    // Scripts
    req.pre_request_script = doc.pre_request_script.clone();
    req.post_response_script = doc.post_response_script.clone();

    (Some(req), skipped)
}

fn bru_method_to_domain(m: &BruMethod) -> HttpMethod {
    match m {
        BruMethod::Get => HttpMethod::Get,
        BruMethod::Post => HttpMethod::Post,
        BruMethod::Put => HttpMethod::Put,
        BruMethod::Patch => HttpMethod::Patch,
        BruMethod::Delete => HttpMethod::Delete,
        BruMethod::Head => HttpMethod::Head,
        BruMethod::Options => HttpMethod::Options,
    }
}

fn bru_body_to_domain(body: &BruBody) -> Body {
    match body {
        BruBody::Json(s) => Body {
            mode: BodyMode::Json,
            content: Some(s.clone()),
            form_data: None,
            file_path: None,
        },
        BruBody::Text(s) => Body {
            mode: BodyMode::Text,
            content: Some(s.clone()),
            form_data: None,
            file_path: None,
        },
        BruBody::Xml(s) => Body {
            mode: BodyMode::Xml,
            content: Some(s.clone()),
            form_data: None,
            file_path: None,
        },
        BruBody::FormUrlEncoded(kvs) => {
            // Encode as `key=value&key2=value2` string. Note: the FormUrlEncoded
            // BodyMode stores the body as a flat string, so disabled fields cannot
            // be represented — they are included here regardless of kv.disabled.
            // Multipart avoids this because FormDataEntry carries an `enabled` flag.
            let encoded = kvs
                .iter()
                .map(|kv| format!("{}={}", kv.key, kv.value))
                .collect::<Vec<_>>()
                .join("&");
            Body {
                mode: BodyMode::FormUrlEncoded,
                content: Some(encoded),
                form_data: None,
                file_path: None,
            }
        }
        BruBody::Multipart(kvs) => {
            let entries = kvs
                .iter()
                .map(|kv| FormDataEntry {
                    key: kv.key.clone(),
                    value: kv.value.clone(),
                    entry_type: FormDataType::Text,
                    enabled: !kv.disabled,
                    content_type: None,
                    description: None,
                })
                .collect();
            Body {
                mode: BodyMode::FormData,
                content: None,
                form_data: Some(entries),
                file_path: None,
            }
        }
    }
}

fn bru_auth_to_domain(auth: &BruAuth) -> Auth {
    match auth {
        BruAuth::Bearer { token } => Auth::Bearer {
            token: token.clone(),
        },
        BruAuth::Basic { username, password } => Auth::Basic {
            username: username.clone(),
            password: password.clone(),
        },
        BruAuth::Digest { username, password } => Auth::Digest {
            username: username.clone(),
            password: password.clone(),
        },
        BruAuth::ApiKey {
            key,
            value,
            placement,
        } => Auth::ApiKey {
            key: key.clone(),
            value: value.clone(),
            placement: placement.clone(),
        },
        BruAuth::AwsV4 {
            access_key_id,
            secret_access_key,
            session_token,
            service,
            region,
            profile_name,
        } => Auth::AwsSigV4 {
            access_key: access_key_id.clone(),
            secret_key: secret_access_key.clone(),
            region: region.clone().unwrap_or_default(),
            service: service.clone().unwrap_or_default(),
            session_token: session_token.clone(),
            profile_name: profile_name.clone(),
        },
        BruAuth::Unknown(_) => Auth::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graphql_doc() -> BruDocument {
        BruDocument {
            meta: Some(BruMeta {
                name: "Users".into(),
                request_type: "graphql".into(),
                seq: Some(2),
            }),
            method: Some(BruMethod::Post),
            url: Some("{{baseUrl}}/graphql".into()),
            headers: vec![BruKeyValue {
                key: "Accept".into(),
                value: "application/json".into(),
                disabled: false,
            }],
            graphql: Some(BruGraphQl {
                query: "{ users { id } }".into(),
                variables: Some("{\"n\": 1}".into()),
                ..BruGraphQl::default()
            }),
            pre_request_script: Some("// pre".into()),
            ..BruDocument::default()
        }
    }

    #[test]
    fn graphql_document_converts_to_a_graphql_request() {
        let (g, skipped) = convert_graphql(&graphql_doc());
        assert!(skipped.is_empty());
        let g = g.expect("graphql request");
        assert_eq!(g.name, "Users");
        assert_eq!(g.seq, Some(2));
        assert_eq!(g.method, HttpMethod::Post);
        assert_eq!(g.url, "{{baseUrl}}/graphql");
        assert_eq!(g.body.query, "{ users { id } }");
        assert_eq!(g.body.variables.as_deref(), Some("{\"n\": 1}"));
        assert_eq!(g.headers.len(), 1);
        assert_eq!(g.pre_request_script.as_deref(), Some("// pre"));
    }

    #[test]
    fn convert_item_routes_by_request_type() {
        let (item, _) = convert_item(&graphql_doc());
        assert!(matches!(item, Some(Converted::GraphQl(_))));
        let (item, _) = convert_item(&doc_with_method(BruMethod::Get, "https://example.com"));
        assert!(matches!(item, Some(Converted::Http(_))));
    }

    #[test]
    fn graphql_document_with_unsupported_auth_still_imports_and_reports() {
        let mut doc = graphql_doc();
        doc.unknown_blocks.push(BruRawBlock {
            name: "auth".into(),
            subtype: Some("oauth2".into()),
            content: String::new(),
        });
        let (g, skipped) = convert_graphql(&doc);
        assert!(g.is_some());
        assert!(matches!(skipped[0], SkipReason::UnsupportedAuthType(_)));
    }

    fn doc_with_method(method: BruMethod, url: &str) -> BruDocument {
        BruDocument {
            meta: Some(BruMeta {
                name: "Test".into(),
                request_type: "http".into(),
                seq: Some(1),
            }),
            method: Some(method),
            url: Some(url.into()),
            ..BruDocument::default()
        }
    }

    #[test]
    fn converts_get_request_name_method_url() {
        let doc = doc_with_method(BruMethod::Get, "https://api.example.com/users");
        let (req, skipped) = convert(&doc);
        assert!(skipped.is_empty());
        let req = req.unwrap();
        assert_eq!(req.name, "Test");
        assert!(format!("{:?}", req).contains("Get") || format!("{:?}", req).contains("GET"));
        assert!(format!("{:?}", req).contains("api.example.com"));
    }

    #[test]
    fn converts_bearer_auth() {
        let mut doc = doc_with_method(BruMethod::Get, "https://example.com");
        doc.auth = Some(BruAuth::Bearer {
            token: "{{token}}".into(),
        });
        let (req, skipped) = convert(&doc);
        assert!(skipped.is_empty());
        let req = req.unwrap();
        assert!(format!("{:?}", req).contains("Bearer") || format!("{:?}", req).contains("bearer"));
    }

    #[test]
    fn unsupported_request_type_produces_skip_reason() {
        let doc = BruDocument {
            meta: Some(BruMeta {
                name: "GQL".into(),
                request_type: "graphql".into(),
                seq: None,
            }),
            unknown_blocks: vec![BruRawBlock {
                name: "unsupported_type".into(),
                subtype: Some("graphql".into()),
                content: String::new(),
            }],
            ..BruDocument::default()
        };
        let (req, skipped) = convert(&doc);
        assert!(req.is_none());
        assert_eq!(skipped.len(), 1);
        assert!(matches!(skipped[0], SkipReason::UnsupportedRequestType(_)));
    }

    #[test]
    fn oauth2_auth_produces_skip_reason() {
        let mut doc = doc_with_method(BruMethod::Get, "https://example.com");
        doc.unknown_blocks.push(BruRawBlock {
            name: "auth".into(),
            subtype: Some("oauth2".into()),
            content: String::new(),
        });
        let (req, skipped) = convert(&doc);
        // Request itself is still produced, auth skip is recorded.
        assert!(req.is_some());
        assert_eq!(skipped.len(), 1);
        assert!(matches!(skipped[0], SkipReason::UnsupportedAuthType(_)));
    }

    #[test]
    fn converts_json_body() {
        let mut doc = doc_with_method(BruMethod::Post, "https://example.com");
        doc.body = Some(BruBody::Json("{\"key\":\"val\"}".into()));
        let (req, skipped) = convert(&doc);
        assert!(skipped.is_empty());
        let req = req.unwrap();
        assert!(req.body.is_some());
    }

    #[test]
    fn disabled_headers_are_preserved() {
        let mut doc = doc_with_method(BruMethod::Get, "https://example.com");
        doc.headers = vec![
            BruKeyValue {
                key: "Accept".into(),
                value: "application/json".into(),
                disabled: false,
            },
            BruKeyValue {
                key: "X-Debug".into(),
                value: "true".into(),
                disabled: true,
            },
        ];
        let (req, _) = convert(&doc);
        let req = req.unwrap();
        assert_eq!(req.headers.len(), 2);
        assert!(req.headers[0].enabled);
        assert!(!req.headers[1].enabled);
    }

    #[test]
    fn seq_is_propagated() {
        let doc = doc_with_method(BruMethod::Get, "https://example.com");
        let (req, _) = convert(&doc);
        assert_eq!(req.unwrap().seq, Some(1));
    }
}
