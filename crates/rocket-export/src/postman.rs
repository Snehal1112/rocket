//! Postman Collection v2.1 serializer.

use std::collections::HashSet;

use rocket_collection::{
    inherited_headers, CollectionVariable, FolderSettings, GraphQlRequest, Request,
};
use rocket_shared::description::Description;
use rocket_shared::types::{
    Auth, Body, BodyMode, FormDataType, Header, PathParam, QueryParam,
};
use rocket_shared::error::DomainResult;
use serde_json::{json, Map, Value};

use crate::events::{folder_events, request_events};
use crate::postman_auth::{noauth_block, postman_auth, PostmanAuth};
use crate::scrub::{
    is_variable_reference, scrub_auth, scrub_form_entries, scrub_header_values, scrub_typed_rows,
    scrub_url,
};
use crate::sensitive::is_sensitive_name;
use crate::report::{ExportAction, ExportNote, ExportReport};
use crate::tree::{CollectionExportTree, ExportFolder, ExportItem};

pub const POSTMAN_SCHEMA: &str =
    "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

/// Options of the Postman serializer.
#[derive(Debug, Clone, Default)]
pub struct PostmanOptions {
    pub preserve_scripts: bool,
}

/// Serialize a tree to a Postman Collection v2.1 document and its report.
pub fn postman_collection(
    tree: &CollectionExportTree,
    opts: &PostmanOptions,
) -> DomainResult<(Value, ExportReport)> {
    let mut report = ExportReport::default();
    let mut info = Map::new();
    info.insert("_postman_id".into(), json!(uuid::Uuid::new_v4().to_string()));
    info.insert("name".into(), json!(tree.name));
    if let Some(docs) = &tree.settings.docs {
        info.insert("description".into(), json!(docs));
    }
    info.insert("schema".into(), json!(POSTMAN_SCHEMA));

    let items = folder_items(&tree.root, &[], &tree.settings.headers, opts, &mut report, "");
    let mut doc = Map::new();
    doc.insert("info".into(), Value::Object(info));
    doc.insert("item".into(), Value::Array(items));
    if let Some(auth) = tree.settings.auth.as_ref() {
        if let Some(a) = auth_value(auth, "(collection)", &mut report) {
            doc.insert("auth".into(), a);
        }
    }
    let vars = variables_json(&tree.settings.variables, "(collection)", &mut report);
    if !vars.is_empty() {
        doc.insert("variable".into(), Value::Array(vars));
    }
    Ok((Value::Object(doc), report))
}

/// Scrub and map an auth value. `None` means the `auth` key is omitted.
fn auth_value(auth: &Auth, path: &str, report: &mut ExportReport) -> Option<Value> {
    let mut scrub_notes = Vec::new();
    let clean = scrub_auth(auth, path, &mut scrub_notes);
    match postman_auth(&clean) {
        PostmanAuth::Omit => {
            report.items.append(&mut scrub_notes);
            None
        }
        PostmanAuth::Value(v) => {
            report.items.append(&mut scrub_notes);
            Some(v)
        }
        // A skipped auth is not exported, so its scrub notes are dropped.
        PostmanAuth::Skipped(kind) => {
            report.items.push(ExportNote {
                path: path.to_string(),
                feature: format!("auth {kind}"),
                action: ExportAction::Skipped,
                detail: None,
            });
            Some(noauth_block())
        }
    }
}

fn variables_json(
    vars: &[CollectionVariable],
    path: &str,
    report: &mut ExportReport,
) -> Vec<Value> {
    vars.iter()
        .map(|v| {
            let mut value = if v.value.is_empty() {
                v.initial_value.clone()
            } else {
                v.value.clone()
            };
            let by_name = !v.secret && is_sensitive_name(&v.key);
            if (v.secret || by_name) && !value.is_empty() && !is_variable_reference(&value) {
                value.clear();
                report.items.push(ExportNote {
                    path: path.to_string(),
                    feature: if v.secret { "Secret variable" } else { "Variable" }.into(),
                    action: ExportAction::Blanked,
                    detail: Some(if v.secret {
                        v.key.clone()
                    } else {
                        format!("{} (by name)", v.key)
                    }),
                });
            }
            let mut o = Map::new();
            o.insert("key".into(), json!(v.key));
            o.insert("value".into(), json!(value));
            o.insert("type".into(), json!("string"));
            if !v.enabled {
                o.insert("disabled".into(), json!(true));
            }
            Value::Object(o)
        })
        .collect()
}

fn sort_key(item: &ExportItem) -> (u8, u32, String) {
    match item {
        ExportItem::Folder(f) => (0, u32::MAX, f.name.clone()),
        ExportItem::Request(r) => (1, r.seq.unwrap_or(u32::MAX), r.name.clone()),
        ExportItem::GraphQl(r) => (1, r.seq.unwrap_or(u32::MAX), r.name.clone()),
        ExportItem::Skipped { .. } => (2, u32::MAX, String::new()),
    }
}

fn unique_name(used: &mut HashSet<String>, name: &str) -> String {
    let mut candidate = name.to_string();
    let mut n = 2;
    while used.contains(&candidate) {
        candidate = format!("{name} ({n})");
        n += 1;
    }
    used.insert(candidate.clone());
    candidate
}

fn folder_items(
    folder: &ExportFolder,
    chain: &[FolderSettings],
    collection_headers: &[Header],
    opts: &PostmanOptions,
    report: &mut ExportReport,
    path: &str,
) -> Vec<Value> {
    let mut sorted: Vec<&ExportItem> = folder.items.iter().collect();
    sorted.sort_by_key(|i| sort_key(i));
    let mut used = HashSet::new();
    let mut out = Vec::new();
    for item in sorted {
        match item {
            ExportItem::Folder(f) => {
                let name = unique_name(&mut used, &f.name);
                let child_path = join(path, &name);
                let mut child_chain = chain.to_vec();
                child_chain.push(f.settings.clone());
                let children = folder_items(
                    f,
                    &child_chain,
                    collection_headers,
                    opts,
                    report,
                    &child_path,
                );
                let mut obj = Map::new();
                obj.insert("name".into(), json!(name));
                if let Some(d) = &f.settings.docs {
                    obj.insert("description".into(), json!(d));
                }
                // A folder with `None` auth carries no auth, like `Inherit`.
                if let Some(a) = f
                    .settings
                    .auth
                    .as_ref()
                    .filter(|a| !matches!(a, Auth::None | Auth::Inherit))
                {
                    if let Some(v) = auth_value(a, &child_path, report) {
                        obj.insert("auth".into(), v);
                    }
                }
                let vars = variables_json(&f.settings.variables, &child_path, report);
                if !vars.is_empty() {
                    obj.insert("variable".into(), Value::Array(vars));
                }
                let events = folder_events(&f.settings, opts, &child_path, &mut report.items);
                if !events.is_empty() {
                    obj.insert("event".into(), Value::Array(events));
                }
                obj.insert("item".into(), Value::Array(children));
                out.push(Value::Object(obj));
            }
            ExportItem::Request(r) => {
                let name = unique_name(&mut used, &r.name);
                let headers = inherited_headers(collection_headers, chain);
                if !r.examples.is_empty() {
                    report.items.push(ExportNote {
                        path: join(path, &name),
                        feature: "request examples".into(),
                        action: ExportAction::Skipped,
                        detail: None,
                    });
                }
                out.push(request_item(&name, r, &headers, opts, &join(path, &name), report));
                report.exported += 1;
            }
            ExportItem::GraphQl(r) => {
                let name = unique_name(&mut used, &r.name);
                let headers = inherited_headers(collection_headers, chain);
                out.push(graphql_item(&name, r, &headers, &join(path, &name), report));
                report.exported += 1;
            }
            ExportItem::Skipped { path, kind } => report.items.push(ExportNote {
                path: path.clone(),
                feature: kind.clone(),
                action: ExportAction::Skipped,
                detail: None,
            }),
        }
    }
    out
}

fn join(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}/{name}")
    }
}

fn desc_value(d: &Option<Description>) -> Option<Value> {
    d.as_ref().and_then(|d| d.content()).map(|c| json!(c))
}

fn header_json(h: &Header) -> Value {
    let mut o = Map::new();
    o.insert("key".into(), json!(h.key));
    o.insert("value".into(), json!(h.value));
    if !h.enabled {
        o.insert("disabled".into(), json!(true));
    }
    Value::Object(o)
}

/// Inherited headers first, then the request's own. An enabled own header
/// replaces an inherited one with the same key.
fn merged_headers(inherited: &[Header], own: &[Header]) -> Vec<Header> {
    let mut out: Vec<Header> = inherited
        .iter()
        .filter(|i| !own.iter().any(|o| o.enabled && o.key.eq_ignore_ascii_case(&i.key)))
        .cloned()
        .collect();
    out.extend(own.iter().cloned());
    out
}

fn has_content_type(headers: &[Header]) -> bool {
    headers
        .iter()
        .any(|h| h.enabled && h.key.eq_ignore_ascii_case("content-type"))
}

fn request_item(
    name: &str,
    r: &Request,
    inherited: &[Header],
    opts: &PostmanOptions,
    path: &str,
    report: &mut ExportReport,
) -> Value {
    let mut headers = merged_headers(inherited, &r.headers);
    let clean_body = r.body.as_ref().map(|b| scrub_body(b, path, &mut report.items));
    let body = clean_body.as_ref().and_then(|b| body_json(b, &mut headers));
    let headers = scrub_header_values(headers, path, &mut report.items);
    let mut req = Map::new();
    req.insert("method".into(), json!(r.method.to_string()));
    if let Some(a) = auth_value(&r.auth, path, report) {
        req.insert("auth".into(), a);
    }
    req.insert("header".into(), Value::Array(headers.iter().map(header_json).collect()));
    if let Some(b) = body {
        req.insert("body".into(), b);
    }
    let url = scrub_url(&r.url, path, &mut report.items);
    let query = scrub_typed_rows(r.query_params.clone(), "Query parameter", path, &mut report.items);
    req.insert("url".into(), postman_url(&url, &query, &r.path_params));
    if let Some(d) = desc_value(&r.description).or_else(|| desc_value(&r.docs)) {
        req.insert("description".into(), d);
    }
    let mut item = Map::new();
    item.insert("name".into(), json!(name));
    let events = request_events(r, opts, path, &mut report.items);
    if !events.is_empty() {
        item.insert("event".into(), Value::Array(events));
    }
    item.insert("request".into(), Value::Object(req));
    let vars = variables_json(&r.variables, path, report);
    if !vars.is_empty() {
        item.insert("variable".into(), Value::Array(vars));
    }
    Value::Object(item)
}

fn graphql_item(
    name: &str,
    r: &GraphQlRequest,
    inherited: &[Header],
    path: &str,
    report: &mut ExportReport,
) -> Value {
    let mut headers = merged_headers(inherited, &r.headers);
    if !has_content_type(&headers) {
        headers.push(Header::new("Content-Type", "application/json"));
    }
    let headers = scrub_header_values(headers, path, &mut report.items);
    let mut req = Map::new();
    req.insert("method".into(), json!(r.method.to_string()));
    if let Some(a) = auth_value(&r.auth, path, report) {
        req.insert("auth".into(), a);
    }
    req.insert("header".into(), Value::Array(headers.iter().map(header_json).collect()));
    req.insert(
        "body".into(),
        json!({
            "mode": "graphql",
            "graphql": {
                "query": r.body.query,
                "variables": r.body.variables.clone().unwrap_or_default(),
            }
        }),
    );
    let url = scrub_url(&r.url, path, &mut report.items);
    let query = scrub_typed_rows(r.query_params.clone(), "Query parameter", path, &mut report.items);
    req.insert("url".into(), postman_url(&url, &query, &r.path_params));
    if let Some(d) = desc_value(&r.description).or_else(|| desc_value(&r.docs)) {
        req.insert("description".into(), d);
    }
    let mut item = Map::new();
    item.insert("name".into(), json!(name));
    item.insert("request".into(), Value::Object(req));
    let vars = variables_json(&r.variables, path, report);
    if !vars.is_empty() {
        item.insert("variable".into(), Value::Array(vars));
    }
    Value::Object(item)
}

/// Blanks secret-like text entries of a form body. Other bodies are returned as they are.
fn scrub_body(body: &Body, path: &str, notes: &mut Vec<ExportNote>) -> Body {
    let mut out = body.clone();
    if matches!(out.mode, BodyMode::FormUrlEncoded | BodyMode::FormData) {
        out.form_data = out.form_data.take().map(|e| scrub_form_entries(e, path, notes));
    }
    out
}

fn body_json(body: &Body, headers: &mut Vec<Header>) -> Option<Value> {
    let content = body.content.clone().unwrap_or_default();
    let raw = |language: &str, mime: &str, headers: &mut Vec<Header>| {
        if !has_content_type(headers) {
            headers.push(Header::new("Content-Type", mime));
        }
        json!({
            "mode": "raw",
            "raw": content,
            "options": { "raw": { "language": language } }
        })
    };
    match body.mode {
        BodyMode::None => None,
        BodyMode::Json | BodyMode::GraphQl => Some(raw("json", "application/json", headers)),
        BodyMode::Xml => Some(raw("xml", "application/xml", headers)),
        BodyMode::Text => Some(raw("text", "text/plain", headers)),
        BodyMode::Sparql => Some(raw("text", "application/sparql-query", headers)),
        BodyMode::FormUrlEncoded => {
            let rows: Vec<Value> = body
                .form_data
                .iter()
                .flatten()
                .map(|e| {
                    let mut o = Map::new();
                    o.insert("key".into(), json!(e.key));
                    o.insert("value".into(), json!(e.value));
                    if !e.enabled {
                        o.insert("disabled".into(), json!(true));
                    }
                    Value::Object(o)
                })
                .collect();
            Some(json!({ "mode": "urlencoded", "urlencoded": rows }))
        }
        BodyMode::FormData => {
            let rows: Vec<Value> = body
                .form_data
                .iter()
                .flatten()
                .map(|e| {
                    let mut o = Map::new();
                    o.insert("key".into(), json!(e.key));
                    match e.entry_type {
                        FormDataType::Text => {
                            o.insert("value".into(), json!(e.value));
                            o.insert("type".into(), json!("text"));
                        }
                        FormDataType::File => {
                            o.insert("type".into(), json!("file"));
                            o.insert("src".into(), json!(e.value));
                        }
                    }
                    if let Some(ct) = &e.content_type {
                        o.insert("contentType".into(), json!(ct));
                    }
                    if !e.enabled {
                        o.insert("disabled".into(), json!(true));
                    }
                    Value::Object(o)
                })
                .collect();
            Some(json!({ "mode": "formdata", "formdata": rows }))
        }
        BodyMode::Binary => {
            let src = body.file_path.clone().unwrap_or_default();
            Some(json!({ "mode": "file", "file": { "src": src } }))
        }
    }
}

/// Build a Postman `url` object. `raw` holds the enabled query pairs only.
pub fn postman_url(url: &str, query: &[QueryParam], path_params: &[PathParam]) -> Value {
    let (base, query_text) = match url.split_once('?') {
        Some((b, q)) => (b, q),
        None => (url, ""),
    };
    // Rows from the url bar come first, then table rows that are not already there.
    let query_text = query_text.split('#').next().unwrap_or("");
    let mut rows: Vec<Header> = Vec::new();
    for seg in query_text.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = seg.split_once('=').unwrap_or((seg, ""));
        if !k.is_empty() {
            rows.push(Header::new(k, v));
        }
    }
    for q in query {
        if q.key.is_empty() || rows.iter().any(|r| r.key == q.key && r.value == q.value) {
            continue;
        }
        rows.push(q.clone());
    }
    let enabled: Vec<String> = rows
        .iter()
        .filter(|r| r.enabled)
        .map(|r| format!("{}={}", r.key, r.value))
        .collect();
    let mut raw = base.to_string();
    if !enabled.is_empty() {
        raw.push('?');
        raw.push_str(&enabled.join("&"));
    }

    let mut obj = Map::new();
    obj.insert("raw".into(), json!(raw));
    if let Some((protocol, rest)) = base.split_once("://") {
        let rest = rest.split('#').next().unwrap_or(rest);
        let (authority, path) = match rest.split_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (rest, None),
        };
        let host_part = authority.rsplit('@').next().unwrap_or(authority);
        let (host, port) = match host_part.rsplit_once(':') {
            Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => {
                (h, Some(p))
            }
            _ => (host_part, None),
        };
        obj.insert("protocol".into(), json!(protocol));
        let host_parts: Vec<&str> = host.split('.').collect();
        obj.insert("host".into(), json!(host_parts));
        if let Some(p) = port {
            obj.insert("port".into(), json!(p));
        }
        if let Some(p) = path {
            let segs: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
            obj.insert("path".into(), json!(segs));
        }
    }
    if !rows.is_empty() {
        let rows: Vec<Value> = rows.iter().map(header_json).collect();
        obj.insert("query".into(), Value::Array(rows));
    }
    if !path_params.is_empty() {
        let rows: Vec<Value> = path_params
            .iter()
            .map(|p| json!({ "key": p.name, "value": p.value }))
            .collect();
        obj.insert("variable".into(), Value::Array(rows));
    }
    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::GraphQlBody;
    use rocket_shared::types::{FormDataEntry, HttpMethod};

    fn req(name: &str, url: &str) -> Request {
        Request::new(name, HttpMethod::Get, url)
    }

    fn tree_with(items: Vec<ExportItem>) -> CollectionExportTree {
        CollectionExportTree {
            name: "demo".into(),
            settings: Default::default(),
            root: ExportFolder {
                name: "demo".into(),
                settings: Default::default(),
                items,
            },
        }
    }

    fn export(tree: &CollectionExportTree) -> (Value, ExportReport) {
        postman_collection(tree, &PostmanOptions::default()).expect("export builds")
    }

    fn one(r: Request) -> Value {
        export(&tree_with(vec![ExportItem::Request(Box::new(r))])).0
    }

    fn body_req(body: Body) -> Request {
        let mut r = req("B", "https://x.test");
        r.body = Some(body);
        r
    }

    fn body_of(mode: BodyMode, content: &str) -> Body {
        Body {
            mode,
            content: Some(content.into()),
            form_data: None,
            file_path: None,
        }
    }

    fn entry(key: &str, value: &str, t: FormDataType, enabled: bool) -> FormDataEntry {
        FormDataEntry {
            key: key.into(),
            value: value.into(),
            entry_type: t,
            enabled,
            content_type: None,
            description: None,
        }
    }

    #[test]
    fn info_block_has_v2_1_schema_and_uuid() {
        let mut t = tree_with(vec![]);
        t.settings.docs = Some("hello".into());
        let (doc, _) = export(&t);
        assert_eq!(doc["info"]["schema"], POSTMAN_SCHEMA);
        let id = doc["info"]["_postman_id"].as_str().expect("id string");
        assert!(uuid::Uuid::parse_str(id).is_ok());
        assert_eq!(doc["info"]["description"], "hello");
        assert_eq!(doc["info"]["name"], "demo");
    }

    #[test]
    fn rocket_shaped_url_query_becomes_url_query_rows() {
        let url = postman_url("https://x.test/users?page=1&sort=name", &[], &[]);
        assert_eq!(url["raw"], "https://x.test/users?page=1&sort=name");
        assert_eq!(
            url["query"],
            serde_json::json!([
                {"key": "page", "value": "1"},
                {"key": "sort", "value": "name"}
            ])
        );
    }

    #[test]
    fn typed_and_table_duplicate_pair_appears_once() {
        let q = vec![Header::new("page", "1"), Header::disabled("debug", "x")];
        let url = postman_url("https://x.test/users?page=1", &q, &[]);
        assert_eq!(url["raw"], "https://x.test/users?page=1");
        assert_eq!(url["query"].as_array().expect("query array").len(), 2);
        assert_eq!(url["query"][1]["key"], "debug");
        assert_eq!(url["query"][1]["disabled"], true);
    }

    #[test]
    fn empty_key_query_rows_are_skipped() {
        let q = vec![Header::new("", "orphan"), Header::new("a", "1")];
        let url = postman_url("https://x.test/p?=skip&b=2", &q, &[]);
        assert_eq!(url["raw"], "https://x.test/p?b=2&a=1");
        assert_eq!(url["query"].as_array().expect("query array").len(), 2);
    }

    #[test]
    fn disabled_query_param_stays_disabled_and_out_of_raw() {
        let mut r = req("List", "https://api.example.com/users");
        r.query_params = vec![Header::new("page", "1"), Header::disabled("debug", "x")];
        let doc = one(r);
        let url = &doc["item"][0]["request"]["url"];
        assert_eq!(url["raw"], "https://api.example.com/users?page=1");
        assert_eq!(url["query"][1]["key"], "debug");
        assert_eq!(url["query"][1]["disabled"], true);
        assert_eq!(url["host"], json!(["api", "example", "com"]));
        assert_eq!(url["path"], json!(["users"]));
    }

    #[test]
    fn url_without_scheme_keeps_only_raw_query_and_variable() {
        let mut r = req("U", "{{baseUrl}}/users/:id");
        r.path_params = vec![PathParam {
            name: "id".into(),
            value: "42".into(),
            description: None,
        }];
        let doc = one(r);
        let url = doc["item"][0]["request"]["url"].as_object().expect("object");
        assert!(!url.contains_key("protocol"));
        assert!(!url.contains_key("host"));
        assert!(!url.contains_key("path"));
        assert_eq!(url["raw"], "{{baseUrl}}/users/:id");
        assert_eq!(url["variable"], json!([{"key": "id", "value": "42"}]));
    }

    #[test]
    fn path_param_uses_colon_and_variable() {
        let mut r = req("U", "https://x.test/users/:id");
        r.path_params = vec![PathParam {
            name: "id".into(),
            value: "42".into(),
            description: None,
        }];
        let doc = one(r);
        let url = &doc["item"][0]["request"]["url"];
        assert_eq!(url["path"], json!(["users", ":id"]));
        assert_eq!(url["variable"][0]["key"], "id");
    }

    #[test]
    fn duplicate_sibling_names_get_suffix() {
        let folder = |n: &str| {
            ExportItem::Folder(ExportFolder {
                name: n.into(),
                settings: Default::default(),
                items: vec![],
            })
        };
        let t = tree_with(vec![
            ExportItem::Request(Box::new(req("List", "https://x.test"))),
            ExportItem::Request(Box::new(req("List", "https://x.test"))),
            folder("Users"),
            folder("Users"),
        ]);
        let (doc, _) = export(&t);
        let names: Vec<&str> = doc["item"]
            .as_array()
            .expect("array")
            .iter()
            .map(|i| i["name"].as_str().expect("name"))
            .collect();
        assert_eq!(names, ["Users", "Users (2)", "List", "List (2)"]);
    }

    #[test]
    fn items_order_folders_first_then_seq_then_name() {
        let mut a = req("a", "https://x.test");
        a.seq = Some(2);
        let mut b = req("b", "https://x.test");
        b.seq = Some(1);
        let c = req("c", "https://x.test");
        let t = tree_with(vec![
            ExportItem::Request(Box::new(a)),
            ExportItem::Request(Box::new(b)),
            ExportItem::Request(Box::new(c)),
            ExportItem::Folder(ExportFolder {
                name: "f".into(),
                settings: Default::default(),
                items: vec![],
            }),
        ]);
        let (doc, _) = export(&t);
        let names: Vec<&str> = doc["item"]
            .as_array()
            .expect("array")
            .iter()
            .map(|i| i["name"].as_str().expect("name"))
            .collect();
        assert_eq!(names, ["f", "b", "a", "c"]);
    }

    #[test]
    fn folder_and_collection_headers_are_copied_onto_requests() {
        let mut t = tree_with(vec![]);
        t.settings.headers = vec![Header::new("A", "1")];
        let folder_settings = FolderSettings {
            headers: vec![Header::new("A", "2"), Header::disabled("C", "x")],
            ..Default::default()
        };
        let mut r = req("r", "https://x.test");
        r.headers = vec![Header::new("B", "3")];
        t.root.items = vec![ExportItem::Folder(ExportFolder {
            name: "f".into(),
            settings: folder_settings,
            items: vec![ExportItem::Request(Box::new(r))],
        })];
        let (doc, _) = export(&t);
        let h = doc["item"][0]["item"][0]["request"]["header"]
            .as_array()
            .expect("headers");
        assert_eq!(h.len(), 2);
        assert_eq!(h[0], json!({"key": "A", "value": "2"}));
        assert_eq!(h[1], json!({"key": "B", "value": "3"}));
    }

    #[test]
    fn raw_body_sets_language_and_content_type() {
        let cases = [
            (BodyMode::Json, "json"),
            (BodyMode::Xml, "xml"),
            (BodyMode::Text, "text"),
            (BodyMode::Sparql, "text"),
        ];
        for (mode, lang) in cases {
            let doc = one(body_req(body_of(mode, "x")));
            let r = &doc["item"][0]["request"];
            assert_eq!(r["body"]["options"]["raw"]["language"], lang);
            assert_eq!(r["header"].as_array().expect("headers").len(), 1);
        }
        let mut with_ct = body_req(body_of(BodyMode::Json, "{}"));
        with_ct.headers = vec![Header::new("content-type", "application/vnd.api+json")];
        let doc = one(with_ct);
        assert_eq!(doc["item"][0]["request"]["header"].as_array().expect("h").len(), 1);
    }

    #[test]
    fn urlencoded_and_formdata_bodies() {
        let rows = vec![
            entry("a", "1", FormDataType::Text, true),
            entry("b", "2", FormDataType::Text, false),
            entry("f", "/tmp/f.txt", FormDataType::File, true),
        ];
        let mut enc = body_of(BodyMode::FormUrlEncoded, "");
        enc.form_data = Some(rows.clone());
        let doc = one(body_req(enc));
        let b = &doc["item"][0]["request"]["body"];
        assert_eq!(b["mode"], "urlencoded");
        assert_eq!(b["urlencoded"][1]["disabled"], true);

        let mut fd = body_of(BodyMode::FormData, "");
        fd.form_data = Some(rows);
        let doc = one(body_req(fd));
        let b = &doc["item"][0]["request"]["body"];
        assert_eq!(b["formdata"][2], json!({"key": "f", "type": "file", "src": "/tmp/f.txt"}));
        assert_eq!(b["formdata"][1]["disabled"], true);
    }

    #[test]
    fn binary_body_uses_file_mode() {
        let mut b = body_of(BodyMode::Binary, "");
        b.file_path = Some("a.bin".into());
        let doc = one(body_req(b));
        assert_eq!(
            doc["item"][0]["request"]["body"],
            json!({"mode": "file", "file": {"src": "a.bin"}})
        );
    }

    #[test]
    fn graphql_request_uses_graphql_body() {
        let mut g = GraphQlRequest::new("g", "https://x.test/graphql");
        g.body = GraphQlBody {
            query: "{ me }".into(),
            variables: Some("{\"a\":1}".into()),
        };
        let (doc, _) = export(&tree_with(vec![ExportItem::GraphQl(Box::new(g))]));
        let b = &doc["item"][0]["request"]["body"];
        assert_eq!(b["mode"], "graphql");
        assert_eq!(b["graphql"]["query"], "{ me }");
        assert_eq!(b["graphql"]["variables"], "{\"a\":1}");
    }

    #[test]
    fn skipped_items_are_reported_and_absent() {
        let t = tree_with(vec![ExportItem::Skipped {
            path: "ws/chat".into(),
            kind: "websocket".into(),
        }]);
        let (doc, report) = export(&t);
        assert_eq!(doc["item"].as_array().expect("array").len(), 0);
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].path, "ws/chat");
        assert_eq!(report.items[0].action, ExportAction::Skipped);
    }

    #[test]
    fn inherit_omits_auth_and_folder_carries_it() {
        let mut r = req("r", "https://x.test");
        r.auth = Auth::Inherit;
        let t = tree_with(vec![ExportItem::Folder(ExportFolder {
            name: "f".into(),
            settings: FolderSettings {
                auth: Some(Auth::Bearer { token: "{{t}}".into() }),
                ..Default::default()
            },
            items: vec![ExportItem::Request(Box::new(r))],
        })]);
        let (doc, _) = export(&t);
        assert!(doc["item"][0]["item"][0]["request"].get("auth").is_none());
        assert_eq!(doc["item"][0]["auth"]["type"], "bearer");
    }

    #[test]
    fn collection_variables_keep_values_and_blank_secrets() {
        use rocket_collection::CollectionVariable;
        let var = |k: &str, v: &str, secret| CollectionVariable {
            key: k.into(),
            value: v.into(),
            initial_value: String::new(),
            enabled: true,
            secret,
        };
        let mut t = tree_with(vec![]);
        t.settings.variables = vec![var("a", "1", false), var("s", "top", true)];
        let (doc, report) = export(&t);
        assert_eq!(doc["variable"][0]["key"], "a");
        assert_eq!(doc["variable"][0]["value"], "1");
        assert_eq!(doc["variable"][1]["value"], "");
        assert_eq!(report.items.len(), 1);
    }

    #[test]
    fn folder_with_none_auth_omits_auth_block() {
        let mut r = req("r", "https://x.test");
        r.auth = Auth::Inherit;
        let mut t = tree_with(vec![ExportItem::Folder(ExportFolder {
            name: "f".into(),
            settings: FolderSettings {
                auth: Some(Auth::None),
                ..Default::default()
            },
            items: vec![ExportItem::Request(Box::new(r))],
        })]);
        t.settings.auth = Some(Auth::Bearer { token: "{{t}}".into() });
        let (doc, _) = export(&t);
        assert!(doc["item"][0].get("auth").is_none());
        assert_eq!(doc["auth"]["type"], "bearer");
    }

    #[test]
    fn request_header_overrides_inherited_case_insensitively() {
        let mut t = tree_with(vec![ExportItem::Request(Box::new({
            let mut r = req("r", "https://x.test");
            r.headers = vec![Header::new("authorization", "Bearer {{b}}")];
            r
        }))]);
        t.settings.headers = vec![Header::new("Authorization", "Bearer {{a}}")];
        let (doc, _) = export(&t);
        let headers = doc["item"][0]["request"]["header"]
            .as_array()
            .expect("header array");
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0]["value"], "Bearer {{b}}");
    }

    #[test]
    fn folder_and_request_variables_are_exported() {
        use rocket_collection::CollectionVariable;
        let var = |k: &str, v: &str, secret| CollectionVariable {
            key: k.into(),
            value: v.into(),
            initial_value: String::new(),
            enabled: true,
            secret,
        };
        let mut r = req("r", "https://x.test");
        r.variables = vec![var("r", "2", false)];
        let t = tree_with(vec![ExportItem::Folder(ExportFolder {
            name: "f".into(),
            settings: FolderSettings {
                variables: vec![var("a", "1", false), var("s", "top", true)],
                ..Default::default()
            },
            items: vec![ExportItem::Request(Box::new(r))],
        })]);
        let (doc, report) = export(&t);
        let folder = &doc["item"][0];
        assert_eq!(folder["variable"][0]["key"], "a");
        assert_eq!(folder["variable"][0]["value"], "1");
        assert_eq!(folder["variable"][1]["key"], "s");
        assert_eq!(folder["variable"][1]["value"], "");
        assert_eq!(folder["item"][0]["variable"][0]["key"], "r");
        assert_eq!(folder["item"][0]["variable"][0]["value"], "2");
        let blanked: Vec<_> = report
            .items
            .iter()
            .filter(|n| n.action == ExportAction::Blanked && n.feature == "Secret variable")
            .collect();
        assert_eq!(blanked.len(), 1);
        assert!(blanked[0].path.starts_with("f"));
    }

    #[test]
    fn collection_and_folder_auth_blocks() {
        let mut t = tree_with(vec![ExportItem::Folder(ExportFolder {
            name: "f".into(),
            settings: FolderSettings {
                auth: Some(Auth::Basic { username: "u".into(), password: "p".into() }),
                ..Default::default()
            },
            items: vec![],
        })]);
        t.settings.auth = Some(Auth::Bearer { token: "abc".into() });
        let (doc, report) = export(&t);
        assert_eq!(doc["auth"]["type"], "bearer");
        assert_eq!(doc["auth"]["bearer"][0]["value"], "");
        assert_eq!(doc["item"][0]["auth"]["basic"][1]["value"], "");
        assert_eq!(report.items.len(), 2);
    }

    #[test]
    fn examples_are_skipped_with_note() {
        use rocket_shared::action::HttpRequestExample;
        let mut r = req("r", "https://x.test");
        r.examples = vec![HttpRequestExample {
            name: "ok".into(),
            description: None,
            request: None,
            response: None,
        }];
        let (_, report) = export(&tree_with(vec![ExportItem::Request(Box::new(r))]));
        assert!(report
            .items
            .iter()
            .any(|n| n.feature == "request examples" && n.action == ExportAction::Skipped));
    }

    #[test]
    fn skipped_auth_has_no_blanked_note() {
        let mut a = req("a", "https://x.test");
        a.auth = Auth::OAuth1(Box::new(rocket_shared::types::OAuth1Auth {
            consumer_secret: Some("literal".into()),
            ..Default::default()
        }));
        let mut b = req("b", "https://x.test");
        b.auth = Auth::Wsse {
            username: "u".into(),
            password: "literal".into(),
        };
        let (doc, report) = export(&tree_with(vec![
            ExportItem::Request(Box::new(a)),
            ExportItem::Request(Box::new(b)),
        ]));
        for i in 0..2 {
            assert_eq!(doc["item"][i]["request"]["auth"], json!({"type": "noauth"}));
        }
        let skipped = report
            .items
            .iter()
            .filter(|n| n.action == ExportAction::Skipped && n.feature.starts_with("auth "))
            .count();
        assert_eq!(skipped, 2);
        assert!(report.items.iter().all(|n| n.action != ExportAction::Blanked));
    }

    #[test]
    fn oauth1_request_gets_noauth_and_note() {
        let mut r = req("r", "https://x.test");
        r.auth = Auth::OAuth1(Box::default());
        let (doc, report) = export(&tree_with(vec![ExportItem::Request(Box::new(r))]));
        assert_eq!(doc["item"][0]["request"]["auth"], json!({"type": "noauth"}));
        assert!(report
            .items
            .iter()
            .any(|n| n.feature.contains("oauth1") && n.action == ExportAction::Skipped));
    }

    #[test]
    fn scripts_become_request_and_folder_events() {
        let mut r = req("r", "https://x.test");
        r.pre_request_script = Some("rok.setVar('a', 1);".into());
        let t = tree_with(vec![ExportItem::Folder(ExportFolder {
            name: "f".into(),
            settings: FolderSettings {
                tests_script: Some("test('t', () => {});".into()),
                ..Default::default()
            },
            items: vec![ExportItem::Request(Box::new(r)), ExportItem::Request(Box::new(req("plain", "https://x.test")))],
        })]);
        let (doc, _) = export(&t);
        let folder = &doc["item"][0];
        assert_eq!(folder["event"][0]["listen"], "test");
        // Requests sort by name, so "plain" comes before "r".
        assert!(folder["item"][0].get("event").is_none());
        let scripted = &folder["item"][1];
        assert_eq!(scripted["event"][0]["listen"], "prerequest");
        assert_eq!(
            scripted["event"][0]["script"]["exec"],
            json!(["pm.variables.set('a', 1);"])
        );
        assert!(doc.get("event").is_none());
    }

    #[test]
    fn preserve_scripts_option_reaches_events() {
        let mut r = req("r", "https://x.test");
        r.pre_request_script = Some("rok.setVar('a', 1);".into());
        let t = tree_with(vec![ExportItem::Request(Box::new(r))]);
        let (doc, _) =
            postman_collection(&t, &PostmanOptions { preserve_scripts: true }).expect("export");
        assert_eq!(
            doc["item"][0]["event"][0]["script"]["exec"],
            json!(["rok.setVar('a', 1);"])
        );
    }

    #[test]
    fn script_notes_reach_the_report_with_the_item_path() {
        let mut r = req("r", "https://x.test");
        r.pre_request_script = Some("rok.sendRequest({});".into());
        let t = tree_with(vec![ExportItem::Folder(ExportFolder {
            name: "f".into(),
            settings: Default::default(),
            items: vec![ExportItem::Request(Box::new(r))],
        })]);
        let (_, report) = export(&t);
        assert!(report
            .items
            .iter()
            .any(|n| n.path == "f/r" && n.action == ExportAction::Skipped));
    }

    #[test]
    fn request_method_and_description() {
        let mut r = req("p", "https://x.test");
        r.method = HttpMethod::Post;
        r.description = Some(Description::text("docs"));
        let doc = one(r);
        assert_eq!(doc["item"][0]["request"]["method"], "POST");
        assert_eq!(doc["item"][0]["request"]["description"], "docs");
    }

    fn blanked_details(report: &ExportReport) -> Vec<(String, Option<String>)> {
        report
            .items
            .iter()
            .filter(|n| n.action == ExportAction::Blanked)
            .map(|n| (n.feature.clone(), n.detail.clone()))
            .collect()
    }

    #[test]
    fn query_pairs_are_scrubbed_by_name_in_raw_and_query() {
        let r = req("q", "https://x.test/p?access_token=abc&token_type=Bearer&a=1");
        let (doc, report) = export(&tree_with(vec![ExportItem::Request(Box::new(r))]));
        let url = &doc["item"][0]["request"]["url"];
        assert_eq!(url["raw"], "https://x.test/p?access_token=&token_type=Bearer&a=1");
        assert_eq!(url["query"][0]["key"], "access_token");
        assert_eq!(url["query"][0]["value"], "");
        assert_eq!(url["query"][1]["value"], "Bearer");
        assert_eq!(
            blanked_details(&report),
            vec![("URL query".to_string(), Some("access_token (by name)".to_string()))]
        );
    }

    #[test]
    fn query_table_rows_are_scrubbed_by_name() {
        let mut r = req("q", "https://x.test/p");
        r.query_params = vec![Header::new("api_key", "abc"), Header::disabled("debug", "x")];
        let doc = one(r);
        let url = &doc["item"][0]["request"]["url"];
        assert_eq!(url["raw"], "https://x.test/p?api_key=");
        assert_eq!(url["query"][0]["value"], "");
        assert_eq!(url["query"][1]["value"], "x");
    }

    #[test]
    fn references_are_kept_in_query_rows() {
        let mut r = req("q", "https://x.test/p?access_token={{t}}");
        r.query_params = vec![Header::new("api_key", "{{k}}")];
        let (doc, report) = export(&tree_with(vec![ExportItem::Request(Box::new(r))]));
        let url = &doc["item"][0]["request"]["url"];
        assert_eq!(url["raw"], "https://x.test/p?access_token={{t}}&api_key={{k}}");
        assert!(blanked_details(&report).is_empty());
    }

    #[test]
    fn form_entries_are_scrubbed_by_name() {
        let mut enc = body_of(BodyMode::FormUrlEncoded, "");
        enc.form_data = Some(vec![
            entry("client_secret", "s", FormDataType::Text, true),
            entry("grant_type", "password", FormDataType::Text, true),
        ]);
        let mut fd = body_of(BodyMode::FormData, "");
        fd.form_data = Some(vec![
            entry("password", "p", FormDataType::Text, true),
            entry("secret_file", "/tmp/s.bin", FormDataType::File, true),
        ]);
        let doc = one(body_req(enc));
        let b = &doc["item"][0]["request"]["body"];
        assert_eq!(b["urlencoded"][0]["value"], "");
        assert_eq!(b["urlencoded"][1]["value"], "password");
        let doc = one(body_req(fd));
        let b = &doc["item"][0]["request"]["body"];
        assert_eq!(b["formdata"][0]["value"], "");
        assert_eq!(b["formdata"][1]["src"], "/tmp/s.bin");
    }

    #[test]
    fn header_rows_use_the_name_rule() {
        let mut r = req("h", "https://x.test");
        r.headers = vec![Header::new("X-Auth-Token", "abc"), Header::new("X-Request-Id", "1")];
        let doc = one(r);
        let h = &doc["item"][0]["request"]["header"];
        assert_eq!(h[0]["value"], "");
        assert_eq!(h[1]["value"], "1");
    }

    #[test]
    fn oauth_urls_are_scrubbed() {
        use rocket_shared::oauth2::{OAuth2ClientCredentials, OAuth2Flow};
        let mut r = req("o", "https://x.test");
        r.auth = Auth::OAuth2(Box::new(OAuth2Flow::AuthorizationCode {
            authorization_url: "https://idp.test/auth?state=s".into(),
            access_token_url: "https://u:pw@idp.test/token?client_secret=x&a=1".into(),
            refresh_token_url: None,
            callback_url: Some("https://u:pw@app.test/cb".into()),
            credentials: OAuth2ClientCredentials {
                client_id: "cid".into(),
                client_secret: "{{cs}}".into(),
                placement: None,
            },
            scope: None,
            state: None,
            pkce: None,
            additional_parameters: None,
            token_config: None,
            settings: None,
        }));
        let doc = one(r);
        let rows = doc["item"][0]["request"]["auth"]["oauth2"].as_array().expect("rows");
        let get = |k: &str| rows.iter().find(|r| r["key"] == k).expect("row")["value"].clone();
        assert_eq!(get("accessTokenUrl"), "https://u:@idp.test/token?client_secret=&a=1");
        assert_eq!(get("authUrl"), "https://idp.test/auth?state=s");
        assert_eq!(get("redirect_uri"), "https://u:@app.test/cb");
    }

    #[test]
    fn variables_are_blanked_by_name_and_secret_refs_kept() {
        use rocket_collection::CollectionVariable;
        let var = |k: &str, v: &str, secret| CollectionVariable {
            key: k.into(),
            value: v.into(),
            initial_value: String::new(),
            enabled: true,
            secret,
        };
        let mut t = tree_with(vec![]);
        t.settings.variables = vec![
            var("api_token", "abc", false),
            var("plain_name", "abc", false),
            var("s1", "{{ref}}", true),
            var("s2", "top", true),
        ];
        let (doc, report) = export(&t);
        let v = &doc["variable"];
        assert_eq!(v[0]["value"], "");
        assert_eq!(v[1]["value"], "abc");
        assert_eq!(v[2]["value"], "{{ref}}");
        assert_eq!(v[3]["value"], "");
        assert_eq!(
            blanked_details(&report),
            vec![
                ("Variable".to_string(), Some("api_token (by name)".to_string())),
                ("Secret variable".to_string(), Some("s2".to_string())),
            ]
        );
    }
}
