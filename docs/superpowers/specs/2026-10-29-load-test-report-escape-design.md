# Escape the load-test HTML report

Ticket: https://github.com/Snehal1112/rocket/issues/117

## Goal

Make the exported load-test HTML report safe to open in a browser, even when a request error message contains markup or script, by escaping the data it embeds and by building the log table without `innerHTML`.

## Overview

`ExportService::to_html` builds a standalone report page. Two places put request-log text into the page without escaping:

- The request log is serialised with `serde_json::to_string` and pasted into an inline `<script>` block (`const log = {log_json};`). serde_json leaves `<` as it is, so an error string that contains `</script>` closes the block, and the text after it is parsed as HTML.
- The table rows are built with `tbody.innerHTML += ... ${e.error ?? ''}`, so an error string such as `<img src=x onerror=...>` becomes live markup.

Evidence that attacker text can reach the field (traced in source, not run):

- `RequestLogEntry.error` is the only string field that reaches the page. Every other value is a number (`crates/rocket-shared/src/load_test.rs:27-47`, `59-79`).
- It is set from `e.to_string()` of the executor error at `crates/rocket-http/src/load_test.rs:765` and `:947`. Nothing truncates or escapes it.
- The OAuth2 token endpoint's response body is copied into the error verbatim (`crates/rocket-infra/src/reqwest_executor.rs:1418`). A hostile token server controls it. Collection-supplied text (an invalid method or header name, a file path) also appears in error strings.
- A failed status never copies the response body into the log, so that path is clean.

Impact is low. The report is only a downloaded file: the `export_load_test` command returns base64 bytes (`src-tauri/src/commands/load_test.rs:54-61`) and the frontend saves them with a synthetic download link (`src/stores/load-test-store.ts:326-345`). It opens later in a normal browser on `file://`, with no Tauri access. The fix is still cheap, and the repo already grants `pdf-*` windows file and shell permissions in `src-tauri/capabilities/default.json`, so a future print-to-PDF flow would make this worse.

## Current state

| Area | Today | Where |
|---|---|---|
| JSON embedding | `serde_json::to_string` output pasted raw into the script block | `crates/rocket-app/src/export_service.rs:59-60`, `109-110` |
| Table rows | Template string assigned with `innerHTML +=` | `export_service.rs:128-138` |
| Other interpolated values | Numbers only (`total`, `succeeded`, `failed`, `rps`, `p95`, `dur`, `log_count`) | `export_service.rs:142-150` |
| Chart library | Script tag from cdnjs, no integrity hash | `export_service.rs:68` |
| PDF format | The same HTML bytes with the extension `pdf` | `export_service.rs:32` |
| Tests | CSV, JSON, KPI, PDF header and CSV quoting. No HTML escaping test | `export_service.rs:156-253` |
| Fixture | `minimal_result()` builds a `LoadTestResult` with two log entries | `export_service.rs:160` |

Gaps: an error string can break out of the data block, and an error string can become markup in the table.

## Non-goals

- Truncating long error strings.
- Pinning or adding an integrity hash to the Chart.js script tag.
- CSV formula neutralisation (cells that start with `=`, `+`, `-` or `@`).
- Changing the export command, the frontend save path, or the PDF behaviour.
- Removing the stale print-to-PDF comment (`export_service.rs:18-19`) or the unused `pdf-*` capability.

## Design

Flow: `LoadTestResult -> to_html -> escaped JSON in the script block -> DOM-built table rows in the browser`.

### 1. Escape the embedded JSON

Add a private function in `export_service.rs`:

```rust
/// JSON that is safe inside an inline script block.
fn script_safe_json<T: serde::Serialize>(value: &T) -> String
```

It serialises with `serde_json::to_string` and then replaces, in this order, `<` with `\u003c`, `>` with `\u003e`, `&` with `\u0026`, U+2028 with `\u2028` and U+2029 with `\u2029`. The result is still valid JSON, and a JS parser reads the same values back. These characters can only appear inside JSON string values, so keys and numbers are not affected. On a serialisation error it returns an empty string, as the two current `unwrap_or_default()` calls do (the page then has `const log = ;` and the script stops; these types cannot fail to serialise, so this is not tested). In Rust source the characters are written `"\u{2028}"` and the replacement text as `"\\u2028"`, because `"\u2028"` does not compile. Tests follow the same rule. The function is a free private function above `impl ExportService`.

`to_html` uses it for both `ts_json` and `log_json`. Example: the error `x</script><img src=x onerror=alert(1)>` is embedded as `x\u003c/script\u003e\u003cimg src=x onerror=alert(1)\u003e`.

### 2. Build the table with DOM calls

Replace the `log.forEach` block (`export_service.rs:129-138`) so that each row is created with `document.createElement('tr')`, and each cell is a `td` whose text is set with `textContent`. The status cell keeps the `ok` or `err` class (`e.status !== null && e.status < 400`, shown as `e.status ?? 'ERR'`). Latency keeps `toFixed(1)`, the error cell shows `e.error ?? ''`. The loop appends the finished row with `tbody.appendChild(tr)`. The page no longer contains the text `innerHTML`.

The template is a `format!` raw string, so literal braces in the JavaScript stay doubled (`{{` and `}}`).

## Error handling and edge cases

| Case | Behaviour |
|---|---|
| Error contains `</script>` | Embedded as `\u003c/script\u003e`. The block does not end early |
| Error contains `<!--` or `<script` | The `<` is escaped, so the HTML parser sees no tag or comment start |
| Error contains `&`, quotes or `>` | Escaped or inert text. The table shows the original characters |
| Error contains U+2028 or U+2029 | Escaped in the JSON. Shown as the original character in the table |
| Error is `None` | The cell is empty, as before |
| Status is `None` | The cell shows `ERR` with the `err` class, as before |
| Empty log | The table body stays empty. The page still builds |
| Very long error | Embedded as is. No truncation |
| Serialisation fails | The script gets an empty value, as today (cannot happen for these types) |
| Non-ASCII text (for example Japanese) | Unchanged in the JSON, because only the five characters above are replaced |

## Testing

All new tests go in the existing `mod tests` of `crates/rocket-app/src/export_service.rs`. They build the input from `minimal_result()` and a `RequestLogEntry` like the one in `csv_escapes_problematic_error_messages`, use `.expect("...")`, and are named as plain sentences in snake case.

- `html_escapes_script_close_in_error_text`: the error is `x</script><img src=x onerror=alert(1)>`. Assert the HTML does not contain `</script><img`, does not contain `onerror=alert(1)>`, and contains `\u003c/script\u003e`.
- `html_keeps_the_template_script_tags_when_errors_contain_markup`: count `</script>` in the HTML for an entry with a hostile error and in the HTML for an entry with `error: None`. The two counts are equal.
- `html_embedded_json_has_no_raw_markup_characters`: build the HTML with the hostile entry, take the lines that start with `const ts = ` and `const log = `, and assert neither contains `<`, `>`, `&`, U+2028 or U+2029.
- `html_keeps_status_class_and_latency_format`: assert the HTML contains `'ERR'`, `toFixed(1)` and `'ok' : 'err'`, so the row rewrite keeps them.
- `html_embedded_log_round_trips_the_error_text` (a guard against over-escaping; it also passes before the fix): find the line that starts with `const log = `, strip the prefix and the trailing semicolon, parse it with `serde_json::from_str::<serde_json::Value>`, and assert `[0]["error"]` equals the original hostile string.
- `html_escapes_line_separators_and_ampersands`: the error holds U+2028, U+2029 and `a&b`. Assert the HTML contains `\u2028`, `\u2029` and `a\u0026b`, and does not contain the raw U+2028 or U+2029 characters.
- `html_builds_log_rows_without_inner_html`: assert the HTML does not contain `innerHTML` and contains `textContent`.
- `html_keeps_non_ascii_error_text_readable` (a regression guard; it also passes before the fix): the error is `接続エラー`. Assert the HTML contains it unchanged.
- Existing tests `html_contains_chart_js_and_kpis`, `pdf_returns_html_bytes`, `csv_escapes_problematic_error_messages`, `csv_has_header_and_rows` and `json_is_valid_and_contains_total` keep passing.

Commands: `cargo test -j4 -p rocket-app export_service` and `cargo check -j4 -p rocket-app`.

## Files changed

- `crates/rocket-app/src/export_service.rs` (the escape function, the template, the tests).

## Constraints

- Touch only `crates/rocket-app/src/export_service.rs`. Anything under `src-tauri/**`, including `capabilities/default.json` and the export command, matches a risk rule and would park the job for a human.
- No formatting, clippy or import-order edits elsewhere. Do not change the CSV or JSON output.
- No production unwrap calls. Tests use `.expect("...")`.
- Cargo commands are per crate and carry `-j4`.
- The job does not touch collection or environment data, so no task needs the opencollection spec reference line.

## Acceptance criteria

- An error text containing `</script>` cannot end the inline script block in the exported HTML.
- The embedded JSON contains no raw `<`, `>`, `&`, U+2028 or U+2029, and it parses back to the original error text.
- The request-log table is built with DOM calls and `textContent`. The page contains no `innerHTML`.
- The status class, the `ERR` label, the latency format and the empty error cell behave as before.
- Non-ASCII error text stays readable in the file.
- The CSV and JSON outputs are byte-for-byte unchanged, and the PDF format still returns the HTML bytes with the extension `pdf`.
- `cargo test -j4 -p rocket-app export_service` and `cargo check -j4 -p rocket-app` pass.
- The diff touches only `crates/rocket-app/src/export_service.rs`.

## Out of scope

- Everything under Non-goals.
- Any change under `src-tauri/**`.

## Follow-ups

- Remove the stale print-to-PDF comment in `export_service.rs` and the unused `pdf-*` window entry in `src-tauri/capabilities/default.json`, or build the PDF flow with a locked-down window.
- Neutralise CSV cells that start with `=`, `+`, `-` or `@`.
- Pin the Chart.js script with an integrity hash, or inline it, so the report does not depend on a CDN.

## Decisions

- How does the table show error text? Options: DOM calls with textContent; an escapeHtml helper; escape in Rust. Chosen: DOM calls with textContent. Decided by: user
- How is the embedded JSON made safe? Options: replace `<`, `>`, `&`, U+2028, U+2029; replace only `<`; base64. Chosen: replace the five characters. Decided by: user
- How do the tests prove it? Options: string checks plus a round trip; string checks only; a headless browser. Chosen: string checks plus a round trip. Decided by: user
- Is the severity raised or lowered by the trace? Chosen: stays Low, because the report only opens from a downloaded file in a browser. Decided by: call
- Is the sibling work (PDF comment and capability, CSV formulas, Chart.js integrity) in this job? Chosen: no, filed as follow-ups, because the first touches `src-tauri/**` and would park the job. Decided by: call
- Is a long error truncated? Chosen: no, because escaping makes length harmless. Decided by: call
