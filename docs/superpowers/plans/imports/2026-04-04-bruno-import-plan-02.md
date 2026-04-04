# Bruno Import — Plan 02: `.bru` Lexer + Parser

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the `.bru` DSL lexer (produces a token stream) and parser (produces a `BruDocument` AST), with full unit test coverage on fixture strings.

**Architecture:** Two-stage pipeline. `lexer.rs` tokenises raw `.bru` text into a flat `Vec<Token>`. `parser.rs` consumes that token stream and builds a typed `BruDocument`. Unknown/unsupported blocks land in `unknown_blocks` — never a hard error. `ast.rs` defines all shared types.

**Tech Stack:** Rust (no external parsing libraries — hand-written lexer/parser)

**Prerequisite:** Plan 01 complete.

**Spec:** `docs/superpowers/specs/2026-04-04-bruno-import-design.md`

---

## Task 1: AST types in `ast.rs`

**Files:**
- Modify: `crates/rocket-import/src/bru/ast.rs`

- [ ] **Step 1: Write `ast.rs`**

```rust
/// The root AST node produced by parsing a single `.bru` file
/// (request file or environment file).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BruDocument {
    pub meta: Option<BruMeta>,
    pub method: Option<BruMethod>,
    pub url: Option<String>,
    pub headers: Vec<BruKeyValue>,
    pub body: Option<BruBody>,
    pub auth: Option<BruAuth>,
    /// Variables from `vars {}` block (environment files).
    pub vars: Vec<BruKeyValue>,
    /// Variables from `vars:secret {}` block (environment files).
    pub secret_vars: Vec<String>,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    /// Unrecognised or unsupported blocks — fed into ImportReport.
    pub unknown_blocks: Vec<BruRawBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BruMeta {
    pub name: String,
    pub request_type: String,  // "http", "graphql", "grpc", "websocket"
    pub seq: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BruKeyValue {
    pub key: String,
    pub value: String,
    pub disabled: bool,  // true when line starts with `~`
}

#[derive(Debug, Clone, PartialEq)]
pub enum BruMethod {
    Get, Post, Put, Patch, Delete, Head, Options,
}

impl BruMethod {
    /// Parse from lowercase block name ("get", "post", …).
    pub fn from_block_name(s: &str) -> Option<Self> {
        match s {
            "get"     => Some(Self::Get),
            "post"    => Some(Self::Post),
            "put"     => Some(Self::Put),
            "patch"   => Some(Self::Patch),
            "delete"  => Some(Self::Delete),
            "head"    => Some(Self::Head),
            "options" => Some(Self::Options),
            _         => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BruBody {
    Json(String),
    Text(String),
    Xml(String),
    FormUrlEncoded(Vec<BruKeyValue>),
    Multipart(Vec<BruKeyValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BruAuth {
    Bearer { token: String },
    Basic { username: String, password: String },
    AwsV4 {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
        service: Option<String>,
        region: Option<String>,
        profile_name: Option<String>,
    },
    ApiKey { key: String, value: String, placement: String },
    Digest { username: String, password: String },
    /// Any auth type not listed above — lands in unknown_blocks instead.
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BruRawBlock {
    pub name: String,
    pub subtype: Option<String>,
    pub content: String,
}
```

- [ ] **Step 2: Verify compile**

```bash
cargo check -p rocket-import
```
Expected: compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-import/src/bru/ast.rs
git commit -m "feat(import): BruDocument AST types"
```

---

## Task 2: Lexer

**Files:**
- Modify: `crates/rocket-import/src/bru/lexer.rs`

- [ ] **Step 1: Write failing tests first**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenises_simple_block() {
        let input = "get {\n  url: https://example.com\n}\n";
        let tokens = tokenise(input).unwrap();
        assert_eq!(tokens, vec![
            Token::BlockOpen { name: "get".into(), subtype: None },
            Token::KeyValue { key: "url".into(), value: "https://example.com".into() },
            Token::BlockClose,
        ]);
    }

    #[test]
    fn tokenises_block_with_subtype() {
        let input = "body:json {\n  {\"a\": 1}\n}\n";
        let tokens = tokenise(input).unwrap();
        assert_eq!(tokens, vec![
            Token::BlockOpen { name: "body".into(), subtype: Some("json".into()) },
            Token::RawText("{\"a\": 1}".into()),
            Token::BlockClose,
        ]);
    }

    #[test]
    fn tokenises_disabled_key_value() {
        let input = "headers {\n  ~X-Debug: true\n}\n";
        let tokens = tokenise(input).unwrap();
        assert_eq!(tokens, vec![
            Token::BlockOpen { name: "headers".into(), subtype: None },
            Token::KeyValue { key: "~X-Debug".into(), value: "true".into() },
            Token::BlockClose,
        ]);
    }

    #[test]
    fn tokenises_empty_block() {
        let input = "headers {\n}\n";
        let tokens = tokenise(input).unwrap();
        assert_eq!(tokens, vec![
            Token::BlockOpen { name: "headers".into(), subtype: None },
            Token::BlockClose,
        ]);
    }

    #[test]
    fn handles_windows_line_endings() {
        let input = "get {\r\n  url: https://example.com\r\n}\r\n";
        let tokens = tokenise(input).unwrap();
        assert!(tokens.contains(&Token::KeyValue {
            key: "url".into(),
            value: "https://example.com".into(),
        }));
    }

    #[test]
    fn raw_text_block_preserves_inner_content() {
        let input = "script:pre-request {\n  const x = 1;\n  bru.setVar('a', x);\n}\n";
        let tokens = tokenise(input).unwrap();
        assert_eq!(tokens[0], Token::BlockOpen {
            name: "script".into(),
            subtype: Some("pre-request".into()),
        });
        if let Token::RawText(text) = &tokens[1] {
            assert!(text.contains("const x = 1;"));
            assert!(text.contains("bru.setVar"));
        } else {
            panic!("expected RawText token");
        }
    }
}
```

Run: `cargo test -p rocket-import bru::lexer`
Expected: FAIL (lexer not implemented yet)

- [ ] **Step 2: Implement `lexer.rs`**

```rust
use crate::error::{ImportError, ImportResult};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Block opening line: `name:subtype {` or `name {`
    BlockOpen { name: String, subtype: Option<String> },
    /// Key-value pair inside a kv-style block: `key: value`
    KeyValue { key: String, value: String },
    /// Raw text content inside a raw-text block (body, script, docs)
    RawText(String),
    /// Closing brace `}`
    BlockClose,
}

/// Raw-text blocks — their content is captured verbatim rather than parsed as key-values.
const RAW_TEXT_BLOCK_NAMES: &[&str] = &["body", "script", "docs"];

/// Tokenise a `.bru` file string into a flat token stream.
pub fn tokenise(input: &str) -> ImportResult<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut lines = input.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        // Skip blank lines and comments.
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Block opening: `name {` or `name:subtype {`
        if trimmed.ends_with('{') {
            let header = trimmed.trim_end_matches('{').trim();
            let (name, subtype) = if let Some((n, s)) = header.split_once(':') {
                (n.trim().to_string(), Some(s.trim().to_string()))
            } else {
                (header.to_string(), None)
            };

            let is_raw = RAW_TEXT_BLOCK_NAMES.contains(&name.as_str());
            tokens.push(Token::BlockOpen { name: name.clone(), subtype });

            // Collect block body.
            let mut raw_lines: Vec<&str> = Vec::new();
            loop {
                match lines.next() {
                    None => break,
                    Some(inner) => {
                        let inner_trimmed = inner.trim();
                        if inner_trimmed == "}" {
                            if is_raw && !raw_lines.is_empty() {
                                // Trim one leading indent level from raw lines.
                                let content = raw_lines
                                    .iter()
                                    .map(|l| l.trim_start_matches("  "))
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                tokens.push(Token::RawText(content.trim().to_string()));
                            }
                            tokens.push(Token::BlockClose);
                            break;
                        }
                        if is_raw {
                            raw_lines.push(inner);
                        } else if !inner_trimmed.is_empty() {
                            // Key-value: `key: value` (value may contain colons)
                            if let Some((k, v)) = inner_trimmed.split_once(':') {
                                tokens.push(Token::KeyValue {
                                    key: k.trim().to_string(),
                                    value: v.trim().to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(tokens)
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket-import bru::lexer
```
Expected: ALL PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-import/src/bru/lexer.rs
git commit -m "feat(import): .bru lexer — tokenises block DSL into token stream"
```

---

## Task 3: Parser

**Files:**
- Modify: `crates/rocket-import/src/bru/parser.rs`

- [ ] **Step 1: Write failing tests first**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bru::ast::*;

    fn parse(s: &str) -> BruDocument {
        super::parse(s).unwrap()
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
        assert_eq!(doc.headers[1].key, "~X-Debug");
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
}
```

Run: `cargo test -p rocket-import bru::parser`
Expected: FAIL (parser not implemented)

- [ ] **Step 2: Implement `parser.rs`**

```rust
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
            _ => { i += 1; }
        }
    }

    Ok(doc)
}

fn dispatch_block(doc: &mut BruDocument, name: &str, subtype: Option<&str>, tokens: &[Token]) {
    match (name, subtype) {
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
    tokens.iter().filter_map(|t| {
        if let Token::KeyValue { key, value } = t {
            Some((key.clone(), value.clone()))
        } else {
            None
        }
    }).collect()
}

fn extract_raw_text(tokens: &[Token]) -> Option<String> {
    tokens.iter().find_map(|t| {
        if let Token::RawText(s) = t { Some(s.clone()) } else { None }
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

fn parse_headers(doc: &mut BruDocument, tokens: &[Token]) {
    for (key, value) in kv_map(tokens) {
        let disabled = key.starts_with('~');
        doc.headers.push(BruKeyValue { key, value, disabled });
    }
}

fn parse_vars(doc: &mut BruDocument, tokens: &[Token]) {
    for (key, value) in kv_map(tokens) {
        let disabled = key.starts_with('~');
        doc.vars.push(BruKeyValue { key, value, disabled });
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
    let raw = extract_raw_text(tokens).unwrap_or_default();
    doc.body = Some(match subtype {
        "json" => BruBody::Json(raw),
        "text" => BruBody::Text(raw),
        "xml"  => BruBody::Xml(raw),
        "form-urlencoded" => BruBody::FormUrlEncoded(
            kv_map(tokens).into_iter().map(|(key, value)| BruKeyValue {
                disabled: key.starts_with('~'), key, value,
            }).collect()
        ),
        "multipart-form" => BruBody::Multipart(
            kv_map(tokens).into_iter().map(|(key, value)| BruKeyValue {
                disabled: key.starts_with('~'), key, value,
            }).collect()
        ),
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
    let get = |k: &str| map.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone()).unwrap_or_default();

    doc.auth = Some(match subtype {
        "bearer" => BruAuth::Bearer { token: get("token") },
        "basic"  => BruAuth::Basic { username: get("username"), password: get("password") },
        "awsv4"  => BruAuth::AwsV4 {
            access_key_id: get("accessKeyId"),
            secret_access_key: get("secretAccessKey"),
            session_token: map.iter().find(|(k, _)| k == "sessionToken").map(|(_, v)| v.clone()),
            service: map.iter().find(|(k, _)| k == "service").map(|(_, v)| v.clone()),
            region: map.iter().find(|(k, _)| k == "region").map(|(_, v)| v.clone()),
            profile_name: map.iter().find(|(k, _)| k == "profileName").map(|(_, v)| v.clone()),
        },
        "apikey" => BruAuth::ApiKey {
            key: get("key"),
            value: get("value"),
            placement: get("placement"),
        },
        "digest" => BruAuth::Digest { username: get("username"), password: get("password") },
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
```

- [ ] **Step 3: Run all tests**

```bash
cargo test -p rocket-import bru::
```
Expected: ALL PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-import/src/bru/
git commit -m "feat(import): .bru parser — tokenises DSL and builds BruDocument AST"
```
