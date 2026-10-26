use super::super::apply_types::{
    AppliedKind, ApplyInputs, ApplyOutput, ApprovedItem, FileChangeKind, RequestRenderer,
    Selection, SkipRequest,
};
use super::super::merge::{Choice, FieldOutcome};
use super::super::plan::{
    build_sync_plan, item_digest, CurrentRequest, SyncInputs, SyncItem, SyncPlan,
};
use super::super::spec::ParsedSpec;
use super::{build_apply, plan_with_links};
use crate::converter::openapi::{baseline_hash, convert_operation, CONVERTER_VERSION};
use crate::error::ImportResult;
use rocket_collection::openapi_baseline::{
    BaselineOperation, OpenApiBaseline, BASELINE_FILE_VERSION,
};
use rocket_collection::openapi_source::{
    OpenApiSource, SourceOperation, SpecLocation, SyncInterval, SyncStatus, SOURCE_FILE_VERSION,
};
use rocket_collection::{
    validate_rel_path, CollectionSettings, CollectionVariable, Request,
};
use rocket_shared::error::DomainResult;
use rocket_shared::types::Header;
use std::collections::{BTreeMap, HashSet};
use std::time::Instant;

const BASE: &str = "https://api.example.com";

#[derive(Clone)]
struct O {
    id: String,
    method: &'static str,
    path: String,
    summary: String,
    tag: String,
    body: Option<&'static str>,
    secured: bool,
    query: Option<&'static str>,
}

fn o(id: &str, path: &str, summary: &str) -> O {
    O {
        id: id.to_string(),
        method: "GET",
        path: path.to_string(),
        summary: summary.to_string(),
        tag: "Pets".to_string(),
        body: None,
        secured: false,
        query: None,
    }
}

fn spec_text(base: &str, ops: &[O]) -> String {
    let mut paths = serde_json::Map::new();
    for op in ops {
        let mut inner = serde_json::json!({
            "operationId": op.id,
            "summary": op.summary,
            "tags": [op.tag],
            "responses": { "200": { "description": "ok" } },
        });
        if let Some(body) = op.body {
            let example: serde_json::Value = serde_json::from_str(body).expect("example");
            inner["requestBody"] = serde_json::json!({
                "content": { "application/json": { "example": example } }
            });
        }
        if let Some(name) = op.query {
            inner["parameters"] = serde_json::json!([
                { "name": name, "in": "query", "schema": { "type": "string" }, "example": "1" }
            ]);
        }
        if op.secured {
            inner["security"] = serde_json::json!([{ "auth": [] }]);
        }
        let entry = paths
            .entry(op.path.clone())
            .or_insert_with(|| serde_json::json!({}));
        entry[op.method.to_lowercase()] = inner;
    }
    serde_json::json!({
        "openapi": "3.0.3",
        "info": { "title": "T", "version": "1" },
        "servers": [{ "url": base }],
        "paths": paths,
        "components": { "securitySchemes": { "auth": { "type": "http", "scheme": "bearer" } } },
    })
    .to_string()
}

fn parsed(base: &str, ops: &[O]) -> ParsedSpec {
    ParsedSpec::parse(&spec_text(base, ops)).expect("spec parses")
}

#[derive(Default)]
struct MemoryRenderer {
    blocked: HashSet<String>,
}

impl RequestRenderer for MemoryRenderer {
    fn render(&self, _path: &str, request: &Request) -> DomainResult<Vec<u8>> {
        Ok(serde_json::to_vec(request).expect("request serializes"))
    }
    fn render_settings(&self, settings: &CollectionSettings) -> DomainResult<(String, Vec<u8>)> {
        Ok((
            "opencollection.yml".to_string(),
            serde_json::to_vec(settings).expect("settings serialize"),
        ))
    }
    fn render_folder(&self, folder_path: &str) -> DomainResult<Vec<u8>> {
        Ok(folder_path.as_bytes().to_vec())
    }
    fn can_rewrite(&self, request_path: &str, _request: &Request) -> DomainResult<bool> {
        Ok(!self.blocked.contains(request_path))
    }
}

struct W {
    source: OpenApiSource,
    baseline: OpenApiBaseline,
    current: Vec<CurrentRequest>,
    existing: HashSet<String>,
    settings: CollectionSettings,
    vars: Vec<String>,
    base_url: Option<String>,
    skips: BTreeMap<String, String>,
}

impl W {
    fn set_base_url(&mut self, url: &str) {
        for v in &mut self.settings.variables {
            if v.key == "baseUrl" {
                v.value = url.to_string();
            }
        }
        self.base_url = Some(url.to_string());
    }
}

fn slug(text: &str) -> String {
    text.replace(' ', "-")
}

/// What an import of the ops leaves behind.
fn world(ops: &[O]) -> W {
    let spec = parsed(BASE, ops);
    let doc = spec.doc();
    let mut operations = Vec::new();
    let mut base_ops = Vec::new();
    let mut current = Vec::new();
    let mut existing = HashSet::new();
    for (i, op) in doc.operations.iter().enumerate() {
        let request = convert_operation(op, &doc.security_schemes, (i + 1) as u32).request;
        let dir = op.tags.first().cloned().unwrap_or_default();
        let request_path = format!("{dir}/{}", slug(&request.name));
        existing.insert(format!("{request_path}.yml"));
        existing.insert(format!("{dir}/folder.yml"));
        operations.push(SourceOperation {
            key: op.key.clone(),
            uid: request.uid.clone(),
            request_path: request_path.clone(),
            baseline_hash: baseline_hash(&request),
        });
        base_ops.push(BaselineOperation {
            key: op.key.clone(),
            request: request.clone(),
        });
        current.push(CurrentRequest {
            uid: request.uid.clone(),
            request_path,
            request,
        });
    }
    let mut settings = CollectionSettings::default();
    settings.variables.push(CollectionVariable {
        key: "baseUrl".to_string(),
        value: BASE.to_string(),
        initial_value: BASE.to_string(),
        enabled: true,
        secret: false,
    });
    W {
        source: OpenApiSource {
            version: SOURCE_FILE_VERSION,
            location: SpecLocation::Https {
                url: "https://example.com/spec.json".to_string(),
            },
            group_by: "tags".to_string(),
            spec_sha256: "old-hash".to_string(),
            etag: None,
            last_modified: None,
            operations,
            source_id: String::new(),
            sync_interval: SyncInterval::Off,
            last_checked_at: None,
            last_status: Some(SyncStatus::Changed),
            pending_sha256: Some("new-hash".to_string()),
        },
        baseline: OpenApiBaseline {
            version: BASELINE_FILE_VERSION,
            converter_version: CONVERTER_VERSION,
            base_url: BASE.to_string(),
            operations: base_ops,
        },
        current,
        existing,
        vars: vec!["baseUrl".to_string()],
        base_url: Some(BASE.to_string()),
        settings,
        skips: BTreeMap::new(),
    }
}

fn sync_inputs<'a>(w: &'a W, spec: &'a ParsedSpec, with_baseline: bool) -> SyncInputs<'a> {
    SyncInputs {
        source: &w.source,
        baseline: with_baseline.then_some(&w.baseline),
        incoming: spec,
        current: &w.current,
        current_base_url: w.base_url.as_deref(),
        existing_variables: &w.vars,
        skips: &w.skips,
    }
}

fn plan_of(w: &W, spec: &ParsedSpec) -> SyncPlan {
    build_sync_plan(&sync_inputs(w, spec, true))
}

fn run_with(
    w: &W,
    spec: &ParsedSpec,
    sel: &Selection,
    renderer: &MemoryRenderer,
    with_baseline: bool,
) -> ImportResult<ApplyOutput> {
    build_apply(&ApplyInputs {
        plan_inputs: sync_inputs(w, spec, with_baseline),
        pending_sha256: "new-hash".to_string(),
        selection: sel,
        renderer,
        existing_paths: &w.existing,
        settings: &w.settings,
    })
}

fn run(w: &W, spec: &ParsedSpec, sel: &Selection) -> ApplyOutput {
    run_with(w, spec, sel, &MemoryRenderer::default(), true).expect("apply works")
}

fn approve(plan: &SyncPlan, keys: &[&str]) -> Selection {
    Selection {
        items: plan
            .items
            .iter()
            .filter(|i| keys.is_empty() || keys.contains(&i.key()))
            .map(|i| ApprovedItem {
                key: i.key().to_string(),
                digest: item_digest(i),
                choices: BTreeMap::new(),
            })
            .collect(),
        ..Selection::default()
    }
}

fn put_bytes<'a>(out: &'a ApplyOutput, rel: &str) -> Option<&'a Vec<u8>> {
    out.files.iter().find_map(|f| match &f.kind {
        FileChangeKind::Put(b) if f.rel_path == rel => Some(b),
        _ => None,
    })
}

fn put_request(out: &ApplyOutput, rel: &str) -> Request {
    let bytes = put_bytes(out, rel).unwrap_or_else(|| panic!("no put for {rel}: {:?}", paths(out)));
    serde_json::from_slice(bytes).expect("request json")
}

fn paths(out: &ApplyOutput) -> Vec<&str> {
    out.files.iter().map(|f| f.rel_path.as_str()).collect()
}

fn pets() -> Vec<O> {
    vec![
        o("listPets", "/pets", "List pets"),
        o("getPet", "/pets/{id}", "Get pet"),
    ]
}

fn add_header(w: &mut W, at: usize) {
    w.current[at]
        .request
        .headers
        .push(Header::new("X-Mine", "1"));
}

#[test]
fn item_digest_changes_with_any_listed_change() {
    let w = world(&pets());
    let mut next = pets();
    next[0].summary = "List all pets".to_string();
    let plan = plan_of(&w, &parsed(BASE, &next));
    let item = plan.items[0].clone();
    let base = item_digest(&item);
    assert_eq!(base, item_digest(&item.clone()));

    let tweak = |f: &dyn Fn(&mut SyncItem)| {
        let mut copy = item.clone();
        f(&mut copy);
        item_digest(&copy)
    };
    let digests = [
        tweak(&|i| {
            if let SyncItem::Changed { key, .. } = i {
                key.push('x');
            }
        }),
        tweak(&|i| {
            if let SyncItem::Changed { fingerprint, .. } = i {
                fingerprint.push('x');
            }
        }),
        tweak(&|i| {
            if let SyncItem::Changed { merge, .. } = i {
                merge.changes[0].field.push('x');
            }
        }),
        tweak(&|i| {
            if let SyncItem::Changed { merge, .. } = i {
                merge.changes[0].outcome = FieldOutcome::Conflict;
            }
        }),
        tweak(&|i| {
            if let SyncItem::Changed { merge, .. } = i {
                merge.changes[0].yours = Some("other".to_string());
            }
        }),
        tweak(&|i| {
            if let SyncItem::Changed { merge, .. } = i {
                merge.changes[0].spec = Some("other".to_string());
            }
        }),
    ];
    for d in &digests {
        assert_ne!(*d, base);
    }
}

#[test]
fn new_endpoint_is_put_in_its_tag_folder_with_a_unique_name() {
    let w = world(&pets());
    let mut next = pets();
    next.push(o("findPet", "/pets/find", "Get pet"));
    let spec = parsed(BASE, &next);
    let plan = plan_of(&w, &spec);
    let out = run(&w, &spec, &approve(&plan, &[]));
    assert_eq!(paths(&out), vec!["Pets/Get-pet-2.yml"]);
    let req = put_request(&out, "Pets/Get-pet-2.yml");
    assert_eq!(req.name, "Get pet (2)");
    assert!(out.report.applied.iter().any(|a| a.kind == AppliedKind::Created));
}

#[test]
fn deleted_tag_folder_is_recreated() {
    let mut w = world(&pets());
    w.existing.clear();
    let mut next = pets();
    next.push(o("findPet", "/pets/find", "Find pet"));
    let spec = parsed(BASE, &next);
    let plan = plan_of(&w, &spec);
    let out = run(&w, &spec, &approve(&plan, &["findPet"]));
    assert_eq!(paths(&out), vec!["Pets/folder.yml", "Pets/Find-pet.yml"]);
}

#[test]
fn existing_folder_is_not_rewritten() {
    let w = world(&pets());
    let mut next = pets();
    next.push(o("findPet", "/pets/find", "Find pet"));
    let spec = parsed(BASE, &next);
    let out = run(&w, &spec, &approve(&plan_of(&w, &spec), &[]));
    assert_eq!(paths(&out), vec!["Pets/Find-pet.yml"]);
}

#[test]
fn new_endpoint_gets_source_and_baseline_entries() {
    let w = world(&pets());
    let mut next = pets();
    next.push(o("findPet", "/pets/find", "Find pet"));
    let spec = parsed(BASE, &next);
    let out = run(&w, &spec, &approve(&plan_of(&w, &spec), &[]));
    let src = out
        .source
        .operations
        .iter()
        .find(|s| s.key == "findPet")
        .expect("source entry");
    let base = out
        .baseline
        .operations
        .iter()
        .find(|b| b.key == "findPet")
        .expect("baseline entry");
    assert_eq!(src.request_path, "Pets/Find-pet");
    assert_eq!(src.uid, base.request.uid);
    assert_eq!(src.baseline_hash, baseline_hash(&base.request));
    assert_eq!(base.request, put_request(&out, "Pets/Find-pet.yml"));
}

#[test]
fn hostile_names_cannot_escape() {
    let w = world(&pets());
    let mut next = pets();
    let mut a = o("a", "/a", "evil");
    a.tag = "../../x".to_string();
    let b = o("con", "/b", "con");
    let c = o("empty", "/c", "");
    let mut d = o("d", "/d", "colon:name");
    d.tag = "C:\\win".to_string();
    let mut e = o("e", "/e", "same");
    e.tag = "..".to_string();
    next.extend([a, b, c, d, e]);
    let spec = parsed(BASE, &next);
    let out = run(&w, &spec, &approve(&plan_of(&w, &spec), &[]));
    let mut seen = HashSet::new();
    for f in &out.files {
        validate_rel_path(&f.rel_path).expect("valid path");
        assert!(!f.rel_path.contains(".."), "{}", f.rel_path);
        assert!(seen.insert(f.rel_path.to_lowercase()), "{}", f.rel_path);
    }
    assert_eq!(out.report.applied.len(), 5);
}

#[test]
fn changed_item_merges_and_advances_the_baseline() {
    let mut old = pets();
    old[0].body = Some(r#"{"limit":1}"#);
    let mut w = world(&old);
    add_header(&mut w, 0);
    w.current[0].request_path = "Elsewhere/renamed".to_string();
    let mut next = old.clone();
    next[0].body = Some(r#"{"limit":2}"#);
    next[0].summary = "List all pets".to_string();
    let spec = parsed(BASE, &next);
    let plan = plan_of(&w, &spec);
    let out = run(&w, &spec, &approve(&plan, &["listPets"]));
    let merged = put_request(&out, "Elsewhere/renamed.yml");
    assert!(merged.headers.iter().any(|h| h.key == "X-Mine"));
    assert!(merged.body.as_ref().is_some_and(|b| b
        .content
        .as_deref()
        .is_some_and(|c| c.contains("\"limit\": 2") || c.contains("\"limit\":2"))));
    let src = &out.source.operations[0];
    assert_eq!(src.request_path, "Elsewhere/renamed");
    let base = &out.baseline.operations[0].request;
    let incoming = convert_operation(&spec.doc().operations[0], &spec.doc().security_schemes, 1);
    assert_eq!(base.name, "List all pets");
    assert_eq!(baseline_hash(base), baseline_hash(&incoming.request));
    assert_eq!(src.baseline_hash, baseline_hash(base));
    assert_eq!(base.uid, w.current[0].uid);
}

#[test]
fn choice_spec_applies_a_conflict_resolution() {
    // The user changed a query value that the new spec removes: a real conflict.
    let mut old = pets();
    old[0].query = Some("limit");
    let mut w = world(&old);
    w.current[0].request.query_params[0].value = "9".to_string();
    let spec = parsed(BASE, &pets());
    let plan = plan_of(&w, &spec);
    let SyncItem::Changed { merge, .. } = &plan.items[0] else {
        panic!("not changed");
    };
    let conflict = merge
        .changes
        .iter()
        .find(|c| c.outcome == FieldOutcome::Conflict)
        .unwrap_or_else(|| panic!("no conflict: {:?}", merge.changes));
    assert_eq!(conflict.field, "query:limit");
    // With no choice the user's value stays.
    let kept = run(&w, &spec, &approve(&plan, &["listPets"]));
    assert_eq!(put_request(&kept, "Pets/List-pets.yml").query_params.len(), 1);

    let mut sel = approve(&plan, &["listPets"]);
    sel.items[0]
        .choices
        .insert(conflict.field.clone(), Choice::Spec);
    let out = run(&w, &spec, &sel);
    assert!(put_request(&out, "Pets/List-pets.yml").query_params.is_empty());
}

#[test]
fn cautious_item_with_one_choice_keeps_the_rest() {
    let mut w = world(&pets());
    add_header(&mut w, 0);
    w.current[0].request.name = "Mine".to_string();
    let mut next = pets();
    next[0].path = "/pets/all".to_string();
    next[0].summary = "List all pets".to_string();
    let spec = parsed(BASE, &next);
    let plan = build_sync_plan(&sync_inputs(&w, &spec, false));
    let mut sel = approve(&plan, &["listPets"]);
    sel.items[0].choices.insert("url".to_string(), Choice::Spec);
    let out = run_with(&w, &spec, &sel, &MemoryRenderer::default(), false).expect("applies");
    let req = put_request(&out, "Pets/List-pets.yml");
    assert_eq!(req.url, "{{baseUrl}}/pets/all");
    assert_eq!(req.name, "Mine");
    assert!(req.headers.iter().any(|h| h.key == "X-Mine"));
}

#[test]
fn unmodeled_yaml_key_makes_the_item_not_rewritable() {
    let w = world(&pets());
    let mut next = pets();
    next[0].summary = "List all pets".to_string();
    let spec = parsed(BASE, &next);
    let plan = plan_of(&w, &spec);
    let mut r = MemoryRenderer::default();
    r.blocked.insert("Pets/List-pets".to_string());
    let out = run_with(&w, &spec, &approve(&plan, &[]), &r, true).expect("applies");
    assert!(out.files.is_empty());
    assert_eq!(out.report.not_rewritable, vec!["listPets"]);
    assert_eq!(out.source.operations, w.source.operations);
    assert_eq!(out.baseline.operations, w.baseline.operations);
    assert_eq!(out.report.remaining, 1);
    assert!(!out.finalized);
}

#[test]
fn removed_unedited_is_deleted() {
    let w = world(&pets());
    let spec = parsed(BASE, &pets()[..1]);
    let out = run(&w, &spec, &approve(&plan_of(&w, &spec), &[]));
    assert_eq!(out.files.len(), 1);
    assert_eq!(out.files[0].rel_path, "Pets/Get-pet.yml");
    assert_eq!(out.files[0].kind, FileChangeKind::Delete);
    assert_eq!(out.report.deleted, vec!["getPet"]);
    assert!(out.source.operations.iter().all(|o| o.key != "getPet"));
    assert!(out.baseline.operations.iter().all(|o| o.key != "getPet"));
}

#[test]
fn removed_edited_is_detached_and_kept() {
    let mut w = world(&pets());
    add_header(&mut w, 1);
    let spec = parsed(BASE, &pets()[..1]);
    let out = run(&w, &spec, &approve(&plan_of(&w, &spec), &[]));
    assert!(out.files.is_empty());
    assert_eq!(out.report.detached, vec!["getPet"]);
    assert!(out.source.operations.iter().all(|o| o.key != "getPet"));
    assert!(out.baseline.operations.iter().all(|o| o.key != "getPet"));
}

#[test]
fn removed_with_unknown_edit_state_is_detached() {
    let w = world(&pets());
    let spec = parsed(BASE, &pets()[..1]);
    let plan = build_sync_plan(&sync_inputs(&w, &spec, false));
    let sel = approve(&plan, &[]);
    let out = run_with(&w, &spec, &sel, &MemoryRenderer::default(), false).expect("applies");
    assert!(out.files.is_empty());
    assert_eq!(out.report.detached, vec!["getPet"]);
}

#[test]
fn stale_digest_is_not_applied() {
    let mut w = world(&pets());
    let mut next = pets();
    next[0].summary = "List all pets".to_string();
    let spec = parsed(BASE, &next);
    let sel = approve(&plan_of(&w, &spec), &[]);
    add_header(&mut w, 0);
    let out = run(&w, &spec, &sel);
    assert_eq!(out.report.stale, vec!["listPets"]);
    assert!(out.files.is_empty());
    assert!(!out.finalized);
}

#[test]
fn spec_change_after_review_is_stale() {
    let w = world(&pets());
    let mut v2 = pets();
    v2[0].summary = "List all pets".to_string();
    let sel = approve(&plan_of(&w, &parsed(BASE, &v2)), &[]);
    let mut v3 = pets();
    v3[0].summary = "Something else".to_string();
    let out = run(&w, &parsed(BASE, &v3), &sel);
    assert_eq!(out.report.stale, vec!["listPets"]);
    assert!(out.files.is_empty());
}

fn first_fingerprint(plan: &SyncPlan) -> (String, String) {
    (
        plan.items[0].key().to_string(),
        plan.items[0].fingerprint().to_string(),
    )
}

#[test]
fn skip_moves_nothing_and_is_recorded() {
    let w = world(&pets());
    let mut next = pets();
    next[0].summary = "List all pets".to_string();
    let spec = parsed(BASE, &next);
    let plan = plan_of(&w, &spec);
    let (key, fp) = first_fingerprint(&plan);
    let sel = Selection {
        skips: vec![SkipRequest {
            key: key.clone(),
            fingerprint: fp.clone(),
        }],
        ..Selection::default()
    };
    let out = run(&w, &spec, &sel);
    assert!(out.files.is_empty());
    assert_eq!(out.local.skips.get(&key), Some(&fp));
    assert_eq!(out.report.skipped, 1);
    assert_eq!(out.source.operations, w.source.operations);
    assert_eq!(out.baseline.operations, w.baseline.operations);
}

#[test]
fn applied_key_clears_its_skip() {
    let mut w = world(&pets());
    w.skips.insert("listPets".to_string(), "old".to_string());
    let mut next = pets();
    next[0].summary = "List all pets".to_string();
    let spec = parsed(BASE, &next);
    let out = run(&w, &spec, &approve(&plan_of(&w, &spec), &[]));
    assert!(!out.local.skips.contains_key("listPets"));
    assert_eq!(out.report.applied.len(), 1);
}

#[test]
fn partial_apply_keeps_the_pending_copy() {
    let w = world(&pets());
    let mut next = pets();
    next[0].summary = "A".to_string();
    next[1].summary = "B".to_string();
    let spec = parsed(BASE, &next);
    let plan = plan_of(&w, &spec);
    let out = run(&w, &spec, &approve(&plan, &["listPets"]));
    assert_eq!(out.report.remaining, 1);
    assert!(!out.finalized);
    assert_eq!(out.source.pending_sha256.as_deref(), Some("new-hash"));
    assert_eq!(out.source.spec_sha256, "old-hash");
}

#[test]
fn full_apply_finalizes_the_spec_hash() {
    let w = world(&pets());
    let mut next = pets();
    next[0].summary = "A".to_string();
    let spec = parsed(BASE, &next);
    let out = run(&w, &spec, &approve(&plan_of(&w, &spec), &[]));
    assert!(out.finalized);
    assert_eq!(out.source.spec_sha256, "new-hash");
    assert_eq!(out.source.pending_sha256, None);
    assert_eq!(out.source.last_status, Some(SyncStatus::Unchanged));
}

#[test]
fn converter_version_is_unchanged() {
    let mut w = world(&pets());
    w.baseline.converter_version = CONVERTER_VERSION + 7;
    let mut next = pets();
    next[0].summary = "A".to_string();
    let spec = parsed(BASE, &next);
    let plan = plan_of(&w, &spec);
    let out = run(&w, &spec, &approve(&plan, &[]));
    assert_eq!(out.baseline.converter_version, CONVERTER_VERSION + 7);
}

#[test]
fn rename_link_merges_as_changed_and_keeps_user_edits() {
    let mut w = world(&pets());
    add_header(&mut w, 0);
    let mut next = pets();
    next[0].id = "listAllPets".to_string();
    next[0].summary = "List all pets".to_string();
    let spec = parsed(BASE, &next);
    let links = vec![("listPets".to_string(), "listAllPets".to_string())];
    let plan = plan_with_links(&sync_inputs(&w, &spec, true), &links).expect("plan");
    let mut sel = approve(&plan, &["listAllPets"]);
    sel.rename_links = links;
    let out = run(&w, &spec, &sel);
    let req = put_request(&out, "Pets/List-pets.yml");
    assert!(req.headers.iter().any(|h| h.key == "X-Mine"));
    assert_eq!(req.name, "List all pets");
    assert!(out.source.operations.iter().any(|o| o.key == "listAllPets"));
    assert!(out.source.operations.iter().all(|o| o.key != "listPets"));
    assert!(out.baseline.operations.iter().all(|o| o.key != "listPets"));
    assert!(out.report.stale.is_empty());
}

#[test]
fn invalid_rename_link_is_stale() {
    let w = world(&pets());
    let spec = parsed(BASE, &pets());
    let sel = Selection {
        rename_links: vec![("nope".to_string(), "listPets".to_string())],
        ..Selection::default()
    };
    let out = run(&w, &spec, &sel);
    assert_eq!(out.report.stale, vec!["nope"]);
    assert_eq!(out.source.operations, w.source.operations);
}

fn settings_of(out: &ApplyOutput) -> Option<CollectionSettings> {
    put_bytes(out, "opencollection.yml").map(|b| serde_json::from_slice(b).expect("settings"))
}

#[test]
fn base_url_is_updated_only_when_untouched() {
    let w = world(&pets());
    let spec = parsed("https://v2.example.com", &pets());
    let plan = plan_of(&w, &spec);
    let mut sel = approve(&plan, &[]);
    sel.variables = vec!["baseUrl".to_string()];
    let out = run(&w, &spec, &sel);
    let settings = settings_of(&out).expect("settings staged");
    assert_eq!(settings.variables[0].value, "https://v2.example.com");
    assert_eq!(out.baseline.base_url, "https://v2.example.com");

    let mut touched = world(&pets());
    touched.set_base_url("https://mine.example.com");
    let plan = plan_of(&touched, &spec);
    let mut sel = approve(&plan, &[]);
    sel.variables = vec!["baseUrl".to_string()];
    let out = run(&touched, &spec, &sel);
    assert!(settings_of(&out).is_none());
    // The baseline moves to the spec value even though the user's value is kept.
    assert_eq!(out.baseline.base_url, "https://v2.example.com");
}

#[test]
fn new_baseline_never_records_a_customized_base_url() {
    let mut w = world(&pets());
    w.set_base_url("https://mine.example.com");
    let mut next = pets();
    next.push(o("findPet", "/pets/find", "Find pet"));
    let spec = parsed(BASE, &next);
    let plan = build_sync_plan(&sync_inputs(&w, &spec, false));
    let sel = approve(&plan, &["findPet"]);
    let first = run_with(&w, &spec, &sel, &MemoryRenderer::default(), false).expect("applies");
    assert_eq!(first.baseline.base_url, BASE);

    // The next sync sees a new spec URL and must keep the user's value.
    w.baseline = first.baseline;
    w.source = first.source;
    let spec2 = parsed("https://v3.example.com", &next);
    let plan2 = plan_of(&w, &spec2);
    let mut sel2 = approve(&plan2, &[]);
    sel2.variables = vec!["baseUrl".to_string()];
    let second = run(&w, &spec2, &sel2);
    assert!(settings_of(&second).is_none());
}

#[test]
fn orphan_baseline_entry_under_the_new_key_is_dropped() {
    let mut w = world(&pets());
    let mut orphan = w.baseline.operations[0].clone();
    orphan.key = "listAllPets".to_string();
    w.baseline.operations.push(orphan);
    let mut next = pets();
    next[0].id = "listAllPets".to_string();
    next[0].summary = "List all pets".to_string();
    let spec = parsed(BASE, &next);
    let links = vec![("listPets".to_string(), "listAllPets".to_string())];
    let plan = plan_with_links(&sync_inputs(&w, &spec, true), &links).expect("plan");
    let mut sel = approve(&plan, &["listAllPets"]);
    sel.rename_links = links;
    let out = run(&w, &spec, &sel);
    let count = out
        .baseline
        .operations
        .iter()
        .filter(|b| b.key == "listAllPets")
        .count();
    assert_eq!(count, 1);
    assert!(out.baseline.operations.iter().all(|b| b.key != "listPets"));
}

#[test]
fn security_variable_is_added_once() {
    let w = world(&pets());
    let mut next = pets();
    next[0].secured = true;
    let spec = parsed(BASE, &next);
    let plan = plan_of(&w, &spec);
    let names: Vec<String> = plan
        .variable_changes
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert!(!names.is_empty());
    let mut sel = approve(&plan, &[]);
    sel.variables = names.iter().chain(names.iter()).cloned().collect();
    let out = run(&w, &spec, &sel);
    let settings = settings_of(&out).expect("settings staged");
    for name in &names {
        let found: Vec<_> = settings.variables.iter().filter(|v| &v.key == name).collect();
        assert_eq!(found.len(), 1);
        assert!(found[0].secret && found[0].value.is_empty());
    }
}

#[test]
fn apply_of_5000_items_is_linear() {
    let old: Vec<O> = (0..5000)
        .map(|i| o(&format!("op{i}"), &format!("/p{i}"), &format!("Name {i}")))
        .collect();
    let w = world(&old);
    let next: Vec<O> = (0..5000)
        .map(|i| o(&format!("op{i}"), &format!("/p{i}"), &format!("Renamed {i}")))
        .collect();
    let spec = parsed(BASE, &next);
    let sel = approve(&plan_of(&w, &spec), &[]);
    assert_eq!(sel.items.len(), 5000);
    let started = Instant::now();
    let out = run(&w, &spec, &sel);
    let took = started.elapsed();
    assert_eq!(out.report.applied.len(), 5000);
    assert!(took.as_secs_f64() < 2.0, "took {took:?}");
}
