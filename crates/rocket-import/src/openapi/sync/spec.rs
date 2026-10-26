//! The public handle for a parsed spec. Callers outside this crate cannot see the neutral
//! document, so they parse the spec text into this and hand it to the sync functions.

use crate::error::ImportResult;
use crate::openapi::{parse_openapi, OpenApiDoc};

/// A spec that was read and checked. It is opaque on purpose.
#[derive(Debug, Clone)]
pub struct ParsedSpec {
    doc: OpenApiDoc,
}

impl ParsedSpec {
    /// Reads JSON or YAML text of an OpenAPI 3.0, 3.1 or Swagger 2.0 spec.
    pub fn parse(text: &str) -> ImportResult<ParsedSpec> {
        parse_openapi(text).map(ParsedSpec::from_doc)
    }

    pub(crate) fn from_doc(doc: OpenApiDoc) -> Self {
        Self { doc }
    }

    pub(crate) fn doc(&self) -> &OpenApiDoc {
        &self.doc
    }

    /// Number of operations in the spec.
    pub fn operation_count(&self) -> usize {
        self.doc.operations.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsed_spec_rejects_a_non_spec() {
        assert!(ParsedSpec::parse("<html></html>").is_err());
        assert!(ParsedSpec::parse("just: text").is_err());
    }

    #[test]
    fn parsed_spec_counts_operations() {
        let text = r#"{"openapi":"3.0.3","info":{"title":"T","version":"1"},
            "paths":{"/a":{"get":{"responses":{"200":{"description":"ok"}}}}}}"#;
        assert_eq!(ParsedSpec::parse(text).expect("parses").operation_count(), 1);
    }
}
