//! A lexical scan of a GraphQL document, just enough to list its operations.
//!
//! It is not a parser. It tracks nesting and skips comments, strings and block
//! strings, so a brace or a keyword inside those never counts. A document that
//! is not valid GraphQL gives a best-effort answer and never panics.

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphQlOperationKind {
    Query,
    Mutation,
    Subscription,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlOperation {
    /// `None` for an anonymous operation.
    pub name: Option<String>,
    pub kind: GraphQlOperationKind,
}

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_char(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

const TRIPLE: [char; 3] = ['"', '"', '"'];

/// Returns the index just past the string that starts at `start`.
fn skip_string(chars: &[char], start: usize) -> usize {
    if chars.get(start..start + 3) == Some(&TRIPLE[..]) {
        let mut i = start + 3;
        while i < chars.len() {
            // `\"""` is an escaped delimiter inside a block string.
            if chars[i] == '\\' && chars.get(i + 1..i + 4) == Some(&TRIPLE[..]) {
                i += 4;
                continue;
            }
            if chars.get(i..i + 3) == Some(&TRIPLE[..]) {
                return i + 3;
            }
            i += 1;
        }
        return chars.len();
    }
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '"' | '\n' => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// Lists the operations a document defines, in order.
pub fn list_operations(document: &str) -> Vec<GraphQlOperation> {
    let chars: Vec<char> = document.chars().collect();
    let mut ops = Vec::new();
    let mut depth: usize = 0;
    // True from an operation or fragment keyword until its selection set opens,
    // so that body is not mistaken for a shorthand query.
    let mut awaiting_body = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == ',' {
            i += 1;
        } else if c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '"' {
            i = skip_string(&chars, i);
        } else if c == '{' {
            if depth == 0 {
                if !awaiting_body {
                    ops.push(GraphQlOperation {
                        name: None,
                        kind: GraphQlOperationKind::Query,
                    });
                }
                awaiting_body = false;
            }
            depth += 1;
            i += 1;
        } else if c == '(' || c == '[' {
            depth += 1;
            i += 1;
        } else if c == '}' || c == ')' || c == ']' {
            depth = depth.saturating_sub(1);
            i += 1;
        } else if is_name_start(c) {
            let start = i;
            while i < chars.len() && is_name_char(chars[i]) {
                i += 1;
            }
            if depth != 0 {
                continue;
            }
            let word: String = chars[start..i].iter().collect();
            let kind = match word.as_str() {
                "query" => Some(GraphQlOperationKind::Query),
                "mutation" => Some(GraphQlOperationKind::Mutation),
                "subscription" => Some(GraphQlOperationKind::Subscription),
                // Fragment and schema definitions have a body that is not an operation.
                "fragment" | "type" | "input" | "enum" | "interface" | "union" | "schema"
                | "extend" | "directive" => {
                    awaiting_body = true;
                    None
                }
                _ => None,
            };
            if let Some(kind) = kind {
                // The operation name, when there is one, is the next name.
                let mut j = i;
                while j < chars.len() && (chars[j].is_whitespace() || chars[j] == ',') {
                    j += 1;
                }
                let name = if j < chars.len() && is_name_start(chars[j]) {
                    let name_start = j;
                    while j < chars.len() && is_name_char(chars[j]) {
                        j += 1;
                    }
                    i = j;
                    Some(chars[name_start..j].iter().collect::<String>())
                } else {
                    None
                };
                ops.push(GraphQlOperation { name, kind });
                awaiting_body = true;
            }
        } else {
            i += 1;
        }
    }
    ops
}

/// Picks the operation name to send.
///
/// - A requested name must exist in the document.
/// - With no request, a document with one operation sends that one.
/// - With several operations and no request, the send path (`fallback_first == false`)
///   fails so the user must choose, and the runner (`fallback_first == true`) runs the first.
///
/// `Ok(None)` means the chosen operation is anonymous, so no `operationName` is sent.
pub fn select_operation(
    document: &str,
    requested: Option<&str>,
    fallback_first: bool,
) -> DomainResult<Option<String>> {
    let ops = list_operations(document);
    if ops.is_empty() {
        return Err(DomainError::InvalidInput(
            "the document has no operation to run".into(),
        ));
    }
    if let Some(name) = requested.map(str::trim).filter(|n| !n.is_empty()) {
        return if ops.iter().any(|o| o.name.as_deref() == Some(name)) {
            Ok(Some(name.to_string()))
        } else {
            Err(DomainError::InvalidInput(format!(
                "the document has no operation named '{name}'"
            )))
        };
    }
    if ops.len() == 1 || fallback_first {
        return Ok(ops[0].name.clone());
    }
    Err(DomainError::InvalidInput(format!(
        "the document defines {} operations; choose one to run",
        ops.len()
    )))
}
#[cfg(test)]
mod tests {
    use super::*;

    fn names(doc: &str) -> Vec<(Option<String>, GraphQlOperationKind)> {
        list_operations(doc)
            .into_iter()
            .map(|o| (o.name, o.kind))
            .collect()
    }

    #[test]
    fn list_operations_finds_a_shorthand_query() {
        assert_eq!(
            names("{ users { id } }"),
            vec![(None, GraphQlOperationKind::Query)]
        );
    }

    #[test]
    fn list_operations_finds_named_operations_of_every_kind() {
        let doc = "query A { a }\nmutation B($x: Int = 1) @dir { b }\nsubscription C { c }";
        assert_eq!(
            names(doc),
            vec![
                (Some("A".into()), GraphQlOperationKind::Query),
                (Some("B".into()), GraphQlOperationKind::Mutation),
                (Some("C".into()), GraphQlOperationKind::Subscription),
            ]
        );
    }

    #[test]
    fn list_operations_finds_an_anonymous_keyword_operation() {
        assert_eq!(
            names("query { a }"),
            vec![(None, GraphQlOperationKind::Query)]
        );
        assert_eq!(
            names("mutation($x: Int) { a(x: $x) }"),
            vec![(None, GraphQlOperationKind::Mutation)]
        );
    }

    #[test]
    fn list_operations_ignores_fragments() {
        let doc = "fragment F on User { id }\nquery Q { user { ...F } }";
        assert_eq!(
            names(doc),
            vec![(Some("Q".into()), GraphQlOperationKind::Query)]
        );
    }

    #[test]
    fn list_operations_ignores_comments_strings_and_block_strings() {
        let doc = "# query Fake { x }\nquery Real {\n  a(s: \"query Nope { }\", t: \"\"\"mutation Also { } \\\"\"\" still\"\"\")\n}\n";
        assert_eq!(
            names(doc),
            vec![(Some("Real".into()), GraphQlOperationKind::Query)]
        );
    }

    #[test]
    fn list_operations_handles_object_defaults_in_variable_definitions() {
        let doc = "query A($f: Filter = {a: 1, b: {c: 2}}) { x }\nquery B { y }";
        assert_eq!(
            names(doc),
            vec![
                (Some("A".into()), GraphQlOperationKind::Query),
                (Some("B".into()), GraphQlOperationKind::Query),
            ]
        );
    }

    #[test]
    fn list_operations_ignores_schema_definition_bodies() {
        let doc = "type A { a: Int }\ninput B { b: Int }\nextend type A { c: Int }\nschema { query: A }\nquery Q { a }";
        assert_eq!(
            names(doc),
            vec![(Some("Q".into()), GraphQlOperationKind::Query)]
        );
        assert!(list_operations("type A { a: Int } enum E { X Y }").is_empty());
    }

    #[test]
    fn list_operations_of_an_empty_or_broken_document_is_empty() {
        assert!(list_operations("").is_empty());
        assert!(list_operations("   # nothing\n").is_empty());
        // An unterminated string must not hang or panic.
        assert!(list_operations("query A { a(s: \"oops").len() <= 1);
    }

    #[test]
    fn select_operation_uses_the_only_operation() {
        assert_eq!(
            select_operation("query A { a }", None, false).expect("select"),
            Some("A".to_string())
        );
        assert_eq!(
            select_operation("{ a }", None, false).expect("select"),
            None
        );
    }

    #[test]
    fn select_operation_requires_a_choice_when_there_are_several() {
        let doc = "query A { a } query B { b }";
        let err = select_operation(doc, None, false).expect_err("must choose");
        assert!(err.to_string().contains("2 operations"), "got: {err}");
    }

    #[test]
    fn select_operation_can_fall_back_to_the_first_for_the_runner() {
        let doc = "query A { a } query B { b }";
        assert_eq!(
            select_operation(doc, None, true).expect("select"),
            Some("A".to_string())
        );
    }

    #[test]
    fn select_operation_validates_a_requested_name() {
        let doc = "query A { a } query B { b }";
        assert_eq!(
            select_operation(doc, Some("B"), false).expect("select"),
            Some("B".to_string())
        );
        let err = select_operation(doc, Some("Z"), false).expect_err("unknown");
        assert!(err.to_string().contains("'Z'"), "got: {err}");
    }

    #[test]
    fn select_operation_rejects_a_document_with_no_operation() {
        let err = select_operation("fragment F on U { id }", None, false).expect_err("none");
        assert!(err.to_string().contains("no operation"), "got: {err}");
    }
}
