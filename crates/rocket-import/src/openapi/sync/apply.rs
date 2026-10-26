//! Turns the items a user approved into file changes and new state. Pure code: it reads the
//! inputs, returns what to write, and does no I/O. The caller stages the result in one file
//! transaction (see `rocket_collection::file_tx`).

use super::apply_paths::{check, unsafe_path, PathAlloc};
use super::apply_types::{
    AppliedItem, AppliedKind, ApplyInputs, ApplyOutput, ApplyReport, FileChange, FileChangeKind,
};
use super::merge::Choice;
use super::plan::{
    build_sync_plan, item_digest, variable_digest, merge_item_indexed, CurrentRequest, SyncInputs, SyncItem,
    SyncPlan, VariableKind,
};
use crate::converter::openapi::{baseline_hash, CONVERTER_VERSION};
use crate::error::ImportResult;
use rocket_collection::openapi_baseline::{
    BaselineOperation, OpenApiBaseline, BASELINE_FILE_VERSION,
};
use rocket_collection::openapi_local::OpenApiLocalState;
use rocket_collection::openapi_source::{OpenApiSource, SourceOperation, SyncStatus};
use rocket_collection::{request_filename_for, CollectionVariable, Request};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The plan after rename links, with the state the links produced.
struct LinkedPlan {
    plan: SyncPlan,
    source: OpenApiSource,
    baseline: Option<OpenApiBaseline>,
    stale: Vec<String>,
}

fn plan_linked(pi: &SyncInputs, links: &[(String, String)]) -> LinkedPlan {
    let mut source = pi.source.clone();
    let mut baseline = pi.baseline.cloned();
    let mut stale = Vec::new();
    link_renames(pi, links, &mut source, baseline.as_mut(), &mut stale);
    let plan = build_sync_plan(&SyncInputs {
        source: &source,
        baseline: baseline.as_ref(),
        incoming: pi.incoming,
        current: pi.current,
        current_base_url: pi.current_base_url,
        existing_variables: pi.existing_variables,
        skips: pi.skips,
    });
    LinkedPlan {
        plan,
        source,
        baseline,
        stale,
    }
}

/// The plan `build_apply` works from once the rename links are applied. Callers compute the
/// digests they show or approve on this plan, so both sides agree. A link that is refused
/// is left out here and reported as stale by `build_apply`.
pub fn plan_with_links(
    inputs: &SyncInputs,
    links: &[(String, String)],
) -> ImportResult<SyncPlan> {
    Ok(plan_linked(inputs, links).plan)
}

/// Applies the selection. Rules run in order: rename links, plan, items, variables, skips,
/// finalize.
pub fn build_apply(inputs: &ApplyInputs) -> ImportResult<ApplyOutput> {
    let pi = &inputs.plan_inputs;
    let linked = plan_linked(pi, &inputs.selection.rename_links);
    let had_baseline = linked.baseline.is_some();
    // A new baseline records the spec's base URL like a fresh import. It never records the
    // user's value, or a customized URL would later look untouched.
    let baseline = linked.baseline.unwrap_or_else(|| OpenApiBaseline {
        version: BASELINE_FILE_VERSION,
        converter_version: CONVERTER_VERSION,
        base_url: pi
            .incoming
            .doc()
            .servers
            .first()
            .cloned()
            .unwrap_or_default(),
        operations: Vec::new(),
    });
    let mut ap = Applier::new(inputs, linked.source, baseline, had_baseline, linked.stale);
    ap.apply_items(&linked.plan)?;
    ap.apply_variables(&linked.plan)?;
    ap.apply_skips(&linked.plan);
    Ok(ap.finish(&linked.plan))
}

// Rule 1: re-point linked keys in the source and the baseline.
fn link_renames(
    pi: &SyncInputs,
    links: &[(String, String)],
    source: &mut OpenApiSource,
    mut baseline: Option<&mut OpenApiBaseline>,
    stale: &mut Vec<String>,
) {
    if links.is_empty() {
        return;
    }
    let incoming: HashSet<&str> = pi
        .incoming
        .doc()
        .operations
        .iter()
        .map(|o| o.key.as_str())
        .collect();
    let mut src_at: HashMap<String, usize> = source
        .operations
        .iter()
        .enumerate()
        .map(|(i, o)| (o.key.clone(), i))
        .collect();
    let mut base_at: HashMap<String, usize> = baseline
        .as_ref()
        .map(|b| {
            b.operations
                .iter()
                .enumerate()
                .map(|(i, o)| (o.key.clone(), i))
                .collect()
        })
        .unwrap_or_default();
    let mut orphans: HashSet<usize> = HashSet::new();
    for (old, new) in links {
        let valid = src_at.contains_key(old)
            && !incoming.contains(old.as_str())
            && incoming.contains(new.as_str())
            && !src_at.contains_key(new);
        let Some(at) = src_at.get(old).copied().filter(|_| valid) else {
            stale.push(old.clone());
            continue;
        };
        source.operations[at].key = new.clone();
        src_at.remove(old);
        src_at.insert(new.clone(), at);
        // An entry already under the new key has no request; drop it so no key repeats.
        if let Some(orphan) = base_at.remove(new) {
            orphans.insert(orphan);
        }
        if let (Some(b), Some(i)) = (baseline.as_deref_mut(), base_at.remove(old)) {
            b.operations[i].key = new.clone();
            base_at.insert(new.clone(), i);
        }
    }
    if let (Some(b), false) = (baseline, orphans.is_empty()) {
        let mut at = 0;
        b.operations.retain(|_| {
            at += 1;
            !orphans.contains(&(at - 1))
        });
    }
}

struct Applier<'a> {
    inputs: &'a ApplyInputs<'a>,
    source: OpenApiSource,
    baseline: OpenApiBaseline,
    had_baseline: bool,
    trusted: bool,
    src_at: HashMap<String, usize>,
    base_at: HashMap<String, usize>,
    cur_by_uid: HashMap<&'a str, &'a CurrentRequest>,
    alloc: PathAlloc,
    seen: HashSet<String>,
    files: Vec<FileChange>,
    drop_keys: HashSet<String>,
    new_src: Vec<SourceOperation>,
    new_base: Vec<BaselineOperation>,
    report: ApplyReport,
    local: OpenApiLocalState,
    resolved: HashSet<String>,
    stale: HashSet<String>,
    base_url_selected: Option<String>,
}

impl<'a> Applier<'a> {
    fn new(
        inputs: &'a ApplyInputs<'a>,
        source: OpenApiSource,
        baseline: OpenApiBaseline,
        had_baseline: bool,
        stale: Vec<String>,
    ) -> Self {
        let alloc = PathAlloc::new(
            inputs.existing_paths,
            source.operations.iter().map(|o| o.request_path.as_str()),
        );
        let mut local = OpenApiLocalState::default();
        local.skips = inputs.plan_inputs.skips.clone();
        let mut report = ApplyReport::default();
        let mut stale_set = HashSet::new();
        for key in stale {
            if stale_set.insert(key.clone()) {
                report.stale.push(key);
            }
        }
        Self {
            inputs,
            trusted: had_baseline && baseline.converter_version == CONVERTER_VERSION,
            src_at: index_of(source.operations.iter().map(|o| o.key.as_str())),
            base_at: index_of(baseline.operations.iter().map(|o| o.key.as_str())),
            cur_by_uid: inputs
                .plan_inputs
                .current
                .iter()
                .map(|c| (c.uid.as_str(), c))
                .collect(),
            source,
            baseline,
            had_baseline,
            alloc,
            seen: HashSet::new(),
            files: Vec::new(),
            drop_keys: HashSet::new(),
            new_src: Vec::new(),
            new_base: Vec::new(),
            report,
            local,
            resolved: HashSet::new(),
            stale: stale_set,
            base_url_selected: None,
        }
    }

    fn mark_stale(&mut self, key: &str) {
        if self.stale.insert(key.to_string()) {
            self.report.stale.push(key.to_string());
        }
    }

    /// Stages a file put or delete once per path (compared case-folded).
    fn stage(&mut self, rel: String, kind: FileChangeKind) -> bool {
        if check(&rel).is_err() || !self.seen.insert(rocket_collection::fold_name(&rel)) {
            return false;
        }
        self.files.push(FileChange {
            rel_path: rel,
            kind,
        });
        true
    }

    // Rules 2 to 5: find each approved item in the plan and apply it.
    fn apply_items(&mut self, plan: &SyncPlan) -> ImportResult<()> {
        let by_key: HashMap<&str, &SyncItem> = plan.items.iter().map(|i| (i.key(), i)).collect();
        let mut done: HashSet<&str> = HashSet::new();
        let selection = self.inputs.selection;
        for approved in &selection.items {
            if !done.insert(approved.key.as_str()) {
                continue;
            }
            let Some(item) = by_key.get(approved.key.as_str()) else {
                self.mark_stale(&approved.key);
                continue;
            };
            if item_digest(item) != approved.digest {
                self.mark_stale(&approved.key);
                continue;
            }
            match item {
                SyncItem::New { .. } => self.apply_new(item)?,
                SyncItem::Changed { .. } => self.apply_changed(item, &approved.choices),
                SyncItem::Removed { .. } => self.apply_removed(item),
            }
        }
        Ok(())
    }

    fn resolve(&mut self, key: &str) {
        self.resolved.insert(key.to_string());
        self.local.skips.remove(key);
    }

    // Rule 3.
    fn apply_new(&mut self, item: &SyncItem) -> ImportResult<()> {
        let SyncItem::New {
            key,
            folder,
            request,
            ..
        } = item
        else {
            return Ok(());
        };
        let dir = self.alloc.folder(folder)?;
        let (path, n) = self.alloc.request(&dir, &request.name)?;
        let mut req = request.clone();
        if n > 1 {
            req.name = format!("{} ({n})", req.name);
        }
        let bytes = self.inputs.renderer.render(&path, &req)?;
        let created: Vec<String> = self.alloc.new_folders.drain(..).collect();
        for folder_path in created {
            let rel = self.alloc.folder_file_for(&folder_path)?;
            let folder_bytes = self.inputs.renderer.render_folder(&folder_path)?;
            if !self.stage(rel.clone(), FileChangeKind::Put(folder_bytes)) {
                return Err(unsafe_path(&rel));
            }
        }
        let rel = request_filename_for(&path);
        if !self.stage(rel.clone(), FileChangeKind::Put(bytes)) {
            return Err(unsafe_path(&rel));
        }
        self.new_src.push(SourceOperation {
            key: key.clone(),
            uid: req.uid.clone(),
            request_path: path.clone(),
            baseline_hash: baseline_hash(&req),
        });
        self.new_base.push(BaselineOperation {
            key: key.clone(),
            request: req,
        });
        self.report.applied.push(AppliedItem {
            key: key.clone(),
            kind: AppliedKind::Created,
            request_path: path,
        });
        self.resolve(key);
        Ok(())
    }

    // Rule 4.
    fn apply_changed(&mut self, item: &SyncItem, choices: &BTreeMap<String, Choice>) {
        let SyncItem::Changed {
            key,
            uid,
            request_path,
            spec,
            ..
        } = item
        else {
            return;
        };
        let (Some(cur), Some(&at)) = (
            self.cur_by_uid.get(uid.as_str()).copied(),
            self.src_at.get(key),
        )
        else {
            self.mark_stale(key);
            return;
        };
        let rewritable = !self.yaml_only(request_path)
            && self
                .inputs
                .renderer
                .can_rewrite(request_path, &cur.request)
                .unwrap_or(false);
        let base_req = self.trusted_request(key);
        let outcome = merge_item_indexed(
            base_req,
            &self.source.operations[at],
            &cur.request,
            spec,
            choices,
        );
        let bytes = self.inputs.renderer.render(request_path, &outcome.merged);
        let staged = match (rewritable, bytes) {
            (true, Ok(bytes)) => {
                self.stage(request_filename_for(request_path), FileChangeKind::Put(bytes))
            }
            _ => false,
        };
        if !staged {
            self.report.not_rewritable.push(key.clone());
            return;
        }
        let mut snapshot = spec.clone();
        snapshot.uid = cur.uid.clone();
        let op = &mut self.source.operations[at];
        op.baseline_hash = baseline_hash(spec);
        op.request_path = request_path.clone();
        match self.base_at.get(key) {
            Some(&b) => self.baseline.operations[b].request = snapshot,
            None => self.new_base.push(BaselineOperation {
                key: key.clone(),
                request: snapshot,
            }),
        }
        self.report.applied.push(AppliedItem {
            key: key.clone(),
            kind: AppliedKind::Updated,
            request_path: request_path.clone(),
        });
        self.resolve(key);
    }

    /// True when the request is stored as `.yaml` only. Writes and deletes target `.yml`, so
    /// such a request is left alone.
    fn yaml_only(&self, request_path: &str) -> bool {
        let existing = self.inputs.existing_paths;
        existing.contains(&format!("{request_path}.yaml"))
            && !existing.contains(&format!("{request_path}.yml"))
    }

    /// The baseline request for a key, only when the baseline can be trusted.
    fn trusted_request(&self, key: &str) -> Option<&Request> {
        if !self.trusted {
            return None;
        }
        self.base_at
            .get(key)
            .map(|&i| &self.baseline.operations[i].request)
    }

    // Rule 5.
    fn apply_removed(&mut self, item: &SyncItem) {
        let SyncItem::Removed {
            key,
            uid,
            request_path,
            edited,
            ..
        } = item
        else {
            return;
        };
        // Only an unedited request that Rocket can write back unchanged is deleted.
        let rewritable = !self.yaml_only(request_path)
            && self.cur_by_uid.get(uid.as_str()).is_some_and(|cur| {
                self.inputs
                    .renderer
                    .can_rewrite(request_path, &cur.request)
                    .unwrap_or(false)
            });
        let deletable = *edited == Some(false)
            && rewritable
            && self.stage(request_filename_for(request_path), FileChangeKind::Delete);
        if deletable {
            self.report.deleted.push(key.clone());
        } else {
            self.report.detached.push(key.clone());
        }
        self.drop_keys.insert(key.clone());
        self.resolve(key);
    }

    // Rule 6.
    fn apply_variables(&mut self, plan: &SyncPlan) -> ImportResult<()> {
        let mut chosen: HashSet<&str> = HashSet::new();
        for approval in &self.inputs.selection.variables {
            let Some(change) = plan.variable_changes.iter().find(|c| c.name == approval.name)
            else {
                continue;
            };
            if variable_digest(change) == approval.digest {
                chosen.insert(approval.name.as_str());
            } else {
                // The spec or the collection changed since the review.
                self.mark_stale(&approval.name);
            }
        }
        let mut settings = self.inputs.settings.clone();
        let mut changed = false;
        for change in plan
            .variable_changes
            .iter()
            .filter(|c| chosen.contains(c.name.as_str()))
        {
            match change.kind {
                VariableKind::BaseUrl => {
                    self.base_url_selected = change.spec.clone();
                    let untouched = change.baseline.is_some() && change.baseline == change.yours;
                    let var = settings.variables.iter_mut().find(|v| v.key == change.name);
                    if let (true, Some(var), Some(spec)) = (untouched, var, &change.spec) {
                        var.value = spec.clone();
                        var.initial_value = spec.clone();
                        changed = true;
                    }
                }
                VariableKind::AddSecurity => {
                    if !settings.variables.iter().any(|v| v.key == change.name) {
                        settings.variables.push(CollectionVariable {
                            key: change.name.clone(),
                            value: String::new(),
                            initial_value: String::new(),
                            enabled: true,
                            secret: true,
                        });
                        changed = true;
                    }
                }
            }
        }
        if changed {
            let (rel, bytes) = self.inputs.renderer.render_settings(&settings)?;
            if !self.stage(rel.clone(), FileChangeKind::Put(bytes)) {
                return Err(unsafe_path(&rel));
            }
        }
        Ok(())
    }

    // Rule 7.
    fn apply_skips(&mut self, plan: &SyncPlan) {
        let by_key: HashMap<&str, &SyncItem> = plan.items.iter().map(|i| (i.key(), i)).collect();
        for skip in &self.inputs.selection.skips {
            let matches = by_key
                .get(skip.key.as_str())
                .is_some_and(|i| i.fingerprint() == skip.fingerprint);
            if matches && !self.resolved.contains(&skip.key) && !self.stale.contains(&skip.key) {
                self.local
                    .skips
                    .insert(skip.key.clone(), skip.fingerprint.clone());
                self.resolved.insert(skip.key.clone());
                self.report.skipped += 1;
            }
        }
    }

    // Rule 8.
    fn finish(mut self, plan: &SyncPlan) -> ApplyOutput {
        self.report.remaining = plan
            .items
            .iter()
            .filter(|i| !self.resolved.contains(i.key()) && !self.stale.contains(i.key()))
            .count();
        let finalized = self.report.remaining == 0 && self.report.stale.is_empty();

        let drop = &self.drop_keys;
        self.source.operations.retain(|o| !drop.contains(&o.key));
        self.source.operations.append(&mut self.new_src);
        self.baseline.operations.retain(|o| !drop.contains(&o.key));
        self.baseline.operations.append(&mut self.new_base);
        if let (true, Some(url)) = (self.had_baseline, self.base_url_selected.take()) {
            self.baseline.base_url = url;
        }
        if finalized {
            self.source.spec_sha256 = self.inputs.pending_sha256.clone();
            self.source.pending_sha256 = None;
            self.source.last_status = Some(SyncStatus::Unchanged);
        }
        ApplyOutput {
            files: self.files,
            source: self.source,
            baseline: self.baseline,
            local: self.local,
            finalized,
            report: self.report,
        }
    }
}

fn index_of<'k>(keys: impl Iterator<Item = &'k str>) -> HashMap<String, usize> {
    keys.enumerate().map(|(i, k)| (k.to_string(), i)).collect()
}

#[cfg(test)]
mod tests;
