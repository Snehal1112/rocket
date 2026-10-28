//! The public handle for a parsed spec. Callers outside this crate cannot see the neutral
//! document, so they parse the spec text into this and hand it to the sync functions.

use crate::error::ImportResult;
use crate::openapi::model::ParamLocation;
use crate::openapi::sync::schema_view::{body_views, BodyView};
use crate::openapi::{parse_openapi, OpenApiDoc};

/// One parameter of an operation, as shown in the spec viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecParamView {
    pub name: String,
    /// `path`, `query`, `header` or `cookie`.
    pub location: String,
    pub required: bool,
}

/// One documented response of an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecResponseView {
    pub status: String,
    pub description: String,
    /// The media types of the response with their schemas. Empty for a response without a body.
    pub bodies: Vec<BodyView>,
}

/// A read-only summary of one operation. It holds names and short texts, never the raw spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecOperationView {
    pub key: String,
    pub method: String,
    pub path: String,
    /// The first tag, or `Untagged`.
    pub tag: String,
    pub summary: Option<String>,
    pub parameters: Vec<SpecParamView>,
    pub responses: Vec<SpecResponseView>,
    /// One entry per media type of the request body. Empty when the operation has no body.
    pub request_body: Vec<BodyView>,
    /// Names of the security schemes the operation uses. Empty means none.
    pub security: Vec<String>,
}

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

    /// The `info.version` text, or `None` when the spec leaves it empty.
    pub fn version(&self) -> Option<&str> {
        Some(self.doc.version.as_str()).filter(|v| !v.is_empty())
    }

    /// The request the importer writes for the operation with this key, as a sync plan builds
    /// it (the sequence number is the operation's place in the spec).
    pub fn converted_request(&self, key: &str) -> Option<rocket_collection::Request> {
        let (index, op) = self
            .doc
            .operations
            .iter()
            .enumerate()
            .find(|(_, o)| o.key == key)?;
        let seq = u32::try_from(index + 1).unwrap_or(u32::MAX);
        Some(
            crate::converter::openapi::convert_operation(op, &self.doc.security_schemes, seq)
                .request,
        )
    }

    /// The server URLs of the spec in document order. Server variables use their defaults.
    pub fn servers(&self) -> &[String] {
        &self.doc.servers
    }

    /// The server URLs for display, at most 10.
    pub fn server_urls(&self) -> Vec<String> {
        self.doc.servers.iter().take(10).cloned().collect()
    }

    /// The `info.title` text.
    pub fn title(&self) -> &str {
        &self.doc.title
    }

    /// The operations in document order, as read-only summaries.
    pub fn operation_views(&self) -> Vec<SpecOperationView> {
        self.doc
            .operations
            .iter()
            .map(|op| SpecOperationView {
                key: op.key.clone(),
                method: op.method.to_string().to_uppercase(),
                path: op.path.clone(),
                tag: op
                    .tags
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Untagged".to_string()),
                summary: op.summary.clone(),
                parameters: op
                    .parameters
                    .iter()
                    .map(|p| SpecParamView {
                        name: p.name.clone(),
                        location: match p.location {
                            ParamLocation::Path => "path",
                            ParamLocation::Query => "query",
                            ParamLocation::Header => "header",
                            ParamLocation::Cookie => "cookie",
                        }
                        .to_string(),
                        required: p.required,
                    })
                    .collect(),
                responses: op
                    .responses
                    .iter()
                    .map(|(status, description)| SpecResponseView {
                        status: status.clone(),
                        description: description.clone(),
                        bodies: op
                            .response_media
                            .iter()
                            .find(|(s, _)| s == status)
                            .map(|(_, media)| body_views(media, false))
                            .unwrap_or_default(),
                    })
                    .collect(),
                request_body: body_views(&op.request_media, op.request_body_required),
                security: op.security.clone(),
            })
            .collect()
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
        assert_eq!(
            ParsedSpec::parse(text).expect("parses").operation_count(),
            1
        );
    }

    const PETS: &str = r#"{"openapi":"3.0.3","info":{"title":"Pets API","version":"2.1"},
        "paths":{
          "/pets":{"get":{"operationId":"listPets","summary":"List pets","tags":["pets"],
            "parameters":[{"name":"limit","in":"query","schema":{"type":"integer"}}],
            "responses":{"200":{"description":"A list"},"404":{"description":"Missing"}}}},
          "/pets/{id}":{"get":{"operationId":"getPet","tags":["pets","extra"],
            "parameters":[{"name":"id","in":"path","required":true,"schema":{"type":"string"}}],
            "responses":{"200":{"description":"One pet"}}}},
          "/health":{"get":{"responses":{"200":{"description":"ok"}}}}
        }}"#;

    #[test]
    fn operation_views_list_method_path_tag_and_summary() {
        let views = ParsedSpec::parse(PETS).expect("parses").operation_views();
        assert_eq!(views.len(), 3);
        assert_eq!(views[0].key, "listPets");
        assert_eq!(views[0].method, "GET");
        assert_eq!(views[0].path, "/pets");
        assert_eq!(views[0].tag, "pets");
        assert_eq!(views[0].summary.as_deref(), Some("List pets"));
        // The first tag wins.
        assert_eq!(views[1].tag, "pets");
    }

    #[test]
    fn an_operation_without_tags_goes_to_untagged() {
        let views = ParsedSpec::parse(PETS).expect("parses").operation_views();
        assert_eq!(views[2].tag, "Untagged");
        assert_eq!(views[2].key, "GET /health");
    }

    #[test]
    fn parameters_and_responses_are_listed() {
        let views = ParsedSpec::parse(PETS).expect("parses").operation_views();
        let get_pet = &views[1];
        assert_eq!(
            get_pet.parameters,
            vec![SpecParamView {
                name: "id".to_string(),
                location: "path".to_string(),
                required: true
            }]
        );
        let list = &views[0];
        assert_eq!(list.parameters[0].location, "query");
        assert!(!list.parameters[0].required);
        let statuses: Vec<(&str, &str)> = list
            .responses
            .iter()
            .map(|r| (r.status.as_str(), r.description.as_str()))
            .collect();
        assert_eq!(statuses, vec![("200", "A list"), ("404", "Missing")]);
    }

    #[test]
    fn unquoted_numeric_response_codes_are_listed() {
        let yaml = "openapi: 3.0.3\ninfo: {title: T, version: '1'}\npaths:\n  /a:\n    get:\n      responses:\n        200: {description: fine}\n";
        let views = ParsedSpec::parse(yaml).expect("parses").operation_views();
        assert_eq!(views[0].responses[0].status, "200");
        assert_eq!(views[0].responses[0].description, "fine");
    }

    #[test]
    fn converted_request_builds_the_importer_request_for_a_key() {
        let spec = ParsedSpec::parse(PETS).expect("parses");
        let request = spec.converted_request("getPet").expect("request");
        assert_eq!(request.method.to_string().to_uppercase(), "GET");
        assert!(request.url.ends_with("/pets/:id"), "{}", request.url);
        assert!(spec.converted_request("missing").is_none());
    }

    #[test]
    fn servers_lists_the_server_urls() {
        let text = r#"{"openapi":"3.0.3","info":{"title":"T","version":"1"},
            "servers":[{"url":"https://api.example.com/v1"}],
            "paths":{"/a":{"get":{"responses":{"200":{"description":"ok"}}}}}}"#;
        let spec = ParsedSpec::parse(text).expect("parses");
        assert_eq!(spec.servers(), ["https://api.example.com/v1".to_string()]);
    }

    #[test]
    fn title_returns_info_title() {
        assert_eq!(ParsedSpec::parse(PETS).expect("parses").title(), "Pets API");
    }
}
