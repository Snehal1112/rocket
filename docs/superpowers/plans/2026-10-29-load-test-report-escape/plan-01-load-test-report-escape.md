# Escape the load-test HTML report Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** Make the exported load-test HTML report safe to open when a request error message contains markup or script.

**Architecture:** One private function escapes the JSON that `to_html` embeds in the inline script block. The table rows move from an `innerHTML +=` template to DOM calls with `textContent`. Everything stays in one file.

**Tech Stack:** Rust (`rocket-app`), `serde_json`, an inline JavaScript template inside a `format!` raw string.

**Spec:** `docs/superpowers/specs/2026-10-29-load-test-report-escape-design.md`

## Global Constraints

- Touch only `crates/rocket-app/src/export_service.rs`. Anything under `src-tauri/**` parks the job for a human.
- No formatting, clippy or import-order edits elsewhere in the file or the repo. Do not change the CSV, JSON or PDF behaviour.
- Cargo commands are per crate and carry `-j4`: `cargo check -j4 -p rocket-app` and `cargo test -j4 -p rocket-app export_service`. Never a bare `cargo check -j4` or `cargo test -j4`.
- No production unwrap calls. Tests use `.expect("...")`.
- The JavaScript lives in a `format!` raw string, so every literal `{` and `}` in it is written `{{` and `}}`.
- In Rust source U+2028 is written `"\u{2028}"` and the replacement text `"\\u2028"`, because `"\u2028"` does not compile. The same rule holds in tests.
- The job does not touch collection or environment data, so no task starts with the opencollection reference line.

## Review Focus

1. An error with `<!--` and `a&b` next to `</script>`. The user expects inert text. Pinned by `html_embedded_json_has_no_raw_markup_characters`.
2. An error with U+2028 and U+2029. The user expects a valid script. Pinned by `html_escapes_line_separators_and_ampersands`.
3. Non-ASCII error text such as `接続エラー`. The user expects it unchanged. Pinned by `html_keeps_non_ascii_error_text_readable`.
4. An empty request log. The user expects an empty table and a working page. Pinned by `html_with_an_empty_log_embeds_an_empty_array`.
5. The time-series JSON goes through the same escape. Pinned by the `const ts = ` half of `html_embedded_json_has_no_raw_markup_characters`.

## File Structure

- Modify `crates/rocket-app/src/export_service.rs`: add the free private function `script_safe_json` above `impl ExportService`, use it in `to_html`, rewrite the `log.forEach` block of the template, and add tests to the existing `mod tests`.

---

### Task 1: Escape the embedded JSON

**Files:**
- Modify: `crates/rocket-app/src/export_service.rs` (function above `impl ExportService`, the two lines in `to_html` that build `ts_json` and `log_json`, and `mod tests`)

**Interfaces:**
- Consumes: `serde_json::to_string`, the existing `LoadTestResult`, `RequestLogEntry` and `minimal_result()` fixture.
- Produces: `fn script_safe_json<T: serde::Serialize>(value: &T) -> String`. It returns the JSON of `value` with `<`, `>`, `&`, U+2028 and U+2029 replaced by `\\u003c`, `\\u003e`, `\\u0026`, `\\u2028` and `\\u2029` (Rust string literals; the page sees a backslash-u escape). On a serialisation error it returns an empty string.

- [ ] **Step 1: Read the code**

Read `crates/rocket-app/src/export_service.rs` end to end: `to_html` (the `ts_json` and `log_json` lines, the `const ts =` and `const log =` lines of the template) and `mod tests` (`minimal_result`, `csv_escapes_problematic_error_messages`).

- [ ] **Step 2: Write the failing tests**

Add two helpers and the tests to `mod tests`. The helpers:

```rust
const HOSTILE: &str = "x</script><img src=x onerror=alert(1)><!-- a&b";

fn result_with_error(error: &str) -> LoadTestResult {
    let mut result = minimal_result();
    result.request_log = vec![RequestLogEntry {
        seq: 0,
        status: None,
        latency_ms: 0.0,
        response_bytes: 0,
        error: Some(error.into()),
        phase_index: 0,
    }];
    result
}

fn html_of(result: &LoadTestResult) -> String {
    let (bytes, _) = ExportService::export(result, ExportFormat::Html).expect("html export");
    String::from_utf8(bytes).expect("utf8 html")
}

fn script_line<'a>(html: &'a str, prefix: &str) -> &'a str {
    html.lines()
        .find(|l| l.starts_with(prefix))
        .expect("script line present")
}
```

One full test, the others are listed in the table:

```rust
#[test]
fn html_escapes_script_close_in_error_text() {
    let html = html_of(&result_with_error(HOSTILE));
    assert!(!html.contains("</script><img"));
    assert!(!html.contains("onerror=alert(1)>"));
    assert!(html.contains("\\u003c/script\\u003e"));
}
```

| Test | Setup | Assertions |
|---|---|---|
| `html_keeps_the_template_script_tags_when_errors_contain_markup` | `html_of(&result_with_error(HOSTILE))` and `html_of(&minimal_result())` | `matches("</script>").count()` is equal for both |
| `html_escapes_line_separators_and_ampersands` | error `"a\u{2028}b\u{2029}c&d"` | HTML contains `"\\u2028"`, `"\\u2029"` and `"c\\u0026d"`; does not contain `'\u{2028}'` or `'\u{2029}'` |
| `html_embedded_json_has_no_raw_markup_characters` | `HOSTILE` plus U+2028 and U+2029 appended | `script_line(&html, "const ts = ")` and `script_line(&html, "const log = ")` contain none of `<`, `>`, `&`, `'\u{2028}'`, `'\u{2029}'` |
| `html_embedded_log_round_trips_the_error_text` | `HOSTILE` | take the `const log = ` line, strip the prefix and the trailing `;`, parse with `serde_json::from_str::<serde_json::Value>(...)`, assert `value[0]["error"] == HOSTILE` |
| `html_keeps_non_ascii_error_text_readable` | error `"接続エラー"` | HTML contains `"接続エラー"` |
| `html_with_an_empty_log_embeds_an_empty_array` | `minimal_result()` with `request_log = vec![]` | `script_line(&html, "const log = ")` equals `"const log = [];"` |

- [ ] **Step 3: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-app export_service`
Expected: the new tests compile. Four fail by assertion (`html_escapes_script_close_in_error_text`, `html_keeps_the_template_script_tags_when_errors_contain_markup`, `html_escapes_line_separators_and_ampersands`, `html_embedded_json_has_no_raw_markup_characters`). Three pass because they also hold on the broken code (`html_embedded_log_round_trips_the_error_text`, `html_keeps_non_ascii_error_text_readable`, `html_with_an_empty_log_embeds_an_empty_array`). The six existing tests pass.

- [ ] **Step 4: Implement `script_safe_json` and use it**

Add `fn script_safe_json<T: serde::Serialize>(value: &T) -> String` as a free private function above `impl ExportService`. It serialises with `serde_json::to_string(value).unwrap_or_default()` and then chains the five `str::replace` calls. Replace the two `serde_json::to_string(...).unwrap_or_default()` lines in `to_html` with `script_safe_json(&result.time_series)` and `script_safe_json(&result.request_log)`. Add a one-line doc comment as a full sentence.

- [ ] **Step 5: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-app export_service` and `cargo check -j4 -p rocket-app`
Expected: 13 tests pass (6 existing, 7 new) and the check is clean.

---

### Task 2: Build the log table with DOM calls

**Files:**
- Modify: `crates/rocket-app/src/export_service.rs` (the `const tbody` and `log.forEach` block of the template, and `mod tests`)

**Interfaces:**
- Consumes: the helpers `HOSTILE`, `result_with_error`, `html_of` and `script_line` from Task 1.
- Produces: a page whose script contains no `innerHTML`, and a `cell(tr, text, cls)` JavaScript function inside the template.

- [ ] **Step 1: Read the code**

Read the `const tbody = document.getElementById('log-body');` line and the `log.forEach` block in the template of `to_html`.

- [ ] **Step 2: Write the failing tests**

```rust
#[test]
fn html_builds_log_rows_without_inner_html() {
    let html = html_of(&minimal_result());
    assert!(!html.contains("innerHTML"));
    assert!(html.contains("textContent"));
}
```

`html_keeps_status_class_and_latency_format`: build `html_of(&minimal_result())` and assert it contains `"'ERR'"`, `"toFixed(1)"` and `"'ok' : 'err'"`.

- [ ] **Step 3: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-app export_service`
Expected: `html_builds_log_rows_without_inner_html` fails because the template still contains `innerHTML`. `html_keeps_status_class_and_latency_format` passes, because the current template already has those three strings. The 13 tests from Task 1 pass.

- [ ] **Step 4: Replace the row-building block**

In the template, keep `const tbody = document.getElementById('log-body');` and replace the `log.forEach` block with this code (braces doubled for `format!`):

```
function cell(tr, text, cls) {{
  const td = document.createElement('td');
  td.textContent = String(text);
  if (cls) td.className = cls;
  tr.appendChild(td);
}}
log.forEach(e => {{
  const ok = e.status !== null && e.status < 400;
  const tr = document.createElement('tr');
  cell(tr, e.seq);
  cell(tr, e.status ?? 'ERR', ok ? 'ok' : 'err');
  cell(tr, e.latencyMs.toFixed(1));
  cell(tr, e.responseBytes);
  cell(tr, e.error ?? '');
  tbody.appendChild(tr);
}});
```

- [ ] **Step 5: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-app export_service` and `cargo check -j4 -p rocket-app`
Expected: 15 tests pass (6 existing, 9 new) and the check is clean. Run `git diff --stat` and confirm only `crates/rocket-app/src/export_service.rs` changed.

Next plan: none
