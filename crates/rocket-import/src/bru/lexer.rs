use crate::error::ImportResult;

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

        // Block opening: `name {`, `name:subtype {`, or `name:subtype [` (list blocks)
        if trimmed.ends_with('{') || trimmed.ends_with('[') {
            let is_list = trimmed.ends_with('[');
            let closer = if is_list { "]" } else { "}" };
            let header = trimmed.trim_end_matches(|c| c == '{' || c == '[').trim();
            let (name, subtype) = if let Some((n, s)) = header.split_once(':') {
                (n.trim().to_string(), Some(s.trim().to_string()))
            } else {
                (header.to_string(), None)
            };

            // List blocks and raw-text blocks both capture content verbatim.
            let is_raw = is_list || RAW_TEXT_BLOCK_NAMES.contains(&name.as_str());
            tokens.push(Token::BlockOpen { name: name.clone(), subtype });

            // Collect block body.
            let mut raw_lines: Vec<&str> = Vec::new();
            loop {
                match lines.next() {
                    None => break,
                    Some(inner) => {
                        let inner_trimmed = inner.trim();
                        if inner_trimmed == closer {
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
