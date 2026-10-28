//! Three-way merge of one request, field by field.
//!
//! `b` is the baseline (the request as last synced), `c` is the user's current request and `s` is
//! the incoming request from the new spec. The user's value wins whenever both sides changed.

use super::json_merge::{merge_json, numbers_are_canonical, took_spec};
use rocket_collection::Request;
use rocket_shared::description::Description;
use rocket_shared::types::{
    Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, HttpMethod, PathParam,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// What a merge did with one field the spec changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldOutcome {
    /// The spec's value was taken.
    TakeSpec,
    /// The user's value was kept.
    KeepYours,
    /// The spec removed something the user changed. The user's value is kept until they choose.
    Conflict,
}

/// One field where the spec's value differs from the baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldChange {
    /// `name`, `method`, `url`, `description`, `header:<name>`, `query:<name>`, `path:<name>`,
    /// `form:<key>`, `body`, `body:$.a.b` or `auth`.
    pub field: String,
    pub outcome: FieldOutcome,
    pub baseline: Option<String>,
    pub yours: Option<String>,
    pub spec: Option<String>,
}

/// The user's decision for one field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Spec,
    Yours,
}

/// The merged request and what the spec changed.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeOutcome {
    pub merged: Request,
    /// Only fields where the spec's value differs from the baseline.
    pub changes: Vec<FieldChange>,
    /// Set by the plan builder when the baseline cannot be trusted. Always false here.
    pub cautious: bool,
}

/// Longest text kept in a `FieldChange` value.
const MAX_SHOWN: usize = 500;

/// Cut long text so a change record stays small.
pub(crate) fn clip(text: &str) -> String {
    if text.chars().count() <= MAX_SHOWN {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(MAX_SHOWN).collect();
    cut.push_str("...");
    cut
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pick {
    Yours,
    Spec,
}

/// Rule 1 and rule 6 for one value. `removal` is true when the spec removed the value, which is
/// the only case that can be a conflict.
pub(crate) fn decide<T: PartialEq>(
    b: &T,
    c: &T,
    s: &T,
    removal: bool,
    choice: Option<&Choice>,
) -> (Pick, FieldOutcome) {
    let (mut pick, mut outcome) = if c == b {
        (Pick::Spec, FieldOutcome::TakeSpec)
    } else if c == s {
        (Pick::Yours, FieldOutcome::KeepYours)
    } else if removal {
        (Pick::Yours, FieldOutcome::Conflict)
    } else {
        (Pick::Yours, FieldOutcome::KeepYours)
    };
    match choice {
        Some(Choice::Spec) => {
            pick = Pick::Spec;
            outcome = FieldOutcome::TakeSpec;
        }
        Some(Choice::Yours) => {
            pick = Pick::Yours;
            outcome = FieldOutcome::KeepYours;
        }
        None => {}
    }
    (pick, outcome)
}

/// Merge the request. Fields outside the synced set are copied from `current`.
pub fn merge_request(
    baseline: &Request,
    current: &Request,
    incoming: &Request,
    choices: &BTreeMap<String, Choice>,
) -> MergeOutcome {
    let mut merged = current.clone();
    let mut changes = Vec::new();

    merged.name = merge_scalar(
        "name",
        &baseline.name,
        &current.name,
        &incoming.name,
        choices,
        &mut changes,
        |v| Some(v.clone()),
    );
    merged.method = merge_scalar(
        "method",
        &baseline.method,
        &current.method,
        &incoming.method,
        choices,
        &mut changes,
        |v: &HttpMethod| Some(v.to_string()),
    );
    merged.url = merge_scalar(
        "url",
        &baseline.url,
        &current.url,
        &incoming.url,
        choices,
        &mut changes,
        |v| Some(v.clone()),
    );
    merged.description = merge_scalar(
        "description",
        &baseline.description,
        &current.description,
        &incoming.description,
        choices,
        &mut changes,
        |v: &Option<Description>| v.as_ref().and_then(|d| d.content()).map(clip),
    );

    let (headers, mut ch) = merge_entries(
        "header",
        true,
        &baseline.headers,
        &current.headers,
        &incoming.headers,
        choices,
    );
    merged.headers = headers;
    changes.append(&mut ch);
    let (query, mut ch) = merge_entries(
        "query",
        false,
        &baseline.query_params,
        &current.query_params,
        &incoming.query_params,
        choices,
    );
    merged.query_params = query;
    changes.append(&mut ch);
    let (paths, mut ch) = merge_entries(
        "path",
        false,
        &baseline.path_params,
        &current.path_params,
        &incoming.path_params,
        choices,
    );
    merged.path_params = paths;
    changes.append(&mut ch);

    merged.body = merge_body(
        &baseline.body,
        &current.body,
        &incoming.body,
        choices,
        &mut changes,
    );
    merged.auth = merge_auth(
        &baseline.auth,
        &current.auth,
        &incoming.auth,
        choices,
        &mut changes,
    );

    MergeOutcome {
        merged,
        changes,
        cautious: false,
    }
}

/// True when any field differs, ignoring `uid`, `file_name` and `seq`.
pub fn request_is_edited(baseline: &Request, current: &Request) -> bool {
    let mut same_identity = current.clone();
    same_identity.uid = baseline.uid.clone();
    same_identity.file_name = baseline.file_name.clone();
    same_identity.seq = baseline.seq;
    same_identity != *baseline
}

// Rule 1: a scalar field.
fn merge_scalar<T: PartialEq + Clone>(
    field: &str,
    b: &T,
    c: &T,
    s: &T,
    choices: &BTreeMap<String, Choice>,
    changes: &mut Vec<FieldChange>,
    show: impl Fn(&T) -> Option<String>,
) -> T {
    if s == b {
        return c.clone();
    }
    let (pick, outcome) = decide(b, c, s, false, choices.get(field));
    changes.push(FieldChange {
        field: field.to_string(),
        outcome,
        baseline: show(b),
        yours: show(c),
        spec: show(s),
    });
    match pick {
        Pick::Spec => s.clone(),
        Pick::Yours => c.clone(),
    }
}

/// A list entry that has a name, a value and maybe an enabled flag.
pub(crate) trait Entry: Clone + PartialEq {
    fn name(&self) -> &str;
    fn value(&self) -> &str;
    fn set_value(&mut self, value: &str);
    fn is_enabled(&self) -> bool;
    fn set_enabled(&mut self, enabled: bool);

    /// Further attributes merged as their own sub-fields, as (name, text) pairs.
    fn extras(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
    fn set_extra(&mut self, _name: &str, _value: &str) {}

    fn show(&self) -> String {
        let mut text = clip(self.value());
        for (name, value) in self.extras() {
            if !value.is_empty() && value != "text" {
                text.push_str(&format!(" [{name}: {value}]"));
            }
        }
        if !self.is_enabled() {
            text.push_str(" (disabled)");
        }
        text
    }
}

impl Entry for Header {
    fn name(&self) -> &str {
        &self.key
    }
    fn value(&self) -> &str {
        &self.value
    }
    fn set_value(&mut self, value: &str) {
        self.value = value.to_string();
    }
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

impl Entry for FormDataEntry {
    fn name(&self) -> &str {
        &self.key
    }
    fn value(&self) -> &str {
        &self.value
    }
    fn set_value(&mut self, value: &str) {
        self.value = value.to_string();
    }
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    fn extras(&self) -> Vec<(&'static str, String)> {
        let kind = match self.entry_type {
            FormDataType::Text => "text",
            FormDataType::File => "file",
        };
        vec![
            ("type", kind.to_string()),
            (
                "content type",
                self.content_type.clone().unwrap_or_default(),
            ),
        ]
    }
    fn set_extra(&mut self, name: &str, value: &str) {
        match name {
            "type" => {
                self.entry_type = if value == "file" {
                    FormDataType::File
                } else {
                    FormDataType::Text
                };
            }
            "content type" => {
                self.content_type = if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                };
            }
            _ => {}
        }
    }
}

impl Entry for PathParam {
    fn name(&self) -> &str {
        &self.name
    }
    fn value(&self) -> &str {
        &self.value
    }
    fn set_value(&mut self, value: &str) {
        self.value = value.to_string();
    }
    fn is_enabled(&self) -> bool {
        true
    }
    fn set_enabled(&mut self, _enabled: bool) {}
}

fn entry_key(name: &str, case_insensitive: bool) -> String {
    if case_insensitive {
        name.to_lowercase()
    } else {
        name.to_string()
    }
}

/// Index of the first entry for each name. A repeated name is left alone.
fn index_entries<T: Entry>(list: &[T], case_insensitive: bool) -> HashMap<String, usize> {
    let mut map = HashMap::with_capacity(list.len());
    for (i, e) in list.iter().enumerate() {
        map.entry(entry_key(e.name(), case_insensitive))
            .or_insert(i);
    }
    map
}

fn show_entry<T: Entry>(entry: Option<&T>) -> Option<String> {
    entry.map(|e| e.show())
}

// Rules 2 and 4: entries matched by name.
pub(crate) fn merge_entries<T: Entry>(
    prefix: &str,
    case_insensitive: bool,
    b: &[T],
    c: &[T],
    s: &[T],
    choices: &BTreeMap<String, Choice>,
) -> (Vec<T>, Vec<FieldChange>) {
    let bi = index_entries(b, case_insensitive);
    let ci = index_entries(c, case_insensitive);
    let si = index_entries(s, case_insensitive);

    // Keys in the spec's order, then those only the baseline has. User-only keys are skipped.
    let mut keys: Vec<String> = Vec::new();
    for (i, e) in s.iter().enumerate() {
        let key = entry_key(e.name(), case_insensitive);
        if si.get(&key) == Some(&i) {
            keys.push(key);
        }
    }
    for (i, e) in b.iter().enumerate() {
        let key = entry_key(e.name(), case_insensitive);
        if bi.get(&key) == Some(&i) && !si.contains_key(&key) {
            keys.push(key);
        }
    }

    let mut out: Vec<Option<T>> = c.iter().cloned().map(Some).collect();
    let mut added: Vec<T> = Vec::new();
    let mut changes = Vec::new();

    for key in keys {
        let bv = bi.get(&key).map(|&i| &b[i]);
        let cidx = ci.get(&key).copied();
        let cv = cidx.map(|i| &c[i]);
        let sv = si.get(&key).map(|&i| &s[i]);
        let display = sv.or(bv).map(|e| e.name()).unwrap_or_default();
        let field = format!("{prefix}:{display}");
        let choice = choices.get(&field);

        let mut outcome = None;
        match (bv, cv, sv) {
            (_, _, None) => {
                // The spec removed the entry.
                if let (Some(bv), Some(cv), Some(idx)) = (bv, cv, cidx) {
                    let (pick, o) = decide(&Some(bv), &Some(cv), &None, true, choice);
                    if pick == Pick::Spec {
                        out[idx] = None;
                    }
                    outcome = Some(o);
                }
            }
            (None, None, Some(sv)) => {
                // A new entry in the spec.
                let (pick, o) = decide(&None, &None, &Some(sv), false, choice);
                if pick == Pick::Spec {
                    added.push(sv.clone());
                }
                outcome = Some(o);
            }
            (None, Some(cv), Some(sv)) => {
                // The user already has an entry with the spec's new name.
                if cv != sv {
                    let (pick, o) = decide(&None, &Some(cv), &Some(sv), false, choice);
                    if let (Pick::Spec, Some(idx)) = (pick, cidx) {
                        apply_attrs(&mut out[idx], sv);
                    }
                    outcome = Some(o);
                }
            }
            (Some(bv), None, Some(sv)) => {
                // The user deleted the entry, so it stays deleted.
                if attrs_differ(bv, sv) {
                    let (pick, o) = decide(&Some(bv), &None, &Some(sv), false, choice);
                    if pick == Pick::Spec {
                        added.push(sv.clone());
                    }
                    outcome = Some(o);
                }
            }
            (Some(bv), Some(cv), Some(sv)) => {
                if attrs_differ(bv, sv) {
                    outcome = Some(merge_entry_attrs(
                        bv,
                        cv,
                        sv,
                        choice,
                        cidx.and_then(|i| out.get_mut(i)).and_then(|o| o.as_mut()),
                    ));
                }
            }
        }
        if let Some(outcome) = outcome {
            changes.push(FieldChange {
                field,
                outcome,
                baseline: show_entry(bv),
                yours: show_entry(cv),
                spec: show_entry(sv),
            });
        }
    }

    let merged = out.into_iter().flatten().chain(added).collect();
    (merged, changes)
}

fn attrs_differ<T: Entry>(a: &T, b: &T) -> bool {
    a.value() != b.value() || a.is_enabled() != b.is_enabled() || a.extras() != b.extras()
}

fn apply_attrs<T: Entry>(target: &mut Option<T>, from: &T) {
    if let Some(t) = target.as_mut() {
        t.set_value(from.value());
        t.set_enabled(from.is_enabled());
        for (name, value) in from.extras() {
            t.set_extra(name, &value);
        }
    }
}

/// Merge `value`, `enabled` and the extras of one entry present on all three sides, each by rule 1.
fn merge_entry_attrs<T: Entry>(
    b: &T,
    c: &T,
    s: &T,
    choice: Option<&Choice>,
    target: Option<&mut T>,
) -> FieldOutcome {
    let Some(target) = target else {
        return FieldOutcome::KeepYours;
    };
    let mut taken = false;
    if s.value() != b.value() {
        let (pick, o) = decide(&b.value(), &c.value(), &s.value(), false, choice);
        if pick == Pick::Spec {
            target.set_value(s.value());
        }
        taken |= o == FieldOutcome::TakeSpec;
    }
    if s.is_enabled() != b.is_enabled() {
        let (pick, o) = decide(
            &b.is_enabled(),
            &c.is_enabled(),
            &s.is_enabled(),
            false,
            choice,
        );
        if pick == Pick::Spec {
            target.set_enabled(s.is_enabled());
        }
        taken |= o == FieldOutcome::TakeSpec;
    }
    let (bx, cx, sx) = (b.extras(), c.extras(), s.extras());
    for (i, (name, sv)) in sx.iter().enumerate() {
        let (Some((_, bv)), Some((_, cv))) = (bx.get(i), cx.get(i)) else {
            continue;
        };
        if sv == bv {
            continue;
        }
        let (pick, o) = decide(bv, cv, sv, false, choice);
        if pick == Pick::Spec {
            target.set_extra(name, sv);
        }
        taken |= o == FieldOutcome::TakeSpec;
    }
    if taken {
        FieldOutcome::TakeSpec
    } else {
        FieldOutcome::KeepYours
    }
}

fn show_body(body: &Option<Body>) -> Option<String> {
    let body = body.as_ref()?;
    if let Some(content) = &body.content {
        return Some(clip(content));
    }
    if let Some(entries) = &body.form_data {
        return Some(format!("{:?} with {} entries", body.mode, entries.len()));
    }
    Some(format!("{:?}", body.mode))
}

/// Parse a JSON body into an object, or `None` when it is not one.
fn json_object(body: &Body) -> Option<serde_json::Map<String, Value>> {
    if body.mode != BodyMode::Json {
        return None;
    }
    match serde_json::from_str::<Value>(body.content.as_deref()?) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// One `name=value` piece of an urlencoded body. `raw` is the piece as written, so rebuilding
/// the body keeps every escape the user or the spec wrote.
#[derive(Clone, PartialEq)]
struct UrlPair {
    /// The name with its escapes decoded, for matching.
    name: String,
    raw: String,
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                if let Some(value) = hex {
                    out.push(value);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl UrlPair {
    fn parse(piece: &str) -> Self {
        let raw_name = piece.split_once('=').map_or(piece, |(name, _)| name);
        Self {
            name: percent_decode(raw_name),
            raw: piece.to_string(),
        }
    }
}

impl Entry for UrlPair {
    fn name(&self) -> &str {
        &self.name
    }
    fn value(&self) -> &str {
        self.raw.split_once('=').map_or("", |(_, value)| value)
    }
    fn set_value(&mut self, value: &str) {
        let raw_name = self
            .raw
            .split_once('=')
            .map_or(self.raw.as_str(), |(n, _)| n);
        self.raw = format!("{raw_name}={value}");
    }
    fn is_enabled(&self) -> bool {
        true
    }
    fn set_enabled(&mut self, _enabled: bool) {}
}

/// Splits an urlencoded body into its pieces. An empty piece (a doubled or trailing `&`) is not
/// a field list a rebuild could reproduce, so it gives `None` and the body merges as one value.
fn url_pairs(body: &Body) -> Option<Vec<UrlPair>> {
    if body.mode != BodyMode::FormUrlEncoded {
        return None;
    }
    let text = body.content.as_deref()?;
    if text.is_empty() {
        return Some(Vec::new());
    }
    let pieces: Vec<&str> = text.split('&').collect();
    if pieces.iter().any(|p| p.is_empty()) {
        return None;
    }
    Some(pieces.into_iter().map(UrlPair::parse).collect())
}

// Rules 3 and 4: the body.
fn merge_body(
    b: &Option<Body>,
    c: &Option<Body>,
    s: &Option<Body>,
    choices: &BTreeMap<String, Choice>,
    changes: &mut Vec<FieldChange>,
) -> Option<Body> {
    if s == b {
        return c.clone();
    }
    if let (Some(bb), Some(cb), Some(sb)) = (b, c, s) {
        // Rebuilding the text must not change the user's numbers, so odd ones skip the key merge.
        let safe = cb.content.as_deref().is_some_and(numbers_are_canonical);
        if let (true, Some(bo), Some(co), Some(so)) =
            (safe, json_object(bb), json_object(cb), json_object(sb))
        {
            let result = merge_json(&bo, &co, &so, choices);
            let mut merged = cb.clone();
            if took_spec(&result.changes) {
                if let Ok(text) = serde_json::to_string_pretty(&result.merged) {
                    merged.content = Some(text);
                } else {
                    // Serializing a parsed value should not fail. Keep the user's text if it does.
                    return c.clone();
                }
            }
            changes.extend(result.changes);
            return Some(merged);
        }
        if let (Some(bp), Some(cp), Some(sp)) = (url_pairs(bb), url_pairs(cb), url_pairs(sb)) {
            let (pairs, mut ch) = merge_entries("form", false, &bp, &cp, &sp, choices);
            changes.append(&mut ch);
            let mut merged = cb.clone();
            merged.content = Some(
                pairs
                    .iter()
                    .map(|p| p.raw.as_str())
                    .collect::<Vec<_>>()
                    .join("&"),
            );
            return Some(merged);
        }
        if let (BodyMode::FormData, BodyMode::FormData, BodyMode::FormData) =
            (&bb.mode, &cb.mode, &sb.mode)
        {
            if let (Some(bf), Some(cf), Some(sf)) = (&bb.form_data, &cb.form_data, &sb.form_data) {
                let (entries, mut ch) = merge_entries("form", false, bf, cf, sf, choices);
                changes.append(&mut ch);
                let mut merged = cb.clone();
                merged.form_data = Some(entries);
                return Some(merged);
            }
        }
    }
    merge_scalar("body", b, c, s, choices, changes, show_body)
}

fn auth_name(auth: &Auth) -> &'static str {
    match auth {
        Auth::None => "None",
        Auth::Basic { .. } => "Basic",
        Auth::Bearer { .. } => "Bearer",
        Auth::ApiKey { .. } => "API key",
        Auth::OAuth2(_) => "OAuth 2.0",
        Auth::AwsSigV4 { .. } => "AWS Signature v4",
        Auth::Inherit => "Inherit",
        Auth::Wsse { .. } => "WSSE",
        Auth::Digest { .. } => "Digest",
        Auth::Ntlm { .. } => "NTLM",
        Auth::OAuth1(_) => "OAuth 1.0",
    }
}

// Rule 5: auth, as refined in fix round 1. Only the type name is shown, so no secret reaches a change record.
fn merge_auth(
    b: &Auth,
    c: &Auth,
    s: &Auth,
    choices: &BTreeMap<String, Choice>,
    changes: &mut Vec<FieldChange>,
) -> Auth {
    if s == b {
        return c.clone();
    }
    let (bt, ct, st) = (
        std::mem::discriminant(b),
        std::mem::discriminant(c),
        std::mem::discriminant(s),
    );
    // The user's auth is replaced by default only when the user did not touch the type.
    let (mut pick, mut outcome) = if ct != bt {
        if st == bt || st == ct {
            (Pick::Yours, FieldOutcome::KeepYours)
        } else {
            (Pick::Yours, FieldOutcome::Conflict)
        }
    } else if st != bt || c == b {
        (Pick::Spec, FieldOutcome::TakeSpec)
    } else {
        (Pick::Yours, FieldOutcome::KeepYours)
    };
    match choices.get("auth") {
        Some(Choice::Spec) => {
            pick = Pick::Spec;
            outcome = FieldOutcome::TakeSpec;
        }
        Some(Choice::Yours) => {
            pick = Pick::Yours;
            outcome = FieldOutcome::KeepYours;
        }
        None => {}
    }
    if c != s || pick == Pick::Spec {
        changes.push(FieldChange {
            field: "auth".to_string(),
            outcome,
            baseline: Some(auth_name(b).to_string()),
            yours: Some(auth_name(c).to_string()),
            spec: Some(auth_name(s).to_string()),
        });
    }
    match pick {
        Pick::Spec => s.clone(),
        Pick::Yours => c.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::Request;
    use rocket_shared::types::{
        Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, HttpMethod, PathParam,
    };
    use std::collections::BTreeMap;

    fn base() -> Request {
        Request::new("List", HttpMethod::Get, "{{baseUrl}}/pets")
    }

    fn run(b: &Request, c: &Request, s: &Request) -> MergeOutcome {
        merge_request(b, c, s, &BTreeMap::new())
    }

    fn json_body(text: &str) -> Option<Body> {
        Some(Body {
            mode: BodyMode::Json,
            content: Some(text.to_string()),
            form_data: None,
            file_path: None,
        })
    }

    fn parsed(out: MergeOutcome) -> serde_json::Value {
        let body = out.merged.body.expect("body");
        serde_json::from_str(body.content.as_deref().expect("content")).expect("json")
    }

    fn change<'a>(out: &'a MergeOutcome, field: &str) -> &'a FieldChange {
        out.changes
            .iter()
            .find(|c| c.field == field)
            .unwrap_or_else(|| panic!("no change for {field}: {:?}", out.changes))
    }

    fn has_change(out: &MergeOutcome, field: &str) -> bool {
        out.changes.iter().any(|c| c.field == field)
    }

    fn q(key: &str, value: &str, enabled: bool) -> Header {
        let mut h = Header::new(key, value);
        h.enabled = enabled;
        h
    }

    fn choice(field: &str, c: Choice) -> BTreeMap<String, Choice> {
        let mut m = BTreeMap::new();
        m.insert(field.to_string(), c);
        m
    }

    // Scalars.

    #[test]
    fn spec_change_is_taken_when_the_user_did_not_touch_it() {
        let b = base();
        let c = base();
        let mut s = base();
        s.url = "{{baseUrl}}/v2/pets".into();
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.url, "{{baseUrl}}/v2/pets");
        assert_eq!(change(&out, "url").outcome, FieldOutcome::TakeSpec);
        assert!(!out.cautious);
    }

    #[test]
    fn user_value_wins_when_both_changed() {
        let b = base();
        let mut c = base();
        c.url = "mine".into();
        let mut s = base();
        s.url = "theirs".into();
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.url, "mine");
        let ch = change(&out, "url");
        assert_eq!(ch.outcome, FieldOutcome::KeepYours);
        assert_eq!(ch.baseline.as_deref(), Some("{{baseUrl}}/pets"));
        assert_eq!(ch.yours.as_deref(), Some("mine"));
        assert_eq!(ch.spec.as_deref(), Some("theirs"));
    }

    #[test]
    fn same_change_on_both_sides_is_no_conflict() {
        let b = base();
        let mut c = base();
        c.name = "Same".into();
        let mut s = base();
        s.name = "Same".into();
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.name, "Same");
        assert_ne!(change(&out, "name").outcome, FieldOutcome::Conflict);
    }

    #[test]
    fn unchanged_spec_lists_nothing() {
        let b = base();
        let mut c = base();
        c.url = "edited".into();
        c.name = "edited".into();
        let out = run(&b, &c, &b.clone());
        assert!(out.changes.is_empty());
        assert_eq!(out.merged.url, "edited");
        assert_eq!(out.merged.name, "edited");
    }

    #[test]
    fn method_and_description_merge_as_scalars() {
        let b = base();
        let c = base();
        let mut s = base();
        s.method = HttpMethod::Post;
        s.description = Some(rocket_shared::description::Description::text("new"));
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.method, HttpMethod::Post);
        assert_eq!(out.merged.description, s.description);
        assert_eq!(change(&out, "method").spec.as_deref(), Some("POST"));
        assert_eq!(change(&out, "description").spec.as_deref(), Some("new"));
    }

    // Entries.

    #[test]
    fn new_spec_header_is_added() {
        let b = base();
        let c = base();
        let mut s = base();
        s.headers.push(Header::new("X-New", "1"));
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.headers, vec![Header::new("X-New", "1")]);
        assert_eq!(change(&out, "header:X-New").outcome, FieldOutcome::TakeSpec);
    }

    #[test]
    fn user_added_header_survives() {
        let b = base();
        let mut c = base();
        c.headers.push(Header::new("X-Mine", "m"));
        let mut s = base();
        s.headers.push(Header::new("X-New", "1"));
        let out = run(&b, &c, &s);
        assert!(out.merged.headers.contains(&Header::new("X-Mine", "m")));
        assert!(out.merged.headers.contains(&Header::new("X-New", "1")));
    }

    #[test]
    fn user_disabled_param_stays_disabled_when_the_spec_changes_its_value() {
        let mut b = base();
        b.query_params.push(q("limit", "10", true));
        let mut c = base();
        c.query_params.push(q("limit", "10", false));
        let mut s = base();
        s.query_params.push(q("limit", "50", true));
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.query_params, vec![q("limit", "50", false)]);
        assert_eq!(change(&out, "query:limit").outcome, FieldOutcome::TakeSpec);
    }

    #[test]
    fn spec_removes_an_untouched_param_so_it_is_removed() {
        let mut b = base();
        b.query_params.push(q("old", "1", true));
        let c = b.clone();
        let s = base();
        let out = run(&b, &c, &s);
        assert!(out.merged.query_params.is_empty());
        assert_eq!(change(&out, "query:old").outcome, FieldOutcome::TakeSpec);
    }

    #[test]
    fn spec_removes_a_param_the_user_changed_is_a_conflict_and_kept_by_default() {
        let mut b = base();
        b.query_params.push(q("old", "1", true));
        let mut c = base();
        c.query_params.push(q("old", "2", true));
        let s = base();
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.query_params, vec![q("old", "2", true)]);
        assert_eq!(change(&out, "query:old").outcome, FieldOutcome::Conflict);
    }

    #[test]
    fn choice_spec_resolves_the_conflict_by_removing_it() {
        let mut b = base();
        b.query_params.push(q("old", "1", true));
        let mut c = base();
        c.query_params.push(q("old", "2", true));
        let s = base();
        let out = merge_request(&b, &c, &s, &choice("query:old", Choice::Spec));
        assert!(out.merged.query_params.is_empty());
    }

    #[test]
    fn header_names_match_case_insensitively() {
        let mut b = base();
        b.headers.push(Header::new("Accept", "a"));
        let c = b.clone();
        let mut s = base();
        s.headers.push(Header::new("accept", "b"));
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.headers.len(), 1);
        assert_eq!(out.merged.headers[0].value, "b");
        assert_eq!(out.merged.headers[0].key, "Accept");
    }

    #[test]
    fn path_param_value_change_by_the_spec_is_taken_if_untouched() {
        let pp = |v: &str| PathParam {
            name: "id".into(),
            value: v.into(),
            description: None,
        };
        let mut b = base();
        b.path_params.push(pp("1"));
        let c = b.clone();
        let mut s = base();
        s.path_params.push(pp("2"));
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.path_params[0].value, "2");
        assert_eq!(change(&out, "path:id").outcome, FieldOutcome::TakeSpec);
    }

    #[test]
    fn param_the_user_deleted_stays_deleted() {
        let mut b = base();
        b.query_params.push(q("a", "1", true));
        let c = base();
        let mut s = base();
        s.query_params.push(q("a", "2", true));
        let out = run(&b, &c, &s);
        assert!(out.merged.query_params.is_empty());
        assert_eq!(change(&out, "query:a").outcome, FieldOutcome::KeepYours);
    }

    // Form data.

    fn form(key: &str, value: &str) -> FormDataEntry {
        FormDataEntry {
            key: key.into(),
            value: value.into(),
            entry_type: FormDataType::Text,
            enabled: true,
            content_type: None,
            description: None,
        }
    }

    fn form_body(entries: Vec<FormDataEntry>) -> Option<Body> {
        Some(Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(entries),
            file_path: None,
        })
    }

    #[test]
    fn form_entries_merge_by_key() {
        let mut b = base();
        b.body = form_body(vec![form("a", "1")]);
        let mut c = base();
        c.body = form_body(vec![form("a", "1"), form("mine", "m")]);
        let mut s = base();
        s.body = form_body(vec![form("a", "2"), form("b", "3")]);
        let out = run(&b, &c, &s);
        assert!(has_change(&out, "form:a"));
        assert!(has_change(&out, "form:b"));
        let got = out.merged.body.expect("body").form_data.expect("form");
        assert_eq!(got, vec![form("a", "2"), form("mine", "m"), form("b", "3")]);
    }

    fn file_form(key: &str, value: &str) -> FormDataEntry {
        let mut f = form(key, value);
        f.entry_type = FormDataType::File;
        f
    }

    #[test]
    fn form_type_change_by_the_spec_is_listed_and_taken_if_untouched() {
        let mut b = base();
        b.body = form_body(vec![form("avatar", "x")]);
        let c = b.clone();
        let mut s = base();
        s.body = form_body(vec![file_form("avatar", "x")]);
        let out = run(&b, &c, &s);
        assert_eq!(change(&out, "form:avatar").outcome, FieldOutcome::TakeSpec);
        let got = out.merged.body.expect("body").form_data.expect("form");
        assert_eq!(got, vec![file_form("avatar", "x")]);
    }

    #[test]
    fn form_type_the_user_changed_is_kept() {
        let mut b = base();
        b.body = form_body(vec![form("avatar", "x")]);
        let mut c = base();
        c.body = form_body(vec![file_form("avatar", "x")]);
        let mut s = base();
        let mut spec_entry = form("avatar", "x");
        spec_entry.content_type = Some("image/png".into());
        s.body = form_body(vec![spec_entry.clone()]);
        let out = run(&b, &c, &s);
        let got = out.merged.body.expect("body").form_data.expect("form");
        assert_eq!(got[0].entry_type, FormDataType::File);
        assert_eq!(got[0].content_type.as_deref(), Some("image/png"));
    }

    #[test]
    fn form_type_conflict_goes_to_the_user() {
        let mut b = base();
        b.body = form_body(vec![form("avatar", "x")]);
        let mut c = base();
        c.body = form_body(vec![file_form("avatar", "x")]);
        let mut s = base();
        s.body = form_body(vec![form("avatar", "y")]);
        let out = run(&b, &c, &s);
        let got = out.merged.body.expect("body").form_data.expect("form");
        assert_eq!(got[0].entry_type, FormDataType::File);
        assert_eq!(got[0].value, "y");
    }

    #[test]
    fn header_removed_by_the_spec_under_another_case_is_a_conflict_when_the_user_changed_it() {
        let mut b = base();
        b.headers.push(Header::new("Accept", "a"));
        let mut c = base();
        c.headers.push(Header::new("ACCEPT", "mine"));
        let s = base();
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.headers, vec![Header::new("ACCEPT", "mine")]);
        assert_eq!(
            change(&out, "header:Accept").outcome,
            FieldOutcome::Conflict
        );
    }

    #[test]
    fn enabled_only_user_change_against_a_spec_removal_is_a_conflict() {
        let mut b = base();
        b.query_params.push(q("limit", "10", true));
        let mut c = base();
        c.query_params.push(q("limit", "10", false));
        let s = base();
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.query_params, vec![q("limit", "10", false)]);
        assert_eq!(change(&out, "query:limit").outcome, FieldOutcome::Conflict);
    }

    // JSON numbers and key paths.

    fn merged_text(out: &MergeOutcome) -> String {
        out.merged
            .body
            .as_ref()
            .and_then(|b| b.content.clone())
            .expect("content")
    }

    #[test]
    fn numbers_the_user_wrote_are_never_rewritten() {
        let cases = [
            "123456789012345678901234567890",
            "1.50",
            "1e3",
            "1E+3",
            "0.10",
        ];
        for num in cases {
            let user = format!("{{\"n\": {num}, \"x\": 1}}");
            let mut b = base();
            b.body = json_body(r#"{"n":1,"x":1}"#);
            let mut c = base();
            c.body = json_body(&user);
            let mut s = base();
            s.body = json_body(r#"{"n":1,"x":2}"#);
            let out = run(&b, &c, &s);
            assert_eq!(merged_text(&out), user, "case {num}");
            let ch = change(&out, "body");
            assert_eq!(ch.outcome, FieldOutcome::KeepYours, "case {num}");
        }
    }

    #[test]
    fn unsafe_numbers_in_an_untouched_body_take_the_spec_whole() {
        let mut b = base();
        b.body = json_body(r#"{"n": 1.50, "x": 1}"#);
        let c = b.clone();
        let mut s = base();
        s.body = json_body(r#"{"n": 1.50, "x": 2}"#);
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.body, s.body);
        assert_eq!(change(&out, "body").outcome, FieldOutcome::TakeSpec);
    }

    #[test]
    fn ordinary_numbers_still_use_the_key_path_merge() {
        let mut b = base();
        b.body = json_body(r#"{"a":1,"b":1}"#);
        let mut c = base();
        c.body = json_body(r#"{"a":-5,"f":0.5,"g":1.0,"s":"1.50 \" 1e3","b":1}"#);
        let mut s = base();
        s.body = json_body(r#"{"a":1,"b":2}"#);
        let out = run(&b, &c, &s);
        assert_eq!(change(&out, "body:$.b").outcome, FieldOutcome::TakeSpec);
        assert_eq!(
            parsed(out),
            serde_json::json!({"a":-5,"f":0.5,"g":1.0,"s":"1.50 \" 1e3","b":2})
        );
    }

    #[test]
    fn json_key_paths_with_special_characters_are_quoted() {
        let mut b = base();
        b.body = json_body(r#"{"a.b":1,"a":{"b":1},"q\"x":1}"#);
        let c = b.clone();
        let mut s = base();
        s.body = json_body(r#"{"a.b":2,"a":{"b":3},"q\"x":4}"#);
        let out = run(&b, &c, &s);
        assert!(has_change(&out, r#"body:$["a.b"]"#));
        assert!(has_change(&out, "body:$.a.b"));
        assert!(has_change(&out, r#"body:$["q\"x"]"#));
        assert_eq!(
            parsed(out),
            serde_json::json!({"a.b":2,"a":{"b":3},"q\"x":4})
        );
    }

    // JSON.

    #[test]
    fn new_key_in_the_spec_is_added_to_the_users_body() {
        let mut b = base();
        b.body = json_body(r#"{"a":1}"#);
        let c = b.clone();
        let mut s = base();
        s.body = json_body(r#"{"a":1,"b":2}"#);
        let out = run(&b, &c, &s);
        assert_eq!(change(&out, "body:$.b").outcome, FieldOutcome::TakeSpec);
        assert_eq!(parsed(out), serde_json::json!({"a":1,"b":2}));
    }

    #[test]
    fn key_the_user_added_is_kept() {
        let mut b = base();
        b.body = json_body(r#"{"a":1}"#);
        let mut c = base();
        c.body = json_body(r#"{"a":1,"mine":true}"#);
        let mut s = base();
        s.body = json_body(r#"{"a":2}"#);
        let out = run(&b, &c, &s);
        assert_eq!(parsed(out), serde_json::json!({"a":2,"mine":true}));
    }

    #[test]
    fn user_edit_to_one_key_and_spec_edit_to_another_both_apply() {
        let mut b = base();
        b.body = json_body(r#"{"a":{"x":1},"b":1}"#);
        let mut c = base();
        c.body = json_body(r#"{"a":{"x":9},"b":1}"#);
        let mut s = base();
        s.body = json_body(r#"{"a":{"x":1},"b":2}"#);
        let out = run(&b, &c, &s);
        assert!(has_change(&out, "body:$.b"));
        assert!(!has_change(&out, "body:$.a.x"));
        assert_eq!(parsed(out), serde_json::json!({"a":{"x":9},"b":2}));
    }

    #[test]
    fn spec_removes_a_key_the_user_changed_is_a_conflict() {
        let mut b = base();
        b.body = json_body(r#"{"a":1,"b":1}"#);
        let mut c = base();
        c.body = json_body(r#"{"a":1,"b":7}"#);
        let mut s = base();
        s.body = json_body(r#"{"a":1}"#);
        let out = run(&b, &c, &s);
        assert_eq!(change(&out, "body:$.b").outcome, FieldOutcome::Conflict);
        assert_eq!(out.merged.body, c.body);
        let out = merge_request(&b, &c, &s, &choice("body:$.b", Choice::Spec));
        assert_eq!(parsed(out), serde_json::json!({"a":1}));
    }

    #[test]
    fn body_text_is_untouched_when_nothing_is_taken() {
        let mut b = base();
        b.body = json_body(r#"{"a":1,"b":1}"#);
        let mut c = base();
        c.body = json_body("{\n    \"b\": 5,\n  \"a\":1 }");
        let mut s = base();
        s.body = json_body(r#"{"a":1,"b":2}"#);
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.body, c.body);
        assert_eq!(change(&out, "body:$.b").outcome, FieldOutcome::KeepYours);
    }

    #[test]
    fn invalid_user_json_falls_back_to_whole_body_rule() {
        let mut b = base();
        b.body = json_body(r#"{"a":1}"#);
        let mut c = base();
        c.body = json_body("{ not json");
        let mut s = base();
        s.body = json_body(r#"{"a":2}"#);
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.body, c.body);
        assert_eq!(change(&out, "body").outcome, FieldOutcome::KeepYours);
    }

    #[test]
    fn non_json_body_uses_the_whole_body_rule() {
        let text = |t: &str| {
            Some(Body {
                mode: BodyMode::Text,
                content: Some(t.into()),
                form_data: None,
                file_path: None,
            })
        };
        let mut b = base();
        b.body = text("a");
        let c = b.clone();
        let mut s = base();
        s.body = text("b");
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.body, s.body);
        assert_eq!(change(&out, "body").outcome, FieldOutcome::TakeSpec);
    }

    fn nest(depth: usize, leaf: &str) -> String {
        let mut t = leaf.to_string();
        for _ in 0..depth {
            t = format!("{{\"k\":{t}}}");
        }
        t
    }

    #[test]
    fn nesting_deeper_than_64_levels_does_not_overflow() {
        // 200 levels: serde_json refuses to parse it, so the whole body is taken.
        let mut b = base();
        b.body = json_body(&nest(200, "1"));
        let c = b.clone();
        let mut s = base();
        s.body = json_body(&nest(200, "2"));
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.body, s.body);
        assert_eq!(change(&out, "body").outcome, FieldOutcome::TakeSpec);

        // 100 levels: parsed, and the levels past 64 are one leaf.
        let mut b = base();
        b.body = json_body(&nest(100, "1"));
        let c = b.clone();
        let mut s = base();
        s.body = json_body(&nest(100, "2"));
        let out = run(&b, &c, &s);
        assert_eq!(
            out.changes.len(),
            1,
            "one leaf change expected: {:?}",
            out.changes
        );
        assert_eq!(out.changes[0].outcome, FieldOutcome::TakeSpec);
        let expect: serde_json::Value = serde_json::from_str(&nest(100, "2")).expect("json");
        assert_eq!(parsed(out), expect);
    }

    #[test]
    fn a_large_array_is_a_leaf() {
        let arr = |last: u32| {
            let items: Vec<String> = (0..100_000u32)
                .map(|i| {
                    if i == 99_999 {
                        last.to_string()
                    } else {
                        i.to_string()
                    }
                })
                .collect();
            format!("{{\"a\":[{}]}}", items.join(","))
        };
        let mut b = base();
        b.body = json_body(&arr(1));
        let c = b.clone();
        let mut s = base();
        s.body = json_body(&arr(2));
        let start = std::time::Instant::now();
        let out = run(&b, &c, &s);
        assert!(start.elapsed().as_secs() < 5);
        assert_eq!(change(&out, "body:$.a").outcome, FieldOutcome::TakeSpec);
        let v = parsed(out);
        let items = v["a"].as_array().expect("array");
        assert_eq!(items.len(), 100_000);
        assert_eq!(items[99_999], serde_json::json!(2));
    }

    // Auth.

    #[test]
    fn same_type_keeps_the_users_values() {
        let bearer = |t: &str| Auth::Bearer { token: t.into() };
        let mut b = base();
        b.auth = bearer("{{t}}");
        let mut c = base();
        c.auth = bearer("real");
        let mut s = base();
        s.auth = bearer("{{other}}");
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.auth, c.auth);
        assert_eq!(change(&out, "auth").outcome, FieldOutcome::KeepYours);
    }

    #[test]
    fn user_switched_to_inherit_and_spec_changed_only_the_value_keeps_inherit() {
        let mut b = base();
        b.auth = Auth::Bearer {
            token: "{{a}}".into(),
        };
        let mut c = base();
        c.auth = Auth::Inherit;
        let mut s = base();
        s.auth = Auth::Bearer {
            token: "{{b}}".into(),
        };
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.auth, Auth::Inherit);
        assert_eq!(change(&out, "auth").outcome, FieldOutcome::KeepYours);
    }

    #[test]
    fn user_switched_type_and_spec_switched_to_a_third_type_is_a_conflict() {
        let mut b = base();
        b.auth = Auth::Bearer {
            token: "{{a}}".into(),
        };
        let mut c = base();
        c.auth = Auth::Basic {
            username: "u".into(),
            password: "secret-pw".into(),
        };
        let mut s = base();
        s.auth = Auth::ApiKey {
            key: "k".into(),
            value: "v".into(),
            placement: "header".into(),
        };
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.auth, c.auth);
        let ch = change(&out, "auth");
        assert_eq!(ch.outcome, FieldOutcome::Conflict);
        assert!(!ch.yours.as_deref().unwrap_or("").contains("secret-pw"));
        let out = merge_request(&b, &c, &s, &choice("auth", Choice::Spec));
        assert_eq!(out.merged.auth, s.auth);
    }

    #[test]
    fn user_kept_the_type_and_spec_changed_the_type_takes_the_spec() {
        let mut b = base();
        b.auth = Auth::Bearer {
            token: "{{a}}".into(),
        };
        let mut c = base();
        c.auth = Auth::Bearer {
            token: "mine".into(),
        };
        let mut s = base();
        s.auth = Auth::Inherit;
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.auth, Auth::Inherit);
        assert_eq!(change(&out, "auth").outcome, FieldOutcome::TakeSpec);
    }

    #[test]
    fn same_type_untouched_by_the_user_takes_the_specs_values() {
        let api = |k: &str| Auth::ApiKey {
            key: k.into(),
            value: "{{v}}".into(),
            placement: "header".into(),
        };
        let mut b = base();
        b.auth = api("X-Old");
        let c = b.clone();
        let mut s = base();
        s.auth = api("X-New");
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.auth, api("X-New"));
        assert_eq!(change(&out, "auth").outcome, FieldOutcome::TakeSpec);
    }

    #[test]
    fn auth_choice_yours_keeps_the_users_auth() {
        let mut b = base();
        b.auth = Auth::Bearer {
            token: "{{a}}".into(),
        };
        let mut c = base();
        c.auth = Auth::Bearer {
            token: "mine".into(),
        };
        let mut s = base();
        s.auth = Auth::Inherit;
        let out = merge_request(&b, &c, &s, &choice("auth", Choice::Yours));
        assert_eq!(out.merged.auth, c.auth);
    }

    #[test]
    fn different_type_takes_the_specs_auth() {
        let mut b = base();
        b.auth = Auth::Bearer {
            token: "{{t}}".into(),
        };
        let mut c = base();
        c.auth = Auth::Bearer {
            token: "real-secret".into(),
        };
        let mut s = base();
        s.auth = Auth::Basic {
            username: "u".into(),
            password: "p".into(),
        };
        let out = run(&b, &c, &s);
        assert_eq!(out.merged.auth, s.auth);
        let ch = change(&out, "auth");
        assert_eq!(ch.outcome, FieldOutcome::TakeSpec);
        assert!(!ch.yours.as_deref().unwrap_or("").contains("real-secret"));
    }

    #[test]
    fn spec_unchanged_keeps_the_users_auth() {
        let mut b = base();
        b.auth = Auth::Bearer {
            token: "{{t}}".into(),
        };
        let mut c = base();
        c.auth = Auth::Basic {
            username: "u".into(),
            password: "p".into(),
        };
        let out = run(&b, &c, &b.clone());
        assert_eq!(out.merged.auth, c.auth);
        assert!(!has_change(&out, "auth"));
    }

    // Never touched.

    fn example() -> rocket_shared::action::HttpRequestExample {
        rocket_shared::action::HttpRequestExample {
            name: "ok".into(),
            description: None,
            request: None,
            response: None,
        }
    }

    #[test]
    fn scripts_tests_docs_examples_settings_tags_and_variables_are_copied_from_current() {
        let b = base();
        let mut c = base();
        c.pre_request_script = Some("pre".into());
        c.post_response_script = Some("post".into());
        c.tests = Some("tests".into());
        c.tags = vec!["t".into()];
        c.docs = Some(rocket_shared::description::Description::text("docs"));
        c.runtime_auth = Some(Auth::Inherit);
        c.settings = Some(rocket_shared::types::RequestSettings {
            encode_url: Some(rocket_shared::types::RequestSettingValue::Value(true)),
            timeout: None,
            follow_redirects: None,
            max_redirects: None,
            verify_ssl: None,
        });
        c.variables = vec![rocket_collection::settings::CollectionVariable {
            key: "k".into(),
            value: "v".into(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        }];
        c.examples = vec![example()];
        c.assertions = vec![rocket_shared::assertion::Assertion::new(
            "res.status",
            "eq",
            Some("200".into()),
        )];
        c.uid = "uid-c".into();
        c.file_name = Some("mine.yml".into());
        c.seq = Some(9);
        let mut s = base();
        s.url = "changed".into();
        s.pre_request_script = Some("spec".into());
        s.tags = vec!["spec".into()];
        let out = run(&b, &c, &s);
        let mut expect = c.clone();
        expect.url = "changed".into();
        assert_eq!(out.merged, expect);
    }

    // Edited.

    #[test]
    fn request_is_edited_ignores_uid_file_name_and_seq() {
        let b = base();
        let mut c = b.clone();
        c.uid = "other".into();
        c.file_name = Some("x.yml".into());
        c.seq = Some(4);
        assert!(!request_is_edited(&b, &c));
    }

    #[test]
    fn request_is_edited_sees_a_script() {
        let b = base();
        let mut c = b.clone();
        c.pre_request_script = Some("x".into());
        assert!(request_is_edited(&b, &c));
    }

    #[test]
    fn request_is_edited_sees_an_example() {
        let b = base();
        let mut c = b.clone();
        c.examples.push(example());
        assert!(request_is_edited(&b, &c));
    }

    #[test]
    fn identical_requests_are_not_edited() {
        let b = base();
        assert!(!request_is_edited(&b, &b.clone()));
    }

    // Choices.

    #[test]
    fn unknown_choice_field_is_ignored() {
        let b = base();
        let mut c = base();
        c.url = "mine".into();
        let mut s = base();
        s.url = "theirs".into();
        let out = merge_request(&b, &c, &s, &choice("nope", Choice::Spec));
        assert_eq!(out.merged.url, "mine");
    }

    #[test]
    fn choice_spec_on_a_keep_yours_field_takes_the_specs_value() {
        let b = base();
        let mut c = base();
        c.url = "mine".into();
        let mut s = base();
        s.url = "theirs".into();
        let out = merge_request(&b, &c, &s, &choice("url", Choice::Spec));
        assert_eq!(out.merged.url, "theirs");
        assert_eq!(change(&out, "url").outcome, FieldOutcome::TakeSpec);
    }

    // Urlencoded bodies.

    fn url_body(text: &str) -> Option<Body> {
        Some(Body {
            mode: BodyMode::FormUrlEncoded,
            content: Some(text.to_string()),
            form_data: None,
            file_path: None,
        })
    }

    fn url_text(out: MergeOutcome) -> String {
        out.merged.body.expect("body").content.expect("content")
    }

    #[test]
    fn urlencoded_fields_merge_independently() {
        let mut b = base();
        b.body = url_body("a=1&b=2");
        let mut c = base();
        c.body = url_body("a=mine&b=2");
        let mut s = base();
        s.body = url_body("a=1&b=3&c=4");
        let out = run(&b, &c, &s);
        assert!(has_change(&out, "form:b") && has_change(&out, "form:c"));
        assert_eq!(url_text(out), "a=mine&b=3&c=4");
    }

    #[test]
    fn urlencoded_pairs_round_trip_exactly() {
        let text = "q=a%26b&token=x=y&flag&name=caf%C3%A9+au+lait";
        let mut b = base();
        b.body = url_body(text);
        let c = b.clone();
        let mut s = base();
        // The spec changes something else, so the merge runs.
        s.body = url_body(&format!("{text}&extra=1"));
        let out = run(&b, &c, &s);
        assert_eq!(url_text(out), format!("{text}&extra=1"));
        // With nothing new in the spec the user's text comes back byte for byte.
        let same = run(&b, &c, &b);
        assert_eq!(url_text(same), text);
    }

    #[test]
    fn a_removed_urlencoded_field_is_dropped_when_untouched() {
        let mut b = base();
        b.body = url_body("a=1&b=2");
        let c = b.clone();
        let mut s = base();
        s.body = url_body("a=1");
        let out = run(&b, &c, &s);
        assert_eq!(url_text(out), "a=1");
    }

    #[test]
    fn a_user_edited_urlencoded_field_survives_a_spec_removal_as_a_conflict() {
        let mut b = base();
        b.body = url_body("a=1&b=2");
        let mut c = base();
        c.body = url_body("a=1&b=mine");
        let mut s = base();
        s.body = url_body("a=1");
        let out = run(&b, &c, &s);
        assert_eq!(change(&out, "form:b").outcome, FieldOutcome::Conflict);
        assert_eq!(url_text(out), "a=1&b=mine");
    }

    #[test]
    fn an_unparsable_urlencoded_body_falls_back_to_the_scalar_merge() {
        // An empty piece (a doubled ampersand) is not a field list, so the body is one value.
        let mut b = base();
        b.body = url_body("a=1&&b=2");
        let c = b.clone();
        let mut s = base();
        s.body = url_body("a=2");
        let out = run(&b, &c, &s);
        assert!(has_change(&out, "body"));
        assert!(!has_change(&out, "form:a"));
        assert_eq!(url_text(out), "a=2");
    }

    #[test]
    fn urlencoded_changes_are_named_form_colon_name() {
        let mut b = base();
        b.body = url_body("limit=1");
        let c = b.clone();
        let mut s = base();
        s.body = url_body("limit=2");
        let out = run(&b, &c, &s);
        assert_eq!(change(&out, "form:limit").outcome, FieldOutcome::TakeSpec);
    }

    #[test]
    fn encoded_and_plain_names_match_each_other() {
        let mut b = base();
        b.body = url_body("my%20key=1");
        let c = b.clone();
        let mut s = base();
        s.body = url_body("my%20key=2");
        let out = run(&b, &c, &s);
        assert!(has_change(&out, "form:my key"));
        assert_eq!(url_text(out), "my%20key=2");
    }
}
