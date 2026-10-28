//! A bounded, plain read model of a schema for the spec viewer.
//!
//! It holds shapes and short texts only. Examples and defaults can carry secrets, so they are
//! never copied, and a long description is cut.

use crate::openapi::reader::REF_MARKER_KEY;
use serde_json::Value as Json;

/// Deepest schema level that is shown.
pub(crate) const MAX_DEPTH: usize = 6;
/// Most schema nodes shown for one schema.
pub(crate) const MAX_NODES: usize = 200;
const MAX_ENUM: usize = 20;
const MAX_TEXT: usize = 300;

/// One node of a schema. `kind` is `object`, `array`, `string`, `integer`, `number`, `boolean`,
/// `null` or `any`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaView {
    pub kind: String,
    pub format: Option<String>,
    pub description: Option<String>,
    pub required: bool,
    pub nullable: bool,
    pub enum_values: Vec<String>,
    /// The name of a schema that this node refers back to, when the reference was cut.
    pub ref_name: Option<String>,
    pub properties: Vec<(String, SchemaView)>,
    pub items: Option<Box<SchemaView>>,
    /// The alternatives of `oneOf` or `anyOf`.
    pub variants: Vec<SchemaView>,
    /// True when part of this node was left out by the depth or size limit.
    pub cut: bool,
}

/// One media type of a body with its schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyView {
    pub content_type: String,
    pub required: bool,
    pub schema: Option<SchemaView>,
}

/// A schema left out because the whole view is large. It shows as cut with nothing inside.
pub fn omitted() -> SchemaView {
    let mut node = SchemaView::empty("any");
    node.cut = true;
    node
}

impl SchemaView {
    /// The number of nodes in this tree, itself included.
    pub fn node_count(&self) -> usize {
        1 + self
            .properties
            .iter()
            .map(|(_, p)| p.node_count())
            .sum::<usize>()
            + self.items.as_ref().map_or(0, |i| i.node_count())
            + self
                .variants
                .iter()
                .map(SchemaView::node_count)
                .sum::<usize>()
    }

    fn empty(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            format: None,
            description: None,
            required: false,
            nullable: false,
            enum_values: Vec::new(),
            ref_name: None,
            properties: Vec::new(),
            items: None,
            variants: Vec::new(),
            cut: false,
        }
    }
}

/// Reads a schema, with local references already inlined, into a bounded view.
pub(crate) fn schema_view(schema: &Json) -> SchemaView {
    let mut budget = MAX_NODES;
    build(schema, 0, &mut budget)
}

fn text_of(value: &Json, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Json::as_str)
        .filter(|t| !t.is_empty())
        .map(|t| t.chars().take(MAX_TEXT).collect())
}

/// The first non-null type name, and whether `null` is allowed (OpenAPI 3.1 type lists).
fn type_of(schema: &Json) -> (Option<String>, bool) {
    let nullable = schema.get("nullable").and_then(Json::as_bool) == Some(true);
    match schema.get("type") {
        Some(Json::String(t)) => (Some(t.clone()), nullable || t == "null"),
        Some(Json::Array(list)) => {
            let names: Vec<&str> = list.iter().filter_map(Json::as_str).collect();
            let has_null = names.contains(&"null");
            let first = names.iter().find(|n| **n != "null").map(|n| n.to_string());
            (first, nullable || has_null)
        }
        _ => (None, nullable),
    }
}

fn required_names(schema: &Json) -> Vec<String> {
    schema
        .get("required")
        .and_then(Json::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Collects the properties of a schema and of its `allOf` parts, with the names marked required.
fn collect_properties<'a>(
    schema: &'a Json,
    props: &mut Vec<(String, &'a Json)>,
    required: &mut Vec<String>,
) {
    required.extend(required_names(schema));
    if let Some(map) = schema.get("properties").and_then(Json::as_object) {
        for (name, value) in map {
            if !props.iter().any(|(n, _)| n == name) {
                props.push((name.clone(), value));
            }
        }
    }
    if let Some(parts) = schema.get("allOf").and_then(Json::as_array) {
        for part in parts {
            collect_properties(part, props, required);
        }
    }
}

fn build(schema: &Json, depth: usize, budget: &mut usize) -> SchemaView {
    if *budget == 0 {
        let mut node = SchemaView::empty("any");
        node.cut = true;
        return node;
    }
    *budget -= 1;
    if let Some(name) = schema.get(REF_MARKER_KEY).and_then(Json::as_str) {
        let mut node = SchemaView::empty("any");
        node.ref_name = Some(name.to_string());
        return node;
    }
    let (declared, nullable) = type_of(schema);
    let mut props: Vec<(String, &Json)> = Vec::new();
    let mut required: Vec<String> = Vec::new();
    collect_properties(schema, &mut props, &mut required);
    let alternatives: Vec<&Json> = ["oneOf", "anyOf"]
        .iter()
        .filter_map(|k| schema.get(*k).and_then(Json::as_array))
        .flatten()
        .collect();
    let kind = match declared.as_deref() {
        Some(t @ ("object" | "array" | "string" | "integer" | "number" | "boolean" | "null")) => t,
        _ if !props.is_empty() => "object",
        _ if schema.get("items").is_some() => "array",
        _ => "any",
    };
    let mut node = SchemaView::empty(kind);
    node.nullable = nullable;
    node.format = text_of(schema, "format");
    node.description = text_of(schema, "description");
    node.enum_values = schema
        .get("enum")
        .and_then(Json::as_array)
        .map(|list| {
            list.iter()
                .take(MAX_ENUM)
                .map(|v| match v {
                    Json::String(s) => s.chars().take(MAX_TEXT).collect(),
                    other => other.to_string().chars().take(MAX_TEXT).collect(),
                })
                .collect()
        })
        .unwrap_or_default();
    if depth >= MAX_DEPTH {
        node.cut = !props.is_empty() || schema.get("items").is_some() || !alternatives.is_empty();
        return node;
    }
    for (name, value) in props {
        if *budget == 0 {
            node.cut = true;
            break;
        }
        let mut child = build(value, depth + 1, budget);
        child.required = required.contains(&name);
        node.properties.push((name, child));
    }
    if let Some(items) = schema.get("items") {
        node.items = Some(Box::new(build(items, depth + 1, budget)));
    }
    for alternative in alternatives {
        if *budget == 0 {
            node.cut = true;
            break;
        }
        node.variants.push(build(alternative, depth + 1, budget));
    }
    node
}

/// The bodies of a media list, each with the required flag of the body.
pub(crate) fn body_views(
    media: &[crate::openapi::model::MediaSpec],
    required: bool,
) -> Vec<BodyView> {
    media
        .iter()
        .map(|m| BodyView {
            content_type: m.content_type.clone(),
            required,
            schema: m.schema.as_ref().map(schema_view),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::sync::spec::ParsedSpec;
    use serde_json::json;

    fn obj() -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": {"type": "integer", "format": "int64", "description": "The id."},
                "name": {"type": "string", "nullable": true}
            }
        })
    }

    #[test]
    fn an_object_schema_lists_properties_with_required_flags() {
        let view = schema_view(&obj());
        assert_eq!(view.kind, "object");
        assert_eq!(view.properties.len(), 2);
        let (id_name, id) = &view.properties[0];
        assert_eq!(id_name, "id");
        assert!(id.required);
        assert_eq!(id.kind, "integer");
        assert_eq!(id.format.as_deref(), Some("int64"));
        assert_eq!(id.description.as_deref(), Some("The id."));
        let (_, name) = &view.properties[1];
        assert!(!name.required && name.nullable);
    }

    #[test]
    fn an_array_schema_has_items() {
        let view = schema_view(&json!({"type": "array", "items": {"type": "string"}}));
        assert_eq!(view.kind, "array");
        assert_eq!(view.items.expect("items").kind, "string");
    }

    #[test]
    fn allof_is_merged_into_properties() {
        let view = schema_view(&json!({
            "allOf": [
                {"type": "object", "required": ["a"], "properties": {"a": {"type": "string"}}},
                {"type": "object", "properties": {"b": {"type": "integer"}}}
            ]
        }));
        assert_eq!(view.kind, "object");
        let names: Vec<&str> = view.properties.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        assert!(view.properties[0].1.required);
    }

    #[test]
    fn oneof_lists_variants() {
        let view = schema_view(&json!({
            "oneOf": [{"type": "string"}, {"type": "integer"}]
        }));
        assert_eq!(view.variants.len(), 2);
        assert_eq!(view.variants[1].kind, "integer");
    }

    #[test]
    fn a_cyclic_schema_is_cut_with_a_reference_name() {
        let view = schema_view(&json!({
            "type": "object",
            "properties": {"child": {"x-rocket-ref": "Node"}}
        }));
        let child = &view.properties[0].1;
        assert_eq!(child.ref_name.as_deref(), Some("Node"));
        assert!(child.properties.is_empty());
    }

    #[test]
    fn a_large_schema_is_capped_and_marked() {
        let mut props = serde_json::Map::new();
        for i in 0..500 {
            props.insert(format!("p{i}"), json!({"type": "string"}));
        }
        let view = schema_view(&json!({"type": "object", "properties": props}));
        assert!(view.properties.len() <= MAX_NODES);
        assert!(view.cut, "the object says it was cut");
        // Depth is capped too.
        let mut deep = json!({"type": "string"});
        for _ in 0..20 {
            deep = json!({"type": "object", "properties": {"n": deep}});
        }
        let mut node = schema_view(&deep);
        let mut depth = 0;
        while let Some((_, next)) = node.properties.into_iter().next() {
            node = next;
            depth += 1;
        }
        assert!(depth <= MAX_DEPTH, "depth was {depth}");
        assert!(node.cut);
    }

    #[test]
    fn examples_and_defaults_are_not_in_the_view() {
        let view = schema_view(&json!({
            "type": "string",
            "example": "sk_live_secret",
            "default": "sk_live_default",
            "description": "A token."
        }));
        let text = format!("{view:?}");
        assert!(!text.contains("sk_live"), "{text}");
    }

    #[test]
    fn enum_values_are_capped_at_20() {
        let values: Vec<String> = (0..50).map(|i| format!("v{i}")).collect();
        let view = schema_view(&json!({"type": "string", "enum": values}));
        assert_eq!(view.enum_values.len(), 20);
        assert_eq!(view.enum_values[0], "v0");
    }

    const SPEC: &str = r#"
openapi: 3.0.3
info: { title: T, version: '1' }
servers:
  - url: https://a.example.com
security:
  - bearer: []
paths:
  /pets:
    post:
      operationId: addPet
      security:
        - bearer: []
        - key: []
      requestBody:
        required: true
        content:
          application/json:
            schema: { type: object, properties: { name: { type: string } } }
          application/xml:
            schema: { type: object, properties: { name: { type: string } } }
      responses:
        '201':
          description: created
          content:
            application/json:
              schema: { type: object, properties: { id: { type: integer } } }
        '204': { description: none }
components:
  securitySchemes:
    bearer: { type: http, scheme: bearer }
    key: { type: apiKey, name: X-Key, in: header }
"#;

    #[test]
    fn a_request_body_lists_each_content_type() {
        let spec = ParsedSpec::parse(SPEC).expect("parse");
        let op = &spec.operation_views()[0];
        let types: Vec<&str> = op
            .request_body
            .iter()
            .map(|b| b.content_type.as_str())
            .collect();
        assert_eq!(types, ["application/json", "application/xml"]);
        assert!(op.request_body[0].required);
        assert_eq!(
            op.request_body[0].schema.as_ref().expect("schema").kind,
            "object"
        );
    }

    #[test]
    fn responses_carry_their_bodies() {
        let spec = ParsedSpec::parse(SPEC).expect("parse");
        let op = &spec.operation_views()[0];
        let created = op
            .responses
            .iter()
            .find(|r| r.status == "201")
            .expect("201");
        assert_eq!(created.bodies.len(), 1);
        assert_eq!(created.bodies[0].content_type, "application/json");
        let none = op
            .responses
            .iter()
            .find(|r| r.status == "204")
            .expect("204");
        assert!(none.bodies.is_empty());
    }

    #[test]
    fn security_lists_scheme_names() {
        let spec = ParsedSpec::parse(SPEC).expect("parse");
        assert_eq!(spec.operation_views()[0].security, ["bearer", "key"]);
    }

    #[test]
    fn server_urls_are_listed_up_to_10() {
        let mut text = String::from("openapi: 3.0.3\ninfo: { title: T, version: '1' }\nservers:\n");
        for i in 0..15 {
            text.push_str(&format!("  - url: https://s{i}.example.com\n"));
        }
        text.push_str("paths: {}\n");
        let spec = ParsedSpec::parse(&text).expect("parse");
        assert_eq!(spec.server_urls().len(), 10);
        assert_eq!(spec.server_urls()[0], "https://s0.example.com");
    }

    #[test]
    fn a_self_referencing_schema_keeps_the_name_in_the_view() {
        let text = r#"
openapi: 3.0.3
info: { title: T, version: '1' }
paths:
  /n:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema: { $ref: '#/components/schemas/Node' }
components:
  schemas:
    Node:
      type: object
      properties:
        next: { $ref: '#/components/schemas/Node' }
"#;
        let spec = ParsedSpec::parse(text).expect("parse");
        let op = &spec.operation_views()[0];
        let schema = op.responses[0].bodies[0].schema.as_ref().expect("schema");
        let next = &schema.properties[0].1;
        assert_eq!(next.ref_name.as_deref(), Some("Node"));
    }

    #[test]
    fn server_urls_hide_credentials_and_queries() {
        let text = "openapi: 3.0.3\ninfo: { title: T, version: '1' }\nservers:\n  - url: https://user:pass@api.example.com/v1?api_key=abc#top\n  - url: https://ok.example.com/v2\npaths: {}\n";
        let spec = ParsedSpec::parse(text).expect("parse");
        assert_eq!(
            spec.server_urls(),
            ["https://api.example.com/v1", "https://ok.example.com/v2"]
        );
    }

    #[test]
    fn node_count_counts_every_node() {
        let view = schema_view(&json!({
            "type": "object",
            "properties": {"a": {"type": "string"}, "b": {"type": "array", "items": {"type": "string"}}}
        }));
        assert_eq!(view.node_count(), 4);
        assert!(omitted().cut && omitted().properties.is_empty());
    }
}
