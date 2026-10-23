//! Postman `event` blocks built from Rocket scripts, tests, assertions and actions.

use rocket_collection::{FolderSettings, Request};
use serde_json::{json, Value};

use crate::postman::PostmanOptions;
use crate::report::{ExportAction, ExportNote};
use crate::scripts::{action_line, assertion_line, translate_script, AssertionOutcome};

fn note(path: &str, feature: String, action: ExportAction, detail: Option<String>) -> ExportNote {
    ExportNote {
        path: path.to_string(),
        feature,
        action,
        detail,
    }
}

fn lines_of(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

/// Lines of one script after translation. A blank script gives no lines.
fn script_lines(
    src: Option<&str>,
    opts: &PostmanOptions,
    path: &str,
    notes: &mut Vec<ExportNote>,
) -> Vec<String> {
    let Some(src) = src.filter(|s| !s.trim().is_empty()) else {
        return Vec::new();
    };
    if opts.preserve_scripts {
        return lines_of(src);
    }
    let translated = translate_script(src);
    if translated.aliased {
        notes.push(note(
            path,
            "script aliases the rok API".into(),
            ExportAction::Preserved,
            Some("kept as written, not translated".into()),
        ));
        return lines_of(src);
    }
    for name in &translated.untranslated {
        notes.push(note(
            path,
            format!("script call {name}"),
            ExportAction::Skipped,
            Some("no Postman equivalent, kept as written".into()),
        ));
    }
    lines_of(&translated.text)
}

fn event(listen: &str, exec: Vec<String>) -> Value {
    json!({ "listen": listen, "script": { "type": "text/javascript", "exec": exec } })
}

/// The `prerequest` and `test` events of one request.
///
/// Post-response script, tests, assertions and after-response actions share one
/// `test` event. Before-request actions have no Postman form and are reported.
pub fn request_events(
    req: &Request,
    opts: &PostmanOptions,
    path: &str,
    notes: &mut Vec<ExportNote>,
) -> Vec<Value> {
    let pre = script_lines(req.pre_request_script.as_deref(), opts, path, notes);
    let mut test = script_lines(req.post_response_script.as_deref(), opts, path, notes);
    let tests = script_lines(req.tests.as_deref(), opts, path, notes);
    if !tests.is_empty() {
        if !test.is_empty() {
            test.push("// Tests".into());
        }
        test.extend(tests);
    }
    for a in &req.assertions {
        match assertion_line(a) {
            AssertionOutcome::Line(l) => test.push(l),
            AssertionOutcome::Disabled => {}
            AssertionOutcome::Unsupported(what) => notes.push(note(
                path,
                format!("assertion '{what}'"),
                ExportAction::Skipped,
                None,
            )),
        }
    }
    for a in req.actions.iter().filter(|a| a.disabled != Some(true)) {
        match action_line(a) {
            Some(l) => test.push(l),
            None => notes.push(note(
                path,
                if a.phase == "after-response" {
                    "action selector".to_string()
                } else {
                    format!("{} action", a.phase)
                },
                ExportAction::Skipped,
                None,
            )),
        }
    }
    let mut events = Vec::new();
    if !pre.is_empty() {
        events.push(event("prerequest", pre));
    }
    if !test.is_empty() {
        events.push(event("test", test));
    }
    events
}

/// The `prerequest` and `test` events of one folder.
pub fn folder_events(
    settings: &FolderSettings,
    opts: &PostmanOptions,
    path: &str,
    notes: &mut Vec<ExportNote>,
) -> Vec<Value> {
    let pre = script_lines(settings.pre_request_script.as_deref(), opts, path, notes);
    let mut test = script_lines(settings.post_response_script.as_deref(), opts, path, notes);
    let tests = script_lines(settings.tests_script.as_deref(), opts, path, notes);
    if !tests.is_empty() {
        if !test.is_empty() {
            test.push("// Tests".into());
        }
        test.extend(tests);
    }
    let mut events = Vec::new();
    if !pre.is_empty() {
        events.push(event("prerequest", pre));
    }
    if !test.is_empty() {
        events.push(event("test", test));
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postman::PostmanOptions;
    use crate::report::ExportAction;
    use rocket_collection::{FolderSettings, Request};
    use rocket_shared::action::{ActionSelector, ActionSetVariable, ActionVariable};
    use rocket_shared::assertion::Assertion;
    use rocket_shared::types::HttpMethod;
    use serde_json::json;

    fn request() -> Request {
        Request::new("r", HttpMethod::Get, "https://x.test")
    }

    fn run(req: &Request, preserve: bool) -> (Vec<serde_json::Value>, Vec<ExportNote>) {
        let mut notes = Vec::new();
        let opts = PostmanOptions { preserve_scripts: preserve };
        let events = request_events(req, &opts, "users/r", &mut notes);
        (events, notes)
    }

    #[test]
    fn request_events_split_prerequest_and_test() {
        let mut r = request();
        r.pre_request_script = Some("rok.setVar('a', 1);".into());
        r.post_response_script = Some("rok.setVar('b', 2);".into());
        r.tests = Some("test('t', () => {});".into());
        r.assertions = vec![Assertion::new("res.status", "eq", Some("200".into()))];
        let (events, notes) = run(&r, false);
        assert!(notes.is_empty());
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["listen"], "prerequest");
        assert_eq!(events[0]["script"]["type"], "text/javascript");
        assert_eq!(events[0]["script"]["exec"], json!(["pm.variables.set('a', 1);"]));
        assert_eq!(events[1]["listen"], "test");
        assert_eq!(
            events[1]["script"]["exec"],
            json!([
                "pm.variables.set('b', 2);",
                "// Tests",
                "pm.test('t', () => {});",
                "pm.test(\"res.status eq 200\", () => { pm.expect(pm.response.code).to.eql(200); });"
            ])
        );
    }

    #[test]
    fn no_scripts_gives_no_events() {
        let (events, notes) = run(&request(), false);
        assert!(events.is_empty());
        assert!(notes.is_empty());
    }

    #[test]
    fn blank_scripts_give_no_events() {
        let mut r = request();
        r.pre_request_script = Some("  \n".into());
        r.tests = Some(String::new());
        assert!(run(&r, false).0.is_empty());
    }

    #[test]
    fn preserve_scripts_keeps_text() {
        let mut r = request();
        r.pre_request_script = Some("rok.getVar('a')".into());
        r.assertions = vec![Assertion::new("res.status", "eq", Some("200".into()))];
        let (events, _) = run(&r, true);
        assert_eq!(events[0]["script"]["exec"], json!(["rok.getVar('a')"]));
        assert!(events[1]["script"]["exec"][0]
            .as_str()
            .expect("line")
            .starts_with("pm.test("));
    }

    #[test]
    fn unmapped_rok_call_adds_skipped_note() {
        let mut r = request();
        r.pre_request_script = Some("rok.sendRequest({});".into());
        let (events, notes) = run(&r, false);
        assert_eq!(events[0]["script"]["exec"][0], "// rok: not translated");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].path, "users/r");
        assert_eq!(notes[0].action, ExportAction::Skipped);
        assert!(notes[0].feature.contains("rok.sendRequest"));
    }

    #[test]
    fn aliased_script_keeps_text_and_adds_preserved_note() {
        let mut r = request();
        r.pre_request_script = Some("const g = rok.getVar;\ng('a');".into());
        let (events, notes) = run(&r, false);
        assert_eq!(
            events[0]["script"]["exec"],
            json!(["const g = rok.getVar;", "g('a');"])
        );
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].action, ExportAction::Preserved);
    }

    #[test]
    fn unknown_operator_adds_skipped_note_and_disabled_adds_none() {
        let mut r = request();
        let mut off = Assertion::new("res.status", "eq", Some("200".into()));
        off.disabled = Some(true);
        r.assertions = vec![Assertion::new("res.status", "matches", Some("2..".into())), off];
        let (events, notes) = run(&r, false);
        assert!(events.is_empty());
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].action, ExportAction::Skipped);
        assert!(notes[0].feature.contains("matches"));
    }

    #[test]
    fn before_request_action_is_skipped() {
        let mut r = request();
        r.actions = vec![ActionSetVariable {
            phase: "before-request".into(),
            selector: ActionSelector {
                expression: "res.body.id".into(),
                method: "jsonq".into(),
            },
            variable: ActionVariable { name: "n".into(), scope: "runtime".into() },
            disabled: None,
            description: None,
        }];
        let (events, notes) = run(&r, false);
        assert!(events.is_empty());
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].action, ExportAction::Skipped);
    }

    #[test]
    fn after_response_action_lands_in_test_event() {
        let mut r = request();
        r.actions = vec![ActionSetVariable {
            phase: "after-response".into(),
            selector: ActionSelector {
                expression: "res.body.id".into(),
                method: "jsonq".into(),
            },
            variable: ActionVariable { name: "n".into(), scope: "collection".into() },
            disabled: None,
            description: None,
        }];
        let (events, _) = run(&r, false);
        assert_eq!(events[0]["listen"], "test");
        assert_eq!(
            events[0]["script"]["exec"],
            json!(["pm.collectionVariables.set(\"n\", pm.response.json().id);"])
        );
    }

    #[test]
    fn folder_events_use_folder_scripts() {
        let settings = FolderSettings {
            pre_request_script: Some("rok.setVar('a', 1);".into()),
            tests_script: Some("test('t', () => {});".into()),
            ..Default::default()
        };
        let mut notes = Vec::new();
        let events = folder_events(&settings, &PostmanOptions::default(), "users", &mut notes);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["listen"], "prerequest");
        assert_eq!(events[1]["listen"], "test");
        assert_eq!(events[1]["script"]["exec"], json!(["pm.test('t', () => {});"]));
    }
}
