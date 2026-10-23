//! Script, assertion and action translation from Rocket to Postman.

use std::collections::HashSet;

use rocket_shared::action::ActionSetVariable;
use rocket_shared::assertion::Assertion;

/// Marker line put above a `rok.` call that has no Postman equivalent.
const NOT_TRANSLATED: &str = "// rok: not translated";

/// Result of translating one script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptTranslation {
    pub text: String,
    /// Distinct `rok.` call paths that have no mapping, in order of appearance.
    pub untranslated: Vec<String>,
    /// The script copies a mapped name into a variable. Its text is kept as written.
    pub aliased: bool,
}

/// Outcome of translating one declarative assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertionOutcome {
    Line(String),
    Disabled,
    /// The operator, or the expression when the operator is known.
    Unsupported(String),
}

#[derive(Clone, Copy)]
enum Rewrite {
    /// Replace the path and keep the call with its arguments.
    Call(&'static str),
    /// Replace `path()` with a value; a call with arguments is left alone.
    EmptyCall(&'static str),
    /// Replace a property read.
    Prop(&'static str),
}

use Rewrite::{Call, EmptyCall, Prop};

/// Exact member paths that are rewritten. Left sides exist in `rok-types.ts`.
const TABLE: &[(&str, Rewrite)] = &[
    ("rok.getVar", Call("pm.variables.get")),
    ("rok.setVar", Call("pm.variables.set")),
    ("rok.getEnvVar", Call("pm.environment.get")),
    ("rok.setEnvVar", Call("pm.environment.set")),
    ("rok.getCollectionVar", Call("pm.collectionVariables.get")),
    ("rok.setCollectionVar", Call("pm.collectionVariables.set")),
    ("rok.getGlobalEnvVar", Call("pm.globals.get")),
    ("rok.setGlobalEnvVar", Call("pm.globals.set")),
    ("test", Call("pm.test")),
    ("expect", Call("pm.expect")),
    ("res.getStatus", EmptyCall("pm.response.code")),
    ("res.status", Prop("pm.response.code")),
    ("res.getBody", EmptyCall("pm.response.json()")),
    ("res.getHeader", Call("pm.response.headers.get")),
    ("req.getUrl", EmptyCall("pm.request.url.toString()")),
    ("req.getMethod", EmptyCall("pm.request.method")),
];

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}

fn is_ident_part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// Index of the next non-whitespace char at or after `from`.
fn skip_space(chars: &[char], from: usize) -> usize {
    let mut j = from;
    while j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    j
}

/// Index after the string literal that starts at `chars[i]`. A template literal
/// skips over its `${}` expressions, which may hold strings of their own.
fn skip_string(chars: &[char], mut i: usize) -> usize {
    let quote = chars[i];
    i += 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            i += 2;
        } else if c == quote {
            return i + 1;
        } else if quote == '`' && c == '$' && chars.get(i + 1) == Some(&'{') {
            i = find_expr_end(chars, i + 2) + 1;
        } else if c == '\n' && quote != '`' {
            return i;
        } else {
            i += 1;
        }
    }
    chars.len()
}

/// Index of the `}` that closes a `${` whose body starts at `i`.
fn find_expr_end(chars: &[char], mut i: usize) -> usize {
    let mut depth = 1usize;
    while i < chars.len() {
        match chars[i] {
            '\'' | '"' | '`' => {
                i = skip_string(chars, i);
                continue;
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    chars.len()
}

/// What the scanner collects while it runs.
struct Scan {
    untranslated: Vec<String>,
    aliased: bool,
}

/// Copy a string literal starting at `chars[i]` (the quote) and return the index after it.
/// Text of a template literal is copied as is, while its `${}` expressions are code and
/// are translated (without marker lines, which would change the string).
fn copy_string(
    chars: &[char],
    mut i: usize,
    out: &mut String,
    line_no: &mut usize,
    scan: &mut Scan,
) -> usize {
    let quote = chars[i];
    out.push(quote);
    i += 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            out.push(c);
            if let Some(next) = chars.get(i + 1) {
                out.push(*next);
            }
            i += 2;
        } else if quote == '`' && c == '$' && chars.get(i + 1) == Some(&'{') {
            let end = find_expr_end(chars, i + 2);
            let inner: String = chars[i + 2..end.min(chars.len())].iter().collect();
            *line_no += inner.matches('\n').count();
            let translated = translate_inner(&inner, false);
            out.push_str("${");
            out.push_str(&translated.text);
            scan.aliased |= translated.aliased;
            for name in translated.untranslated {
                if !scan.untranslated.contains(&name) {
                    scan.untranslated.push(name);
                }
            }
            if end < chars.len() {
                out.push('}');
            }
            i = end + 1;
        } else {
            out.push(c);
            i += 1;
            if c == quote {
                break;
            }
            if c == '\n' {
                *line_no += 1;
                if quote != '`' {
                    break;
                }
            }
        }
    }
    i.min(chars.len())
}

/// Insert the marker line above the current output line, once per input line.
fn insert_marker(out: &mut String, line_no: usize, marked: &mut HashSet<usize>) {
    if !marked.insert(line_no) {
        return;
    }
    let start = out.rfind('\n').map_or(0, |p| p + 1);
    let indent: String = out[start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    out.insert_str(start, &format!("{indent}{NOT_TRANSLATED}\n"));
}

/// Rewrite the exact Rocket call paths of `src` to `pm.*`.
///
/// A single pass skips string literals, template literal text and comments, so text in
/// them is never changed. Aliases and computed access are not followed: a script that
/// copies a mapped name keeps its original text and sets `aliased`.
pub fn translate_script(src: &str) -> ScriptTranslation {
    translate_inner(src, true)
}

/// `markers` is false inside a `${}` expression, where a marker line would change the string.
fn translate_inner(src: &str, markers: bool) -> ScriptTranslation {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut scan = Scan {
        untranslated: Vec::new(),
        aliased: false,
    };
    let mut marked: HashSet<usize> = HashSet::new();
    let mut shadowed: HashSet<&'static str> = HashSet::new();
    let mut line_no = 0usize;
    let mut prev_sig: Option<char> = None;
    let mut prev_word = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line_no += 1;
            out.push(c);
            i += 1;
        } else if c.is_whitespace() {
            out.push(c);
            i += 1;
        } else if matches!(c, '\'' | '"' | '`') {
            i = copy_string(&chars, i, &mut out, &mut line_no, &mut scan);
            prev_sig = Some(c);
            prev_word.clear();
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(chars[i]);
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            out.push_str("/*");
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                if chars[i] == '\n' {
                    line_no += 1;
                }
                out.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                out.push_str("*/");
                i += 2;
            }
        } else if is_ident_start(c) {
            let start = i;
            i += 1;
            while i < chars.len() && is_ident_part(chars[i]) {
                i += 1;
            }
            while i + 1 < chars.len() && chars[i] == '.' && is_ident_start(chars[i + 1]) {
                i += 2;
                while i < chars.len() && is_ident_part(chars[i]) {
                    i += 1;
                }
            }
            let chain: String = chars[start..i].iter().collect();
            let host = ["res", "req"]
                .into_iter()
                .find(|h| chain == *h || chain.starts_with(&format!("{h}.")));
            let declaring = host.is_some()
                && chain.len() == 3
                && matches!(prev_word.as_str(), "const" | "let" | "var");
            if declaring {
                shadowed.extend(host);
            }
            let member_access = prev_sig == Some('.') || prev_word == "function" || declaring;
            let local_host = host.is_some_and(|h| shadowed.contains(h));
            let j = skip_space(&chars, i);
            let is_call = chars.get(j) == Some(&'(');
            let entry = TABLE.iter().find(|(k, _)| *k == chain).map(|(_, r)| *r);
            let is_rok = chain == "rok" || chain.starts_with("rok.");
            let report = |out: &mut String, scan: &mut Scan, marked: &mut HashSet<usize>| {
                if markers {
                    insert_marker(out, line_no, marked);
                }
                if !scan.untranslated.contains(&chain) {
                    scan.untranslated.push(chain.clone());
                }
            };
            if member_access || local_host {
                out.push_str(&chain);
            } else if let Some(rewrite) = entry {
                match rewrite {
                    Prop(to) => out.push_str(to),
                    Call(to) if is_call => out.push_str(to),
                    EmptyCall(to) if is_call => {
                        let k = skip_space(&chars, j + 1);
                        if chars.get(k) == Some(&')') {
                            out.push_str(to);
                            i = k + 1;
                        } else {
                            report(&mut out, &mut scan, &mut marked);
                            out.push_str(&chain);
                        }
                    }
                    _ => {
                        let bare = chain == "test" || chain == "expect";
                        if !bare || prev_sig == Some('=') {
                            scan.aliased = true;
                        }
                        out.push_str(&chain);
                    }
                }
            } else if is_rok {
                if chain == "rok" || (!is_call && prev_sig == Some('=')) {
                    scan.aliased = true;
                } else {
                    report(&mut out, &mut scan, &mut marked);
                }
                out.push_str(&chain);
            } else if host.is_some() && chain.len() > 3 {
                report(&mut out, &mut scan, &mut marked);
                out.push_str(&chain);
            } else {
                out.push_str(&chain);
            }
            prev_sig = chars.get(i.saturating_sub(1)).copied();
            prev_word = chain;
        } else {
            out.push(c);
            i += 1;
            prev_sig = Some(c);
            prev_word.clear();
        }
    }
    if scan.aliased {
        return ScriptTranslation {
            text: src.to_string(),
            untranslated: Vec::new(),
            aliased: true,
        };
    }
    ScriptTranslation {
        text: out,
        untranslated: scan.untranslated,
        aliased: false,
    }
}

fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

fn push_segment(js: &mut String, seg: &str) {
    if !seg.is_empty() && seg.chars().all(|c| c.is_ascii_digit()) {
        js.push_str(&format!("[{seg}]"));
    } else if seg.chars().next().is_some_and(is_ident_start) && seg.chars().all(is_ident_part) {
        js.push('.');
        js.push_str(seg);
    } else {
        js.push_str(&format!("[{}]", json_string(seg)));
    }
}

/// Postman JavaScript for a Rocket assertion or action expression.
fn expression_js(expr: &str) -> Option<String> {
    match expr {
        "res.status" => return Some("pm.response.code".into()),
        "res.responseTime" => return Some("pm.response.responseTime".into()),
        "res.body" => return Some("pm.response.text()".into()),
        _ => {}
    }
    if let Some(path) = expr.strip_prefix("res.body.") {
        let mut js = String::from("pm.response.json()");
        for seg in path.split('.') {
            push_segment(&mut js, seg);
        }
        return Some(js);
    }
    expr.strip_prefix("res.headers.")
        .map(|name| format!("pm.response.headers.get({})", json_string(name)))
}

/// A JSON number stays a number, anything else becomes a string.
fn js_value(v: &str) -> String {
    if serde_json::from_str::<serde_json::Number>(v).is_ok() {
        v.to_string()
    } else {
        json_string(v)
    }
}

/// Translate one assertion to a `pm.test` line.
pub fn assertion_line(a: &Assertion) -> AssertionOutcome {
    if a.disabled == Some(true) {
        return AssertionOutcome::Disabled;
    }
    let suffix = match a.operator.as_str() {
        "eq" => "to.eql",
        "neq" => "to.not.eql",
        "gt" => "to.be.above",
        "gte" => "to.be.at.least",
        "lt" => "to.be.below",
        "lte" => "to.be.at.most",
        "contains" => "to.include",
        other => return AssertionOutcome::Unsupported(other.to_string()),
    };
    let Some(lhs) = expression_js(&a.expression) else {
        return AssertionOutcome::Unsupported(a.expression.clone());
    };
    let raw = a.value.as_deref().unwrap_or("");
    let value = if a.operator == "contains" {
        json_string(raw)
    } else {
        js_value(raw)
    };
    let name = if raw.is_empty() {
        format!("{} {}", a.expression, a.operator)
    } else {
        format!("{} {} {}", a.expression, a.operator, raw)
    };
    AssertionOutcome::Line(format!(
        "pm.test({}, () => {{ pm.expect({lhs}).{suffix}({value}); }});",
        json_string(&name)
    ))
}

/// Translate an after-response set-variable action to a `pm.*.set` line.
pub fn action_line(a: &ActionSetVariable) -> Option<String> {
    if a.phase != "after-response" || a.disabled == Some(true) {
        return None;
    }
    let target = match a.variable.scope.as_str() {
        "collection" => "pm.collectionVariables",
        "environment" => "pm.environment",
        _ => "pm.variables",
    };
    let value = expression_js(&a.selector.expression)?;
    Some(format!("{target}.set({}, {value});", json_string(&a.variable.name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::action::{ActionSelector, ActionSetVariable, ActionVariable};
    use rocket_shared::assertion::Assertion;

    fn assertion(expr: &str, op: &str, value: Option<&str>) -> Assertion {
        Assertion::new(expr, op, value.map(str::to_string))
    }

    fn action(phase: &str, scope: &str) -> ActionSetVariable {
        ActionSetVariable {
            phase: phase.into(),
            selector: ActionSelector {
                expression: "res.body.id".into(),
                method: "jsonq".into(),
            },
            variable: ActionVariable {
                name: "n".into(),
                scope: scope.into(),
            },
            disabled: None,
            description: None,
        }
    }

    fn line(a: &Assertion) -> String {
        match assertion_line(a) {
            AssertionOutcome::Line(l) => l,
            other => panic!("expected a line, got {other:?}"),
        }
    }

    #[test]
    fn translates_variable_and_response_calls() {
        let src = "const id = rok.getVar('id');\nrok.setEnvVar('t', res.getBody().token);\ntest('ok', () => { expect(res.getStatus()).to.equal(200); });\n";
        let out = translate_script(src);
        assert_eq!(
            out.text,
            "const id = pm.variables.get('id');\npm.environment.set('t', pm.response.json().token);\npm.test('ok', () => { pm.expect(pm.response.code).to.equal(200); });\n"
        );
        assert!(out.untranslated.is_empty());
        assert!(!out.aliased);
    }

    #[test]
    fn ignores_rok_text_inside_strings_and_comments() {
        let src = "// rok.setVar('a')\nconst s = \"rok.getVar('b')\";\nconst t = `rok.getEnvVar`;\n/* rok.setVar('c') */\nconst u = 'it\\'s rok.getVar';";
        let out = translate_script(src);
        assert_eq!(out.text, src);
        assert!(out.untranslated.is_empty());
    }

    #[test]
    fn unmapped_call_gets_marker_and_is_listed() {
        let out = translate_script("  rok.sendRequest({});\nrok.sendRequest({});");
        assert_eq!(
            out.text,
            "  // rok: not translated\n  rok.sendRequest({});\n// rok: not translated\nrok.sendRequest({});"
        );
        assert_eq!(out.untranslated, ["rok.sendRequest"]);
    }

    #[test]
    fn aliased_script_is_kept_and_reported() {
        let src = "const g = rok.getVar;\ng('a');";
        let out = translate_script(src);
        assert!(out.aliased);
        assert_eq!(out.text, src);
    }

    #[test]
    fn destructured_rok_is_an_alias() {
        let src = "const { getVar } = rok;\ngetVar('a');";
        let out = translate_script(src);
        assert!(out.aliased);
        assert_eq!(out.text, src);
    }

    #[test]
    fn member_of_another_object_is_not_rewritten() {
        let src = "helper.test('x');\nfoo().expect(1);\nmy.res.getStatus();";
        let out = translate_script(src);
        assert_eq!(out.text, src);
    }

    #[test]
    fn res_status_property_and_header_calls() {
        let src = "a(res.status);\nb(res.getHeader('x'));\nc(req.getUrl());\nd(req.getMethod());\n";
        let out = translate_script(src);
        assert_eq!(
            out.text,
            "a(pm.response.code);\nb(pm.response.headers.get('x'));\nc(pm.request.url.toString());\nd(pm.request.method);\n"
        );
    }

    #[test]
    fn get_body_with_arguments_is_reported() {
        let out = translate_script("const raw = res.getBody({ raw: true });");
        assert_eq!(
            out.text,
            "// rok: not translated\nconst raw = res.getBody({ raw: true });"
        );
        assert_eq!(out.untranslated, ["res.getBody"]);
    }

    #[test]
    fn unmapped_response_helpers_get_marker_and_are_listed() {
        let out = translate_script("const t = res.getResponseTime();\nreq.setHeader('a', 'b');");
        assert_eq!(
            out.text,
            "// rok: not translated\nconst t = res.getResponseTime();\n// rok: not translated\nreq.setHeader('a', 'b');"
        );
        assert_eq!(out.untranslated, ["res.getResponseTime", "req.setHeader"]);
    }

    #[test]
    fn local_res_variable_is_not_treated_as_the_response() {
        let src = "const res = await rok.sendRequest({});\nconsole.log(res.status, res.data);";
        let out = translate_script(src);
        assert_eq!(
            out.text,
            "// rok: not translated\nconst res = await rok.sendRequest({});\nconsole.log(res.status, res.data);"
        );
        assert_eq!(out.untranslated, ["rok.sendRequest"]);
    }

    #[test]
    fn rok_call_inside_template_expression_is_translated() {
        let out = translate_script("const u = `${rok.getEnvVar('base')}/users`;");
        assert_eq!(out.text, "const u = `${pm.environment.get('base')}/users`;");
        assert!(out.untranslated.is_empty());
    }

    #[test]
    fn unmapped_call_inside_template_expression_is_listed_without_a_marker() {
        let src = "const u = `line1\n${rok.sendRequest({})}`;";
        let out = translate_script(src);
        assert_eq!(out.text, src);
        assert_eq!(out.untranslated, ["rok.sendRequest"]);
    }

    #[test]
    fn nested_template_text_is_not_rewritten() {
        let src = "const x = `a ${c ? `rok.setVar('a')` : ''}`;";
        let out = translate_script(src);
        assert_eq!(out.text, src);
        assert!(out.untranslated.is_empty());
    }

    #[test]
    fn template_with_literal_dollar_and_braces_is_left_alone() {
        let src = "const x = `price $5 {not code} ${a}`;";
        assert_eq!(translate_script(src).text, src);
    }

    #[test]
    fn assertion_lines_for_each_operator() {
        let cases = [
            ("eq", "to.eql"),
            ("neq", "to.not.eql"),
            ("gt", "to.be.above"),
            ("gte", "to.be.at.least"),
            ("lt", "to.be.below"),
            ("lte", "to.be.at.most"),
        ];
        for (op, suffix) in cases {
            assert_eq!(
                line(&assertion("res.status", op, Some("200"))),
                format!(
                    "pm.test(\"res.status {op} 200\", () => {{ pm.expect(pm.response.code).{suffix}(200); }});"
                )
            );
        }
        assert_eq!(
            line(&assertion("res.body.name", "contains", Some("a"))),
            "pm.test(\"res.body.name contains a\", () => { pm.expect(pm.response.json().name).to.include(\"a\"); });"
        );
    }

    #[test]
    fn assertion_expression_paths() {
        let l = line(&assertion("res.body.a.0.b", "eq", Some("x")));
        assert!(l.contains("pm.expect(pm.response.json().a[0].b)"), "{l}");
        let l = line(&assertion("res.body.x-y", "eq", Some("1")));
        assert!(l.contains("pm.response.json()[\"x-y\"]"), "{l}");
        let l = line(&assertion("res.headers.content-type", "contains", Some("json")));
        assert!(l.contains("pm.response.headers.get(\"content-type\")"), "{l}");
        let l = line(&assertion("res.responseTime", "lt", Some("500")));
        assert!(l.contains("pm.response.responseTime"), "{l}");
    }

    #[test]
    fn unknown_operator_is_reported_and_disabled_is_silent() {
        assert_eq!(
            assertion_line(&assertion("res.status", "matches", Some("2.."))),
            AssertionOutcome::Unsupported("matches".into())
        );
        let mut off = assertion("res.status", "eq", Some("200"));
        off.disabled = Some(true);
        assert_eq!(assertion_line(&off), AssertionOutcome::Disabled);
    }

    #[test]
    fn unknown_expression_is_reported() {
        assert!(matches!(
            assertion_line(&assertion("req.url", "eq", Some("x"))),
            AssertionOutcome::Unsupported(_)
        ));
    }

    #[test]
    fn after_response_action_becomes_set_line() {
        assert_eq!(
            action_line(&action("after-response", "collection")).as_deref(),
            Some("pm.collectionVariables.set(\"n\", pm.response.json().id);")
        );
        assert_eq!(
            action_line(&action("after-response", "environment")).as_deref(),
            Some("pm.environment.set(\"n\", pm.response.json().id);")
        );
        assert_eq!(
            action_line(&action("after-response", "runtime")).as_deref(),
            Some("pm.variables.set(\"n\", pm.response.json().id);")
        );
    }

    #[test]
    fn before_request_action_has_no_line() {
        assert_eq!(action_line(&action("before-request", "runtime")), None);
    }
}
