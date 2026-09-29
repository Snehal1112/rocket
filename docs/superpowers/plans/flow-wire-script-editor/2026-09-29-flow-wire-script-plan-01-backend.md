# Flow Wire Script Editor — Plan 01: Backend script rule and logs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Flow wire and If/Switch scripts accept one expression or several lines that end with `return value`, and every step reports its script `console.log` output.

**Architecture:** The Rust wrapper embeds the user source as a JSON string literal inside a small JS snippet. The real V8 parser picks the form without running the code: one expression, the same without a trailing `;`, then a script with `return`. The If/Switch coercion is applied to the result. A new logs-returning evaluator feeds `console_entries` into a new `FlowStepResult.logs` / `FlowStepCompleted.logs` field.

**Tech Stack:** Rust (rocket-app, rocket-shared), Deno script engine (rocket-infra, dev-dependency only), serde, tokio tests.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-wire-script-editor-design.md`

## Global Constraints
- 📖 Before starting any task, read `docs/superpowers/specs/opencollection-spec-reference.md` (variable scope) and `crates/rocket-app/CLAUDE.md`.
- `evaluate_var_expression` (Vars tab) keeps its signature and behaviour.
- Existing single-expression wires, If conditions and Switch values must evaluate exactly as before.
- Serde: `#[serde(rename_all = "camelCase")]` only on IPC DTOs; `FlowLogLevel` uses `rename_all = "lowercase"`.
- No unwrap calls in production code. Code comments: short full sentences ending with a period.
- Cargo: always `-j4`; never `cargo test --workspace`. Targeted: `cargo test -j4 -p rocket-app <filter>`, `cargo test -j4 -p rocket-shared <filter>`.
- Commits: use the `dev-workflow-skills:1-git-commit` skill (skip its subagent steps if you cannot dispatch subagents); conventional commits.

## Review Focus
1. Source with double quotes, backticks, `${}` and newlines — must reach the engine intact (JSON-encoded), never break the wrapper.
2. An expression with a trailing `;` (`response.body;`) — must still return its value, not `undefined`.
3. A thrown error after `console.log` — the logs before the throw must still be reported on the failed step.
4. Scripted fake-engine tests keyed on `"!!("` / `"String("` — must keep matching the new wrapper text.
5. An object-literal expression `{ a: 1 }` — must return the object, not run as a block.
6. A runtime `SyntaxError` thrown by user code (e.g. `JSON.parse('x')`) — must surface as that error, never be mistaken for a parse failure that switches form and runs the code twice.

---

### Task 1: Expression-or-return script wrapper with coercion

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`wrap_with_response_object` ~63-80, `resolve_flow_wire_expression` ~91-116, `evaluate_flow_route_expression` ~122-142, If arm ~731-736, Switch arm ~754-760, tests ~1198-1280 and the `real_engine_*` tests ~4041+)

**Interfaces:**
- Produces:
  - `pub enum FlowCoercion { Raw, Bool, Str }` (derive `Debug, Clone, Copy, PartialEq, Eq`).
  - `fn flow_script(source: &str, coercion: FlowCoercion) -> DomainResult<String>` (private).
  - `pub async fn evaluate_flow_route_expression(&self, collection: &str, output: &CapturedOutput, source: &str, coercion: FlowCoercion) -> DomainResult<String>` — callers now pass the RAW condition/value plus a coercion, never `!!(…)`/`String(…)` text.
  - `resolve_flow_wire_expression` keeps its signature and uses `FlowCoercion::Raw`.

- [ ] **Step 1: Write the failing real-engine tests** (append next to the existing `real_engine_*` tests; they use `real_engine_service()` and `sample_response_output()`):

```rust
    #[tokio::test]
    async fn real_engine_wire_runs_a_multi_line_body_with_return() {
        let svc = real_engine_service();
        let output = CapturedOutput::Request(Box::new(sample_response_output()));
        let value = svc
            .resolve_flow_wire_expression(
                "my-api",
                &output,
                "const t = response.body.token;\nreturn 'Bearer ' + t;",
            )
            .await
            .expect("a body with return must run");
        assert!(value.starts_with("Bearer "), "got {value}");
    }

    #[tokio::test]
    async fn real_engine_runtime_syntax_error_is_reported_once() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("x"));
        let err = svc
            .resolve_flow_wire_expression(
                "my-api",
                &output,
                "console.log('ran');\nreturn JSON.parse('not json');",
            )
            .await
            .expect_err("a thrown SyntaxError at run time must fail the wire");
        assert!(err.to_string().contains("SyntaxError"), "got {err}");
    }

    #[tokio::test]
    async fn real_engine_wire_keeps_quotes_backticks_and_newlines() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("x"));
        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "`a\"b` + '\\n' + \"c'd\"")
            .await
            .expect("special characters must survive");
        assert_eq!(value, "a\"b\nc'd");
    }

    #[tokio::test]
    async fn real_engine_wire_object_literal_is_an_expression() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("x"));
        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "{ a: 1 }")
            .await
            .expect("an object literal is an expression");
        assert_eq!(value, r#"{"a":1}"#);
    }

    #[tokio::test]
    async fn real_engine_wire_trailing_semicolon_still_returns_the_value() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("hello"));
        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "response.body;")
            .await
            .expect("a trailing semicolon must not lose the value");
        assert_eq!(value, "hello");
    }

    #[tokio::test]
    async fn real_engine_wire_trailing_line_comment_is_safe() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("hello"));
        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "response.body // note")
            .await
            .expect("a trailing comment must not swallow the wrapper");
        assert_eq!(value, "hello");
    }

    #[tokio::test]
    async fn real_engine_wire_body_without_return_is_an_error() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("hello"));
        let err = svc
            .resolve_flow_wire_expression("my-api", &output, "const a = 1;\nconsole.log(a);")
            .await
            .expect_err("a wire body with no return resolves to undefined");
        assert!(err.to_string().contains("null/undefined"), "got {err}");
    }

    #[tokio::test]
    async fn real_engine_if_coerces_a_multi_line_body() {
        let svc = real_engine_service();
        let output = CapturedOutput::Request(Box::new(sample_response_output()));
        let value = svc
            .evaluate_flow_route_expression(
                "my-api",
                &output,
                "const ok = response.status === 200;\nreturn ok;",
                FlowCoercion::Bool,
            )
            .await
            .expect("an If body must run");
        assert_eq!(value, "true");
    }

    #[tokio::test]
    async fn real_engine_switch_coerces_a_multi_line_body() {
        let svc = real_engine_service();
        let output = CapturedOutput::Request(Box::new(sample_response_output()));
        let value = svc
            .evaluate_flow_route_expression(
                "my-api",
                &output,
                "const p = response.body.plan;\nreturn p;",
                FlowCoercion::Str,
            )
            .await
            .expect("a Switch body must run");
        assert_eq!(value, "pro");
    }

    #[tokio::test]
    async fn real_engine_reports_a_syntax_error_in_both_forms() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("x"));
        let err = svc
            .resolve_flow_wire_expression("my-api", &output, "return (")
            .await
            .expect_err("broken source must fail");
        assert!(err.to_string().contains("SyntaxError"), "got {err}");
    }
```
If `sample_response_output()`'s body has no `token`/`plan` field or 200 status, read it and adjust the expected values to that fixture instead of changing the fixture.

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -j4 -p rocket-app real_engine`
Expected: compile error on `FlowCoercion` (proves the tests need the new API). After adding only the enum and the new route signature, the multi-line and trailing-`;` tests must FAIL with a SyntaxError or wrong value before the new wrapper lands.

- [ ] **Step 3: Implement the wrapper and the new route signature**

Replace `wrap_with_response_object` with:

```rust
/// How a Flow script's result is turned into the value the caller needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowCoercion {
    /// A wire uses the result as is.
    Raw,
    /// An If node needs `true` or `false`.
    Bool,
    /// A Switch node compares a string.
    Str,
}

/// Builds the JS that runs one Flow script against a plain `response` object.
///
/// The user source is embedded as a JSON string literal, so quotes, backticks
/// and newlines cannot break the wrapper. The real JS parser picks the form,
/// without running the code: one expression first, then the same without a
/// trailing `;`, then a script that sends its value with `return`. The chosen
/// form then runs exactly once. Only the Flow entry points use this. The Vars
/// tab keeps `res.body` syntax.
fn flow_script(source: &str, coercion: FlowCoercion) -> DomainResult<String> {
    let literal = serde_json::to_string(source)
        .map_err(|e| DomainError::Internal(format!("failed to encode flow script: {e}")))?;
    // The coercion text keeps the `!!(` and `String(` markers the scripted
    // test engine matches on.
    let (open, close) = match coercion {
        FlowCoercion::Raw => ("(", ")"),
        FlowCoercion::Bool => ("!!(", ")"),
        FlowCoercion::Str => ("String(", ")"),
    };
    Ok(format!(
        r#"(() => {{
  const src = {literal};
  const isParseError = (e) => e instanceof SyntaxError;
  let fn = null;
  const asExpression = (text) => new Function('response', 'return (' + text + '\n)');
  // 1. One expression. This keeps `{{ a: 1 }}` an object and every saved wire unchanged.
  try {{ fn = asExpression(src); }}
  catch (e) {{ if (!isParseError(e)) throw e; }}
  // 2. One expression with a trailing `;`, such as `response.body;`.
  if (fn === null) {{
    try {{ fn = asExpression(src.replace(/;\s*$/, '')); }}
    catch (e) {{ if (!isParseError(e)) throw e; }}
  }}
  // 3. A script that sends its value with `return`. A parse error here is the one reported.
  if (fn === null) fn = new Function('response', src);
  const response = {{
    status: res.getStatus(),
    statusText: res.getStatusText(),
    headers: res.getHeaders(),
    body: res.getBody(),
    duration_ms: res.getResponseTime(),
  }};
  return {open}fn(response){close};
}})()"#
    ))
}
```
All three detection steps only construct functions (parse); none runs user code, so a `SyntaxError` thrown by the user's code at run time (for example `JSON.parse('x')`) is reported as-is and the code never runs twice. If `new Function` turns out to be blocked in the sandbox, stop and report it rather than working around it.

In `resolve_flow_wire_expression`, pass `&flow_script(expression, FlowCoercion::Raw)?` instead of `&wrap_with_response_object(expression)`.

Change `evaluate_flow_route_expression` to take `source: &str, coercion: FlowCoercion` and pass `&flow_script(source, coercion)?`. Update its doc comment: callers pass the raw condition or value, and the coercion is applied to the result.

In the If arm, replace `&format!("!!({condition})")` with `condition, FlowCoercion::Bool`. In the Switch arm, replace `&format!("String({value})")` with `value, FlowCoercion::Str`.

Update every existing test call of `evaluate_flow_route_expression` (~1209, 1227, 1245, 1263, 1276 and the `real_engine_*` tests): strip the `!!(…)`/`String(…)` text from the argument and pass the matching `FlowCoercion`. Scripted-engine rules keyed on `"!!("` and `"String("` keep working because the wrapper still emits those markers. If a scripted rule's needle contains a double quote, it now appears escaped (`\"`) in the code; adjust that needle, not the wrapper.

- [ ] **Step 4: Run the tests and watch them pass**

Run: `cargo test -j4 -p rocket-app flow_execution_service`
Expected: all pass, including the new real-engine tests.

- [ ] **Step 5: Check and commit**

Run: `cargo clippy -j4 -p rocket-app --tests -- -D warnings` and `cargo check -j4 -p rocket-app -p rocket`.
Commit subject: `feat(flow): accept multi-line scripts on wires`.

---

### Task 2: Report script console output on each step

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (new types near `FlowSkipReason` ~17; `FlowStepCompleted` ~185-206; its tests ~674-860)
- Modify: `crates/rocket-app/src/execution_service.rs` (`evaluate_var_expression` ~1494-1542)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (the two Flow helpers, `execute_node` ~646+, the run loop ~553-598, `FlowStepResult` ~370-388, `step_completed_event` ~392-404, `result_to_step` ~817+, `skipped_step`/`failed_step` ~874-900)

**Interfaces:**
- Consumes: `flow_script`, `FlowCoercion` from Task 1.
- Produces:
  - In `rocket_shared::events`: `pub enum FlowLogLevel { Log, Warn, Error }` (`#[serde(rename_all = "lowercase")]`, derive `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`) and `pub struct FlowLogEntry { pub level: FlowLogLevel, pub message: String }` (derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`).
  - `DomainEvent::FlowStepCompleted` gains `#[serde(default, skip_serializing_if = "Vec::is_empty")] logs: Vec<FlowLogEntry>`.
  - `FlowStepResult` gains the same `logs: Vec<FlowLogEntry>` field (camelCase DTO; the name stays `logs`).
  - `RequestExecutionService::evaluate_expression_with_logs(&self, collection_root: &str, expression: &str, response_json: &str) -> (DomainResult<serde_json::Value>, Vec<ConsoleEntry>)`.
  - `pub struct FlowScriptOutcome { pub result: DomainResult<String>, pub logs: Vec<FlowLogEntry> }`; `resolve_flow_wire_expression` and `evaluate_flow_route_expression` return `FlowScriptOutcome` instead of `DomainResult<String>`. Update the Task 1 tests to read `.result` (e.g. `.await.result.expect(…)`).

- [ ] **Step 1: Write the failing tests**

In `events.rs` tests, add:

```rust
    #[test]
    fn flow_step_completed_serializes_logs_in_lowercase_and_omits_empty_logs() {
        let with_logs = DomainEvent::FlowStepCompleted {
            run_id: "01J".into(),
            node_id: "n".into(),
            status: FlowNodeStatus::Success,
            status_code: None,
            duration_ms: None,
            error: None,
            value: None,
            skip_reason: None,
            branch: None,
            logs: vec![FlowLogEntry { level: FlowLogLevel::Warn, message: "hi".into() }],
        };
        let json = serde_json::to_string(&with_logs).expect("serialize");
        assert!(json.contains(r#""logs":[{"level":"warn","message":"hi"}]"#), "got {json}");

        let without = DomainEvent::FlowStepCompleted {
            run_id: "01J".into(),
            node_id: "n".into(),
            status: FlowNodeStatus::Success,
            status_code: None,
            duration_ms: None,
            error: None,
            value: None,
            skip_reason: None,
            branch: None,
            logs: vec![],
        };
        let json = serde_json::to_string(&without).expect("serialize");
        assert!(!json.contains("logs"), "got {json}");
    }
```
Add `logs: vec![]` to every other `FlowStepCompleted { … }` literal in that file; their expected JSON strings stay unchanged.

In `flow_execution_service.rs` tests, add real-engine tests:

```rust
    #[tokio::test]
    async fn real_engine_wire_returns_console_logs() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("hello"));
        let outcome = svc
            .resolve_flow_wire_expression("my-api", &output, "console.log('seen', response.body);\nreturn 1;")
            .await;
        assert_eq!(outcome.result.expect("value"), "1");
        assert_eq!(outcome.logs.len(), 1);
        assert_eq!(outcome.logs[0].level, FlowLogLevel::Log);
        assert!(outcome.logs[0].message.contains("seen"));
    }

    #[tokio::test]
    async fn real_engine_keeps_logs_written_before_a_throw() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("hello"));
        let outcome = svc
            .resolve_flow_wire_expression("my-api", &output, "console.warn('before');\nthrow new Error('boom');")
            .await;
        assert!(outcome.result.is_err());
        assert_eq!(outcome.logs.len(), 1);
        assert_eq!(outcome.logs[0].level, FlowLogLevel::Warn);
    }
```
And a run-level test using the real engine: a flow with an Input `hello` wired into an Output whose edge expression is `console.log('out');\nreturn response.body;`. Find an existing run test that calls `.run(` on an Input→Output flow, copy its setup, and swap in the real engine. Assert the Output step's `logs` has one entry whose message contains `out`, and that the `FlowStepCompleted` event published for that node carries the same logs (reuse the recording event publisher the existing run tests use). Also assert a failed step keeps its logs: an Output edge `console.log('x');\nthrow new Error('no');` gives a Failed step with one log entry.

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -j4 -p rocket-shared flow_step_completed` and `cargo test -j4 -p rocket-app real_engine`
Expected: compile errors for the missing `logs` field and types.

- [ ] **Step 3: Implement**

1. `events.rs`: add `FlowLogLevel` and `FlowLogEntry` (with a short doc comment each) after `FlowSkipReason`, and the `logs` field (doc: `/// Script console output from this step, oldest first.`) at the end of `FlowStepCompleted`.
2. `execution_service.rs`: move the body of `evaluate_var_expression` into `evaluate_expression_with_logs`, returning `(result, console_entries)`:
   - setup failures (no engine, bad response JSON, engine `Err`) return `(Err(e), vec![])`;
   - otherwise take `result.console_entries`, and return `Err(InvalidInput(err))` when `result.error` is set, else the `__jsonq_result__` value, always paired with those entries.
   `evaluate_var_expression` becomes `self.evaluate_expression_with_logs(collection_root, expression, response_json).await.0`, with its doc comment unchanged.
3. `flow_execution_service.rs`:
   - Add `fn to_flow_logs(entries: Vec<ConsoleEntry>) -> Vec<FlowLogEntry>` mapping `ConsoleLevel::{Log, Warn, Error}` to `FlowLogLevel::{Log, Warn, Error}`.
   - Add `FlowScriptOutcome` (doc: the value and console output of one Flow script). Rewrite both helpers to call `evaluate_expression_with_logs(…, &script, …)` where `script` comes from `flow_script`; if `flow_script` fails, return `FlowScriptOutcome { result: Err(e), logs: vec![] }`. Keep the existing null/undefined and string conversion rules inside `result`.
   - Give `execute_node` a new last parameter `logs: &mut Vec<FlowLogEntry>`. At every helper call site (Output ~688, Request wires ~711, If ~731, Switch ~754), do `logs.extend(outcome.logs)` first, then use `outcome.result?` as the value. In the Request arm, after `execute_with_external_secrets` succeeds, also `logs.extend(to_flow_logs(output.console_entries.clone()))`.
   - In the run loop, create `let mut node_logs = Vec::new();` before calling `execute_node`, pass `&mut node_logs`, and after `let step = result_to_step(…)` do `let step = FlowStepResult { logs: node_logs, ..step };`.
   - Add `#[serde(default, skip_serializing_if = "Vec::is_empty")] pub logs: Vec<FlowLogEntry>` (doc as above) to `FlowStepResult`; set `logs: Vec::new()` in `result_to_step`'s `base`, `skipped_step` and `failed_step`.
   - `step_completed_event` copies `logs: step.logs.clone()`.

- [ ] **Step 4: Run the tests and watch them pass**

Run: `cargo test -j4 -p rocket-shared`, then `cargo test -j4 -p rocket-app flow_execution_service`, then `cargo test -j4 -p rocket-app execution_service`
Expected: all pass.

- [ ] **Step 5: Check and commit**

Run: `cargo clippy -j4 -p rocket-app -p rocket-shared --tests -- -D warnings` and `cargo check -j4 -p rocket-app -p rocket`.
Commit subject: `feat(flow): report script console output per step`.
