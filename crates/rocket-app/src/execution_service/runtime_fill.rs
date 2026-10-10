//! The variable pass that runs after the pre-request scripts.
//!
//! The first resolution (`resolve_request_parts`) runs before any script, so it cannot see
//! runtime variables. It records each field as a `FieldTemplate`: the original text, the name of
//! every placeholder in it and the value the first pass put there. After the scripts,
//! `resolve_runtime_placeholders` renders each field the script did not rewrite again from that
//! template, with runtime values in place of the placeholders they name. Every other placeholder
//! keeps its first-pass value, so a `{{$dynamic}}` value is never made twice. A value is put in as
//! it is and never scanned for placeholders again.
//!
//! Text a script wrote is different: it has no template. In it only the names the first pass
//! could not resolve, and the names the script itself added, are filled, and never from the
//! RocketVault scope.

use rocket_environment::resolve;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

/// One request field as the first pass saw it.
#[derive(Debug, Clone, Default)]
pub(crate) struct FieldTemplate {
    /// The original text, before any variable was put in.
    text: String,
    /// The text is JSON (a GraphQL body), so values inside strings are JSON-escaped.
    json: bool,
    /// The trimmed name of each placeholder, in order.
    names: Vec<String>,
    /// The value the first pass put in for each placeholder, in order.
    values: Vec<String>,
    /// The text the first pass produced.
    pub output: String,
}

impl FieldTemplate {
    /// Resolves `text` with `vars` and records what each placeholder became. The names no scope
    /// held are added to `unresolved`.
    pub(crate) fn record(
        text: &str,
        json: bool,
        vars: &HashMap<String, String>,
        unresolved: &mut HashSet<String>,
    ) -> Self {
        let names = RefCell::new(Vec::new());
        let values = RefCell::new(Vec::new());
        let missing = RefCell::new(Vec::new());
        let one = |placeholder: &str| -> String {
            let result = resolve(placeholder, vars);
            names.borrow_mut().push(placeholder_name(placeholder));
            values.borrow_mut().push(result.output.clone());
            missing.borrow_mut().extend(result.unresolved);
            result.output
        };
        let output = if json {
            crate::graphql_request::resolve_json_text(text, one)
        } else {
            substitute(text, |_, placeholder| one(placeholder))
        };
        unresolved.extend(missing.into_inner());
        Self {
            text: text.to_string(),
            json,
            names: names.into_inner(),
            values: values.into_inner(),
            output,
        }
    }

    /// The text again, with each placeholder that names a key of `runtime` set to that value.
    /// A `$dynamic` name is never replaced, and every other placeholder keeps its first value.
    pub(crate) fn render(&self, runtime: &HashMap<String, String>) -> String {
        let pick = |index: usize, placeholder: &str| -> String {
            let name = self
                .names
                .get(index)
                .map(String::as_str)
                .unwrap_or_default();
            if !name.starts_with('$') {
                if let Some(value) = runtime.get(name) {
                    return value.clone();
                }
            }
            self.values
                .get(index)
                .cloned()
                .unwrap_or_else(|| placeholder.to_string())
        };
        if self.json {
            let index = Cell::new(0);
            crate::graphql_request::resolve_json_text(&self.text, |placeholder| {
                let at = index.get();
                index.set(at + 1);
                pick(at, placeholder)
            })
        } else {
            substitute(&self.text, pick)
        }
    }
}

/// Replaces each `{{...}}` placeholder of `text` with `value(index, placeholder)`. It reads
/// placeholders exactly as `rocket_environment::resolve` does: `{{` up to the first `}}`, and
/// an unclosed `{{` is plain text.
fn substitute(text: &str, mut value: impl FnMut(usize, &str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut index = 0;
    while let Some(start) = rest.find("{{") {
        let Some(len) = rest[start + 2..].find("}}") else {
            break;
        };
        let end = start + 2 + len + 2;
        out.push_str(&rest[..start]);
        out.push_str(&value(index, &rest[start..end]));
        index += 1;
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The trimmed name inside a `{{...}}` placeholder.
fn placeholder_name(placeholder: &str) -> String {
    placeholder
        .strip_prefix("{{")
        .and_then(|s| s.strip_suffix("}}"))
        .unwrap_or(placeholder)
        .trim()
        .to_string()
}

/// The names of the placeholders in `text`, `$dynamic` names left out.
pub(crate) fn placeholder_names(text: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    substitute(text, |_, placeholder| {
        let name = placeholder_name(placeholder);
        if !name.starts_with('$') {
            names.insert(name);
        }
        String::new()
    });
    names
}

/// Adds the names that `new` has and `old` does not to `allowed`. A name a script copied from
/// the request it was given (`req.getUrl()`) is not added, because it may come from inside a
/// variable value.
pub(crate) fn allow_new_names(allowed: &mut HashSet<String>, old: Option<&str>, new: &str) {
    let before = old.map(placeholder_names).unwrap_or_default();
    allowed.extend(
        placeholder_names(new)
            .into_iter()
            .filter(|n| !before.contains(n)),
    );
}

/// Fills the placeholders of script-written `text` whose name is a key of `vars`. Other
/// placeholders, `$dynamic` ones included, stay as written. Values are not scanned again.
pub(crate) fn fill_names(text: &str, vars: &HashMap<String, String>) -> String {
    if !text.contains("{{") {
        return text.to_string();
    }
    substitute(text, |_, placeholder| {
        vars.get(&placeholder_name(placeholder))
            .cloned()
            .unwrap_or_else(|| placeholder.to_string())
    })
}

/// Swaps path parameter values in a URL a script rewrote. Only the path is read: after the
/// scheme and authority, before `?` or `#`. One pass goes over its segments from left to right.
/// A segment equal to an old value, or starting with one and then a character that cannot be part
/// of a value (such as `.json`), gets the new value. Each segment changes at most once and the
/// new text is never read again. `swaps` holds (old encoded value, new encoded value) pairs.
pub(crate) fn swap_path_values(url: &str, swaps: &[(String, String)]) -> String {
    if swaps.is_empty() {
        return url.to_string();
    }
    let (start, end) = path_range(url);
    let mut ordered: Vec<&(String, String)> =
        swaps.iter().filter(|(old, _)| !old.is_empty()).collect();
    // A longer old value wins over a shorter one that is its prefix.
    ordered.sort_by_key(|(old, _)| std::cmp::Reverse(old.len()));
    let path = url[start..end]
        .split('/')
        .map(|segment| {
            for (old, new) in &ordered {
                if let Some(rest) = segment.strip_prefix(old.as_str()) {
                    let ends_value = rest.chars().next().map_or(true, |c| {
                        !(c.is_ascii_alphanumeric() || c == '_' || c == '%' || c == '-')
                    });
                    if ends_value {
                        return format!("{new}{rest}");
                    }
                }
            }
            segment.to_string()
        })
        .collect::<Vec<_>>()
        .join("/");
    format!("{}{}{}", &url[..start], path, &url[end..])
}

/// Byte range of the path in `url`, as `rocket_http::substitute_path_params` reads it.
fn path_range(url: &str) -> (usize, usize) {
    let is_delimiter = |c: char| matches!(c, '/' | '?' | '#');
    let after_scheme = url
        .find("://")
        .filter(|i| !url[..*i].contains(is_delimiter))
        .map_or(0, |i| i + 3);
    let start = url[after_scheme..]
        .find(is_delimiter)
        .map_or(url.len(), |i| after_scheme + i);
    let end = url[start..]
        .find(['?', '#'])
        .map_or(url.len(), |i| start + i);
    (start, end)
}

/// The first pass's templates of every field of a request, in request order.
#[derive(Debug, Clone, Default)]
pub(crate) struct RequestTemplates {
    /// The URL before path parameters were put in.
    pub url: FieldTemplate,
    /// The URL the first pass produced, path parameters included.
    pub url_output: String,
    /// Path parameter names and value templates.
    pub path_params: Vec<(String, FieldTemplate)>,
    /// Query key and value templates.
    pub query: Vec<(FieldTemplate, FieldTemplate)>,
    /// Header key and value templates, after collection and folder defaults applied.
    pub headers: Vec<(FieldTemplate, FieldTemplate)>,
    /// The body content template.
    pub body: Option<FieldTemplate>,
    /// Form-data value templates, by entry.
    pub form: Vec<FieldTemplate>,
    /// Auth field templates, in `map_auth_fields` order.
    pub auth: Vec<FieldTemplate>,
    /// Names a placeholder of the request text named that no scope held.
    pub unresolved: HashSet<String>,
}

/// Matches the current key-value pairs to the templates in order, so that a pair a script
/// did not change is rendered from its template and any other pair is filled as script text.
/// A pair matches the first template, from the last match on, whose output equals it.
pub(crate) fn fill_pairs(
    pairs: &mut [(&mut String, &mut String)],
    templates: &[(FieldTemplate, FieldTemplate)],
    runtime: &HashMap<String, String>,
    script_vars: &HashMap<String, String>,
) {
    let mut cursor = 0;
    for (key, value) in pairs.iter_mut() {
        let found = templates[cursor.min(templates.len())..]
            .iter()
            .position(|(k, v)| k.output == **key && v.output == **value);
        match found {
            Some(offset) => {
                let (k, v) = &templates[cursor + offset];
                **key = k.render(runtime);
                **value = v.render(runtime);
                cursor += offset + 1;
            }
            None => {
                **key = fill_names(key, script_vars);
                **value = fill_names(value, script_vars);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn record_matches_resolve_output() {
        let v = vars(&[("a", "1"), ("b", "{{c}}")]);
        for text in [
            "x{{a}}y{{ b }}z{{missing}}",
            "{{{a}}",
            "{{a}}}",
            "open {{ a",
            "{{a}b}}",
            "",
        ] {
            let mut unresolved = HashSet::new();
            let t = FieldTemplate::record(text, false, &v, &mut unresolved);
            assert_eq!(t.output, resolve(text, &v).output, "{text}");
        }
    }

    #[test]
    fn render_replaces_only_runtime_names_and_never_rescans_values() {
        let v = vars(&[("token", "old"), ("next", "https://evil/{{vault.k}}")]);
        let mut unresolved = HashSet::new();
        let t = FieldTemplate::record(
            "{{next}}?t={{token}}&id={{$guid}}",
            false,
            &v,
            &mut unresolved,
        );
        let first_id = t.output.rsplit('=').next().unwrap_or_default().to_string();
        let out = t.render(&vars(&[("token", "fresh"), ("vault.k", "secret")]));
        assert_eq!(
            out,
            format!("https://evil/{{{{vault.k}}}}?t=fresh&id={first_id}")
        );
    }

    #[test]
    fn fill_names_leaves_unknown_and_dynamic_names() {
        let out = fill_names("{{a}} {{b}} {{$guid}} {{ open", &vars(&[("a", "{{b}}")]));
        assert_eq!(out, "{{b}} {{b}} {{$guid}} {{ open");
    }

    #[test]
    fn swap_path_values_reads_only_the_path() {
        let swaps = vec![("1".to_string(), "42".to_string())];
        assert_eq!(
            swap_path_values("https://api1.test/items/1?page=1&x=1#1", &swaps),
            "https://api1.test/items/42?page=1&x=1#1"
        );
        assert_eq!(
            swap_path_values("https://h.test/files/1.json/12", &swaps),
            "https://h.test/files/42.json/12"
        );
    }

    #[test]
    fn swap_path_values_never_replaces_twice() {
        let swaps = vec![
            ("ab".to_string(), "cd".to_string()),
            ("d".to_string(), "zz".to_string()),
        ];
        assert_eq!(
            swap_path_values("https://h.test/ab/d?q=ab", &swaps),
            "https://h.test/cd/zz?q=ab"
        );
    }

    #[test]
    fn allow_new_names_skips_names_the_old_text_had() {
        let mut allowed = HashSet::new();
        allow_new_names(
            &mut allowed,
            Some("https://h/{{inner}}"),
            "https://h/{{inner}}/{{fresh}}",
        );
        assert_eq!(allowed, HashSet::from(["fresh".to_string()]));
    }
}
