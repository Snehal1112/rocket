//! Maps one OpenAPI operation to a Rocket request.

use crate::openapi::{Operation, ParamLocation, Parameter, SecurityScheme, SecuritySchemeKind};
use crate::report::SkipReason;
use rocket_collection::Request;
use rocket_shared::description::Description;
use rocket_shared::types::{
    Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, HttpMethod, PathParam,
};
use serde_json::Value as Json;
use sha2::{Digest, Sha256};

/// Version of the conversion rules. Bump it on any change to `convert_operation` output, so a
/// sync knows a baseline was built by different rules.
pub const CONVERTER_VERSION: u32 = 1;

/// The collection variable every request URL starts with.
pub(crate) const BASE_URL_VARIABLE: &str = "baseUrl";
/// Longest request name, in characters.
const MAX_NAME_CHARS: usize = 100;
/// Deepest schema level `sample_json` follows.
const MAX_SAMPLE_DEPTH: usize = 8;

/// What one operation became, and what had to be left out.
pub(crate) struct ConvertedOperation {
    pub request: Request,
    pub skipped: Vec<SkipReason>,
}

pub(crate) fn convert_operation(
    op: &Operation,
    schemes: &[SecurityScheme],
    seq: u32,
) -> ConvertedOperation {
    let mut skipped = Vec::new();
    let mut request = Request::new(
        request_name(op),
        op.method.clone(),
        format!("{{{{{BASE_URL_VARIABLE}}}}}{}", colon_path(&op.path)),
    );
    request.seq = Some(seq);
    request.tags = op.tags.clone();
    request.description = op
        .description
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .map(Description::text);

    request.path_params = path_param_names(&op.path)
        .into_iter()
        .map(|name| {
            let value = op
                .parameters
                .iter()
                .find(|p| p.location == ParamLocation::Path && p.name == name)
                .map(example_text)
                .unwrap_or_default();
            PathParam {
                name,
                value,
                description: None,
            }
        })
        .collect();

    let scheme = op
        .security
        .iter()
        .find_map(|name| schemes.iter().find(|s| &s.name == name));
    // The API key the auth block already sends, so a parameter for it is not sent twice.
    let api_key = match scheme.map(|s| &s.kind) {
        Some(SecuritySchemeKind::ApiKey {
            param_name,
            in_query,
        }) => Some((param_name.as_str(), *in_query)),
        _ => None,
    };
    let is_api_key = |param: &Parameter, in_query: bool| {
        api_key.is_some_and(|(name, query)| {
            query == in_query && name.eq_ignore_ascii_case(&param.name)
        })
    };

    for param in &op.parameters {
        match param.location {
            ParamLocation::Path => {}
            ParamLocation::Query => {
                if !is_api_key(param, true) {
                    request.query_params.push(row(param));
                }
            }
            ParamLocation::Header => {
                // Auth is handled from the security schemes, not from a header parameter.
                if !param.name.eq_ignore_ascii_case("authorization") && !is_api_key(param, false) {
                    request.headers.push(row(param));
                }
            }
            ParamLocation::Cookie => skipped.push(SkipReason::UnsupportedRequestType(format!(
                "{}: cookie parameter {} was not imported",
                op.key, param.name
            ))),
        }
    }

    request.body = op.request_body.as_ref().map(|spec| {
        let sample = spec
            .example
            .clone()
            .or_else(|| spec.schema.as_ref().map(|s| sample_json(s, 0)));
        convert_body(&spec.content_type, spec.schema.as_ref(), sample)
    });

    if let Some(scheme) = scheme {
        match auth_for(scheme) {
            Some(auth) => request.auth = auth,
            None => skipped.push(SkipReason::UnsupportedAuthType(scheme.name.clone())),
        }
    }

    ConvertedOperation { request, skipped }
}

/// The request name: `summary`, else `operationId`, else `METHOD /path`. Whitespace is
/// collapsed so a long Markdown summary stays on one line, and the result is cut to 100 characters.
pub(crate) fn request_name(op: &Operation) -> String {
    let collapse = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = op
        .summary
        .as_deref()
        .map(collapse)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            op.operation_id
                .as_deref()
                .map(collapse)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| format!("{} {}", method_name(&op.method), op.path));
    name.chars().take(MAX_NAME_CHARS).collect()
}

/// Names of the collection variables a scheme's auth placeholders refer to.
pub(crate) fn security_variable_names(scheme: &SecurityScheme) -> Vec<String> {
    let base = variable_base(&scheme.name);
    match scheme.kind {
        SecuritySchemeKind::Bearer => vec![format!("{base}_token")],
        SecuritySchemeKind::Basic => vec![format!("{base}_username"), format!("{base}_password")],
        SecuritySchemeKind::ApiKey { .. } => vec![format!("{base}_value")],
        SecuritySchemeKind::OAuth2 | SecuritySchemeKind::Unsupported(_) => Vec::new(),
    }
}

/// Hex SHA-256 over the fields a sync would overwrite: method, URL, query and path
/// parameters, headers, body and auth. Scripts, tests, docs, examples, name and order are
/// left out, so editing them never makes a request look changed.
pub fn baseline_hash(request: &Request) -> String {
    let canonical = serde_json::json!({
        "method": request.method,
        "url": request.url,
        "query": request.query_params,
        "path": request.path_params,
        "headers": request.headers,
        "body": request.body,
        "auth": request.auth,
    });
    let digest = Sha256::digest(canonical.to_string().as_bytes());
    format!("{digest:x}")
}

/// A sample value for a schema: `example`, then `default`, then the first `enum` value,
/// else a placeholder of the right type. Stops at 8 levels.
pub(crate) fn sample_json(schema: &Json, depth: usize) -> Json {
    if depth >= MAX_SAMPLE_DEPTH {
        return Json::Null;
    }
    for key in ["example", "default"] {
        if let Some(v) = schema.get(key) {
            return v.clone();
        }
    }
    if let Some(first) = schema
        .get("enum")
        .and_then(Json::as_array)
        .and_then(|e| e.first())
    {
        return first.clone();
    }
    for key in ["allOf", "oneOf", "anyOf"] {
        if let Some(first) = schema
            .get(key)
            .and_then(Json::as_array)
            .and_then(|a| a.first())
        {
            return sample_json(first, depth + 1);
        }
    }
    let kind = schema.get("type").and_then(Json::as_str);
    match kind {
        Some("object") | None if schema.get("properties").is_some() || kind == Some("object") => {
            let mut out = serde_json::Map::new();
            if let Some(props) = schema.get("properties").and_then(Json::as_object) {
                for (name, prop) in props {
                    out.insert(name.clone(), sample_json(prop, depth + 1));
                }
            }
            Json::Object(out)
        }
        Some("array") => match schema.get("items") {
            Some(items) => Json::Array(vec![sample_json(items, depth + 1)]),
            None => Json::Array(Vec::new()),
        },
        Some("string") => Json::String("string".to_string()),
        Some("integer") | Some("number") => Json::from(0),
        Some("boolean") => Json::Bool(false),
        _ => Json::Null,
    }
}

fn method_name(method: &HttpMethod) -> String {
    match method {
        HttpMethod::Get => "GET".to_string(),
        HttpMethod::Post => "POST".to_string(),
        HttpMethod::Put => "PUT".to_string(),
        HttpMethod::Patch => "PATCH".to_string(),
        HttpMethod::Delete => "DELETE".to_string(),
        HttpMethod::Options => "OPTIONS".to_string(),
        HttpMethod::Head => "HEAD".to_string(),
        HttpMethod::Trace => "TRACE".to_string(),
        HttpMethod::Connect => "CONNECT".to_string(),
        HttpMethod::Custom(name) => name.to_uppercase(),
    }
}

/// `/orders/{id}` becomes `/orders/:id`.
fn colon_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        match rest[start..].find('}') {
            Some(len) => {
                out.push_str(&rest[..start]);
                out.push(':');
                out.push_str(&rest[start + 1..start + len]);
                rest = &rest[start + len + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

fn path_param_names(path: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        let name = &rest[start + 1..start + len];
        if !name.is_empty() && !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
        rest = &rest[start + len + 1..];
    }
    names
}

fn variable_base(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn json_text(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Null => String::new(),
        other => other.to_string(),
    }
}

fn example_text(param: &Parameter) -> String {
    param
        .example
        .as_ref()
        .or_else(|| param.schema.as_ref().and_then(|s| s.get("default")))
        .map(json_text)
        .unwrap_or_default()
}

/// A query or header row: on when the spec requires it, off otherwise.
fn row(param: &Parameter) -> Header {
    let value = example_text(param);
    let mut header = if param.required {
        Header::new(&param.name, value)
    } else {
        Header::disabled(&param.name, value)
    };
    header.description = param
        .description
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .map(Description::text);
    header
}

fn auth_for(scheme: &SecurityScheme) -> Option<Auth> {
    let base = variable_base(&scheme.name);
    match &scheme.kind {
        SecuritySchemeKind::Bearer => Some(Auth::Bearer {
            token: format!("{{{{{base}_token}}}}"),
        }),
        SecuritySchemeKind::Basic => Some(Auth::Basic {
            username: format!("{{{{{base}_username}}}}"),
            password: format!("{{{{{base}_password}}}}"),
        }),
        SecuritySchemeKind::ApiKey {
            param_name,
            in_query,
        } => Some(Auth::ApiKey {
            key: param_name.clone(),
            value: format!("{{{{{base}_value}}}}"),
            placement: if *in_query { "query" } else { "header" }.to_string(),
        }),
        SecuritySchemeKind::OAuth2 | SecuritySchemeKind::Unsupported(_) => None,
    }
}

fn convert_body(content_type: &str, schema: Option<&Json>, sample: Option<Json>) -> Body {
    let lower = content_type.to_lowercase();
    let properties = schema
        .and_then(|s| s.get("properties"))
        .and_then(Json::as_object);
    if lower.contains("json") {
        let content = sample.map_or_else(
            || "{}".to_string(),
            |v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".to_string()),
        );
        return text_body(BodyMode::Json, content);
    }
    if lower.contains("x-www-form-urlencoded") {
        let pairs: Vec<String> = properties
            .map(|props| {
                props
                    .iter()
                    .map(|(name, prop)| format!("{name}={}", json_text(&sample_json(prop, 1))))
                    .collect()
            })
            .unwrap_or_default();
        return text_body(BodyMode::FormUrlEncoded, pairs.join("&"));
    }
    if lower.contains("multipart/form-data") {
        let entries = properties
            .map(|props| {
                props
                    .iter()
                    .map(|(name, prop)| {
                        let is_file = prop.get("format").and_then(Json::as_str) == Some("binary");
                        FormDataEntry {
                            key: name.clone(),
                            value: if is_file {
                                String::new()
                            } else {
                                json_text(&sample_json(prop, 1))
                            },
                            entry_type: if is_file {
                                FormDataType::File
                            } else {
                                FormDataType::Text
                            },
                            enabled: true,
                            content_type: None,
                            description: None,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(entries),
            file_path: None,
        };
    }
    let content = sample.as_ref().map(json_text).unwrap_or_default();
    if lower.contains("xml") {
        return text_body(BodyMode::Xml, content);
    }
    text_body(BodyMode::Text, content)
}

fn text_body(mode: BodyMode, content: String) -> Body {
    Body {
        mode,
        content: Some(content),
        form_data: None,
        file_path: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::model::RequestBodySpec;
    use rocket_shared::types::{Auth, BodyMode, FormDataType};
    use serde_json::json;

    fn operation(method: HttpMethod, path: &str) -> Operation {
        Operation {
            key: format!("{method:?} {path}"),
            method,
            path: path.to_string(),
            summary: None,
            operation_id: None,
            description: None,
            tags: Vec::new(),
            parameters: Vec::new(),
            request_body: None,
            security: Vec::new(),
            deprecated: false,
            responses: Vec::new(),
            request_media: Vec::new(),
            request_body_required: false,
            response_media: Vec::new(),
        }
    }

    fn param(name: &str, location: ParamLocation, required: bool) -> Parameter {
        Parameter {
            name: name.to_string(),
            location,
            required,
            example: None,
            schema: None,
            description: None,
        }
    }

    fn body(
        content_type: &str,
        schema: Option<serde_json::Value>,
        example: Option<serde_json::Value>,
    ) -> Option<RequestBodySpec> {
        Some(RequestBodySpec {
            content_type: content_type.to_string(),
            schema,
            example,
        })
    }

    fn scheme(name: &str, kind: SecuritySchemeKind) -> SecurityScheme {
        SecurityScheme {
            name: name.to_string(),
            kind,
        }
    }

    #[test]
    fn url_uses_base_url_and_colon_path_params() {
        let mut op = operation(HttpMethod::Get, "/orders/{id}");
        op.parameters.push(param("id", ParamLocation::Path, true));
        let out = convert_operation(&op, &[], 1);
        assert_eq!(out.request.url, "{{baseUrl}}/orders/:id");
        assert_eq!(out.request.path_params.len(), 1);
        assert_eq!(out.request.path_params[0].name, "id");
        assert_eq!(out.request.seq, Some(1));
    }

    #[test]
    fn query_and_header_params_become_rows() {
        let mut op = operation(HttpMethod::Get, "/orders");
        let mut page = param("page", ParamLocation::Query, true);
        page.example = Some(json!(1));
        op.parameters.push(page);
        op.parameters
            .push(param("status", ParamLocation::Query, false));
        let mut trace = param("X-Trace", ParamLocation::Header, false);
        trace.example = Some(json!("abc"));
        op.parameters.push(trace);
        op.parameters
            .push(param("session", ParamLocation::Cookie, false));
        let out = convert_operation(&op, &[], 1);

        let page = &out.request.query_params[0];
        assert_eq!(
            (page.key.as_str(), page.value.as_str(), page.enabled),
            ("page", "1", true)
        );
        let status = &out.request.query_params[1];
        assert_eq!((status.key.as_str(), status.enabled), ("status", false));
        let trace = &out.request.headers[0];
        assert_eq!(
            (trace.key.as_str(), trace.value.as_str(), trace.enabled),
            ("X-Trace", "abc", false)
        );
        assert_eq!(out.skipped.len(), 1);
        assert!(matches!(
            out.skipped[0],
            SkipReason::UnsupportedRequestType(_)
        ));
    }

    #[test]
    fn json_body_uses_example_then_schema() {
        let mut op = operation(HttpMethod::Post, "/orders");
        op.request_body = body(
            "application/json",
            Some(json!({"type": "object", "properties": {"sku": {"type": "string"}}})),
            Some(json!({"sku": "A-1"})),
        );
        let with_example = convert_operation(&op, &[], 1).request.body.expect("body");
        assert_eq!(with_example.mode, BodyMode::Json);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                with_example.content.as_deref().expect("content")
            )
            .expect("json"),
            json!({"sku": "A-1"})
        );

        op.request_body = body(
            "application/json",
            Some(
                json!({"type": "object", "properties": {"sku": {"type": "string"}, "qty": {"type": "integer"}}}),
            ),
            None,
        );
        let from_schema = convert_operation(&op, &[], 1).request;
        let content = from_schema.body.expect("body").content.expect("content");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&content).expect("json"),
            json!({"sku": "string", "qty": 0})
        );
        let ct = from_schema
            .headers
            .iter()
            .filter(|h| h.key.eq_ignore_ascii_case("content-type"))
            .count();
        assert!(ct <= 1);
    }

    #[test]
    fn form_and_xml_bodies_map_modes() {
        let mut op = operation(HttpMethod::Post, "/upload");
        op.request_body = body(
            "application/x-www-form-urlencoded",
            Some(json!({"type": "object", "properties": {"a": {"type": "string"}}})),
            None,
        );
        assert_eq!(
            convert_operation(&op, &[], 1)
                .request
                .body
                .expect("body")
                .mode,
            BodyMode::FormUrlEncoded
        );

        op.request_body = body(
            "multipart/form-data",
            Some(json!({"type": "object", "properties": {
                "note": {"type": "string"},
                "file": {"type": "string", "format": "binary"}
            }})),
            None,
        );
        let multipart = convert_operation(&op, &[], 1).request.body.expect("body");
        assert_eq!(multipart.mode, BodyMode::FormData);
        let entries = multipart.form_data.expect("entries");
        let file = entries
            .iter()
            .find(|e| e.key == "file")
            .expect("file entry");
        assert_eq!(file.entry_type, FormDataType::File);
        let note = entries
            .iter()
            .find(|e| e.key == "note")
            .expect("note entry");
        assert_eq!(note.entry_type, FormDataType::Text);

        op.request_body = body("application/xml", None, Some(json!("<a/>")));
        let xml = convert_operation(&op, &[], 1).request.body.expect("body");
        assert_eq!(xml.mode, BodyMode::Xml);
        assert_eq!(xml.content.as_deref(), Some("<a/>"));
    }

    #[test]
    fn auth_references_variable_never_a_value() {
        let schemes = vec![
            scheme("bearerAuth", SecuritySchemeKind::Bearer),
            scheme("basicAuth", SecuritySchemeKind::Basic),
            scheme(
                "keyAuth",
                SecuritySchemeKind::ApiKey {
                    param_name: "X-Api-Key".into(),
                    in_query: false,
                },
            ),
            scheme(
                "qAuth",
                SecuritySchemeKind::ApiKey {
                    param_name: "key".into(),
                    in_query: true,
                },
            ),
            scheme("oauth", SecuritySchemeKind::OAuth2),
        ];
        let mut op = operation(HttpMethod::Get, "/a");

        op.security = vec!["bearerAuth".into()];
        assert_eq!(
            convert_operation(&op, &schemes, 1).request.auth,
            Auth::Bearer {
                token: "{{bearerAuth_token}}".into()
            }
        );

        op.security = vec!["basicAuth".into()];
        assert_eq!(
            convert_operation(&op, &schemes, 1).request.auth,
            Auth::Basic {
                username: "{{basicAuth_username}}".into(),
                password: "{{basicAuth_password}}".into()
            }
        );

        op.security = vec!["keyAuth".into()];
        assert_eq!(
            convert_operation(&op, &schemes, 1).request.auth,
            Auth::ApiKey {
                key: "X-Api-Key".into(),
                value: "{{keyAuth_value}}".into(),
                placement: "header".into()
            }
        );

        op.security = vec!["qAuth".into()];
        assert_eq!(
            convert_operation(&op, &schemes, 1).request.auth,
            Auth::ApiKey {
                key: "key".into(),
                value: "{{qAuth_value}}".into(),
                placement: "query".into()
            }
        );

        op.security = vec!["oauth".into()];
        let out = convert_operation(&op, &schemes, 1);
        assert_eq!(out.request.auth, Auth::None);
        assert!(matches!(out.skipped[0], SkipReason::UnsupportedAuthType(_)));
    }

    #[test]
    fn parameter_named_like_the_api_key_is_not_imported_as_a_row() {
        let schemes = vec![
            scheme(
                "keyAuth",
                SecuritySchemeKind::ApiKey {
                    param_name: "X-Api-Key".into(),
                    in_query: false,
                },
            ),
            scheme(
                "qAuth",
                SecuritySchemeKind::ApiKey {
                    param_name: "api_key".into(),
                    in_query: true,
                },
            ),
        ];
        let mut op = operation(HttpMethod::Get, "/a");
        let mut header = param("x-api-key", ParamLocation::Header, true);
        header.example = Some(json!("abc123"));
        op.parameters.push(header);
        op.parameters
            .push(param("X-Trace", ParamLocation::Header, false));
        let mut query = param("api_key", ParamLocation::Query, true);
        query.example = Some(json!("abc123"));
        op.parameters.push(query);
        op.parameters
            .push(param("page", ParamLocation::Query, false));

        op.security = vec!["keyAuth".into()];
        let out = convert_operation(&op, &schemes, 1).request;
        let header_keys: Vec<&str> = out.headers.iter().map(|h| h.key.as_str()).collect();
        assert_eq!(header_keys, vec!["X-Trace"]);
        let query_keys: Vec<&str> = out.query_params.iter().map(|h| h.key.as_str()).collect();
        assert_eq!(query_keys, vec!["api_key", "page"]);

        op.security = vec!["qAuth".into()];
        let out = convert_operation(&op, &schemes, 1).request;
        let header_keys: Vec<&str> = out.headers.iter().map(|h| h.key.as_str()).collect();
        assert_eq!(header_keys, vec!["x-api-key", "X-Trace"]);
        let query_keys: Vec<&str> = out.query_params.iter().map(|h| h.key.as_str()).collect();
        assert_eq!(query_keys, vec!["page"]);
    }

    #[test]
    fn security_variable_names_cover_every_placeholder() {
        assert_eq!(
            security_variable_names(&scheme("my scheme", SecuritySchemeKind::Bearer)),
            vec!["my_scheme_token".to_string()]
        );
        assert_eq!(
            security_variable_names(&scheme("b", SecuritySchemeKind::Basic)),
            vec!["b_username".to_string(), "b_password".to_string()]
        );
        assert!(security_variable_names(&scheme("o", SecuritySchemeKind::OAuth2)).is_empty());
    }

    #[test]
    fn name_rule_and_length_cap() {
        let mut op = operation(HttpMethod::Get, "/orders");
        op.operation_id = Some("listOrders".into());
        op.summary = Some("List orders".into());
        assert_eq!(request_name(&op), "List orders");
        op.summary = None;
        assert_eq!(request_name(&op), "listOrders");
        op.operation_id = None;
        assert_eq!(request_name(&op), "GET /orders");
        op.summary = Some("word ".repeat(100));
        assert_eq!(request_name(&op).chars().count(), 100);
        op.summary = Some("line one\n\nline   two".into());
        assert_eq!(request_name(&op), "line one line two");
    }

    #[test]
    fn baseline_hash_ignores_user_fields_but_not_imported_ones() {
        let mut op = operation(HttpMethod::Post, "/orders");
        op.request_body = body("application/json", None, Some(json!({"a": 1})));
        let base = convert_operation(&op, &[], 1).request;
        let h = baseline_hash(&base);
        assert_eq!(h.len(), 64);

        let mut user_edit = base.clone();
        user_edit.pre_request_script = Some("x".into());
        user_edit.tests = Some("y".into());
        user_edit.name = "Renamed".into();
        user_edit.seq = Some(99);
        user_edit.docs = None;
        assert_eq!(baseline_hash(&user_edit), h);

        let mut url = base.clone();
        url.url.push('x');
        assert_ne!(baseline_hash(&url), h);
        let mut header = base.clone();
        header
            .headers
            .push(rocket_shared::types::Header::new("A", "b"));
        assert_ne!(baseline_hash(&header), h);
        let mut content = base.clone();
        content.body.as_mut().expect("body").content = Some("{}".into());
        assert_ne!(baseline_hash(&content), h);
        let mut auth = base.clone();
        auth.auth = Auth::Bearer { token: "t".into() };
        assert_ne!(baseline_hash(&auth), h);
    }

    #[test]
    fn sample_json_picks_example_default_enum_then_type() {
        assert_eq!(
            sample_json(&json!({"type": "string", "example": "x"}), 0),
            json!("x")
        );
        assert_eq!(
            sample_json(&json!({"type": "integer", "default": 7}), 0),
            json!(7)
        );
        assert_eq!(
            sample_json(&json!({"type": "string", "enum": ["a", "b"]}), 0),
            json!("a")
        );
        assert_eq!(sample_json(&json!({"type": "boolean"}), 0), json!(false));
        assert_eq!(
            sample_json(&json!({"type": "array", "items": {"type": "number"}}), 0),
            json!([0])
        );
    }

    #[test]
    fn sample_json_stops_on_deep_schema() {
        let mut schema = json!({"type": "string"});
        for _ in 0..50 {
            schema = json!({"type": "object", "properties": {"n": schema}});
        }
        let mut value = sample_json(&schema, 0);
        let mut depth = 0;
        while let Some(next) = value.get("n").cloned() {
            value = next;
            depth += 1;
        }
        assert!(depth <= 8, "depth was {depth}");
    }
}
