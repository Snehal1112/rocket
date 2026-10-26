//! Builds the sync plan: what a new spec would change, given the last synced baseline and the
//! user's current requests. Pure code, nothing is written.

use super::merge::{merge_request, request_is_edited, FieldOutcome, MergeOutcome};
use super::rename::{pair_renames, RenameFacts};
use crate::converter::openapi::{
    baseline_hash, convert_operation, security_variable_names, BASE_URL_VARIABLE, CONVERTER_VERSION,
};
use crate::openapi::layout::folder_names;
use crate::openapi::{GroupBy, OpenApiDoc, Operation};
use rocket_collection::openapi_baseline::OpenApiBaseline;
use rocket_collection::openapi_source::{OpenApiSource, SourceOperation};
use rocket_collection::Request;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The fingerprint of a removed item.
const REMOVED_FINGERPRINT: &str = "removed";

/// A request of the collection as it is now. The caller loads these.
#[derive(Debug, Clone)]
pub struct CurrentRequest {
    pub uid: String,
    pub request_path: String,
    pub request: Request,
}

/// Everything the plan needs. `skips` maps an operation key to the fingerprint it was skipped at.
pub struct SyncInputs<'a> {
    pub source: &'a OpenApiSource,
    pub baseline: Option<&'a OpenApiBaseline>,
    pub incoming: &'a OpenApiDoc,
    pub current: &'a [CurrentRequest],
    pub current_base_url: Option<&'a str>,
    pub existing_variables: &'a [String],
    pub skips: &'a BTreeMap<String, String>,
}

/// One endpoint the plan reports.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncItem {
    New {
        key: String,
        name: String,
        /// Raw folder names, outermost first. The apply step makes them safe.
        folder: Vec<String>,
        request: Request,
        fingerprint: String,
        possibly_renamed_from: Option<String>,
    },
    Changed {
        key: String,
        uid: String,
        request_path: String,
        name: String,
        merge: MergeOutcome,
        fingerprint: String,
    },
    Removed {
        key: String,
        uid: String,
        request_path: String,
        name: String,
        /// `None` when it cannot be known (cautious mode). The apply step treats it as edited.
        edited: Option<bool>,
        possibly_renamed_to: Option<String>,
    },
}

impl SyncItem {
    /// The operation key.
    pub fn key(&self) -> &str {
        match self {
            SyncItem::New { key, .. }
            | SyncItem::Changed { key, .. }
            | SyncItem::Removed { key, .. } => key,
        }
    }

    fn fingerprint(&self) -> &str {
        match self {
            SyncItem::New { fingerprint, .. } | SyncItem::Changed { fingerprint, .. } => {
                fingerprint
            }
            SyncItem::Removed { .. } => REMOVED_FINGERPRINT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableKind {
    BaseUrl,
    AddSecurity,
}

/// A collection variable the spec would change or add.
#[derive(Debug, Clone, PartialEq)]
pub struct VariableChange {
    pub name: String,
    pub kind: VariableKind,
    /// The collection's value now, if the variable exists.
    pub yours: Option<String>,
    /// The spec's value. `None` for a new empty secret.
    pub spec: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SyncPlan {
    pub items: Vec<SyncItem>,
    pub variable_changes: Vec<VariableChange>,
    /// Items left out because the user skipped them at the same spec version.
    pub skipped_hidden: usize,
    /// Keys whose request the user deleted. They are not recreated.
    pub deleted_by_user: Vec<String>,
    /// True when any item was planned without a reliable baseline.
    pub cautious: bool,
    pub warnings: Vec<String>,
}

/// Hex SHA-256 over the synced fields: those of `baseline_hash` plus name and description.
pub fn fingerprint(request: &Request) -> String {
    let canonical = serde_json::json!({
        "synced": baseline_hash(request),
        "name": request.name,
        "description": request.description,
    });
    format!("{:x}", Sha256::digest(canonical.to_string().as_bytes()))
}

/// The indexes of rule 1, each built once.
struct Indexes<'a> {
    source_by_key: HashMap<&'a str, &'a SourceOperation>,
    current_by_uid: HashMap<&'a str, &'a CurrentRequest>,
    baseline_by_key: HashMap<&'a str, &'a Request>,
    incoming_keys: HashSet<&'a str>,
}

/// An item and the facts the rename step needs.
struct Draft {
    item: SyncItem,
    facts: Option<RenameFacts>,
}

/// Plan a sync. Rules run in order: index, classify and merge, skips, renames, variables.
pub fn build_sync_plan(inputs: &SyncInputs) -> SyncPlan {
    let mut plan = SyncPlan::default();
    let idx = build_indexes(inputs);
    let drafts = classify(inputs, &idx, &mut plan);
    let mut drafts = apply_skips(drafts, inputs.skips, &mut plan);
    hint_renames(&mut drafts);
    plan.items = drafts.into_iter().map(|d| d.item).collect();
    plan.variable_changes = variable_changes(inputs);
    if plan.cautious {
        plan.warnings.push(
            "There is no reliable record of the last sync for some requests. Nothing of yours \
             will be overwritten and no removal is offered as safe."
                .to_string(),
        );
    }
    plan
}

// Rule 1: index everything once.
fn build_indexes<'a>(inputs: &SyncInputs<'a>) -> Indexes<'a> {
    let trusted = inputs
        .baseline
        .filter(|b| b.converter_version == CONVERTER_VERSION);
    Indexes {
        source_by_key: inputs
            .source
            .operations
            .iter()
            .map(|o| (o.key.as_str(), o))
            .collect(),
        current_by_uid: inputs.current.iter().map(|c| (c.uid.as_str(), c)).collect(),
        baseline_by_key: trusted
            .map(|b| {
                b.operations
                    .iter()
                    .map(|o| (o.key.as_str(), &o.request))
                    .collect()
            })
            .unwrap_or_default(),
        incoming_keys: inputs
            .incoming
            .operations
            .iter()
            .map(|o| o.key.as_str())
            .collect(),
    }
}

// Rule 2: sort every key into New, Changed, Removed or deleted by the user. New and Changed
// follow the incoming order, then Removed follows the source order (rule 8).
fn classify(inputs: &SyncInputs, idx: &Indexes, plan: &mut SyncPlan) -> Vec<Draft> {
    let group_by = GroupBy::parse(&inputs.source.group_by).unwrap_or_default();
    let schemes = &inputs.incoming.security_schemes;
    let mut drafts = Vec::new();

    for (n, op) in inputs.incoming.operations.iter().enumerate() {
        let seq = u32::try_from(n + 1).unwrap_or(u32::MAX);
        let incoming = convert_operation(op, schemes, seq).request;
        match idx.source_by_key.get(op.key.as_str()) {
            None => drafts.push(new_draft(op, incoming, group_by)),
            Some(source_op) => match idx.current_by_uid.get(source_op.uid.as_str()) {
                None => plan.deleted_by_user.push(op.key.clone()),
                Some(cur) => {
                    if let Some(item) = changed_item(op, source_op, cur, &incoming, idx, plan) {
                        drafts.push(Draft { item, facts: None });
                    }
                }
            },
        }
    }

    for source_op in &inputs.source.operations {
        if idx.incoming_keys.contains(source_op.key.as_str()) {
            continue;
        }
        match idx.current_by_uid.get(source_op.uid.as_str()) {
            None => plan.deleted_by_user.push(source_op.key.clone()),
            Some(cur) => drafts.push(removed_draft(source_op, cur, idx, plan)),
        }
    }
    drafts
}

fn new_draft(op: &Operation, request: Request, group_by: GroupBy) -> Draft {
    let facts = RenameFacts {
        method: request.method.clone(),
        url: request.url.clone(),
        name: request.name.clone(),
    };
    Draft {
        item: SyncItem::New {
            key: op.key.clone(),
            name: request.name.clone(),
            folder: folder_names(op, group_by),
            fingerprint: fingerprint(&request),
            request,
            possibly_renamed_from: None,
        },
        facts: Some(facts),
    }
}

// Rules 3 and 4 for a removed operation: only a trusted baseline for this key can say whether
// the user edited the request.
fn removed_draft(
    source_op: &SourceOperation,
    cur: &CurrentRequest,
    idx: &Indexes,
    plan: &mut SyncPlan,
) -> Draft {
    let baseline = idx.baseline_by_key.get(source_op.key.as_str()).copied();
    let edited = match baseline {
        Some(b) => Some(request_is_edited(b, &cur.request)),
        None => {
            plan.cautious = true;
            None
        }
    };
    let shown = baseline.unwrap_or(&cur.request);
    Draft {
        item: SyncItem::Removed {
            key: source_op.key.clone(),
            uid: cur.uid.clone(),
            request_path: cur.request_path.clone(),
            name: cur.request.name.clone(),
            edited,
            possibly_renamed_to: None,
        },
        facts: Some(RenameFacts {
            method: shown.method.clone(),
            url: shown.url.clone(),
            name: shown.name.clone(),
        }),
    }
}

// Rules 3 and 4: a Changed item only when the merge reports a change.
fn changed_item(
    op: &Operation,
    source_op: &SourceOperation,
    cur: &CurrentRequest,
    incoming: &Request,
    idx: &Indexes,
    plan: &mut SyncPlan,
) -> Option<SyncItem> {
    let merge = match idx.baseline_by_key.get(op.key.as_str()) {
        Some(baseline) => merge_request(baseline, &cur.request, incoming, &BTreeMap::new()),
        None => {
            let merge = cautious_merge(source_op, &cur.request, incoming);
            if !merge.changes.is_empty() {
                plan.cautious = true;
            }
            merge
        }
    };
    if merge.changes.is_empty() {
        return None;
    }
    Some(SyncItem::Changed {
        key: op.key.clone(),
        uid: cur.uid.clone(),
        request_path: cur.request_path.clone(),
        name: cur.request.name.clone(),
        merge,
        fingerprint: fingerprint(incoming),
    })
}

/// Rule 4. The current request stands in for the baseline. A request whose synced fields still
/// hash to the stored value is untouched, so the spec's differences are taken, except name and
/// description, which the hash does not cover. Otherwise nothing is taken.
fn cautious_merge(
    source_op: &SourceOperation,
    current: &Request,
    incoming: &Request,
) -> MergeOutcome {
    let untouched = baseline_hash(current) == source_op.baseline_hash;
    let mut out = merge_request(current, current, incoming, &BTreeMap::new());
    out.cautious = true;
    if untouched {
        out.merged.name = current.name.clone();
        out.merged.description = current.description.clone();
    } else {
        out.merged = current.clone();
    }
    for change in &mut out.changes {
        if !untouched || change.field == "name" || change.field == "description" {
            change.outcome = FieldOutcome::KeepYours;
        }
    }
    out
}

// Rule 5: drop the items the user skipped at this very fingerprint.
fn apply_skips(
    drafts: Vec<Draft>,
    skips: &BTreeMap<String, String>,
    plan: &mut SyncPlan,
) -> Vec<Draft> {
    drafts
        .into_iter()
        .filter(|d| {
            let hidden = skips
                .get(d.item.key())
                .is_some_and(|f| f == d.item.fingerprint());
            if hidden {
                plan.skipped_hidden += 1;
            }
            !hidden
        })
        .collect()
}

// Rule 6: pair a single removed item with a single new one.
fn hint_renames(drafts: &mut [Draft]) {
    let mut removed_at = Vec::new();
    let mut new_at = Vec::new();
    let mut removed_facts = Vec::new();
    let mut new_facts = Vec::new();
    for (i, d) in drafts.iter().enumerate() {
        let Some(facts) = &d.facts else { continue };
        match d.item {
            SyncItem::Removed { .. } => {
                removed_at.push(i);
                removed_facts.push(facts.clone());
            }
            SyncItem::New { .. } => {
                new_at.push(i);
                new_facts.push(facts.clone());
            }
            SyncItem::Changed { .. } => {}
        }
    }
    for (r, n) in pair_renames(&removed_facts, &new_facts) {
        let (ri, ni) = (removed_at[r], new_at[n]);
        let removed_key = drafts[ri].item.key().to_string();
        let new_key = drafts[ni].item.key().to_string();
        if let SyncItem::Removed {
            possibly_renamed_to,
            ..
        } = &mut drafts[ri].item
        {
            *possibly_renamed_to = Some(new_key);
        }
        if let SyncItem::New {
            possibly_renamed_from,
            ..
        } = &mut drafts[ni].item
        {
            *possibly_renamed_from = Some(removed_key);
        }
    }
}

// Rule 7: the base URL (scalar rule) and the variables of new security schemes.
fn variable_changes(inputs: &SyncInputs) -> Vec<VariableChange> {
    let mut out = Vec::new();
    let non_empty = |s: &str| (!s.trim().is_empty()).then(|| s.to_string());
    let spec = inputs.incoming.servers.first().and_then(|s| non_empty(s));
    let yours = inputs.current_base_url.and_then(non_empty);
    // With no baseline the user's value stands in for it, so only the spec's news shows.
    let baseline = match inputs.baseline {
        Some(b) => non_empty(&b.base_url),
        None => yours.clone(),
    };
    if spec.is_some() && spec != baseline && spec != yours {
        out.push(VariableChange {
            name: BASE_URL_VARIABLE.to_string(),
            kind: VariableKind::BaseUrl,
            yours,
            spec,
        });
    }

    let mut known: HashSet<String> = inputs.existing_variables.iter().cloned().collect();
    for scheme in &inputs.incoming.security_schemes {
        for name in security_variable_names(scheme) {
            if known.insert(name.clone()) {
                out.push(VariableChange {
                    name,
                    kind: VariableKind::AddSecurity,
                    yours: None,
                    spec: None,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse_openapi;
    use rocket_collection::openapi_baseline::{BaselineOperation, BASELINE_FILE_VERSION};
    use rocket_collection::openapi_source::{SpecLocation, SyncInterval, SOURCE_FILE_VERSION};
    use std::time::Instant;

    const BASE: &str = "https://api.example.com";

    #[derive(Clone)]
    struct Op {
        id: String,
        method: &'static str,
        path: String,
        summary: String,
        tag: &'static str,
        body: Option<&'static str>,
    }

    fn op(id: &str, method: &'static str, path: &str, summary: &str) -> Op {
        Op {
            id: id.to_string(),
            method,
            path: path.to_string(),
            summary: summary.to_string(),
            tag: "Pets",
            body: None,
        }
    }

    fn spec_text(servers: &[&str], ops: &[Op], bearer: &[&str]) -> String {
        let servers: Vec<_> = servers
            .iter()
            .map(|u| serde_json::json!({ "url": u }))
            .collect();
        let paths: Vec<String> = ops
            .iter()
            .map(|o| {
                let mut inner = serde_json::json!({
                    "operationId": o.id,
                    "summary": o.summary,
                    "tags": [o.tag],
                    "responses": { "200": { "description": "ok" } },
                });
                if let Some(body) = o.body {
                    let example: serde_json::Value =
                        serde_json::from_str(body).expect("example json");
                    inner["requestBody"] = serde_json::json!({
                        "content": { "application/json": { "example": example } }
                    });
                }
                if let Some(first) = bearer.first() {
                    inner["security"] = serde_json::json!([{ *first: [] }]);
                }
                format!(
                    "{}: {{ {}: {} }}",
                    json_str(&o.path),
                    json_str(&o.method.to_lowercase()),
                    inner
                )
            })
            .collect();
        let schemes: serde_json::Map<String, serde_json::Value> = bearer
            .iter()
            .map(|n| {
                (
                    n.to_string(),
                    serde_json::json!({ "type": "http", "scheme": "bearer" }),
                )
            })
            .collect();
        format!(
            "{{\"openapi\":\"3.0.3\",\"info\":{{\"title\":\"T\",\"version\":\"1\"}},\"servers\":{},\
             \"paths\":{{{}}},\"components\":{{\"securitySchemes\":{}}}}}",
            serde_json::Value::Array(servers),
            paths.join(","),
            serde_json::Value::Object(schemes)
        )
    }

    fn json_str(s: &str) -> String {
        serde_json::Value::String(s.to_string()).to_string()
    }

    fn doc(ops: &[Op]) -> OpenApiDoc {
        parse_openapi(&spec_text(&[BASE], ops, &[])).expect("spec parses")
    }

    struct World {
        source: OpenApiSource,
        baseline: OpenApiBaseline,
        current: Vec<CurrentRequest>,
        variables: Vec<String>,
    }

    /// What an import of `doc` leaves behind.
    fn import(doc: &OpenApiDoc, group_by: GroupBy) -> World {
        let mut operations = Vec::new();
        let mut baseline_ops = Vec::new();
        let mut current = Vec::new();
        for (i, o) in doc.operations.iter().enumerate() {
            let request = convert_operation(o, &doc.security_schemes, (i + 1) as u32).request;
            let request_path = format!("{}/{}", o.tags.first().cloned().unwrap_or_default(), i);
            operations.push(SourceOperation {
                key: o.key.clone(),
                uid: request.uid.clone(),
                request_path: request_path.clone(),
                baseline_hash: baseline_hash(&request),
            });
            baseline_ops.push(BaselineOperation {
                key: o.key.clone(),
                request: request.clone(),
            });
            current.push(CurrentRequest {
                uid: request.uid.clone(),
                request_path,
                request,
            });
        }
        let mut variables = vec![BASE_URL_VARIABLE.to_string()];
        for s in &doc.security_schemes {
            variables.extend(security_variable_names(s));
        }
        World {
            source: OpenApiSource {
                version: SOURCE_FILE_VERSION,
                location: SpecLocation::Https {
                    url: "https://example.com/spec.json".to_string(),
                },
                group_by: group_by.as_str().to_string(),
                spec_sha256: String::new(),
                etag: None,
                last_modified: None,
                operations,
                source_id: String::new(),
                sync_interval: SyncInterval::Off,
                last_checked_at: None,
                last_status: None,
                pending_sha256: None,
            },
            baseline: OpenApiBaseline {
                version: BASELINE_FILE_VERSION,
                converter_version: CONVERTER_VERSION,
                base_url: doc.servers.first().cloned().unwrap_or_default(),
                operations: baseline_ops,
            },
            current,
            variables,
        }
    }

    fn plan_with(
        w: &World,
        baseline: Option<&OpenApiBaseline>,
        incoming: &OpenApiDoc,
        skips: &BTreeMap<String, String>,
    ) -> SyncPlan {
        build_sync_plan(&SyncInputs {
            source: &w.source,
            baseline,
            incoming,
            current: &w.current,
            current_base_url: Some(BASE),
            existing_variables: &w.variables,
            skips,
        })
    }

    fn plan(w: &World, incoming: &OpenApiDoc) -> SyncPlan {
        plan_with(w, Some(&w.baseline), incoming, &BTreeMap::new())
    }

    fn changed<'a>(p: &'a SyncPlan, key: &str) -> &'a MergeOutcome {
        p.items
            .iter()
            .find_map(|i| match i {
                SyncItem::Changed { key: k, merge, .. } if k == key => Some(merge),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no Changed item for {key}: {:?}", p.items))
    }

    fn keys(p: &SyncPlan) -> Vec<&str> {
        p.items.iter().map(|i| i.key()).collect()
    }

    fn pets() -> Vec<Op> {
        vec![
            op("listPets", "GET", "/pets", "List pets"),
            op("getPet", "GET", "/pets/{id}", "Get a pet"),
        ]
    }

    #[test]
    fn unchanged_spec_gives_an_empty_plan() {
        let d = doc(&pets());
        let w = import(&d, GroupBy::Tags);
        let p = plan(&w, &doc(&pets()));
        assert!(p.items.is_empty());
        assert!(p.variable_changes.is_empty());
        assert!(p.deleted_by_user.is_empty());
        assert!(!p.cautious);
        assert_eq!(p.skipped_hidden, 0);
    }

    #[test]
    fn new_operation_is_new_with_its_tag_folder_and_a_converted_request() {
        let w = import(&doc(&pets()), GroupBy::Tags);
        let mut ops = pets();
        let mut o = op("addPet", "POST", "/pets", "Add a pet");
        o.path = "/orders".to_string();
        o.tag = "Orders";
        ops.push(o);
        let p = plan(&w, &doc(&ops));
        assert_eq!(p.items.len(), 1);
        let SyncItem::New {
            key,
            name,
            folder,
            request,
            fingerprint: fp,
            possibly_renamed_from,
        } = &p.items[0]
        else {
            panic!("not New: {:?}", p.items[0]);
        };
        assert_eq!(key, "addPet");
        assert_eq!(name, "Add a pet");
        assert_eq!(folder, &vec!["Orders".to_string()]);
        assert_eq!(request.url, "{{baseUrl}}/orders");
        assert_eq!(fp, &fingerprint(request));
        assert!(possibly_renamed_from.is_none());
    }

    #[test]
    fn new_operation_with_paths_grouping_uses_the_path_folders() {
        let w = import(&doc(&pets()), GroupBy::Paths);
        let mut ops = pets();
        ops.push(op("listOrders", "GET", "/api/v1/orders/{id}", "Orders"));
        let p = plan(&w, &doc(&ops));
        let SyncItem::New { folder, .. } = &p.items[0] else {
            panic!("not New");
        };
        assert_eq!(
            folder,
            &vec!["api".to_string(), "v1".into(), "orders".into()]
        );
    }

    #[test]
    fn changed_summary_url_or_body_gives_a_changed_item_with_those_fields() {
        let mut old = pets();
        old[0].body = Some(r#"{"limit":1}"#);
        let w = import(&doc(&old), GroupBy::Tags);
        let mut new = old.clone();
        new[0].summary = "List all pets".to_string();
        new[0].path = "/pets/all".to_string();
        new[0].body = Some(r#"{"limit":2}"#);
        let p = plan(&w, &doc(&new));
        assert_eq!(keys(&p), vec!["listPets"]);
        let m = changed(&p, "listPets");
        let fields: Vec<&str> = m.changes.iter().map(|c| c.field.as_str()).collect();
        assert!(fields.contains(&"name"), "{fields:?}");
        assert!(fields.contains(&"url"), "{fields:?}");
        assert!(fields.iter().any(|f| f.starts_with("body")), "{fields:?}");
        assert_eq!(m.merged.url, "{{baseUrl}}/pets/all");
        assert!(!m.cautious);
    }

    #[test]
    fn removed_operation_unedited_is_removed_with_edited_false() {
        let w = import(&doc(&pets()), GroupBy::Tags);
        let p = plan(&w, &doc(&pets()[..1]));
        let SyncItem::Removed { key, edited, .. } = &p.items[0] else {
            panic!("not Removed");
        };
        assert_eq!(key, "getPet");
        assert_eq!(*edited, Some(false));
        assert!(!p.cautious);
    }

    #[test]
    fn removed_operation_with_a_user_script_is_edited_true() {
        let mut w = import(&doc(&pets()), GroupBy::Tags);
        w.current[1].request.pre_request_script = Some("console.log(1)".to_string());
        let p = plan(&w, &doc(&pets()[..1]));
        let SyncItem::Removed { edited, .. } = &p.items[0] else {
            panic!("not Removed");
        };
        assert_eq!(*edited, Some(true));
    }

    #[test]
    fn request_moved_to_another_folder_is_still_matched_by_uid() {
        let mut w = import(&doc(&pets()), GroupBy::Tags);
        w.current[0].request_path = "Elsewhere/renamed-by-user".to_string();
        let mut new = pets();
        new[0].summary = "List pets v2".to_string();
        let p = plan(&w, &doc(&new));
        let SyncItem::Changed { request_path, .. } = &p.items[0] else {
            panic!("not Changed");
        };
        assert_eq!(request_path, "Elsewhere/renamed-by-user");
    }

    #[test]
    fn request_the_user_deleted_is_listed_in_deleted_by_user_and_not_recreated() {
        let mut w = import(&doc(&pets()), GroupBy::Tags);
        w.current.remove(0);
        let mut new = pets();
        new[0].summary = "List pets v2".to_string();
        let p = plan(&w, &doc(&new));
        assert!(p.items.is_empty());
        assert_eq!(p.deleted_by_user, vec!["listPets".to_string()]);

        // Removed from the spec as well: still the user's deletion, not a Removed item.
        let p = plan(&w, &doc(&pets()[1..]));
        assert!(p.items.is_empty());
        assert_eq!(p.deleted_by_user, vec!["listPets".to_string()]);
    }

    #[test]
    fn skipped_change_is_hidden_until_the_fingerprint_changes() {
        let w = import(&doc(&pets()), GroupBy::Tags);
        let mut v2 = pets();
        v2[0].summary = "Second".to_string();
        let first = plan(&w, &doc(&v2));
        let SyncItem::Changed {
            fingerprint: fp, ..
        } = &first.items[0]
        else {
            panic!("not Changed");
        };
        let skips = BTreeMap::from([("listPets".to_string(), fp.clone())]);
        let hidden = plan_with(&w, Some(&w.baseline), &doc(&v2), &skips);
        assert!(hidden.items.is_empty());
        assert_eq!(hidden.skipped_hidden, 1);

        let mut v3 = pets();
        v3[0].summary = "Third".to_string();
        let shown = plan_with(&w, Some(&w.baseline), &doc(&v3), &skips);
        assert_eq!(keys(&shown), vec!["listPets"]);
        assert_eq!(shown.skipped_hidden, 0);
    }

    #[test]
    fn a_skipped_removal_is_hidden_with_the_fixed_fingerprint() {
        let w = import(&doc(&pets()), GroupBy::Tags);
        let skips = BTreeMap::from([("getPet".to_string(), "removed".to_string())]);
        let p = plan_with(&w, Some(&w.baseline), &doc(&pets()[..1]), &skips);
        assert!(p.items.is_empty());
        assert_eq!(p.skipped_hidden, 1);
    }

    #[test]
    fn rename_hint_pairs_a_single_operation_id_change() {
        let w = import(&doc(&pets()), GroupBy::Tags);
        let mut new = pets();
        new[1].id = "fetchPet".to_string();
        let p = plan(&w, &doc(&new));
        assert_eq!(keys(&p), vec!["fetchPet", "getPet"]);
        let SyncItem::New {
            possibly_renamed_from,
            ..
        } = &p.items[0]
        else {
            panic!("not New");
        };
        assert_eq!(possibly_renamed_from.as_deref(), Some("getPet"));
        let SyncItem::Removed {
            possibly_renamed_to,
            ..
        } = &p.items[1]
        else {
            panic!("not Removed");
        };
        assert_eq!(possibly_renamed_to.as_deref(), Some("fetchPet"));
    }

    #[test]
    fn rename_hint_pairs_a_single_path_change_with_the_same_summary() {
        let w = import(&doc(&pets()), GroupBy::Tags);
        let mut new = pets();
        new[1].id = "getAnimal".to_string();
        new[1].path = "/animals/{id}".to_string();
        let p = plan(&w, &doc(&new));
        let SyncItem::New {
            possibly_renamed_from,
            ..
        } = &p.items[0]
        else {
            panic!("not New");
        };
        assert_eq!(possibly_renamed_from.as_deref(), Some("getPet"));
        let SyncItem::Removed {
            possibly_renamed_to,
            ..
        } = &p.items[1]
        else {
            panic!("not Removed");
        };
        assert_eq!(possibly_renamed_to.as_deref(), Some("getAnimal"));
    }

    #[test]
    fn ambiguous_candidates_get_no_hint() {
        let old = vec![op("r1", "GET", "/a", "Same"), op("r2", "GET", "/b", "Same")];
        let w = import(&doc(&old), GroupBy::Tags);
        let p = plan(&w, &doc(&[op("n1", "GET", "/c", "Same")]));
        assert_eq!(p.items.len(), 3);
        for item in &p.items {
            match item {
                SyncItem::New {
                    possibly_renamed_from,
                    ..
                } => {
                    assert!(possibly_renamed_from.is_none())
                }
                SyncItem::Removed {
                    possibly_renamed_to,
                    ..
                } => {
                    assert!(possibly_renamed_to.is_none())
                }
                SyncItem::Changed { .. } => panic!("unexpected Changed"),
            }
        }
    }

    #[test]
    fn a_pair_with_a_different_method_gets_no_hint() {
        let w = import(&doc(&pets()), GroupBy::Tags);
        let mut new = pets();
        new[1].id = "putPet".to_string();
        new[1].method = "PUT";
        let p = plan(&w, &doc(&new));
        assert!(p.items.iter().all(|i| !matches!(
            i,
            SyncItem::New {
                possibly_renamed_from: Some(_),
                ..
            }
        )));
    }

    #[test]
    fn no_baseline_is_cautious_and_never_overwrites_an_edited_request() {
        let mut w = import(&doc(&pets()), GroupBy::Tags);
        w.current[0].request.url = "{{baseUrl}}/my-pets".to_string();
        w.current[0].request.tests = Some("test()".to_string());
        let mut new = pets();
        new[0].summary = "List pets v2".to_string();
        new[0].path = "/pets/v2".to_string();
        new.truncate(1);
        let p = plan_with(&w, None, &doc(&new), &BTreeMap::new());
        assert!(p.cautious);
        assert!(!p.warnings.is_empty());
        let m = changed(&p, "listPets");
        assert!(m.cautious);
        assert!(!m.changes.is_empty());
        assert!(m
            .changes
            .iter()
            .all(|c| c.outcome == FieldOutcome::KeepYours));
        assert_eq!(m.merged, w.current[0].request);
        let SyncItem::Removed { edited, .. } = &p.items[1] else {
            panic!("not Removed");
        };
        assert_eq!(*edited, None);
    }

    #[test]
    fn no_baseline_and_an_untouched_request_takes_the_specs_changes() {
        let w = import(&doc(&pets()), GroupBy::Tags);
        let mut new = pets();
        new[0].path = "/pets/v2".to_string();
        new[0].summary = "Renamed in the spec".to_string();
        let p = plan_with(&w, None, &doc(&new), &BTreeMap::new());
        assert!(p.cautious);
        let m = changed(&p, "listPets");
        assert!(m.cautious);
        let url = m.changes.iter().find(|c| c.field == "url").expect("url");
        assert_eq!(url.outcome, FieldOutcome::TakeSpec);
        assert_eq!(m.merged.url, "{{baseUrl}}/pets/v2");
        // The name is not covered by the stored hash, so it is never taken silently.
        let name = m.changes.iter().find(|c| c.field == "name").expect("name");
        assert_eq!(name.outcome, FieldOutcome::KeepYours);
        assert_eq!(m.merged.name, "List pets");
    }

    #[test]
    fn another_converter_version_is_cautious() {
        let mut w = import(&doc(&pets()), GroupBy::Tags);
        w.baseline.converter_version = CONVERTER_VERSION + 1;
        // A baseline from other rules would look like a mass edit if it were trusted.
        for o in &mut w.baseline.operations {
            o.request.url = "{{baseUrl}}/other".to_string();
        }
        w.current[0].request.url = "{{baseUrl}}/mine".to_string();
        let mut new = pets();
        new[0].summary = "List pets v2".to_string();
        let p = plan(&w, &doc(&new));
        assert!(p.cautious);
        let m = changed(&p, "listPets");
        assert!(m.cautious);
        assert_eq!(m.merged, w.current[0].request);
        let p = plan(&w, &doc(&pets()[..1]));
        let edited = p
            .items
            .iter()
            .find_map(|i| match i {
                SyncItem::Removed { edited, .. } => Some(*edited),
                _ => None,
            })
            .expect("a Removed item");
        assert_eq!(edited, None);
    }

    #[test]
    fn key_missing_from_the_baseline_is_cautious_for_that_request_only() {
        let mut w = import(&doc(&pets()), GroupBy::Tags);
        w.baseline.operations.retain(|o| o.key != "getPet");
        let mut new = pets();
        new[0].summary = "List pets v2".to_string();
        new[1].summary = "Get a pet v2".to_string();
        let p = plan(&w, &doc(&new));
        assert!(p.cautious);
        assert!(!changed(&p, "listPets").cautious);
        assert!(changed(&p, "getPet").cautious);
    }

    #[test]
    fn base_url_change_is_listed_and_the_users_edit_wins() {
        let w = import(&doc(&pets()), GroupBy::Tags);
        let new = parse_openapi(&spec_text(&["https://v2.example.com"], &pets(), &[]))
            .expect("spec parses");
        let p = build_sync_plan(&SyncInputs {
            source: &w.source,
            baseline: Some(&w.baseline),
            incoming: &new,
            current: &w.current,
            current_base_url: Some("https://mine.example.com"),
            existing_variables: &w.variables,
            skips: &BTreeMap::new(),
        });
        assert_eq!(
            p.variable_changes,
            vec![VariableChange {
                name: "baseUrl".to_string(),
                kind: VariableKind::BaseUrl,
                yours: Some("https://mine.example.com".to_string()),
                spec: Some("https://v2.example.com".to_string()),
            }]
        );
        // Unchanged server: nothing listed.
        let same = plan(&w, &doc(&pets()));
        assert!(same.variable_changes.is_empty());
    }

    #[test]
    fn a_new_security_scheme_adds_its_variables() {
        let w = import(&doc(&pets()), GroupBy::Tags);
        let new =
            parse_openapi(&spec_text(&[BASE], &pets(), &["bearerAuth"])).expect("spec parses");
        let expected: Vec<String> = new
            .security_schemes
            .iter()
            .flat_map(security_variable_names)
            .collect();
        assert!(!expected.is_empty());
        let p = plan(&w, &new);
        let names: Vec<&String> = p.variable_changes.iter().map(|v| &v.name).collect();
        assert_eq!(names, expected.iter().collect::<Vec<_>>());
        assert!(p
            .variable_changes
            .iter()
            .all(|v| v.kind == VariableKind::AddSecurity));
        // The variable exists already: nothing to add.
        let mut w2 = import(&doc(&pets()), GroupBy::Tags);
        w2.variables.extend(expected);
        assert!(plan(&w2, &new).variable_changes.is_empty());
    }

    #[test]
    fn items_come_in_a_stable_order() {
        let old = vec![
            op("a", "GET", "/a", "A"),
            op("b", "GET", "/b", "B"),
            op("c", "GET", "/c", "C"),
            op("d", "GET", "/d", "D"),
        ];
        let w = import(&doc(&old), GroupBy::Tags);
        let new = vec![
            op("n1", "POST", "/n1", "N1"),
            op("c", "GET", "/c", "C changed"),
            op("n2", "POST", "/n2", "N2"),
            op("a", "GET", "/a", "A changed"),
        ];
        let p = plan(&w, &doc(&new));
        assert_eq!(keys(&p), vec!["n1", "c", "n2", "a", "b", "d"]);
        assert_eq!(keys(&plan(&w, &doc(&new))), keys(&p));
    }

    #[test]
    fn plan_for_5000_operations_is_linear() {
        let old: Vec<Op> = (0..5000)
            .map(|i| {
                op(
                    &format!("op{i}"),
                    "GET",
                    &format!("/r{i}"),
                    &format!("Op {i}"),
                )
            })
            .collect();
        let w = import(&doc(&old), GroupBy::Tags);
        // Every 10th changes, the last 100 go away and 100 new ones arrive.
        let mut new: Vec<Op> = old[..4900].to_vec();
        for (i, o) in new.iter_mut().enumerate() {
            if i % 10 == 0 {
                o.summary = format!("Changed {i}");
            }
        }
        for i in 0..100 {
            new.push(op(
                &format!("new{i}"),
                "POST",
                &format!("/new{i}"),
                &format!("New {i}"),
            ));
        }
        let incoming = doc(&new);
        let start = Instant::now();
        let p = plan(&w, &incoming);
        let elapsed = start.elapsed();
        assert_eq!(p.items.len(), 490 + 100 + 100);
        assert!(elapsed.as_secs_f64() < 2.0, "took {elapsed:?}");
    }
}
