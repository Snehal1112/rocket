//! Three-way merge of JSON objects by key path.
//!
//! Objects are merged key by key. Arrays and scalars are leaves, compared with `==`. Nesting
//! deeper than `MAX_JSON_DEPTH` is a leaf too, so a hostile body cannot overflow the stack.

use super::merge::{clip, decide, Choice, FieldChange, FieldOutcome, Pick};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Objects nested deeper than this are compared as one value.
pub(crate) const MAX_JSON_DEPTH: usize = 64;

/// The merged document and the changes the spec brought, one per leaf key path.
pub(crate) struct JsonMerge {
    pub merged: Value,
    pub changes: Vec<FieldChange>,
}

/// Merge three JSON documents. Each must be an object, the caller checks that.
pub(crate) fn merge_json(
    baseline: &Map<String, Value>,
    current: &Map<String, Value>,
    spec: &Map<String, Value>,
    choices: &BTreeMap<String, Choice>,
) -> JsonMerge {
    let mut changes = Vec::new();
    let merged = walk(baseline, current, spec, "$", 0, choices, &mut changes);
    JsonMerge {
        merged: Value::Object(merged),
        changes,
    }
}

fn show(value: Option<&Value>) -> Option<String> {
    value.map(|v| clip(&v.to_string()))
}

fn walk(
    b: &Map<String, Value>,
    c: &Map<String, Value>,
    s: &Map<String, Value>,
    path: &str,
    depth: usize,
    choices: &BTreeMap<String, Choice>,
    changes: &mut Vec<FieldChange>,
) -> Map<String, Value> {
    let mut out = c.clone();
    // Keys only the user has are never in this set, so they stay untouched.
    let keys: BTreeSet<&String> = b.keys().chain(s.keys()).collect();
    for key in keys {
        let (bv, cv, sv) = (b.get(key), c.get(key), s.get(key));
        if sv == bv {
            continue;
        }
        let key_path = format!("{path}.{key}");
        if depth < MAX_JSON_DEPTH {
            if let (Some(Value::Object(bo)), Some(Value::Object(co)), Some(Value::Object(so))) =
                (bv, cv, sv)
            {
                let inner = walk(bo, co, so, &key_path, depth + 1, choices, changes);
                out.insert(key.clone(), Value::Object(inner));
                continue;
            }
        }
        let field = format!("body:{key_path}");
        let (pick, outcome) = decide(&bv, &cv, &sv, sv.is_none(), choices.get(&field));
        if pick == Pick::Spec {
            match sv {
                Some(v) => {
                    out.insert(key.clone(), v.clone());
                }
                None => {
                    out.remove(key);
                }
            }
        }
        if cv != sv {
            changes.push(FieldChange {
                field,
                outcome,
                baseline: show(bv),
                yours: show(cv),
                spec: show(sv),
            });
        }
    }
    out
}

/// True when any change took the spec's value, so the body text must be rebuilt.
pub(crate) fn took_spec(changes: &[FieldChange]) -> bool {
    changes.iter().any(|c| c.outcome == FieldOutcome::TakeSpec)
}
