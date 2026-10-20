//! Size caps for live event payloads sent to the webview.
//!
//! These caps apply only to the event. History, stored conversations and the
//! value a command returns keep the full data. A cut is always marked, so
//! the user can tell the text is not complete.

/// The largest text one streamed message event carries, in bytes. Used for
/// gRPC stream messages, GraphQL subscription results and WebSocket frames.
pub const MAX_STREAM_MESSAGE_BYTES: usize = 256 * 1024;

/// The most console entries one `ConsoleOutput` event carries.
pub const MAX_CONSOLE_ENTRIES: usize = 1_000;

/// The most test results one `TestsCompleted` event carries.
pub const MAX_TEST_RESULTS: usize = 1_000;

/// The largest string one console entry or test result field carries, in bytes.
pub const MAX_ENTRY_TEXT_BYTES: usize = 16 * 1024;

/// The text added after a cut. `{}` is the number of bytes left out.
fn marker(cut_bytes: usize) -> String {
    format!("...[truncated {cut_bytes} bytes]")
}

/// Cuts `text` to at most `limit` bytes on a character boundary and adds a
/// marker that says how many bytes were left out. Text under the limit is
/// returned unchanged. Returns the text and whether it was cut. The marker
/// comes on top of the limit, so the result can be up to 40 bytes longer.
pub fn cap_with_marker(text: String, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let mut cut = limit;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let left_out = text.len() - cut;
    let mut out = text;
    out.truncate(cut);
    out.push_str(&marker(left_out));
    (out, true)
}

/// Caps every string in a JSON value at `MAX_ENTRY_TEXT_BYTES`, one level
/// deep in objects. Console entries and test results are flat objects.
fn cap_entry_strings(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(text) => {
            let (capped, _) = cap_with_marker(std::mem::take(text), MAX_ENTRY_TEXT_BYTES);
            *text = capped;
        }
        serde_json::Value::Object(map) => {
            for field in map.values_mut() {
                if let serde_json::Value::String(text) = field {
                    let (capped, _) = cap_with_marker(std::mem::take(text), MAX_ENTRY_TEXT_BYTES);
                    *text = capped;
                }
            }
        }
        _ => {}
    }
}

/// Caps console entries for the `ConsoleOutput` event: each string at
/// `MAX_ENTRY_TEXT_BYTES`, and at most `MAX_CONSOLE_ENTRIES` entries. When
/// entries are left out, the last kept entry is a `warn` that says how many.
pub fn cap_console_entries(mut entries: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    if entries.len() > MAX_CONSOLE_ENTRIES {
        let kept = MAX_CONSOLE_ENTRIES - 1;
        let left_out = entries.len() - kept;
        entries.truncate(kept);
        entries.push(serde_json::json!({
            "level": "warn",
            "message": format!("...[{left_out} more console entries truncated]"),
        }));
    }
    for entry in &mut entries {
        cap_entry_strings(entry);
    }
    entries
}

/// Caps test results for the `TestsCompleted` event: each string at
/// `MAX_ENTRY_TEXT_BYTES`, and at most `MAX_TEST_RESULTS` results. When
/// results are left out, the last kept result has the status `truncated`
/// and a name that says how many.
pub fn cap_test_results(mut results: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    if results.len() > MAX_TEST_RESULTS {
        let kept = MAX_TEST_RESULTS - 1;
        let left_out = results.len() - kept;
        results.truncate(kept);
        results.push(serde_json::json!({
            "name": format!("...[{left_out} more test results truncated]"),
            "status": "truncated",
            "error": null,
        }));
    }
    for result in &mut results {
        cap_entry_strings(result);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_with_marker_leaves_text_under_the_limit_alone() {
        assert_eq!(cap_with_marker("abc".into(), 3), ("abc".to_string(), false));
        assert_eq!(cap_with_marker(String::new(), 0), (String::new(), false));
    }

    #[test]
    fn cap_with_marker_cuts_and_says_how_much_was_left_out() {
        let (text, cut) = cap_with_marker("abcdefghij".into(), 4);
        assert!(cut);
        assert_eq!(text, "abcd...[truncated 6 bytes]");
    }

    #[test]
    fn cap_with_marker_cuts_on_a_character_boundary() {
        // "é" is two bytes, so a limit of 2 falls inside it.
        let (text, cut) = cap_with_marker("aé€b".into(), 2);
        assert!(cut);
        assert_eq!(text, format!("a{}", marker(6)));
        let (text, _) = cap_with_marker("€€".into(), 4);
        assert_eq!(text, format!("€{}", marker(3)));
    }

    #[test]
    fn console_entries_are_capped_in_count_and_size() {
        let mut entries: Vec<serde_json::Value> = (0..MAX_CONSOLE_ENTRIES + 5)
            .map(|i| serde_json::json!({ "level": "log", "message": format!("line {i}") }))
            .collect();
        entries[0]["message"] = serde_json::json!("x".repeat(MAX_ENTRY_TEXT_BYTES + 10));
        let capped = cap_console_entries(entries);
        assert_eq!(capped.len(), MAX_CONSOLE_ENTRIES);
        let first = capped[0]["message"].as_str().unwrap_or_default();
        assert!(
            first.ends_with(&marker(10)),
            "{}",
            &first[first.len() - 30..]
        );
        let last = &capped[MAX_CONSOLE_ENTRIES - 1];
        assert_eq!(last["level"], "warn");
        assert_eq!(last["message"], "...[6 more console entries truncated]");
        assert_eq!(capped[MAX_CONSOLE_ENTRIES - 2]["message"], "line 998");
    }

    #[test]
    fn console_entries_under_the_caps_are_unchanged() {
        let entries = vec![serde_json::json!({ "level": "log", "message": "hi" })];
        assert_eq!(cap_console_entries(entries.clone()), entries);
    }

    #[test]
    fn test_results_are_capped_in_count_and_size() {
        let mut results: Vec<serde_json::Value> = (0..MAX_TEST_RESULTS + 1)
            .map(|i| serde_json::json!({ "name": format!("t{i}"), "status": "passed", "error": null }))
            .collect();
        results[1]["error"] = serde_json::json!("e".repeat(MAX_ENTRY_TEXT_BYTES + 1));
        let capped = cap_test_results(results);
        assert_eq!(capped.len(), MAX_TEST_RESULTS);
        assert!(capped[1]["error"]
            .as_str()
            .is_some_and(|e| e.ends_with(&marker(1))));
        assert!(capped[1]["status"] == "passed" && capped[0]["error"].is_null());
        let last = &capped[MAX_TEST_RESULTS - 1];
        assert_eq!(last["status"], "truncated");
        assert_eq!(last["name"], "...[2 more test results truncated]");
    }
}
