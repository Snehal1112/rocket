# Neutralise CSV formula cells Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A load-test CSV export never lets a spreadsheet run a formula that came from a request error message.

**Architecture:** A private function prefixes a single quote to any error text that starts with a formula character, before the existing RFC 4180 quoting. Only the `error` column holds free text, so only that column changes.

**Tech Stack:** Rust (`rocket-app`).

**Spec:** GitHub issue #127 (https://github.com/Snehal1112/rocket/issues/127), a follow-up of `docs/superpowers/specs/2026-10-29-load-test-report-escape-design.md`. The ticket is one line, so the decisions below are this plan's own calls.

## Global Constraints

- Touch only `crates/rocket-app/src/export_service.rs`. Nothing under `src-tauri/**`.
- No formatting, clippy or import-order edits elsewhere in the file or the repo.
- Cargo commands carry `-j4` and `-p rocket-app`: `cargo test -j4 -p rocket-app export_service` and `cargo check -j4 -p rocket-app`. Never a bare workspace run.
- No production unwrap calls. New tests use `.expect("...")`.
- The CSV header and every non-error column stay byte-for-byte as they are. JSON and HTML output do not change.

## Decisions (this plan's calls)

- Characters that trigger the prefix: `=`, `+`, `-`, `@` (the ticket) plus tab and carriage return (the OWASP list, same attack).
- The neutraliser is a single quote prefix. A spreadsheet shows the text and hides the quote, and it is the usual fix.
- The prefix is added before `csv_quote`, so a cell that needs quoting becomes `"'=a,b"`.
- Only the first character is checked. `x=1` is left alone.

## Review Focus

1. Error text starting with `=`, `+`, `-` or `@`: the user expects a leading quote. Pinned by `csv_prefixes_a_quote_to_error_text_that_starts_with_a_formula_character`.
2. Error text starting with a tab or a carriage return: same. Pinned by `csv_prefixes_a_quote_to_error_text_that_starts_with_tab_or_carriage_return`.
3. A formula character in the middle (`x=1`) or ordinary text: left alone. Pinned by `csv_leaves_ordinary_error_text_unchanged`.
4. A formula that also needs quoting (`=a,b`): quote first, then RFC 4180. Pinned by `csv_prefixes_before_quoting`.
5. No error (`None`) or empty text: the cell stays empty. Pinned by `csv_keeps_an_empty_error_cell_empty`.

## File Structure

- Modify `crates/rocket-app/src/export_service.rs`: add `fn neutralise_formula(s: &str) -> String` next to `csv_quote`, call it in `to_csv`, add tests to the existing `mod tests`.

---

### Task 1: Prefix formula-looking error text

**Files:**
- Modify: `crates/rocket-app/src/export_service.rs` (`to_csv`, a new private function, `mod tests`)

**Interfaces:**
- Consumes: the existing test helper `result_with_error(error: &str) -> LoadTestResult` (one log entry with `seq 0`, `status None`, latency `0.0`, `response_bytes 0`, `phase_index 0`) and `minimal_result()`.
- Produces: `fn neutralise_formula(s: &str) -> String` (an associated function of `ExportService`, like `csv_quote`).

- [ ] **Step 1: Read the code**

Read `to_csv`, `csv_quote` and the tests `csv_has_header_and_rows` and `csv_escapes_problematic_error_messages` in `crates/rocket-app/src/export_service.rs`. The row format is `{seq},{status},{latency:.2},{bytes},{error},{phase}`, so `result_with_error` gives the second line `0,,0.00,0,<error cell>,0`.

- [ ] **Step 2: Write the failing tests**

Add a helper and one full test. The others are in the table.

```rust
fn csv_of(result: &LoadTestResult) -> String {
    let (bytes, ext) = ExportService::export(result, ExportFormat::Csv).expect("csv export");
    assert_eq!(ext, "csv");
    String::from_utf8(bytes).expect("utf8 csv")
}

#[test]
fn csv_prefixes_a_quote_to_error_text_that_starts_with_a_formula_character() {
    for (input, cell) in [
        ("=1+1", "'=1+1"),
        ("+cmd", "'+cmd"),
        ("-2+3", "'-2+3"),
        ("@SUM(A1)", "'@SUM(A1)"),
    ] {
        let csv = csv_of(&result_with_error(input));
        assert_eq!(csv.lines().nth(1), Some(format!("0,,0.00,0,{cell},0").as_str()), "{input}");
    }
}
```

| Test | Input error | Assertion |
|---|---|---|
| `csv_prefixes_a_quote_to_error_text_that_starts_with_tab_or_carriage_return` | `"\tx"` and `"\rx"` | `csv.contains("0,,0.00,0,'\tx,0")` (a tab is not quoted by `csv_quote`) and `csv.contains("0,,0.00,0,\"'\rx\",0")` (a carriage return forces RFC 4180 quoting) |
| `csv_leaves_ordinary_error_text_unchanged` | `"x=1"`, `"connection refused"`, `"a-b"` | line 2 equals `0,,0.00,0,<input>,0` |
| `csv_prefixes_before_quoting` | `"=a,b"` | line 2 equals `0,,0.00,0,"'=a,b",0` |
| `csv_keeps_an_empty_error_cell_empty` | `""` and a log entry with `error: None` | line 2 equals `0,,0.00,0,,0` for both |

- [ ] **Step 3: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-app export_service`
Expected: the four prefix and quoting tests fail by assertion (no leading quote today). `csv_leaves_ordinary_error_text_unchanged` and `csv_keeps_an_empty_error_cell_empty` pass, because they describe today's behaviour. All existing tests pass.

- [ ] **Step 4: Implement `fn neutralise_formula(s: &str) -> String`**

An associated function in `impl ExportService`, above `csv_quote`. It returns `format!("'{s}")` when `s` starts with one of `=`, `+`, `-`, `@`, `\t`, `\r`, and `s.to_string()` otherwise. Add a one-line doc comment as a full sentence. In `to_csv` change the error line to `Self::csv_quote(&Self::neutralise_formula(entry.error.as_deref().unwrap_or("")))`.

- [ ] **Step 5: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-app export_service` and `cargo check -j4 -p rocket-app`
Expected: all tests in `export_service` pass (the previous count plus 5 new) and the check is clean. Run `git diff --stat` and confirm only `crates/rocket-app/src/export_service.rs` changed.

- [ ] **Step 6: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill, path-scoped: `git commit --only -- crates/rocket-app/src/export_service.rs`. Subject: `fix(export): neutralise formula cells in load-test CSV`. Footer: `Fixes: #127`.

Next plan: docs/superpowers/plans/2026-10-29-small-followups/plan-02-chartjs-integrity.md
