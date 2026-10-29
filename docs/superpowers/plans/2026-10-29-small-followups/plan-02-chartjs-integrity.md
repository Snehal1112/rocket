# Pin Chart.js in the load-test report Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The exported load-test report only runs the exact Chart.js file it was built for, even though it loads from a CDN.

**Architecture:** Two constants hold the CDN URL and its integrity hash. The `<script>` tag in the report template uses both, with `crossorigin="anonymous"`. A browser then refuses a changed file.

**Tech Stack:** Rust (`rocket-app`), Subresource Integrity (SRI).

**Spec:** GitHub issue #128 (https://github.com/Snehal1112/rocket/issues/128), a follow-up of `docs/superpowers/specs/2026-10-29-load-test-report-escape-design.md`. The ticket is one line, so the decisions below are this plan's own calls.

## Global Constraints

- Touch only `crates/rocket-app/src/export_service.rs`. Nothing under `src-tauri/**`.
- No formatting, clippy or import-order edits elsewhere.
- Cargo commands carry `-j4` and `-p rocket-app`.
- The JavaScript and CSS in the template live in a `format!` raw string, so literal braces stay doubled. The new tag has no braces; the new values are passed as named `format!` arguments.
- The report keeps loading Chart.js 4.4.1 from `https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.1/chart.umd.min.js`. No version change.

## Decisions (this plan's calls)

- The ticket allows "pin with an integrity hash, or inline it". This plan pins. Inlining adds about 200 KB to every report, and the repo has no vendored copy (`package.json` has `recharts`, not `chart.js`).
- The hash is the `sha512` value that cdnjs publishes for this file: `sha512-CQBWl4fJHWbryGE+Pc7UAxWMUMNMWzWxF4SQo9CgkJIN1kx6djDQZjh3Y8SZ1d+6I+1zze6Z7kHXO7q3UyZAWw==`. It was checked on 2026-10-29: the file from the URL is 200,807 bytes and its sha512 equals the cdnjs API value (`https://api.cdnjs.com/libraries/Chart.js/4.4.1?fields=sri`).
- SRI needs a CORS request. `crossorigin="anonymous"` is required, and cdnjs answers with `access-control-allow-origin: *`, so the report still loads from `file://`.
- A report opened offline still draws no charts, as today. The table and totals still render.
- A test cannot reach the network, so tests pin the constants and the tag. Re-checking the hash against the live file is a manual step in this plan.

## Review Focus

1. The tag has the integrity attribute and `crossorigin`. Without `crossorigin` a browser ignores the hash. Pinned by `html_pins_chart_js_with_integrity_and_crossorigin`.
2. The URL and hash stay together if the version is bumped. Pinned by `chart_js_constants_name_the_same_version`.
3. Only one external script is loaded. Pinned by `html_loads_exactly_one_external_script`.
4. The page still contains the inline script and numbers from the job 046 escape fix (no regression). Pinned by the existing `html_contains_chart_js_and_kpis` test, which must keep passing.
5. The hash is the right one for the file. Not testable offline; covered by the manual step in Task 1, Step 5.

## File Structure

- Modify `crates/rocket-app/src/export_service.rs`: add `CHART_JS_URL` and `CHART_JS_SRI` constants, use them in the `<script src=...>` tag of `to_html`, add tests.

---

### Task 1: Pin the Chart.js script tag

**Files:**
- Modify: `crates/rocket-app/src/export_service.rs` (constants above `impl ExportService`, the template tag, the `format!` arguments, `mod tests`)

**Interfaces:**
- Consumes: the test helper `html_of(result: &LoadTestResult) -> String` and `minimal_result()`.
- Produces: `const CHART_JS_URL: &str` and `const CHART_JS_SRI: &str` (module-private).

- [ ] **Step 1: Read the code**

Read the `<script src="https://cdnjs...chart.umd.min.js"></script>` line in `to_html`, its `format!` argument list at the end of the function, and the test `html_contains_chart_js_and_kpis`.

- [ ] **Step 2: Write the failing tests**

```rust
#[test]
fn html_pins_chart_js_with_integrity_and_crossorigin() {
    let html = html_of(&minimal_result());
    let tag = html
        .lines()
        .find(|l| l.contains("chart.umd.min.js"))
        .expect("chart script tag");
    assert!(tag.contains(&format!("src=\"{CHART_JS_URL}\"")));
    assert!(tag.contains(&format!("integrity=\"{CHART_JS_SRI}\"")));
    assert!(tag.contains("crossorigin=\"anonymous\""));
}
```

| Test | Setup | Assertions |
|---|---|---|
| `chart_js_constants_name_the_same_version` | none | `CHART_JS_URL` contains `/Chart.js/4.4.1/`; `CHART_JS_SRI` starts with `sha512-` and has length 95 |
| `html_loads_exactly_one_external_script` | `html_of(&minimal_result())` | `html.matches("<script src=").count() == 1` |

- [ ] **Step 3: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-app export_service`
Expected: the test module fails to compile with `cannot find value 'CHART_JS_URL' in this scope` (the constants do not exist yet).

- [ ] **Step 4: Implement the constants and the tag**

Add above `impl ExportService`:

```rust
const CHART_JS_URL: &str = "https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.1/chart.umd.min.js";
const CHART_JS_SRI: &str = "sha512-CQBWl4fJHWbryGE+Pc7UAxWMUMNMWzWxF4SQo9CgkJIN1kx6djDQZjh3Y8SZ1d+6I+1zze6Z7kHXO7q3UyZAWw==";
```

In the template replace the tag with `<script src="{chart_url}" integrity="{chart_sri}" crossorigin="anonymous"></script>` and add `chart_url = CHART_JS_URL, chart_sri = CHART_JS_SRI,` to the `format!` arguments. Add a doc comment on the constants as full sentences, saying that the hash must change with the version.

- [ ] **Step 5: Run the tests, then check the hash by hand**

Run: `cargo test -j4 -p rocket-app export_service` and `cargo check -j4 -p rocket-app`
Expected: all tests pass (the previous count plus 3 new). `html_contains_chart_js_and_kpis` still passes.

Then, with network access, run `curl -s https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.1/chart.umd.min.js | openssl dgst -sha512 -binary | openssl base64 -A`. Expected output: `CQBWl4fJHWbryGE+Pc7UAxWMUMNMWzWxF4SQo9CgkJIN1kx6djDQZjh3Y8SZ1d+6I+1zze6Z7kHXO7q3UyZAWw==`. If there is no network, say so in the final report instead of skipping the step silently. Run `git diff --stat` and confirm only `crates/rocket-app/src/export_service.rs` changed.

- [ ] **Step 6: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill, path-scoped: `git commit --only -- crates/rocket-app/src/export_service.rs`. Subject: `fix(export): pin Chart.js with an integrity hash`. Footer: `Fixes: #128`.

Next plan: docs/superpowers/plans/2026-10-29-small-followups/plan-03-pdf-leftovers.md
