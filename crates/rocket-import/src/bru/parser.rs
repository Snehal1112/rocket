use super::lexer::nested_depth;
use crate::bru::ast::*;
use crate::bru::lexer::{tokenise, Token};
use crate::error::ImportResult;

/// Parse a `.bru` file string into a `BruDocument`.
/// Unknown blocks never cause an error — they land in `unknown_blocks`.
pub fn parse(input: &str) -> ImportResult<BruDocument> {
    let tokens = tokenise(input)?;
    let mut doc = BruDocument::default();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i] {
            Token::BlockOpen { name, subtype } => {
                let name = name.clone();
                let subtype = subtype.clone();

                // Collect all tokens up to matching BlockClose.
                i += 1;
                let mut block_tokens: Vec<Token> = Vec::new();
                while i < tokens.len() {
                    if tokens[i] == Token::BlockClose {
                        i += 1;
                        break;
                    }
                    block_tokens.push(tokens[i].clone());
                    i += 1;
                }

                dispatch_block(&mut doc, &name, subtype.as_deref(), &block_tokens);
            }
            _ => {
                i += 1;
            }
        }
    }

    Ok(doc)
}

fn dispatch_block(doc: &mut BruDocument, name: &str, subtype: Option<&str>, tokens: &[Token]) {
    match (name, subtype) {
        ("ws", None) => parse_ws_block(doc, tokens),
        ("meta", None) => parse_meta(doc, tokens),
        ("headers", None) => parse_headers(doc, tokens),
        ("vars", None) => parse_vars(doc, tokens),
        ("vars", Some("secret")) => parse_secret_vars(doc, tokens),
        ("auth", Some(st)) => parse_auth(doc, st, tokens),
        ("body", Some(st)) => parse_body(doc, st, tokens),
        ("script", Some("pre-request")) => {
            doc.pre_request_script = extract_raw_text(tokens);
        }
        ("script", Some("post-response")) => {
            doc.post_response_script = extract_raw_text(tokens);
        }
        _ => {
            // Method blocks: get, post, put, patch, delete, head, options
            if let Some(method) = BruMethod::from_block_name(name) {
                doc.method = Some(method);
                parse_method_block(doc, tokens);
            } else {
                // Unknown block — record for ImportReport.
                doc.unknown_blocks.push(BruRawBlock {
                    name: name.to_string(),
                    subtype: subtype.map(String::from),
                    content: extract_raw_text(tokens).unwrap_or_default(),
                });
            }
        }
    }
}

fn kv_map(tokens: &[Token]) -> Vec<(String, String)> {
    tokens
        .iter()
        .filter_map(|t| {
            if let Token::KeyValue { key, value } = t {
                Some((key.clone(), value.clone()))
            } else {
                None
            }
        })
        .collect()
}

fn extract_raw_text(tokens: &[Token]) -> Option<String> {
    tokens.iter().find_map(|t| {
        if let Token::RawText(s) = t {
            Some(s.clone())
        } else {
            None
        }
    })
}

fn parse_meta(doc: &mut BruDocument, tokens: &[Token]) {
    let map = kv_map(tokens);
    let get = |k: &str| map.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    doc.meta = Some(BruMeta {
        name: get("name").unwrap_or_default(),
        request_type: get("type").unwrap_or_default(),
        seq: get("seq").and_then(|s| s.parse().ok()),
    });
}

fn parse_method_block(doc: &mut BruDocument, tokens: &[Token]) {
    let map = kv_map(tokens);
    if let Some((_, url)) = map.iter().find(|(k, _)| k == "url") {
        doc.url = Some(url.clone());
    }
}

fn parse_ws_block(doc: &mut BruDocument, tokens: &[Token]) {
    let map = kv_map(tokens);
    if let Some((_, url)) = map.iter().find(|(k, _)| k == "url") {
        doc.url = Some(url.clone());
    }
    if let Some((_, mode)) = map.iter().find(|(k, _)| k == "auth") {
        doc.ws_auth_mode = Some(mode.clone());
    }
}

/// Parses the inside of a `body:ws` block: repeated `name [kind] { content }` entries.
/// The braces of JSON content are balanced, so the entry ends at the first `}` line that
/// brings the nesting depth back to zero.
pub(crate) fn parse_ws_messages(raw: &str) -> Vec<BruWsMessage> {
    let mut out = Vec::new();
    let mut lines = raw.lines();
    while let Some(line) = lines.next() {
        let Some(header) = line.trim().strip_suffix('{') else {
            continue;
        };
        let header = header.trim();
        let (name, kind) = match (header.rfind('['), header.rfind(']')) {
            (Some(open), Some(close)) if open < close => (
                header[..open].trim().to_string(),
                header[open + 1..close].trim().to_string(),
            ),
            _ => (header.to_string(), "text".to_string()),
        };

        let mut depth = 1usize;
        let mut body: Vec<&str> = Vec::new();
        for inner in lines.by_ref() {
            let trimmed = inner.trim();
            if trimmed == "}" && depth == 1 {
                break;
            }
            depth = nested_depth(depth, trimmed, '{', '}', true);
            body.push(inner);
        }
        out.push(BruWsMessage {
            name,
            kind,
            content: dedent(&body),
        });
    }
    out
}

/// Removes the indentation shared by every non-blank line, then trims the ends.
fn dedent(lines: &[&str]) -> String {
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| if l.len() >= indent { &l[indent..] } else { l.trim_start() })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn parse_headers(doc: &mut BruDocument, tokens: &[Token]) {
    for (key, value) in kv_map(tokens) {
        let disabled = key.starts_with('~');
        let key = if disabled {
            key.trim_start_matches('~').to_string()
        } else {
            key
        };
        doc.headers.push(BruKeyValue {
            key,
            value,
            disabled,
        });
    }
}

fn parse_vars(doc: &mut BruDocument, tokens: &[Token]) {
    for (key, value) in kv_map(tokens) {
        let disabled = key.starts_with('~');
        let key = if disabled {
            key.trim_start_matches('~').to_string()
        } else {
            key
        };
        doc.vars.push(BruKeyValue {
            key,
            value,
            disabled,
        });
    }
}

fn parse_secret_vars(doc: &mut BruDocument, tokens: &[Token]) {
    // vars:secret uses a list format `[ NAME1\n NAME2 ]` — captured as RawText.
    if let Some(raw) = extract_raw_text(tokens) {
        for line in raw.lines() {
            let name = line.trim().trim_matches(['[', ']']).trim();
            if !name.is_empty() {
                doc.secret_vars.push(name.to_string());
            }
        }
    }
}

fn parse_body(doc: &mut BruDocument, subtype: &str, tokens: &[Token]) {
    if subtype == "ws" {
        doc.ws_messages = parse_ws_messages(&extract_raw_text(tokens).unwrap_or_default());
        return;
    }
    let raw = extract_raw_text(tokens).unwrap_or_default();
    doc.body = Some(match subtype {
        "json" => BruBody::Json(raw),
        "text" => BruBody::Text(raw),
        "xml" => BruBody::Xml(raw),
        "form-urlencoded" => BruBody::FormUrlEncoded(
            kv_map(tokens)
                .into_iter()
                .map(|(key, value)| {
                    let disabled = key.starts_with('~');
                    let key = if disabled {
                        key.trim_start_matches('~').to_string()
                    } else {
                        key
                    };
                    BruKeyValue {
                        key,
                        value,
                        disabled,
                    }
                })
                .collect(),
        ),
        "multipart-form" => BruBody::Multipart(
            kv_map(tokens)
                .into_iter()
                .map(|(key, value)| {
                    let disabled = key.starts_with('~');
                    let key = if disabled {
                        key.trim_start_matches('~').to_string()
                    } else {
                        key
                    };
                    BruKeyValue {
                        key,
                        value,
                        disabled,
                    }
                })
                .collect(),
        ),
        "graphql" => {
            doc.graphql.get_or_insert_with(Default::default).query = raw;
            return;
        }
        "graphql:vars" => {
            // An empty vars block is the same as none.
            if !raw.trim().is_empty() {
                doc.graphql.get_or_insert_with(Default::default).variables = Some(raw);
            }
            return;
        }
        other => {
            doc.unknown_blocks.push(BruRawBlock {
                name: "body".into(),
                subtype: Some(other.into()),
                content: raw,
            });
            return;
        }
    });
}

fn parse_auth(doc: &mut BruDocument, subtype: &str, tokens: &[Token]) {
    let map = kv_map(tokens);
    let get = |k: &str| {
        map.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };

    doc.auth = Some(match subtype {
        "bearer" => BruAuth::Bearer {
            token: get("token"),
        },
        "basic" => BruAuth::Basic {
            username: get("username"),
            password: get("password"),
        },
        "awsv4" => BruAuth::AwsV4 {
            access_key_id: get("accessKeyId"),
            secret_access_key: get("secretAccessKey"),
            session_token: map
                .iter()
                .find(|(k, _)| k == "sessionToken")
                .map(|(_, v)| v.clone()),
            service: map
                .iter()
                .find(|(k, _)| k == "service")
                .map(|(_, v)| v.clone()),
            region: map
                .iter()
                .find(|(k, _)| k == "region")
                .map(|(_, v)| v.clone()),
            profile_name: map
                .iter()
                .find(|(k, _)| k == "profileName")
                .map(|(_, v)| v.clone()),
        },
        "apikey" => BruAuth::ApiKey {
            key: get("key"),
            value: get("value"),
            placement: get("placement"),
        },
        "digest" => BruAuth::Digest {
            username: get("username"),
            password: get("password"),
        },
        other => {
            // oauth2 and others land as unknown.
            doc.unknown_blocks.push(BruRawBlock {
                name: "auth".into(),
                subtype: Some(other.into()),
                content: String::new(),
            });
            return;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> BruDocument {
        super::parse(s).unwrap()
    }

    #[test]
    fn parses_graphql_body_and_vars_blocks() {
        let doc = parse(
            "meta {\n  name: Users\n  type: graphql\n  seq: 1\n}\n\npost {\n  url: https://api.example.com/graphql\n  body: graphql\n  auth: none\n}\n\nbody:graphql {\n  query Users($n: Int) {\n    users(first: $n) { id }\n  }\n}\n\nbody:graphql:vars {\n  {\n    \"n\": 5\n  }\n}\n",
        );
        let gql = doc.graphql.expect("graphql body parsed");
        assert!(gql.query.contains("users(first: $n)"), "{}", gql.query);
        assert!(gql.variables.as_deref().expect("vars").contains("\"n\": 5"));
        assert!(doc.body.is_none(), "a GraphQL body is not an HTTP body");
        assert!(
            doc.unknown_blocks.iter().all(|b| b.name != "body"),
            "{:?}",
            doc.unknown_blocks
        );
        assert_eq!(doc.method, Some(BruMethod::Post));
    }

    #[test]
    fn parses_meta_block() {
        let doc = parse("meta {\n  name: Get Users\n  type: http\n  seq: 1\n}\n");
        let meta = doc.meta.unwrap();
        assert_eq!(meta.name, "Get Users");
        assert_eq!(meta.request_type, "http");
        assert_eq!(meta.seq, Some(1));
    }

    #[test]
    fn parses_get_block_url() {
        let doc = parse("get {\n  url: https://api.example.com/users\n}\n");
        assert_eq!(doc.method, Some(BruMethod::Get));
        assert_eq!(doc.url.as_deref(), Some("https://api.example.com/users"));
    }

    #[test]
    fn parses_headers() {
        let doc = parse("headers {\n  Content-Type: application/json\n  ~X-Debug: true\n}\n");
        assert_eq!(doc.headers.len(), 2);
        assert_eq!(doc.headers[0].key, "Content-Type");
        assert!(!doc.headers[0].disabled);
        assert_eq!(doc.headers[1].key, "X-Debug");
        assert!(doc.headers[1].disabled);
    }

    #[test]
    fn parses_json_body() {
        let doc = parse("body:json {\n  {\"page\": 1}\n}\n");
        assert!(matches!(doc.body, Some(BruBody::Json(_))));
        if let Some(BruBody::Json(s)) = doc.body {
            assert!(s.contains("\"page\""));
        }
    }

    #[test]
    fn parses_bearer_auth() {
        let doc = parse("auth:bearer {\n  token: {{authToken}}\n}\n");
        assert!(matches!(doc.auth, Some(BruAuth::Bearer { .. })));
    }

    #[test]
    fn parses_pre_request_script() {
        let doc = parse("script:pre-request {\n  bru.setVar('ts', Date.now());\n}\n");
        assert!(doc.pre_request_script.is_some());
        assert!(doc.pre_request_script.unwrap().contains("bru.setVar"));
    }

    #[test]
    fn unknown_blocks_land_in_unknown_blocks() {
        let doc = parse("graphql {\n  url: https://api.example.com/graphql\n}\n");
        assert_eq!(doc.unknown_blocks.len(), 1);
        assert_eq!(doc.unknown_blocks[0].name, "graphql");
    }

    #[test]
    fn parses_env_vars_block() {
        let doc = parse("vars {\n  baseUrl: https://localhost:3000\n  apiKey: secret123\n}\n");
        assert_eq!(doc.vars.len(), 2);
        assert_eq!(doc.vars[0].key, "baseUrl");
    }

    #[test]
    fn parses_secret_vars_block() {
        let doc = parse("vars:secret [\n  DB_PASSWORD\n  API_SECRET\n]\n");
        assert_eq!(doc.secret_vars.len(), 2);
        assert!(doc.secret_vars.contains(&"DB_PASSWORD".to_string()));
    }

    const WS_BRU: &str = r#"meta {
  name: Echo
  type: ws
  seq: 2
}

ws {
  url: wss://echo.websocket.org
  body: ws
  auth: bearer
}

headers {
  X-Trace: abc
  ~X-Off: 1
}

auth:bearer {
  token: {{token}}
}

body:ws {
  message 1 [json] {
    {"name":"Bruno","nested":{"a":[1,2]}}
  }

  message 2 [text] {
    hello
  }
}
"#;

    #[test]
    fn parses_a_websocket_request_document() {
        let doc = parse(WS_BRU);
        assert!(doc.is_websocket());
        assert_eq!(doc.meta.as_ref().map(|m| m.name.as_str()), Some("Echo"));
        assert_eq!(doc.meta.as_ref().and_then(|m| m.seq), Some(2));
        assert_eq!(doc.url.as_deref(), Some("wss://echo.websocket.org"));
        assert_eq!(doc.ws_auth_mode.as_deref(), Some("bearer"));
        assert_eq!(doc.headers.len(), 2);
        assert!(doc.headers[1].disabled);
        assert!(matches!(&doc.auth, Some(BruAuth::Bearer { token }) if token == "{{token}}"));
        assert_eq!(doc.ws_messages.len(), 2);
    }

    #[test]
    fn parses_messages_whose_json_contains_braces() {
        let doc = parse(WS_BRU);
        assert_eq!(doc.ws_messages[0].name, "message 1");
        assert_eq!(doc.ws_messages[0].kind, "json");
        assert_eq!(
            doc.ws_messages[0].content,
            r#"{"name":"Bruno","nested":{"a":[1,2]}}"#
        );
        assert_eq!(doc.ws_messages[1].name, "message 2");
        assert_eq!(doc.ws_messages[1].kind, "text");
        assert_eq!(doc.ws_messages[1].content, "hello");
    }


    #[test]
    fn multi_line_message_content_keeps_its_relative_indentation() {
        let raw = "message 1 [json] {\n  {\n    \"a\": 1\n  }\n}\n";
        let messages = parse_ws_messages(raw);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn a_message_header_without_a_kind_defaults_to_text() {
        let messages = parse_ws_messages("ping {\n  hi\n}\n");
        assert_eq!(messages[0].name, "ping");
        assert_eq!(messages[0].kind, "text");
        assert_eq!(messages[0].content, "hi");
    }

    #[test]
    fn an_http_document_is_not_a_websocket() {
        let doc = parse("meta {\n  name: A\n  type: http\n}\nget {\n  url: https://x\n}\n");
        assert!(!doc.is_websocket());
        assert!(doc.ws_messages.is_empty());
    }

    #[test]
    fn braces_inside_json_strings_do_not_end_a_message() {
        let doc = parse(
            "meta {\n  name: A\n  type: ws\n}\n\nbody:ws {\n  one [json] {\n    {\"pattern\":\"}\"}\n  }\n\n  two [json] {\n    {\"open\":\"{\"}\n  }\n\n  three [text] {\n    ok\n  }\n}\n\nheaders {\n  X-A: 1\n}\n",
        );
        assert_eq!(doc.ws_messages.len(), 3, "{:?}", doc.ws_messages);
        assert_eq!(doc.ws_messages[0].content, "{\"pattern\":\"}\"}");
        assert_eq!(doc.ws_messages[1].content, "{\"open\":\"{\"}");
        assert_eq!(doc.ws_messages[2].content, "ok");
        assert_eq!(doc.headers.len(), 1, "blocks after body:ws must still parse");
    }
}
