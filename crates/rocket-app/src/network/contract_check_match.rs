//! Contract check on the consumer side: matching, verdicts and redaction (pure).
//!
//! The verdict compares each offered change with the consumer's own request fields directly
//! (plan deviation 2). It is conservative: a request that does not touch a field in the way
//! that matters is `pass`, and a field it cannot judge is `pass`.

use rocket_collection::contract::RequestSignatureSnapshot;
use rocket_export::is_sensitive_name;
use rocket_network::{ContractChange, ContractChangeType, ContractPolicyWire, ContractVerdict};

/// The value that replaces a sensitive old or new value.
const HIDDEN: &str = "[hidden]";

/// The segment that matches any one segment.
const WILDCARD: &str = "*";

/// One request of the active workspace that a change may match.
pub struct LocalRequest {
    pub collection: String,
    pub name: String,
    pub signature: RequestSignatureSnapshot,
}

/// Splits a URL or path template into its path segments.
///
/// The scheme, host, query and fragment are dropped. A URL that does not start with `/` (after
/// the scheme) starts with a host or a `{{base}}` variable, so its first segment is dropped.
/// `{{var}}`, `:id` and `{id}` segments become `*`. Empty segments are skipped.
pub fn normalize_path(url: &str) -> Vec<String> {
    let url = url.trim();
    let url = url.split(['?', '#']).next().unwrap_or_default();
    let (rest, has_host) = match url.find("://") {
        Some(at) => (&url[at + 3..], true),
        None => (url, !url.starts_with('/')),
    };
    let mut segments = rest.split('/');
    if has_host {
        segments.next();
    }
    segments
        .filter(|s| !s.is_empty())
        .map(|s| {
            if variable_name(s).is_some() {
                WILDCARD.to_string()
            } else {
                s.to_string()
            }
        })
        .collect()
}

/// Whether `url` has the shape of `template`: segment by segment, case-sensitive, and `*` on
/// either side matches any one segment.
/// A path without a leading slash loses its first segment as the host.
pub fn paths_match(template: &str, url: &str) -> bool {
    let a = normalize_path(template);
    let b = normalize_path(url);
    a.len() == b.len()
        && a.iter()
            .zip(&b)
            .all(|(x, y)| x == WILDCARD || y == WILDCARD || x == y)
}

/// One verdict per change, with the indexes into `locals` of the requests it matched.
///
/// A change matches a request whose method is equal ignoring case and whose URL matches the
/// change's path template. No match is `not_used`; otherwise the worst verdict wins. The
/// change's `breaking` flag already carries the provider's policy, so `_policy` does not
/// change any verdict.
pub fn verdicts(
    changes: &[ContractChange],
    _policy: ContractPolicyWire,
    locals: &[LocalRequest],
) -> Vec<(ContractVerdict, Vec<usize>)> {
    changes
        .iter()
        .map(|change| {
            let matched: Vec<usize> = locals
                .iter()
                .enumerate()
                .filter(|(_, l)| request_matches(change, &l.signature))
                .map(|(i, _)| i)
                .collect();
            if matched.is_empty() {
                return (ContractVerdict::NotUsed, matched);
            }
            let breaks = matched
                .iter()
                .any(|&i| breaks_request(change, &locals[i].signature));
            let verdict = if breaks {
                ContractVerdict::Break
            } else {
                ContractVerdict::Pass
            };
            (verdict, matched)
        })
        .collect()
}

/// Replaces the old and new values with `[hidden]` for auth fields, body fields and sensitive
/// field names. A value that is `None` stays `None`.
pub fn redact_change(mut change: ContractChange) -> ContractChange {
    if is_sensitive_field(&change.field) {
        for value in [&mut change.old_value, &mut change.new_value]
            .into_iter()
            .flatten()
        {
            *value = HIDDEN.to_string();
        }
    }
    change
}

/// Whether a change field may hold a secret.
///
/// A field whose first word is `auth` (any case, words split at `:`, `.` and `_`) and every body
/// field always may. Other fields follow the export name rule.
fn is_sensitive_field(field: &str) -> bool {
    let first = field.split([':', '.', '_']).next().unwrap_or_default();
    if first.eq_ignore_ascii_case("auth") || is_body_field(field) {
        return true;
    }
    let name = field.split_once(':').map_or(field, |(_, name)| name);
    is_sensitive_name(name)
}

/// Whether the change is about this request: same method ignoring case and a matching path.
fn request_matches(change: &ContractChange, sig: &RequestSignatureSnapshot) -> bool {
    change.method.trim().eq_ignore_ascii_case(sig.method.trim())
        && paths_match(&change.path_template, &sig.url_pattern)
}

/// The field a change is about, parsed from its `field` text.
enum Field<'a> {
    Header(&'a str),
    Query(&'a str),
    Path(&'a str),
    Form(&'a str),
    Body,
    Auth,
    Unknown,
}

/// Whether the field is `body` or a `body:<path>` field.
fn is_body_field(field: &str) -> bool {
    field == "body" || field.starts_with("body:")
}

/// Whether a value is unknown here: a `{{variable}}` or a value the provider hid.
fn is_unknown_value(value: &str) -> bool {
    value.contains("{{") || value == HIDDEN
}

fn parse_field(field: &str) -> Field<'_> {
    if is_body_field(field) {
        return Field::Body;
    }
    if field == "auth" {
        return Field::Auth;
    }
    match field.split_once(':') {
        Some(("header", name)) => Field::Header(name),
        Some(("query", name)) => Field::Query(name),
        Some(("path", name)) => Field::Path(name),
        Some(("form", name)) => Field::Form(name),
        _ => Field::Unknown,
    }
}

/// Whether the change breaks this request. A change that is not breaking never breaks.
fn breaks_request(change: &ContractChange, sig: &RequestSignatureSnapshot) -> bool {
    if !change.breaking {
        return false;
    }
    let new = change.new_value.as_deref();
    match parse_field(&change.field) {
        Field::Header(name) => {
            let sent = sig
                .headers
                .iter()
                .find(|h| h.key.eq_ignore_ascii_case(name))
                .map(|h| h.value.as_str());
            breaks_entry(change.change_type, sent, new)
        }
        Field::Query(name) => {
            let sent = sig
                .query_params
                .iter()
                .find(|q| q.key == name)
                .map(|q| q.value.as_str());
            breaks_entry(change.change_type, sent, new)
        }
        Field::Form(name) => {
            let sent = sig
                .form_fields
                .iter()
                .find(|f| f.key == name)
                .map(|f| f.value.as_str());
            breaks_entry(change.change_type, sent, new)
        }
        Field::Path(name) => {
            // The request already matched the template, so an added or changed variable is
            // there by position. Only a removed variable the request still names breaks.
            change.change_type == ContractChangeType::Removed
                && path_variable_names(&sig.url_pattern).any(|v| v == name)
        }
        Field::Body => {
            let has_body = has_body(sig);
            match change.change_type {
                ContractChangeType::Added => !has_body,
                ContractChangeType::Removed | ContractChangeType::Changed => has_body,
            }
        }
        Field::Auth => breaks_auth(change.change_type, &sig.auth_type, new),
        Field::Unknown => false,
    }
}

/// The verdict rule for a header, query parameter or form field the request may send.
fn breaks_entry(kind: ContractChangeType, sent: Option<&str>, new: Option<&str>) -> bool {
    match (kind, sent) {
        (ContractChangeType::Added, None) => true,
        (ContractChangeType::Added, Some(_)) => false,
        (ContractChangeType::Removed, sent) => sent.is_some(),
        (ContractChangeType::Changed, None) => false,
        (ContractChangeType::Changed, Some(value)) => match new {
            // A variable or a hidden new value cannot be compared, so it never breaks.
            Some(new) => !is_unknown_value(value) && new != HIDDEN && value != new,
            None => false,
        },
    }
}

/// The verdict rule for the auth type. `inherit` and `none` take the folder or collection auth
/// at send time, so they are unknown here and never break.
fn breaks_auth(kind: ContractChangeType, local: &str, new: Option<&str>) -> bool {
    let local = local.trim();
    if local.is_empty()
        || local.eq_ignore_ascii_case("inherit")
        || local.eq_ignore_ascii_case("none")
    {
        return false;
    }
    match kind {
        ContractChangeType::Removed => true,
        ContractChangeType::Added | ContractChangeType::Changed => {
            // A hidden new value is unknown, so it never breaks.
            new.is_some_and(|new| new != HIDDEN && !new.trim().eq_ignore_ascii_case(local))
        }
    }
}

/// Whether the request sends a body: non-blank text or at least one form field.
fn has_body(sig: &RequestSignatureSnapshot) -> bool {
    sig.body_content
        .as_deref()
        .is_some_and(|b| !b.trim().is_empty())
        || !sig.form_fields.is_empty()
}

/// The variable names in the path of a URL: `{{name}}`, `:name` and `{name}` segments.
fn path_variable_names(url: &str) -> impl Iterator<Item = &str> {
    let url = url.split(['?', '#']).next().unwrap_or_default();
    url.split('/').filter_map(variable_name)
}

/// The variable name of a whole `{{name}}`, `{name}` or `:name` segment.
fn variable_name(segment: &str) -> Option<&str> {
    let name = if let Some(inner) = segment
        .strip_prefix("{{")
        .and_then(|s| s.strip_suffix("}}"))
    {
        inner
    } else if let Some(inner) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        inner
    } else {
        segment.strip_prefix(':')?
    };
    let name = name.trim();
    (!name.is_empty()).then_some(name)
}
