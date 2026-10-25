//! Reads OpenAPI 3.0, 3.1 and Swagger 2.0 documents (JSON or YAML) into the neutral model.

use crate::error::{ImportError, ImportResult};
use crate::openapi::model::{
    OpenApiDoc, Operation, ParamLocation, Parameter, RequestBodySpec, SecurityScheme,
    SecuritySchemeKind,
};
use rocket_shared::types::HttpMethod;
use serde_json::Value as Json;
use serde_yaml::Value as Yaml;
use std::collections::HashSet;

/// Longest `$ref` chain the reader follows to one item.
const MAX_REF_DEPTH: usize = 32;
/// Deepest `$ref` nesting inlined into one schema. Examples are built to depth 8, so deeper
/// levels are never read.
const MAX_INLINE_DEPTH: usize = 10;
/// Most nodes one schema, parameter or body is expanded to.
const MAX_SCHEMA_NODES: usize = 10_000;

const METHODS: [(&str, HttpMethod); 8] = [
    ("get", HttpMethod::Get),
    ("put", HttpMethod::Put),
    ("post", HttpMethod::Post),
    ("delete", HttpMethod::Delete),
    ("options", HttpMethod::Options),
    ("head", HttpMethod::Head),
    ("patch", HttpMethod::Patch),
    ("trace", HttpMethod::Trace),
];

/// Read an OpenAPI 3.0, 3.1 or Swagger 2.0 document (JSON or YAML) into the neutral model.
///
/// The document is walked as YAML so paths and operations keep their document order.
pub fn parse_openapi(text: &str) -> ImportResult<OpenApiDoc> {
    let root: Yaml = serde_yaml::from_str(text).map_err(|e| {
        ImportError::NotAnOpenApiSpec(format!("the text is not valid JSON or YAML: {e}"))
    })?;
    let is_v2 = spec_is_v2(&root)?;
    let mut warnings = Vec::new();

    let info = root.get("info");
    let title = info.and_then(|i| str_of(i, "title")).unwrap_or_default();
    let version = info.and_then(|i| str_of(i, "version")).unwrap_or_default();
    let servers = if is_v2 {
        servers_v2(&root)
    } else {
        servers_v3(&root)
    };
    let security_schemes = security_schemes(&root, is_v2);
    let operations = operations(&root, is_v2, &mut warnings);

    Ok(OpenApiDoc {
        title,
        version,
        servers,
        operations,
        security_schemes,
        warnings,
    })
}

fn spec_is_v2(root: &Yaml) -> ImportResult<bool> {
    if let Some(v) = root.get("openapi").and_then(Yaml::as_str) {
        return if v.starts_with("3.") {
            Ok(false)
        } else {
            Err(ImportError::NotAnOpenApiSpec(format!(
                "unsupported OpenAPI version {v}"
            )))
        };
    }
    if let Some(v) = root.get("swagger") {
        let text = v
            .as_str()
            .map(str::to_string)
            .or_else(|| v.as_f64().map(|n| format!("{n:.1}")));
        return match text.as_deref() {
            Some("2.0") => Ok(true),
            other => Err(ImportError::NotAnOpenApiSpec(format!(
                "unsupported Swagger version {}",
                other.unwrap_or("(unreadable)")
            ))),
        };
    }
    Err(ImportError::NotAnOpenApiSpec(
        "no `openapi` or `swagger` version field".to_string(),
    ))
}

fn str_of(v: &Yaml, key: &str) -> Option<String> {
    v.get(key).and_then(Yaml::as_str).map(str::to_string)
}

fn bool_of(v: &Yaml, key: &str) -> bool {
    v.get(key).and_then(Yaml::as_bool).unwrap_or(false)
}

fn servers_v3(root: &Yaml) -> Vec<String> {
    let Some(list) = root.get("servers").and_then(Yaml::as_sequence) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|server| {
            let mut url = str_of(server, "url")?;
            if let Some(vars) = server.get("variables").and_then(Yaml::as_mapping) {
                for (name, var) in vars {
                    let (Some(name), Some(default)) = (name.as_str(), str_of(var, "default"))
                    else {
                        continue;
                    };
                    url = url.replace(&format!("{{{name}}}"), &default);
                }
            }
            // Request paths start with `/`, so a trailing slash would double it.
            Some(url.trim_end_matches('/').to_string())
        })
        .collect()
}

fn servers_v2(root: &Yaml) -> Vec<String> {
    let Some(host) = str_of(root, "host") else {
        return Vec::new();
    };
    let scheme = root
        .get("schemes")
        .and_then(Yaml::as_sequence)
        .and_then(|s| s.first())
        .and_then(Yaml::as_str)
        .unwrap_or("https");
    let base = str_of(root, "basePath").unwrap_or_default();
    let base = if base == "/" {
        ""
    } else {
        base.trim_end_matches('/')
    };
    vec![format!("{scheme}://{host}{base}")]
}

fn security_schemes(root: &Yaml, is_v2: bool) -> Vec<SecurityScheme> {
    let defs = if is_v2 {
        root.get("securityDefinitions")
    } else {
        root.get("components")
            .and_then(|c| c.get("securitySchemes"))
    };
    let Some(defs) = defs.and_then(Yaml::as_mapping) else {
        return Vec::new();
    };
    defs.iter()
        .filter_map(|(name, def)| {
            let name = name.as_str()?.to_string();
            let kind = match str_of(def, "type").as_deref() {
                Some("http") => match str_of(def, "scheme").map(|s| s.to_lowercase()).as_deref() {
                    Some("bearer") => SecuritySchemeKind::Bearer,
                    Some("basic") => SecuritySchemeKind::Basic,
                    other => SecuritySchemeKind::Unsupported(format!(
                        "http {}",
                        other.unwrap_or("(no scheme)")
                    )),
                },
                Some("basic") => SecuritySchemeKind::Basic,
                Some("apiKey") => SecuritySchemeKind::ApiKey {
                    param_name: str_of(def, "name").unwrap_or_default(),
                    in_query: str_of(def, "in").as_deref() == Some("query"),
                },
                Some("oauth2") => SecuritySchemeKind::OAuth2,
                other => SecuritySchemeKind::Unsupported(other.unwrap_or("(no type)").to_string()),
            };
            Some(SecurityScheme { name, kind })
        })
        .collect()
}

/// Follow a local `#/a/b` pointer inside the document.
fn resolve_pointer<'a>(root: &'a Yaml, pointer: &str) -> Option<&'a Yaml> {
    let path = pointer.strip_prefix("#/")?;
    let mut cur = root;
    for raw in path.split('/') {
        let seg = raw.replace("~1", "/").replace("~0", "~");
        cur = match cur {
            Yaml::Mapping(_) => cur.get(seg.as_str())?,
            Yaml::Sequence(items) => items.get(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Follow a chain of `$ref` objects to the item they point at. A remote or broken
/// reference gives `None`, and a remote one adds a warning.
fn deref<'a>(root: &'a Yaml, value: &'a Yaml, warnings: &mut Vec<String>) -> Option<&'a Yaml> {
    let mut cur = value;
    for _ in 0..MAX_REF_DEPTH {
        let Some(target) = cur.get("$ref").and_then(Yaml::as_str) else {
            return Some(cur);
        };
        if !target.starts_with("#/") {
            warnings.push(format!("remote $ref was not followed: {target}"));
            return None;
        }
        cur = resolve_pointer(root, target)?;
    }
    None
}

/// Convert a YAML value to JSON with local `$ref` objects replaced by their targets.
/// A reference that loops back on itself, or nests deeper than `MAX_INLINE_DEPTH`, becomes
/// `{}`. `budget` counts the nodes left to build, so a schema that refers to the next one
/// several times over cannot grow without bound; once it is spent, values become `null`.
fn to_json(
    root: &Yaml,
    value: &Yaml,
    depth: usize,
    visiting: &mut Vec<String>,
    budget: &mut usize,
    warnings: &mut Vec<String>,
) -> Json {
    if *budget == 0 {
        return Json::Null;
    }
    *budget -= 1;
    match value {
        Yaml::Null => Json::Null,
        Yaml::Bool(b) => Json::Bool(*b),
        Yaml::Number(n) => {
            if let Some(i) = n.as_i64() {
                Json::from(i)
            } else if let Some(u) = n.as_u64() {
                Json::from(u)
            } else {
                n.as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map_or(Json::Null, Json::Number)
            }
        }
        Yaml::String(s) => Json::String(s.clone()),
        Yaml::Sequence(items) => Json::Array(
            items
                .iter()
                .map(|v| to_json(root, v, depth, visiting, budget, warnings))
                .collect(),
        ),
        Yaml::Mapping(map) => {
            if let Some(target) = value.get("$ref").and_then(Yaml::as_str) {
                if !target.starts_with("#/") {
                    warnings.push(format!("remote $ref was not followed: {target}"));
                    return Json::Null;
                }
                if depth >= MAX_INLINE_DEPTH || visiting.iter().any(|v| v == target) {
                    return Json::Object(serde_json::Map::new());
                }
                let Some(resolved) = resolve_pointer(root, target) else {
                    return Json::Null;
                };
                visiting.push(target.to_string());
                let out = to_json(root, resolved, depth + 1, visiting, budget, warnings);
                visiting.pop();
                return out;
            }
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if let Some(key) = k.as_str() {
                    out.insert(
                        key.to_string(),
                        to_json(root, v, depth, visiting, budget, warnings),
                    );
                }
            }
            Json::Object(out)
        }
        Yaml::Tagged(tagged) => to_json(root, &tagged.value, depth, visiting, budget, warnings),
    }
}

fn schema_json(root: &Yaml, value: &Yaml, warnings: &mut Vec<String>) -> Json {
    let mut budget = MAX_SCHEMA_NODES;
    to_json(root, value, 0, &mut Vec::new(), &mut budget, warnings)
}

fn location_of(raw: &str) -> Option<ParamLocation> {
    match raw {
        "path" => Some(ParamLocation::Path),
        "query" => Some(ParamLocation::Query),
        "header" => Some(ParamLocation::Header),
        "cookie" => Some(ParamLocation::Cookie),
        _ => None,
    }
}

/// One entry of a `parameters` list. `Ok(None)` means "not a plain parameter" (a 2.0 body or
/// form field, or an unreadable one); those are handled elsewhere or skipped.
fn parse_parameter(
    root: &Yaml,
    raw: &Yaml,
    is_v2: bool,
    warnings: &mut Vec<String>,
) -> Option<Parameter> {
    let param = deref(root, raw, warnings)?;
    let name = str_of(param, "name")?;
    let location = location_of(&str_of(param, "in")?)?;
    let required = location == ParamLocation::Path || bool_of(param, "required");
    let schema = if is_v2 {
        Some(schema_json(root, param, warnings))
    } else {
        param.get("schema").map(|s| schema_json(root, s, warnings))
    };
    let example = param
        .get("example")
        .or_else(|| param.get("x-example"))
        .or_else(|| {
            param
                .get("examples")
                .and_then(Yaml::as_mapping)
                .and_then(|m| m.values().next())
                .and_then(|e| e.get("value"))
        })
        .map(|v| schema_json(root, v, warnings));
    Some(Parameter {
        name,
        location,
        required,
        example,
        schema,
        description: str_of(param, "description"),
    })
}

fn parse_request_body(
    root: &Yaml,
    op: &Yaml,
    raw_params: &[&Yaml],
    is_v2: bool,
    warnings: &mut Vec<String>,
) -> Option<RequestBodySpec> {
    if is_v2 {
        let body_param = raw_params
            .iter()
            .filter_map(|p| deref(root, p, warnings))
            .find(|p| str_of(p, "in").as_deref() == Some("body"))?;
        let content_type = first_string(op.get("consumes"))
            .or_else(|| first_string(root.get("consumes")))
            .unwrap_or_else(|| "application/json".to_string());
        return Some(RequestBodySpec {
            content_type,
            schema: body_param
                .get("schema")
                .map(|s| schema_json(root, s, warnings)),
            example: None,
        });
    }
    let body = deref(root, op.get("requestBody")?, warnings)?;
    let content = body.get("content")?.as_mapping()?;
    let (content_type, media) = content
        .iter()
        .find(|(k, _)| k.as_str() == Some("application/json"))
        .or_else(|| content.iter().next())?;
    let example = media
        .get("example")
        .or_else(|| {
            media
                .get("examples")
                .and_then(Yaml::as_mapping)
                .and_then(|m| m.values().next())
                .and_then(|e| e.get("value"))
        })
        .map(|v| schema_json(root, v, warnings));
    Some(RequestBodySpec {
        content_type: content_type.as_str()?.to_string(),
        schema: media.get("schema").map(|s| schema_json(root, s, warnings)),
        example,
    })
}

fn first_string(v: Option<&Yaml>) -> Option<String> {
    v?.as_sequence()?.first()?.as_str().map(str::to_string)
}

fn string_list(v: Option<&Yaml>) -> Vec<String> {
    v.and_then(Yaml::as_sequence)
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Names of the schemes an operation requires. An operation's own `security` replaces the
/// document-wide one, and an empty list means no security.
fn security_names(root: &Yaml, op: &Yaml) -> Vec<String> {
    let list = op.get("security").or_else(|| root.get("security"));
    let mut names = Vec::new();
    for requirement in list.and_then(Yaml::as_sequence).into_iter().flatten() {
        for key in requirement.as_mapping().into_iter().flat_map(|m| m.keys()) {
            if let Some(name) = key.as_str() {
                if !names.iter().any(|n| n == name) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

fn operations(root: &Yaml, is_v2: bool, warnings: &mut Vec<String>) -> Vec<Operation> {
    let Some(paths) = root.get("paths").and_then(Yaml::as_mapping) else {
        return Vec::new();
    };
    let mut out: Vec<Operation> = Vec::new();
    let mut used_keys: HashSet<String> = HashSet::new();

    for (path_key, path_item) in paths {
        let Some(path) = path_key.as_str() else {
            continue;
        };
        let Some(path_item) = deref(root, path_item, warnings) else {
            continue;
        };
        let Some(items) = path_item.as_mapping() else {
            continue;
        };
        let path_level: Vec<&Yaml> = path_item
            .get("parameters")
            .and_then(Yaml::as_sequence)
            .map(|s| s.iter().collect())
            .unwrap_or_default();

        for (name, op) in items {
            let Some(method) = name
                .as_str()
                .and_then(|n| METHODS.iter().find(|(m, _)| *m == n.to_lowercase()))
                .map(|(_, method)| method.clone())
            else {
                continue;
            };
            if !op.is_mapping() {
                continue;
            }
            let op_level: Vec<&Yaml> = op
                .get("parameters")
                .and_then(Yaml::as_sequence)
                .map(|s| s.iter().collect())
                .unwrap_or_default();

            let mut parameters: Vec<Parameter> = Vec::new();
            for raw in path_level.iter().chain(op_level.iter()) {
                let Some(param) = parse_parameter(root, raw, is_v2, warnings) else {
                    continue;
                };
                // An operation-level parameter replaces a path-level one with the same name and place.
                parameters.retain(|p| !(p.name == param.name && p.location == param.location));
                parameters.push(param);
            }
            let all_raw: Vec<&Yaml> = path_level.iter().chain(op_level.iter()).copied().collect();

            let operation_id = str_of(op, "operationId").filter(|s| !s.is_empty());
            let base_key = operation_id.clone().unwrap_or_else(|| {
                format!(
                    "{} {path}",
                    name.as_str().unwrap_or_default().to_uppercase()
                )
            });
            let mut key = base_key.clone();
            let mut n = 1u32;
            while !used_keys.insert(key.clone()) {
                n += 1;
                key = format!("{base_key}-{n}");
            }

            out.push(Operation {
                key,
                method,
                path: path.to_string(),
                summary: str_of(op, "summary"),
                operation_id,
                description: str_of(op, "description"),
                tags: string_list(op.get("tags")),
                parameters,
                request_body: parse_request_body(root, op, &all_raw, is_v2, warnings),
                security: security_names(root, op),
                deprecated: bool_of(op, "deprecated"),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::model::{Operation, ParamLocation};
    use rocket_shared::types::HttpMethod;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/openapi/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).expect("fixture should be readable")
    }

    fn op<'a>(doc: &'a OpenApiDoc, key: &str) -> &'a Operation {
        doc.operations
            .iter()
            .find(|o| o.key == key)
            .expect("operation should exist")
    }

    #[test]
    fn parses_3_0_json_operations() {
        let doc = parse_openapi(&fixture("orders-3.0.json")).expect("spec should parse");
        assert_eq!(doc.operations.len(), 3);
        let list = op(&doc, "listOrders");
        assert_eq!(list.method, HttpMethod::Get);
        assert_eq!(list.path, "/orders");
        assert_eq!(list.summary.as_deref(), Some("List orders"));
        assert_eq!(list.tags, vec!["Order Management".to_string()]);
        assert_eq!(list.security, vec!["bearerAuth".to_string()]);
        let get = op(&doc, "getOrder");
        let id = get
            .parameters
            .iter()
            .find(|p| p.name == "id")
            .expect("path-level parameter should be merged");
        assert_eq!(id.location, ParamLocation::Path);
        assert!(id.required);
    }

    #[test]
    fn parses_3_1_yaml_and_2_0_yaml_to_same_shape() {
        for name in ["orders-3.1.yaml", "store-2.0.yaml"] {
            let doc = parse_openapi(&fixture(name)).expect("spec should parse");
            assert_eq!(doc.title, "Orders", "{name}");
            assert_eq!(
                doc.servers.first().map(String::as_str),
                Some("https://api.example.com/v1"),
                "{name}"
            );
            let create = op(&doc, "createOrder");
            let body = create
                .request_body
                .as_ref()
                .expect("POST /orders should have a body");
            assert_eq!(body.content_type, "application/json", "{name}");
            assert!(body.schema.is_some(), "{name}");
        }
    }

    #[test]
    fn local_ref_is_resolved_and_remote_ref_warns() {
        let doc = parse_openapi(&fixture("orders-3.0.json")).expect("spec should parse");
        let page = op(&doc, "listOrders")
            .parameters
            .iter()
            .find(|p| p.name == "page")
            .expect("local $ref parameter should resolve");
        assert!(page.required);
        assert_eq!(page.example, Some(serde_json::json!(1)));

        let text = r##"
openapi: 3.0.3
info: { title: T, version: '1' }
paths:
  /a:
    get:
      parameters:
        - $ref: 'https://example.com/y.yaml#/A'
        - { name: ok, in: query }
      responses: { '200': { description: ok } }
"##;
        let doc = parse_openapi(text).expect("spec should parse");
        assert_eq!(doc.operations[0].parameters.len(), 1);
        assert_eq!(doc.operations[0].parameters[0].name, "ok");
        assert_eq!(doc.warnings.len(), 1);
    }

    #[test]
    fn self_referencing_schema_terminates() {
        let doc = parse_openapi(&fixture("recursive-3.0.yaml")).expect("spec should parse");
        let body = doc.operations[0]
            .request_body
            .as_ref()
            .expect("body should exist");
        assert!(body.schema.is_some());
    }

    #[test]
    fn duplicate_keys_get_suffix() {
        let text = r##"
openapi: 3.0.3
info: { title: T, version: '1' }
paths:
  /a:
    get: { operationId: dup, responses: { '200': { description: ok } } }
  /b:
    get: { operationId: dup, responses: { '200': { description: ok } } }
"##;
        let doc = parse_openapi(text).expect("spec should parse");
        let keys: Vec<&str> = doc.operations.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(keys, vec!["dup", "dup-2"]);
    }

    #[test]
    fn rejects_non_openapi_and_swagger_1() {
        for text in [r#"{"swagger":"1.2"}"#, r#"{"hello":1}"#, "not: [valid"] {
            let err = parse_openapi(text).expect_err("should be rejected");
            assert!(matches!(err, ImportError::NotAnOpenApiSpec(_)), "{text}");
        }
    }

    #[test]
    fn trace_method_is_kept() {
        let text = r##"
openapi: 3.0.3
info: { title: T, version: '1' }
paths:
  /a:
    trace: { responses: { '200': { description: ok } } }
"##;
        let doc = parse_openapi(text).expect("spec should parse");
        assert_eq!(doc.operations[0].method, HttpMethod::Trace);
        assert_eq!(doc.operations[0].key, "TRACE /a");
    }

    #[test]
    fn schema_chain_that_doubles_each_level_stays_fast() {
        // Each schema refers to the next one twice, so naive inlining builds 2^20 nodes.
        let mut text = String::from(
            "openapi: 3.0.3\ninfo: { title: T, version: '1' }\npaths:\n  /a:\n    post:\n      requestBody:\n        content:\n          application/json:\n            schema: { $ref: '#/components/schemas/S0' }\n      responses: { '200': { description: ok } }\ncomponents:\n  schemas:\n",
        );
        for i in 0..20 {
            text.push_str(&format!(
                "    S{i}: {{ type: object, properties: {{ a: {{ $ref: '#/components/schemas/S{n}' }}, b: {{ $ref: '#/components/schemas/S{n}' }} }} }}\n",
                n = i + 1
            ));
        }
        text.push_str("    S20: { type: string }\n");
        let started = std::time::Instant::now();
        let doc = parse_openapi(&text).expect("spec should parse");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "took {:?}",
            started.elapsed()
        );
        assert!(doc.operations[0]
            .request_body
            .as_ref()
            .expect("body")
            .schema
            .is_some());
    }

    #[test]
    fn server_url_loses_its_trailing_slash() {
        let text = r##"
openapi: 3.0.3
info: { title: T, version: '1' }
servers:
  - url: https://api.x.com/
paths: {}
"##;
        let doc = parse_openapi(text).expect("spec should parse");
        assert_eq!(doc.servers, vec!["https://api.x.com".to_string()]);
    }

    #[test]
    fn document_order_is_kept() {
        let text = r##"
openapi: 3.0.3
info: { title: T, version: '1' }
paths:
  /zebra:
    get: { responses: { '200': { description: ok } } }
  /apple:
    get: { responses: { '200': { description: ok } } }
"##;
        let doc = parse_openapi(text).expect("spec should parse");
        let paths: Vec<&str> = doc.operations.iter().map(|o| o.path.as_str()).collect();
        assert_eq!(paths, vec!["/zebra", "/apple"]);
    }
}
