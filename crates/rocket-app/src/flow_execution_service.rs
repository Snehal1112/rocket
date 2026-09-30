use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use rocket_collection::Request;
use rocket_flow::{handle, FlowEdge, FlowNode, FlowNodeKind, InlineRequestData, RequestSource};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Body, BodyMode, Header, HttpMethod};
use rocket_shared::VariableValue;

use crate::execution_service::{
    ExecuteRequestInput, ExecuteRequestOutput, RequestExecutionService,
};
use crate::flow_cancel::{cancel_pair, CancelHandle, CancelSignal};
use crate::flow_debug::{build_debug_request, cap_exchange};
use crate::flow_routing::{decide_fate, NodeFate, NodeOutcome};
use crate::runner_sequence::{build_step_input, RunItem};

/// One node's fully-executed result, kept around so a downstream edge's
/// wiring expression can be evaluated against it.
#[derive(Debug, Clone)]
pub enum CapturedOutput {
    Request(Box<ExecuteRequestOutput>),
    Value(VariableValue),
}

/// A node that ran: its captured output plus the exit it left through
/// (`handle::RESULT` for every non-routing node).
#[derive(Debug, Clone)]
pub(crate) struct ExecutedNode {
    pub(crate) output: CapturedOutput,
    pub(crate) chosen_exit: String,
    /// Set by a repeat-until poll that met its condition.
    pub(crate) poll: Option<crate::flow_poll::PollStats>,
    /// The value the step reports, masked, when it differs from the raw
    /// captured value. Set by an Input node.
    pub(crate) reported_value: Option<String>,
}

impl ExecutedNode {
    pub(crate) fn plain(output: CapturedOutput) -> Self {
        Self {
            output,
            chosen_exit: handle::RESULT.to_string(),
            poll: None,
            reported_value: None,
        }
    }
}

/// Serializes `output` into the `HttpResponse` JSON a wire or route
/// expression runs against. `Value` outputs (Input/Output nodes) become a
/// synthetic 200 response whose `body` is the raw value, so one
/// `response.xxx` convention works for every node kind.
fn captured_output_response_json(output: &CapturedOutput) -> DomainResult<String> {
    let response = match output {
        CapturedOutput::Request(out) => out.response.clone(),
        CapturedOutput::Value(value) => rocket_http::HttpResponse {
            status: 200,
            status_text: "OK".to_string(),
            headers: Vec::new(),
            body: value.data().to_string(),
            duration_ms: 0,
            ttfb_ms: 0,
            size_bytes: value.data().len(),
        },
    };
    serde_json::to_string(&response)
        .map_err(|e| DomainError::Internal(format!("failed to serialize captured output: {e}")))
}

/// How a Flow script's result is turned into the value the caller needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowCoercion {
    /// A wire uses the result as is.
    Raw,
    /// An If node needs `true` or `false`.
    Bool,
    /// A Switch node compares a string.
    Str,
    /// A Transform node needs any value except `undefined`.
    Required,
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
        FlowCoercion::Required => ("__requireValue(", ")"),
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
  const __requireValue = (v) => {{
    if (v === undefined) throw new Error('script returned no value');
    return v;
  }};
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

/// Builds the JS that runs an `accept_when` condition against a `request`
/// object. `res.getBody()` returns the call object built by
/// `callback_request_json`, parsed. The same three parse forms as
/// `flow_script` apply, and the result is coerced with `!!(`.
fn flow_callback_script(source: &str) -> DomainResult<String> {
    let literal = serde_json::to_string(source)
        .map_err(|e| DomainError::Internal(format!("failed to encode flow script: {e}")))?;
    Ok(format!(
        r#"(() => {{
  const src = {literal};
  const isParseError = (e) => e instanceof SyntaxError;
  let fn = null;
  const asExpression = (text) => new Function('request', 'return (' + text + '\n)');
  try {{ fn = asExpression(src); }}
  catch (e) {{ if (!isParseError(e)) throw e; }}
  if (fn === null) {{
    try {{ fn = asExpression(src.replace(/;\s*$/, '')); }}
    catch (e) {{ if (!isParseError(e)) throw e; }}
  }}
  if (fn === null) fn = new Function('request', src);
  const request = res.getBody();
  return !!(fn(request));
}})()"#
    ))
}

/// The `request` object an `accept_when` condition sees. Headers and query
/// become objects (a repeated name keeps its last value); the body is parsed
/// JSON when it parses, else the raw text.
fn callback_request_json(call: &crate::callback_listener::ReceivedCall) -> serde_json::Value {
    let body = serde_json::from_str::<serde_json::Value>(&call.body)
        .unwrap_or_else(|_| serde_json::Value::String(call.body.clone()));
    let to_object = |pairs: &[(String, String)]| {
        serde_json::Value::Object(
            pairs
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect(),
        )
    };
    serde_json::json!({
        "method": call.method,
        "path": call.path,
        "query": to_object(&call.query),
        "headers": to_object(&call.headers),
        "body": body,
    })
}

impl RequestExecutionService {
    /// Evaluates `expression` (a jsonq/JS snippet such as `"response.body"` or
    /// `"response.body.token"`) against `output`, reusing the same
    /// script-engine mechanism `evaluate_var_expression` uses for the Vars
    /// tab's preview — not a second sandbox invocation path. The expression
    /// sees a `response` object with `status`, `statusText`, `headers`,
    /// `body` (parsed JSON, else text) and `duration_ms`.
    ///
    /// A string result is returned as-is and other JSON values are
    /// stringified. A `null` or `undefined` result is an `InvalidInput`
    /// error, so a wire never injects the literal text "null".
    pub async fn resolve_flow_wire_expression(
        &self,
        collection: &str,
        output: &CapturedOutput,
        expression: &str,
        secret_values: &HashSet<String>,
    ) -> FlowScriptOutcome {
        let script = match flow_script(expression, FlowCoercion::Raw) {
            Ok(script) => script,
            Err(e) => return FlowScriptOutcome::failed(e),
        };
        let response_json = match captured_output_response_json(output) {
            Ok(json) => json,
            Err(e) => return FlowScriptOutcome::failed(e),
        };
        let (result, entries) = self
            .evaluate_expression_with_logs(
                collection,
                &script,
                &response_json,
                secret_values.clone(),
            )
            .await;
        let result = result.and_then(|value| match value {
            // A `null` or `undefined` result would wire the literal text "null"
            // into the request, so it is an error instead.
            serde_json::Value::Null => Err(DomainError::InvalidInput(format!(
                "expression '{expression}' resolved to null/undefined"
            ))),
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        });
        FlowScriptOutcome {
            result,
            logs: to_flow_logs(entries),
        }
    }

    /// Evaluates an If/Switch routing script, or a Transform script, against output.
    /// Callers pass the raw condition or value, and `coercion` is applied to the result, so the
    /// result is always a string; a `null` result becomes `"null"` rather
    /// than an error, which keeps a missing value routable by a case.
    pub async fn evaluate_flow_route_expression(
        &self,
        collection: &str,
        output: &CapturedOutput,
        source: &str,
        coercion: FlowCoercion,
        secret_values: &HashSet<String>,
    ) -> FlowScriptOutcome {
        let script = match flow_script(source, coercion) {
            Ok(script) => script,
            Err(e) => return FlowScriptOutcome::failed(e),
        };
        let response_json = match captured_output_response_json(output) {
            Ok(json) => json,
            Err(e) => return FlowScriptOutcome::failed(e),
        };
        let (result, entries) = self
            .evaluate_expression_with_logs(
                collection,
                &script,
                &response_json,
                secret_values.clone(),
            )
            .await;
        FlowScriptOutcome {
            result: result.map(|value| match value {
                serde_json::Value::Null => "null".to_string(),
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            }),
            logs: to_flow_logs(entries),
        }
    }

    /// Evaluates a Transform node's script against `output`. The script may be
    /// one expression or a function body that returns a value. Any result
    /// except `undefined` is accepted, and it comes back as text: a string as
    /// is, `null` as `"null"`, and anything else as compact JSON.
    pub async fn evaluate_flow_transform_script(
        &self,
        collection: &str,
        output: &CapturedOutput,
        source: &str,
        secret_values: &HashSet<String>,
    ) -> FlowScriptOutcome {
        self.evaluate_flow_route_expression(
            collection,
            output,
            source,
            FlowCoercion::Required,
            secret_values,
        )
        .await
    }

    /// Evaluates a Wait for callback node's `accept_when` against one
    /// received call. The script sees `request` (see `callback_request_json`)
    /// and its result is `"true"` or `"false"`.
    pub async fn evaluate_flow_callback_condition(
        &self,
        collection: &str,
        call: &crate::callback_listener::ReceivedCall,
        source: &str,
        secret_values: &HashSet<String>,
    ) -> FlowScriptOutcome {
        let script = match flow_callback_script(source) {
            Ok(script) => script,
            Err(e) => return FlowScriptOutcome::failed(e),
        };
        let body = callback_request_json(call).to_string();
        let carrier = rocket_http::HttpResponse {
            status: 200,
            status_text: "OK".to_string(),
            headers: Vec::new(),
            size_bytes: body.len(),
            body,
            duration_ms: 0,
            ttfb_ms: 0,
        };
        let response_json = match serde_json::to_string(&carrier) {
            Ok(json) => json,
            Err(e) => {
                return FlowScriptOutcome::failed(DomainError::Internal(format!(
                    "failed to serialize callback: {e}"
                )))
            }
        };
        let (result, entries) = self
            .evaluate_expression_with_logs(
                collection,
                &script,
                &response_json,
                secret_values.clone(),
            )
            .await;
        FlowScriptOutcome {
            result: result.map(|value| match value {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            }),
            logs: to_flow_logs(entries),
        }
    }
}

/// The value and console output of one Flow script.
#[derive(Debug)]
pub struct FlowScriptOutcome {
    pub result: DomainResult<String>,
    pub logs: Vec<FlowLogEntry>,
}

impl FlowScriptOutcome {
    /// An outcome for a script that could not be run, so it has no output.
    fn failed(error: DomainError) -> Self {
        Self {
            result: Err(error),
            logs: Vec::new(),
        }
    }
}

pub(crate) fn to_flow_logs(entries: Vec<ConsoleEntry>) -> Vec<FlowLogEntry> {
    entries
        .into_iter()
        .map(|entry| FlowLogEntry {
            level: match entry.level {
                ConsoleLevel::Log => FlowLogLevel::Log,
                ConsoleLevel::Warn => FlowLogLevel::Warn,
                ConsoleLevel::Error => FlowLogLevel::Error,
            },
            message: entry.message,
        })
        .collect()
}

/// Builds an `ExecuteRequestInput` for a `FlowNodeKind::Request` node, before
/// any wire overrides (see `apply_wired_overrides`) are applied.
///
/// Both `Saved` and `Inline` sources resolve down to a `rocket_collection::Request`
/// value, then reuse `crate::runner_sequence::build_step_input` — the exact
/// function the Collection Runner already uses for the same "saved request →
/// ExecuteRequestInput" problem — rather than a second, parallel mapping.
pub fn build_execute_request_input(
    collection_repo: &dyn rocket_collection::CollectionRepository,
    collection: &str,
    environment_name: Option<&str>,
    global_env_name: Option<&str>,
    node: &FlowNode,
) -> DomainResult<ExecuteRequestInput> {
    let FlowNodeKind::Request { label, source, .. } = &node.kind else {
        return Err(DomainError::InvalidInput(format!(
            "node '{}' is not a Request node",
            node.id
        )));
    };

    let (request, request_path) = match source {
        RequestSource::Saved { request_path } => {
            let request = collection_repo.get_request(collection, request_path)?;
            (request, request_path.clone())
        }
        RequestSource::Inline { request: inline } => (
            build_inline_request(label, inline)?,
            format!("__flow_inline__/{}", node.id),
        ),
    };

    let item = RunItem {
        name: request.name.clone(),
        request_path,
        request,
    };
    Ok(build_step_input(
        &item,
        collection,
        environment_name,
        global_env_name,
        rocket_workspace::RequestGuardPolicy::default(),
    ))
}

/// Turns an ad hoc `InlineRequestData` into a `rocket_collection::Request`
/// value object so it can flow through the same `build_step_input` path a
/// saved request uses. `request_path` for an inline node is a synthetic,
/// never-resolves-to-a-real-file sentinel (`"__flow_inline__/<node id>"`) —
/// `RequestExecutionService::build_variable_scopes` already treats a failed
/// `get_folder_chain_variables`/`get_request_variables` lookup as "no
/// variables at this scope" (`if let Ok(...)`), which is exactly correct
/// here: an inline request isn't part of the collection tree and should not
/// inherit folder-chain variables.
fn build_inline_request(label: &str, inline: &InlineRequestData) -> DomainResult<Request> {
    let method: HttpMethod = inline.method.parse()?;
    let mut request = Request::new(label, method, inline.url.clone());
    request.headers = inline
        .headers
        .iter()
        .map(|h| Header {
            key: h.name.clone(),
            value: h.value.clone(),
            enabled: true,
            description: None,
        })
        .collect();
    request.body = inline.body.as_ref().map(|content| Body {
        mode: BodyMode::Json,
        content: Some(content.clone()),
        form_data: None,
        file_path: None,
    });
    Ok(request)
}

/// Mutates `input` in place, applying each edge in `edges` whose id is a key
/// in `resolved` onto the field its `target_field` path names. Supported
/// paths for Phase 1:
///
/// - `"url"` replaces the URL.
/// - `"body"` replaces the body content. A missing body or a `none`-mode body
///   becomes a JSON body. A text-like mode (JSON, XML, text, SPARQL) is kept.
///   A form or binary body has no single content string, so it is an error.
/// - `"headers[N].value"` (all-digit `N`) sets the value of the header at
///   index `N`. An out-of-range index is an error.
/// - `"headers[Name].value"` (any other `Name`) sets the value of the first
///   header whose key matches `Name` case-insensitively, or appends a new
///   header when none matches. This is the form the Flow UI uses, because it
///   does not know a saved request's header order.
///
/// A wired header is always enabled, so the wire takes effect. Any other
/// path is a `DomainError` — never a silent no-op, since a wire the user drew
/// that quietly does nothing would be far more confusing than a run that
/// fails with a clear reason.
pub fn apply_wired_overrides(
    input: &mut ExecuteRequestInput,
    resolved: &HashMap<String, String>,
    edges: &[FlowEdge],
) -> DomainResult<()> {
    for e in edges {
        let Some(value) = resolved.get(&e.id) else {
            continue;
        };
        match e.target_field.as_str() {
            "url" => input.url = value.clone(),
            "body" => apply_body_override(input, e, value)?,
            field => {
                let Some(selector) = field
                    .strip_prefix("headers[")
                    .and_then(|rest| rest.strip_suffix("].value"))
                else {
                    return Err(DomainError::InvalidInput(format!(
                        "edge '{}': unrecognized target_field '{}'",
                        e.id, e.target_field
                    )));
                };
                apply_header_override(input, e, selector, value)?;
            }
        }
    }
    Ok(())
}

/// Writes `value` into the request body for a `"body"` wire.
fn apply_body_override(
    input: &mut ExecuteRequestInput,
    e: &FlowEdge,
    value: &str,
) -> DomainResult<()> {
    let body = input.body.get_or_insert(Body {
        mode: BodyMode::Json,
        content: None,
        form_data: None,
        file_path: None,
    });
    match body.mode {
        // A `none` body sends nothing, so it is promoted to JSON.
        BodyMode::None => body.mode = BodyMode::Json,
        BodyMode::Json | BodyMode::Xml | BodyMode::Text | BodyMode::Sparql => {}
        // These modes never read `content`, so writing it would do nothing.
        BodyMode::FormUrlEncoded | BodyMode::FormData | BodyMode::Binary => {
            return Err(DomainError::InvalidInput(format!(
                "edge '{}': cannot wire a value into a {:?} body",
                e.id, body.mode
            )));
        }
    }
    body.content = Some(value.to_string());
    Ok(())
}

/// Writes `value` into the header `selector` names, for a
/// `"headers[<selector>].value"` wire.
fn apply_header_override(
    input: &mut ExecuteRequestInput,
    e: &FlowEdge,
    selector: &str,
    value: &str,
) -> DomainResult<()> {
    if selector.trim().is_empty() {
        return Err(DomainError::InvalidInput(format!(
            "edge '{}': malformed target_field '{}'",
            e.id, e.target_field
        )));
    }

    if selector.bytes().all(|b| b.is_ascii_digit()) {
        let index: usize = selector.parse().map_err(|_| {
            DomainError::InvalidInput(format!(
                "edge '{}': malformed target_field '{}'",
                e.id, e.target_field
            ))
        })?;
        let headers_len = input.headers.len();
        let header = input.headers.get_mut(index).ok_or_else(|| {
            DomainError::InvalidInput(format!(
                "edge '{}': header index {} out of range (request has {} headers)",
                e.id, index, headers_len
            ))
        })?;
        header.value = value.to_string();
        header.enabled = true;
        return Ok(());
    }

    let name = selector.trim();
    match input
        .headers
        .iter_mut()
        .find(|h| h.key.eq_ignore_ascii_case(name))
    {
        Some(header) => {
            header.value = value.to_string();
            header.enabled = true;
        }
        None => input.headers.push(Header {
            key: name.to_string(),
            value: value.to_string(),
            enabled: true,
            description: None,
        }),
    }
    Ok(())
}

use std::sync::{Arc, Mutex};

use rocket_scripting::{ConsoleEntry, ConsoleLevel};
use rocket_shared::events::{
    DomainEvent, FlowDebugRequest, FlowLogEntry, FlowLogLevel, FlowNodeStatus, FlowSkipReason,
};
use ulid::Ulid;

/// Input DTO for `FlowExecutionService::run`.
#[derive(Debug, Clone)]
pub struct RunFlowInput {
    pub collection: String,
    pub flow_name: String,
    pub environment_name: Option<String>,
    pub global_env_name: Option<String>,
}

/// One node's outcome within a run, as reported in `FlowRunSummary::steps`
/// and the `FlowStepCompleted` event. IPC DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowStepResult {
    pub node_id: String,
    pub status: FlowNodeStatus,
    pub status_code: Option<u16>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
    /// The node's captured output value for Output and Input nodes, or the
    /// received method (e.g. `POST`) for a succeeded Wait for callback node.
    /// `None` for every other node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Set only when `status` is `Skipped`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<FlowSkipReason>,
    /// The exit a succeeded If/Switch node took. `None` for every other node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Script console output from this step, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logs: Vec<FlowLogEntry>,
    /// The request as sent and its response, masked. Only for Request nodes in debug mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_request: Option<FlowDebugRequest>,
    /// How many times a repeat-until Request node sent its request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    /// The request this step sent and its response, masked and size-capped.
    /// Set for every Request node that sent and for an accepted Wait for
    /// callback, whatever the Debug mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<FlowDebugRequest>,
}

/// Builds the `FlowStepCompleted` event for one recorded step, so the live
/// event and the returned summary can never disagree.
fn step_completed_event(run_id: &str, step: &FlowStepResult) -> DomainEvent {
    DomainEvent::FlowStepCompleted {
        run_id: run_id.to_string(),
        node_id: step.node_id.clone(),
        status: step.status,
        status_code: step.status_code,
        duration_ms: step.duration_ms,
        error: step.error.clone(),
        value: step.value.clone(),
        skip_reason: step.skip_reason,
        branch: step.branch.clone(),
        logs: step.logs.clone(),
        debug_request: step.debug_request.clone().map(Box::new),
        exchange: step.exchange.clone().map(Box::new),
        attempts: step.attempts,
    }
}

/// The full result of one `FlowExecutionService::run` call. IPC DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRunSummary {
    pub run_id: String,
    pub steps: Vec<FlowStepResult>,
    pub stopped_reason: String,
}

/// What a node needs to know about the run it belongs to.
pub(crate) struct NodeRunContext {
    pub(crate) run_id: String,
    pub(crate) node_id: String,
    /// Fires when the run is cancelled. A waiting node selects on it.
    pub(crate) cancel: CancelSignal,
}

/// Registers a run as in flight and removes every trace of it when dropped,
/// so no exit path of `run` can leak the run id or its cancel handle.
struct RunRegistration<'a> {
    service: &'a FlowExecutionService,
    run_id: String,
}

impl<'a> RunRegistration<'a> {
    fn new(service: &'a FlowExecutionService, run_id: &str) -> (Self, CancelSignal) {
        let (handle, signal) = cancel_pair();
        if let Ok(mut handles) = service.cancel_handles.lock() {
            handles.insert(run_id.to_string(), handle);
        }
        if let Ok(mut set) = service.in_flight.lock() {
            set.insert(run_id.to_string());
        }
        let registration = Self {
            service,
            run_id: run_id.to_string(),
        };
        (registration, signal)
    }
}

impl Drop for RunRegistration<'_> {
    fn drop(&mut self) {
        self.service.clear_cancellation(&self.run_id);
        if let Ok(mut set) = self.service.in_flight.lock() {
            set.remove(&self.run_id);
        }
        if let Ok(mut handles) = self.service.cancel_handles.lock() {
            handles.remove(&self.run_id);
        }
    }
}

/// Orchestrates one Flow run: loads the graph, walks it in dependency order,
/// and dispatches each node using the building blocks in this same module
/// (`build_execute_request_input`, `apply_wired_overrides`,
/// `resolve_flow_wire_expression`). Holds no execution machinery of its own —
/// `run()` takes the `RequestExecutionService` to drive, the same pattern
/// `CollectionRunnerService::run` uses.
pub struct FlowExecutionService {
    flow_repo: Box<dyn rocket_flow::FlowRepository>,
    collection_repo: Box<dyn rocket_collection::CollectionRepository>,
    events: Box<dyn rocket_shared::events::EventPublisher>,
    cancelled: Arc<Mutex<HashSet<String>>>,
    in_flight: Arc<Mutex<HashSet<String>>>,
    /// One cancel handle per in-flight run. `cancel` triggers it, so a node
    /// that is waiting stops at once.
    cancel_handles: Arc<Mutex<HashMap<String, CancelHandle>>>,
    /// Opens run-scoped callback endpoints for Wait for callback nodes.
    callback_listener: Box<dyn crate::callback_listener::CallbackListener>,
}

impl FlowExecutionService {
    pub fn new(
        flow_repo: Box<dyn rocket_flow::FlowRepository>,
        collection_repo: Box<dyn rocket_collection::CollectionRepository>,
        events: Box<dyn rocket_shared::events::EventPublisher>,
    ) -> Self {
        Self {
            flow_repo,
            collection_repo,
            events,
            cancelled: Arc::new(Mutex::new(HashSet::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            cancel_handles: Arc::new(Mutex::new(HashMap::new())),
            callback_listener: Box::new(crate::callback_listener::NoCallbackListener),
        }
    }

    /// Replaces the default `NoCallbackListener`. `src-tauri` passes the
    /// real server; tests pass a `FakeCallbackListener`.
    pub fn with_callback_listener(
        mut self,
        listener: Box<dyn crate::callback_listener::CallbackListener>,
    ) -> Self {
        self.callback_listener = listener;
        self
    }

    /// Loads the named flow and validates it into a dependency-ordered node id
    /// list. A structurally invalid or cyclic graph is rejected here (defense
    /// in depth — saving should already have refused it) rather than ever
    /// starting a run against it.
    fn load_ordered_nodes(
        &self,
        collection: &str,
        flow_name: &str,
    ) -> DomainResult<(rocket_flow::Flow, Vec<String>)> {
        let flow = self.flow_repo.get(collection, flow_name)?;
        let order = rocket_flow::validate(&flow).map_err(|e| {
            DomainError::InvalidInput(format!("flow '{flow_name}' is not runnable: {e}"))
        })?;
        Ok((flow, order))
    }

    /// Asks an in-progress run to stop. The run ends after the node that is
    /// running now. A node that is waiting (a poll or a callback wait) stops
    /// waiting at once; a request already in flight still finishes.
    /// Cancelling an unknown or finished run id is a no-op, mirroring
    /// `CollectionRunnerService::cancel`.
    pub fn cancel(&self, run_id: &str) {
        if let Ok(in_flight) = self.in_flight.lock() {
            if !in_flight.contains(run_id) {
                return;
            }
        }
        if let Ok(mut cancelled) = self.cancelled.lock() {
            cancelled.insert(run_id.to_string());
        }
        if let Ok(handles) = self.cancel_handles.lock() {
            if let Some(handle) = handles.get(run_id) {
                handle.cancel();
            }
        }
    }

    fn is_cancelled(&self, run_id: &str) -> bool {
        self.cancelled
            .lock()
            .map(|set| set.contains(run_id))
            .unwrap_or(false)
    }

    fn clear_cancellation(&self, run_id: &str) {
        if let Ok(mut set) = self.cancelled.lock() {
            set.remove(run_id);
        }
    }

    /// Runs every node of `input.flow_name` in dependency order. Each node's
    /// fate is decided at its turn from its predecessors' outcomes
    /// (`flow_routing::decide_fate`): it runs, is skipped (`upstream_failed`
    /// or `branch_not_taken`), or fails on ambiguous inputs. Cancellation is
    /// checked before each node and after it, so a cancelled run records the
    /// node that was running and nothing after it.
    pub async fn run(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
    ) -> DomainResult<FlowRunSummary> {
        let (flow, order) = self.load_ordered_nodes(&input.collection, &input.flow_name)?;

        // Fetch every External Secret value once for the whole run. Each
        // Request node reuses this map, so a run of N requests makes one
        // vault round-trip per secret instead of N.
        let external_secrets = exec
            .resolve_external_secrets(Some(&input.collection), input.environment_name.as_deref())
            .await?;
        let nodes_by_id: HashMap<&str, &FlowNode> =
            flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

        // Open every callback endpoint before the run is registered or
        // announced. A failure here ends the call with no events and nothing
        // left in `in_flight`. `callbacks` lives until `run` returns, so
        // every endpoint closes on every exit path.
        let mut callbacks =
            crate::flow_callbacks::RunCallbacks::open_all(self.callback_listener.as_ref(), &flow)
                .await?;

        let run_id = Ulid::new().to_string();
        let (registration, cancel_signal) = RunRegistration::new(self, &run_id);
        self.events.publish(DomainEvent::FlowRunStarted {
            run_id: run_id.clone(),
            flow_name: input.flow_name.clone(),
            collection: input.collection.clone(),
            total_nodes: flow.nodes.len(),
        });

        let mut captured: HashMap<String, CapturedOutput> = HashMap::new();
        let mut outcomes: HashMap<String, NodeOutcome> = HashMap::new();
        let mut steps: Vec<FlowStepResult> = Vec::new();
        let mut stopped_reason = "completed".to_string();

        for node_id in &order {
            if self.is_cancelled(&run_id) {
                stopped_reason = "cancelled".to_string();
                break;
            }

            let incoming: Vec<&FlowEdge> = flow
                .edges
                .iter()
                .filter(|e| e.target_node_id == *node_id)
                .collect();
            // Routing nodes may observe a failed Request's response (§6.3.1).
            let target_is_routing = matches!(
                nodes_by_id.get(node_id.as_str()).map(|n| &n.kind),
                Some(FlowNodeKind::If { .. }) | Some(FlowNodeKind::Switch { .. })
            );

            let (step, outcome) = match decide_fate(&incoming, &outcomes, target_is_routing) {
                NodeFate::Skip(reason) => {
                    (skipped_step(node_id, reason), NodeOutcome::Skipped(reason))
                }
                NodeFate::Fail(message) => {
                    self.publish_started(&run_id, node_id);
                    (
                        failed_step(node_id, message),
                        NodeOutcome::Failed { responded: false },
                    )
                }
                NodeFate::Run { data_edges } => {
                    self.publish_started(&run_id, node_id);
                    // `validate` only returns ids from `flow.nodes`, so a miss
                    // here is a bug. It fails this node instead of panicking.
                    let node_opt = nodes_by_id.get(node_id.as_str()).copied();
                    let mut node_logs = Vec::new();
                    let mut node_debug = None;
                    let mut node_exchange = None;
                    let mut node_poll_stats = None;
                    let mut ctx = NodeRunContext {
                        run_id: run_id.clone(),
                        node_id: node_id.clone(),
                        cancel: cancel_signal.clone(),
                    };
                    let result = match node_opt {
                        Some(node) => {
                            self.execute_node(
                                exec,
                                &input,
                                node,
                                &data_edges,
                                &captured,
                                &external_secrets,
                                &mut node_logs,
                                &mut node_debug,
                                &mut node_exchange,
                                &mut node_poll_stats,
                                &mut ctx,
                                &mut callbacks,
                            )
                            .await
                        }
                        None => Err(DomainError::Internal(format!(
                            "node '{node_id}' is missing from the flow"
                        ))),
                    };
                    // A cancel that landed while this node ran turns an error
                    // into "cancelled". A finished node keeps its real result.
                    let cancelled_now = ctx.cancel.is_cancelled();
                    let step = if cancelled_now && result.is_err() {
                        failed_step(node_id, "cancelled".to_string())
                    } else {
                        result_to_step(node_id, node_opt, &result)
                    };
                    let step = FlowStepResult {
                        logs: node_logs,
                        debug_request: node_debug,
                        exchange: node_exchange,
                        ..step
                    };
                    // A failed poll still shows its last response (§6.3).
                    let step = match node_poll_stats {
                        Some(stats) if result.is_err() => FlowStepResult {
                            status_code: Some(stats.status_code),
                            duration_ms: Some(stats.elapsed_ms),
                            attempts: Some(stats.attempts),
                            ..step
                        },
                        _ => step,
                    };
                    let outcome = match &result {
                        Ok(executed) if step.status == FlowNodeStatus::Success => {
                            NodeOutcome::Succeeded {
                                chosen_exit: executed.chosen_exit.clone(),
                            }
                        }
                        // `Ok` but not a success is only possible for a Request
                        // with a non-2xx response (see `result_to_step`). It
                        // has a response a routing node may observe (§6.3.1).
                        Ok(_) => NodeOutcome::Failed { responded: true },
                        Err(_) => NodeOutcome::Failed { responded: false },
                    };
                    // Every downstream consumer reads from this map, so a node
                    // with several dependents (fan-out) is captured once. A
                    // non-2xx Request is captured too — that is what a routing
                    // node observes. An `Err` captures nothing.
                    if let Ok(executed) = result {
                        captured.insert(node_id.clone(), executed.output);
                    }
                    if cancelled_now {
                        self.events.publish(step_completed_event(&run_id, &step));
                        steps.push(step);
                        stopped_reason = "cancelled".to_string();
                        break;
                    }
                    (step, outcome)
                }
            };

            self.events.publish(step_completed_event(&run_id, &step));
            outcomes.insert(node_id.clone(), outcome);
            steps.push(step);
        }

        // Deregister before `FlowRunFinished`, as before this guard existed.
        drop(registration);

        let failed_count = steps
            .iter()
            .filter(|s| s.status == FlowNodeStatus::Failed)
            .count();
        let skipped_count = steps
            .iter()
            .filter(|s| s.status == FlowNodeStatus::Skipped)
            .count();
        let not_taken_count = steps
            .iter()
            .filter(|s| s.skip_reason == Some(FlowSkipReason::BranchNotTaken))
            .count();
        self.events.publish(DomainEvent::FlowRunFinished {
            run_id: run_id.clone(),
            stopped_reason: stopped_reason.clone(),
            node_count: steps.len(),
            failed_count,
            skipped_count,
            not_taken_count,
        });

        Ok(FlowRunSummary {
            run_id,
            steps,
            stopped_reason,
        })
    }

    fn publish_started(&self, run_id: &str, node_id: &str) {
        self.events.publish(DomainEvent::FlowStepStarted {
            run_id: run_id.to_string(),
            node_id: node_id.to_string(),
        });
    }

    /// Reports progress for the node `ctx` belongs to. Waiting nodes call
    /// this.
    pub(crate) fn publish_progress(
        &self,
        ctx: &NodeRunContext,
        attempt: Option<u32>,
        max_attempts: Option<u32>,
        message: String,
    ) {
        self.events.publish(DomainEvent::FlowStepProgress {
            run_id: ctx.run_id.clone(),
            node_id: ctx.node_id.clone(),
            attempt,
            max_attempts,
            message,
        });
    }

    /// Dispatches one node by kind, feeding it only `data_edges` — the live,
    /// non-trigger edges `decide_fate` selected. Returns the node's captured
    /// output and chosen exit, or an error if the node itself failed.
    // The node needs the run's inputs, captured outputs and secrets, and
    // `logs` collects the console output, `debug` the debug record,
    // `exchange` the capped exchange record and `poll_stats` how a failed
    // poll went.
    #[allow(clippy::too_many_arguments)]
    async fn execute_node(
        &self,
        exec: &RequestExecutionService,
        input: &RunFlowInput,
        node: &FlowNode,
        data_edges: &[&FlowEdge],
        captured: &HashMap<String, CapturedOutput>,
        external_secrets: &HashMap<String, String>,
        logs: &mut Vec<FlowLogEntry>,
        debug: &mut Option<FlowDebugRequest>,
        exchange: &mut Option<FlowDebugRequest>,
        poll_stats: &mut Option<crate::flow_poll::FailedPollStats>,
        ctx: &mut NodeRunContext,
        callbacks: &mut crate::flow_callbacks::RunCallbacks,
    ) -> DomainResult<ExecutedNode> {
        // Script logs redact the same secrets a Request node's script does.
        let secret_values = exec.secret_values(
            input.global_env_name.as_deref(),
            Some(&input.collection),
            input.environment_name.as_deref(),
            external_secrets,
        );
        match &node.kind {
            FlowNodeKind::Input { value, .. } => {
                // Resolve with the scope a request uses (global < collection <
                // environment), so an If, Switch or Output sees the real value.
                // An unknown `{{name}}` stays as literal text.
                let vars = exec.build_variable_context(
                    input.global_env_name.as_deref(),
                    Some(&input.collection),
                    input.environment_name.as_deref(),
                    None,
                    external_secrets,
                );
                let resolved = rocket_environment::resolve(value.data(), &vars).output;
                // Wires get the raw value. The step shows it with secrets masked.
                let reported = crate::redaction::redact_secrets(&resolved, &secret_values);
                Ok(ExecutedNode {
                    reported_value: Some(reported),
                    ..ExecutedNode::plain(CapturedOutput::Value(VariableValue::simple(resolved)))
                })
            }
            FlowNodeKind::Output { .. } => {
                let Some(edge) = data_edges.first() else {
                    return Ok(ExecutedNode::plain(CapturedOutput::Value(
                        VariableValue::simple(""),
                    )));
                };
                // An Output node shows one value. Picking one of several wires
                // would silently drop the others, so this is an error.
                if data_edges.len() > 1 {
                    return Err(DomainError::InvalidInput(format!(
                        "output node '{}' has more than one incoming wire",
                        node.id
                    )));
                }
                let source_output = captured_source(node, edge, captured)?;
                let outcome = exec
                    .resolve_flow_wire_expression(
                        &input.collection,
                        source_output,
                        &edge.expression,
                        &secret_values,
                    )
                    .await;
                logs.extend(outcome.logs);
                let value = outcome.result?;
                Ok(ExecutedNode::plain(CapturedOutput::Value(
                    VariableValue::simple(value),
                )))
            }
            FlowNodeKind::Request {
                debug: debug_on,
                repeat_until,
                ..
            } => {
                let mut request_input = build_execute_request_input(
                    self.collection_repo.as_ref(),
                    &input.collection,
                    input.environment_name.as_deref(),
                    input.global_env_name.as_deref(),
                    node,
                )?;
                // Callback URLs (`{{callback.<name>}}`) resolve in every
                // field and script of every request in the run.
                request_input.flow_vars = callbacks.vars().clone();

                let mut resolved = HashMap::new();
                for edge in data_edges {
                    let source_output = captured_source(node, edge, captured)?;
                    let outcome = exec
                        .resolve_flow_wire_expression(
                            &input.collection,
                            source_output,
                            &edge.expression,
                            &secret_values,
                        )
                        .await;
                    logs.extend(outcome.logs);
                    let value = outcome.result?;
                    resolved.insert(edge.id.clone(), value);
                }
                let edges_owned: Vec<FlowEdge> = data_edges.iter().map(|e| (*e).clone()).collect();
                apply_wired_overrides(&mut request_input, &resolved, &edges_owned)?;

                // Wires were resolved once above; every attempt reuses them.
                if let Some(repeat) = repeat_until {
                    return self
                        .run_repeat_until(
                            exec,
                            input,
                            request_input,
                            repeat,
                            external_secrets,
                            &secret_values,
                            *debug_on,
                            logs,
                            debug,
                            exchange,
                            poll_stats,
                            ctx,
                        )
                        .await;
                }

                let mut sent = None;
                let result = exec
                    .execute_capturing(request_input, external_secrets, &mut sent)
                    .await;
                if let Some(sent) = &sent {
                    let error = result.as_ref().err().map(|e| e.to_string());
                    let record = build_debug_request(
                        sent,
                        result.as_ref().ok().map(|o| &o.response),
                        error.as_deref(),
                        &secret_values,
                    );
                    if *debug_on {
                        *debug = Some(record.clone());
                    }
                    *exchange = Some(cap_exchange(record));
                }
                let output = result?;
                logs.extend(to_flow_logs(output.console_entries.clone()));
                Ok(ExecutedNode::plain(CapturedOutput::Request(Box::new(
                    output,
                ))))
            }
            FlowNodeKind::If { condition, .. } => {
                let source = single_input(node, data_edges, captured)?;
                let outcome = exec
                    .evaluate_flow_route_expression(
                        &input.collection,
                        source,
                        condition,
                        FlowCoercion::Bool,
                        &secret_values,
                    )
                    .await;
                logs.extend(outcome.logs);
                let raw = outcome.result?;
                let chosen_exit = match raw.as_str() {
                    "true" => handle::TRUE,
                    "false" => handle::FALSE,
                    other => {
                        return Err(DomainError::InvalidInput(format!(
                            "condition of node '{}' evaluated to '{other}', expected true or false",
                            node.id
                        )))
                    }
                };
                Ok(ExecutedNode {
                    output: source.clone(),
                    chosen_exit: chosen_exit.to_string(),
                    poll: None,
                    reported_value: None,
                })
            }
            FlowNodeKind::Switch { value, cases, .. } => {
                let source = single_input(node, data_edges, captured)?;
                let outcome = exec
                    .evaluate_flow_route_expression(
                        &input.collection,
                        source,
                        value,
                        FlowCoercion::Str,
                        &secret_values,
                    )
                    .await;
                logs.extend(outcome.logs);
                let raw = outcome.result?;
                let chosen_exit = cases
                    .iter()
                    .find(|case| case.matches == raw)
                    .map(|case| handle::case_handle(&case.id))
                    .unwrap_or_else(|| handle::DEFAULT.to_string());
                Ok(ExecutedNode {
                    output: source.clone(),
                    chosen_exit,
                    poll: None,
                    reported_value: None,
                })
            }
            FlowNodeKind::WaitForCallback {
                timeout_ms,
                accept_when,
                ..
            } => {
                self.wait_for_callback(
                    exec,
                    input,
                    node,
                    *timeout_ms,
                    accept_when.as_deref(),
                    &secret_values,
                    logs,
                    exchange,
                    ctx,
                    callbacks,
                )
                .await
            }
            FlowNodeKind::Transform { script, .. } => {
                let source = single_input(node, data_edges, captured)?;
                let outcome = exec
                    .evaluate_flow_transform_script(
                        &input.collection,
                        source,
                        script,
                        &secret_values,
                    )
                    .await;
                logs.extend(outcome.logs);
                let text = outcome.result?;
                // Wires get the raw text. The step shows it with secrets masked.
                let reported = crate::redaction::redact_secrets(&text, &secret_values);
                Ok(ExecutedNode {
                    reported_value: Some(reported),
                    ..ExecutedNode::plain(CapturedOutput::Value(VariableValue::simple(text)))
                })
            }
        }
    }
}

/// The captured output of `edge`'s source. Topological order guarantees it
/// exists; a miss is reported as an internal error, not a panic.
fn captured_source<'c>(
    node: &FlowNode,
    edge: &FlowEdge,
    captured: &'c HashMap<String, CapturedOutput>,
) -> DomainResult<&'c CapturedOutput> {
    captured.get(&edge.source_node_id).ok_or_else(|| {
        DomainError::Internal(format!(
            "node '{}' depends on '{}' which has not executed yet — topological order violated",
            node.id, edge.source_node_id
        ))
    })
}

/// The captured output feeding a single-input node (If, Switch or Transform) through its one live input edge. `validate` (V1) and `decide_fate` guarantee exactly one;
/// anything else is reported, never panicked on. The source may be a
/// Request that failed with a non-2xx status (spec §6.3.1): its response
/// was captured and is used exactly like a successful one.
fn single_input<'c>(
    node: &FlowNode,
    data_edges: &[&FlowEdge],
    captured: &'c HashMap<String, CapturedOutput>,
) -> DomainResult<&'c CapturedOutput> {
    match data_edges {
        [edge] if edge.target_field == handle::INPUT => captured_source(node, edge, captured),
        _ => Err(DomainError::Internal(format!(
            "node '{}' needs exactly one live '{}' input, found {}",
            node.id,
            handle::INPUT,
            data_edges.len()
        ))),
    }
}

/// Turns one node's `execute_node` outcome into its `FlowStepResult`. A node
/// counts as failed when `execute_node` errored, or when it produced a
/// `Request` response that is not 2xx. A routing node always succeeds when
/// it evaluated, even though it passes a `Request` capture through, and
/// reports the exit it took in `branch`.
fn result_to_step(
    node_id: &str,
    node: Option<&FlowNode>,
    result: &DomainResult<ExecutedNode>,
) -> FlowStepResult {
    let kind = node.map(|n| &n.kind);
    let is_routing = matches!(
        kind,
        Some(FlowNodeKind::If { .. }) | Some(FlowNodeKind::Switch { .. })
    );
    let base = FlowStepResult {
        node_id: node_id.to_string(),
        status: FlowNodeStatus::Success,
        status_code: None,
        duration_ms: None,
        error: None,
        value: None,
        skip_reason: None,
        branch: None,
        logs: Vec::new(),
        debug_request: None,
        attempts: None,
        exchange: None,
    };
    match result {
        Ok(executed) if is_routing => FlowStepResult {
            branch: Some(executed.chosen_exit.clone()),
            ..base
        },
        // A poll that met its condition succeeds whatever the final status:
        // the author's condition decides "done".
        Ok(ExecutedNode {
            output: CapturedOutput::Request(out),
            poll: Some(stats),
            ..
        }) => FlowStepResult {
            status_code: Some(out.response.status),
            duration_ms: Some(stats.elapsed_ms),
            attempts: Some(stats.attempts),
            ..base
        },
        Ok(ExecutedNode {
            output: CapturedOutput::Request(out),
            ..
        }) => {
            let status = out.response.status;
            let success = out.response.is_success();
            let is_wait = matches!(kind, Some(FlowNodeKind::WaitForCallback { .. }));
            FlowStepResult {
                status: if success {
                    FlowNodeStatus::Success
                } else {
                    FlowNodeStatus::Failed
                },
                status_code: Some(status),
                duration_ms: Some(out.response.duration_ms),
                error: (!success).then(|| format!("non-2xx response: {status}")),
                // A Wait for callback node reports the method it received.
                value: is_wait.then(|| out.response.status_text.clone()),
                ..base
            }
        }
        Ok(ExecutedNode {
            output: CapturedOutput::Value(v),
            reported_value,
            ..
        }) => {
            // An Input or Transform node reports its masked value, an Output node its capture.
            let value = match kind {
                Some(FlowNodeKind::Input { .. }) | Some(FlowNodeKind::Transform { .. }) => {
                    reported_value
                        .clone()
                        .or_else(|| Some(v.data().to_string()))
                }
                Some(FlowNodeKind::Output { .. }) => Some(v.data().to_string()),
                _ => None,
            };
            FlowStepResult { value, ..base }
        }
        Err(e) => failed_step(node_id, e.to_string()),
    }
}

fn skipped_step(node_id: &str, reason: FlowSkipReason) -> FlowStepResult {
    FlowStepResult {
        node_id: node_id.to_string(),
        status: FlowNodeStatus::Skipped,
        status_code: None,
        duration_ms: None,
        error: None,
        value: None,
        skip_reason: Some(reason),
        branch: None,
        logs: Vec::new(),
        debug_request: None,
        attempts: None,
        exchange: None,
    }
}

fn failed_step(node_id: &str, message: String) -> FlowStepResult {
    FlowStepResult {
        node_id: node_id.to_string(),
        status: FlowNodeStatus::Failed,
        status_code: None,
        duration_ms: None,
        error: Some(message),
        value: None,
        skip_reason: None,
        branch: None,
        logs: Vec::new(),
        debug_request: None,
        attempts: None,
        exchange: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rocket_collection::{Collection, CollectionRepository, CollectionSettings, Request};
    use rocket_environment::{Environment, EnvironmentRepository};
    use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
    use rocket_http::{CookieJar, CookieRepository, HttpExecutor, HttpRequest, HttpResponse};
    use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
    use rocket_shared::events::{DomainEvent, EventPublisher};
    use std::sync::Arc;

    // ---- Minimal fakes shared by every task in this file ---------------
    // Only the methods this file's code paths actually call return real
    // values; everything else is `unimplemented!()` — standard practice for
    // a narrow test double (this crate's convention is inline, per-module
    // mocks; see `rocket-app/CLAUDE.md`).

    struct FakeCollectionRepo {
        requests: std::sync::Mutex<HashMap<(String, String), Request>>,
        settings: CollectionSettings,
    }
    impl FakeCollectionRepo {
        fn new() -> Self {
            Self {
                requests: std::sync::Mutex::new(HashMap::new()),
                settings: CollectionSettings::default(),
            }
        }
        fn with_settings(mut self, settings: CollectionSettings) -> Self {
            self.settings = settings;
            self
        }
        fn with_request(self, collection: &str, path: &str, request: Request) -> Self {
            self.requests
                .lock()
                .expect("lock FakeCollectionRepo")
                .insert((collection.to_string(), path.to_string()), request);
            self
        }
    }
    impl CollectionRepository for FakeCollectionRepo {
        fn list(&self) -> DomainResult<Vec<rocket_collection::CollectionSummary>> {
            unimplemented!()
        }
        fn get(&self, _name: &str) -> DomainResult<Collection> {
            unimplemented!()
        }
        fn get_summaries(&self, _name: &str) -> DomainResult<Collection> {
            unimplemented!()
        }
        fn create(&self, _name: &str) -> DomainResult<Collection> {
            unimplemented!()
        }
        fn delete(&self, _name: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn rename(&self, _old_name: &str, _new_name: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
            self.requests
                .lock()
                .expect("lock FakeCollectionRepo")
                .get(&(collection.to_string(), path.to_string()))
                .cloned()
                .ok_or_else(|| DomainError::NotFound(format!("{collection}/{path}")))
        }
        fn save_request(&self, _c: &str, _p: &str, _r: &Request) -> DomainResult<String> {
            unimplemented!()
        }
        fn rename_request(&self, _c: &str, _o: &str, _n: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete_request(&self, _c: &str, _p: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn create_folder(&self, _c: &str, _p: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete_folder(&self, _c: &str, _p: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn move_item(&self, _sc: &str, _sp: &str, _dc: &str, _dp: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn reorder_items(&self, _c: &str, _p: &str, _order: &[String]) -> DomainResult<()> {
            unimplemented!()
        }
        fn get_settings(&self, _name: &str) -> DomainResult<CollectionSettings> {
            Ok(self.settings.clone())
        }
        fn save_settings(&self, _name: &str, _settings: &CollectionSettings) -> DomainResult<()> {
            unimplemented!()
        }
        fn get_folder_chain_variables(
            &self,
            _c: &str,
            _p: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            Ok(Vec::new())
        }
        fn get_folder_variables(
            &self,
            _c: &str,
            _p: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            unimplemented!()
        }
        fn save_folder_variables(
            &self,
            _c: &str,
            _p: &str,
            _vars: Vec<rocket_collection::CollectionVariable>,
        ) -> DomainResult<()> {
            unimplemented!()
        }
        fn get_request_variables(
            &self,
            _c: &str,
            _p: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            Ok(Vec::new())
        }
        fn save_request_variables(
            &self,
            _c: &str,
            _p: &str,
            _vars: Vec<rocket_collection::CollectionVariable>,
        ) -> DomainResult<()> {
            unimplemented!()
        }
    }

    struct NullEnvRepo;
    impl EnvironmentRepository for NullEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(Vec::new())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            Err(DomainError::NotFound(name.to_string()))
        }
        fn save(&self, _env: &Environment) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete(&self, _name: &str) -> DomainResult<()> {
            unimplemented!()
        }
    }

    struct NullCookieRepo;
    impl CookieRepository for NullCookieRepo {
        fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
            Ok(Vec::new())
        }
        fn get_by_domain(&self, _domain: &str) -> DomainResult<Option<CookieJar>> {
            Ok(None)
        }
        fn save(&self, _jar: &CookieJar) -> DomainResult<()> {
            Ok(())
        }
        fn clear(&self) -> DomainResult<()> {
            Ok(())
        }
    }

    struct NullHistoryRepo;
    impl HistoryRepository for NullHistoryRepo {
        fn list(&self, _limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
            Ok(Vec::new())
        }
        fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
            Err(DomainError::NotFound(id.to_string()))
        }
        fn save(&self, _entry: &HistoryEntry) -> DomainResult<()> {
            Ok(())
        }
        fn clear(&self) -> DomainResult<()> {
            Ok(())
        }
        fn search(&self, _filter: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
            Ok(Vec::new())
        }
    }

    struct NullExecutor;
    #[async_trait]
    impl HttpExecutor for NullExecutor {
        async fn execute(&self, _request: &HttpRequest) -> DomainResult<HttpResponse> {
            unimplemented!("this test never dispatches a real HTTP call")
        }
    }

    struct NullEventPublisher;
    impl EventPublisher for NullEventPublisher {
        fn publish(&self, _event: DomainEvent) {}
    }

    struct EmptySecretManagerRepo;
    impl rocket_environment::SecretManagerRepository for EmptySecretManagerRepo {
        fn list(&self) -> DomainResult<Vec<rocket_environment::SecretManagerConnection>> {
            Ok(Vec::new())
        }
        fn get(
            &self,
            _id: &str,
        ) -> DomainResult<Option<rocket_environment::SecretManagerConnection>> {
            Ok(None)
        }
        fn save(&self, _c: &rocket_environment::SecretManagerConnection) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete(&self, _id: &str) -> DomainResult<()> {
            unimplemented!()
        }
    }

    /// Script engine stub that always resolves the jsonq snippet to a fixed
    /// value — mirrors `FixedJsonqEngine` in `execution_service.rs`'s own
    /// tests (this file cannot import that one, it's private to that
    /// module's `#[cfg(test)]`, so it is re-declared here per this crate's
    /// existing "each module owns its own inline mocks" convention).
    struct FixedJsonqEngine {
        value: serde_json::Value,
    }
    #[async_trait]
    impl ScriptEngine for FixedJsonqEngine {
        async fn execute(&self, _ctx: ScriptContext) -> DomainResult<ScriptResult> {
            let mut vars = HashMap::new();
            vars.insert("__jsonq_result__".to_string(), self.value.clone());
            Ok(ScriptResult {
                runtime_vars: vars,
                ..Default::default()
            })
        }
    }

    struct ErrorJsonqEngine;
    #[async_trait]
    impl ScriptEngine for ErrorJsonqEngine {
        async fn execute(&self, _ctx: ScriptContext) -> DomainResult<ScriptResult> {
            Ok(ScriptResult {
                error: Some("ReferenceError: nope".into()),
                ..Default::default()
            })
        }
    }

    /// What `ScriptedJsonqEngine` answers for one rule.
    enum Scripted {
        /// Resolve to this JSON value.
        Value(serde_json::Value),
        /// Report a script error with this message.
        Throw(&'static str),
        /// Compute the value from the response the expression runs against.
        FromResponse(fn(Option<&rocket_http::HttpResponse>) -> serde_json::Value),
    }

    /// Script engine that answers by the first rule whose needle occurs in
    /// the generated code, so route wrappers (`!!(`, `String(`) and plain
    /// wire expressions can be told apart in one run. Unmatched code
    /// resolves to `"https://api.example.com/wired"`.
    struct ScriptedJsonqEngine {
        rules: Vec<(&'static str, Scripted)>,
    }
    #[async_trait]
    impl ScriptEngine for ScriptedJsonqEngine {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            let rule = self
                .rules
                .iter()
                .find(|(needle, _)| ctx.code.contains(needle))
                .map(|(_, answer)| answer);
            let value = match rule {
                Some(Scripted::Value(v)) => v.clone(),
                Some(Scripted::Throw(message)) => {
                    return Ok(ScriptResult {
                        error: Some((*message).to_string()),
                        ..Default::default()
                    })
                }
                Some(Scripted::FromResponse(f)) => f(ctx.response.as_ref()),
                None => serde_json::json!("https://api.example.com/wired"),
            };
            let mut vars = HashMap::new();
            vars.insert("__jsonq_result__".to_string(), value);
            Ok(ScriptResult {
                runtime_vars: vars,
                ..Default::default()
            })
        }
    }

    fn scripted(rules: Vec<(&'static str, Scripted)>) -> Box<dyn ScriptEngine> {
        Box::new(ScriptedJsonqEngine { rules })
    }

    #[tokio::test]
    async fn route_expression_passes_the_raw_source_and_applies_coercion() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            scripted(vec![(
                "response.status === 200",
                Scripted::Value(serde_json::json!(true)),
            )]),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .evaluate_flow_route_expression(
                "my-api",
                &output,
                "response.status === 200",
                FlowCoercion::Bool,
                &HashSet::new(),
            )
            .await
            .result
            .expect("route expression must resolve");

        assert_eq!(value, "true");
    }

    #[tokio::test]
    async fn route_expression_null_is_the_string_null_not_an_error() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::Value::Null,
            }),
        );
        let output = CapturedOutput::Value(VariableValue::simple("x"));

        let value = svc
            .evaluate_flow_route_expression(
                "my-api",
                &output,
                "response.body.plan",
                FlowCoercion::Str,
                &HashSet::new(),
            )
            .await
            .result
            .expect("a null route result must not be an error");

        assert_eq!(value, "null");
    }

    #[tokio::test]
    async fn route_expression_returns_strings_unquoted() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("pro"),
            }),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .evaluate_flow_route_expression(
                "my-api",
                &output,
                "response.body.plan",
                FlowCoercion::Str,
                &HashSet::new(),
            )
            .await
            .result
            .expect("resolve");

        assert_eq!(value, "pro");
    }

    #[tokio::test]
    async fn route_expression_stringifies_non_string_results() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!(200),
            }),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .evaluate_flow_route_expression(
                "my-api",
                &output,
                "response.status",
                FlowCoercion::Str,
                &HashSet::new(),
            )
            .await
            .result
            .expect("resolve");

        assert_eq!(value, "200");
    }

    #[tokio::test]
    async fn route_expression_script_error_is_invalid_input() {
        let svc = service_with_engine(FakeCollectionRepo::new(), Box::new(ErrorJsonqEngine));
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let err = svc
            .evaluate_flow_route_expression(
                "my-api",
                &output,
                "nope.nope",
                FlowCoercion::Bool,
                &HashSet::new(),
            )
            .await
            .result
            .expect_err("a throwing route expression must be an Err");

        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    fn service_with_engine(
        collection_repo: FakeCollectionRepo,
        engine: Box<dyn ScriptEngine>,
    ) -> RequestExecutionService {
        RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(NullExecutor),
            Box::new(NullHistoryRepo),
            Box::new(collection_repo),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(engine)
    }

    fn sample_response_output() -> ExecuteRequestOutput {
        ExecuteRequestOutput {
            response: HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: r#"{"token":"abc123"}"#.into(),
                duration_ms: 10,
                ttfb_ms: 5,
                size_bytes: 20,
            },
            test_results: Vec::new(),
            console_entries: Vec::new(),
            script_error: None,
            deferred_history: None,
        }
    }

    #[tokio::test]
    async fn resolves_expression_against_request_output() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("abc123"),
            }),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "response.body", &HashSet::new())
            .await
            .result
            .expect("expression should resolve");

        assert_eq!(value, "abc123");
    }

    #[tokio::test]
    async fn resolves_expression_against_input_node_value() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("hello"),
            }),
        );
        let output = CapturedOutput::Value(VariableValue::simple("hello"));

        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "response.body", &HashSet::new())
            .await
            .result
            .expect("expression should resolve");

        assert_eq!(value, "hello");
    }

    #[tokio::test]
    async fn script_error_surfaces_as_domain_error_not_panic() {
        let svc = service_with_engine(FakeCollectionRepo::new(), Box::new(ErrorJsonqEngine));
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let err = svc
            .resolve_flow_wire_expression("my-api", &output, "response.nope.nope", &HashSet::new())
            .await
            .result
            .expect_err("a throwing expression must be an Err, not a panic");

        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn null_result_is_invalid_input_not_the_string_null() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::Value::Null,
            }),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let err = svc
            .resolve_flow_wire_expression(
                "my-api",
                &output,
                "response.body.missing",
                &HashSet::new(),
            )
            .await
            .result
            .expect_err("a null result must be an Err, not Ok(\"null\")");

        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn non_string_result_is_stringified() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!(42),
            }),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "response.status", &HashSet::new())
            .await
            .result
            .expect("a number must resolve");

        assert_eq!(value, "42");
    }

    use rocket_flow::{InlineHeader, NodePosition};

    fn saved_flow_node(id: &str, request_path: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Request {
                debug: false,
                repeat_until: None,
                label: "Get Auth Token".to_string(),
                source: RequestSource::Saved {
                    request_path: request_path.to_string(),
                },
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn inline_flow_node(id: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Request {
                debug: false,
                repeat_until: None,
                label: "Ping".to_string(),
                source: RequestSource::Inline {
                    request: InlineRequestData {
                        method: "post".to_string(),
                        url: "https://api.example.com/ping".to_string(),
                        headers: vec![InlineHeader {
                            name: "X-Test".to_string(),
                            value: "1".to_string(),
                        }],
                        body: Some(r#"{"ok":true}"#.to_string()),
                    },
                },
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    #[test]
    fn saved_source_resolves_via_collection_repo_and_reuses_build_step_input() {
        let mut saved = Request::new(
            "Get Auth Token",
            HttpMethod::Get,
            "https://api.example.com/login",
        );
        saved.tags = vec!["auth".to_string()];
        let repo = FakeCollectionRepo::new().with_request("my-api", "auth/login.yml", saved);

        let node = saved_flow_node("n1", "auth/login.yml");
        let input = build_execute_request_input(&repo, "my-api", Some("dev"), None, &node)
            .expect("saved source must resolve");

        assert_eq!(input.method, HttpMethod::Get);
        assert_eq!(input.url, "https://api.example.com/login");
        assert_eq!(input.collection.as_deref(), Some("my-api"));
        assert_eq!(input.environment_name.as_deref(), Some("dev"));
        assert_eq!(input.request_path.as_deref(), Some("auth/login.yml"));
        assert_eq!(input.tags, vec!["auth".to_string()]);
    }

    #[test]
    fn build_execute_request_input_threads_global_env_name() {
        let mut saved = Request::new(
            "Get Auth Token",
            HttpMethod::Get,
            "https://api.example.com/login",
        );
        saved.tags = vec!["auth".to_string()];
        let repo = FakeCollectionRepo::new().with_request("my-api", "auth/login.yml", saved);

        let node = saved_flow_node("n1", "auth/login.yml");
        let input =
            build_execute_request_input(&repo, "my-api", Some("dev"), Some("shared-global"), &node)
                .expect("saved source must resolve");

        assert_eq!(input.global_env_name.as_deref(), Some("shared-global"));
    }

    #[test]
    fn saved_source_propagates_not_found_instead_of_defaulting() {
        let repo = FakeCollectionRepo::new();
        let node = saved_flow_node("n1", "does/not/exist.yml");

        let err = build_execute_request_input(&repo, "my-api", None, None, &node)
            .expect_err("a missing saved request must error, not silently build an empty request");

        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn inline_source_builds_request_from_embedded_fields() {
        let repo = FakeCollectionRepo::new();
        let node = inline_flow_node("n2");

        let input = build_execute_request_input(&repo, "my-api", None, None, &node)
            .expect("inline source must build");

        assert_eq!(input.method, HttpMethod::Post);
        assert_eq!(input.url, "https://api.example.com/ping");
        assert_eq!(input.headers.len(), 1);
        assert_eq!(input.headers[0].key, "X-Test");
        assert_eq!(input.headers[0].value, "1");
        let body = input.body.expect("inline body must be set");
        assert_eq!(body.content.as_deref(), Some(r#"{"ok":true}"#));
        assert_eq!(body.mode, BodyMode::Json);
    }

    #[test]
    fn inline_source_with_unparseable_method_is_invalid_input_not_a_panic() {
        let repo = FakeCollectionRepo::new();
        let mut node = inline_flow_node("n2");
        if let FlowNodeKind::Request {
            source: RequestSource::Inline { request },
            ..
        } = &mut node.kind
        {
            request.method = "FETCH".to_string();
        }

        let err = build_execute_request_input(&repo, "my-api", None, None, &node)
            .expect_err("an invalid method string must be InvalidInput");

        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn non_request_node_is_rejected() {
        let repo = FakeCollectionRepo::new();
        let node = FlowNode {
            id: "n3".to_string(),
            kind: FlowNodeKind::Output {
                label: "Result".to_string(),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        };

        let err = build_execute_request_input(&repo, "my-api", None, None, &node)
            .expect_err("an Output node has no request to build");

        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    fn edge(id: &str, target_node: &str, target_field: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: "src".to_string(),
            target_node_id: target_node.to_string(),
            target_field: target_field.to_string(),
            expression: "response.body".to_string(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        }
    }

    fn sample_execute_input() -> ExecuteRequestInput {
        let repo = FakeCollectionRepo::new();
        build_execute_request_input(&repo, "my-api", None, None, &inline_flow_node("n2"))
            .expect("build sample input")
    }

    #[test]
    fn empty_overrides_leave_input_unchanged() {
        let input = sample_execute_input();
        let mut mutated = input.clone();
        apply_wired_overrides(&mut mutated, &HashMap::new(), &[]).expect("no-op must succeed");
        // Compare every field, not just the ones a wire can target.
        assert_eq!(
            serde_json::to_value(&mutated).expect("serialize mutated"),
            serde_json::to_value(&input).expect("serialize input")
        );
    }

    #[test]
    fn edge_without_resolved_value_is_skipped() {
        let input = sample_execute_input();
        let mut mutated = input.clone();
        let edges = vec![edge("e1", "n2", "url")];

        apply_wired_overrides(&mut mutated, &HashMap::new(), &edges).expect("skip must succeed");

        assert_eq!(mutated.url, input.url);
    }

    #[test]
    fn url_override_replaces_url() {
        let mut input = sample_execute_input();
        let edges = vec![edge("e1", "n2", "url")];
        let mut resolved = HashMap::new();
        resolved.insert(
            "e1".to_string(),
            "https://api.example.com/v2/ping".to_string(),
        );

        apply_wired_overrides(&mut input, &resolved, &edges).expect("url override must apply");

        assert_eq!(input.url, "https://api.example.com/v2/ping");
    }

    #[test]
    fn header_value_override_replaces_the_named_index() {
        let mut input = sample_execute_input();
        let edges = vec![edge("e1", "n2", "headers[0].value")];
        let mut resolved = HashMap::new();
        resolved.insert("e1".to_string(), "42".to_string());

        apply_wired_overrides(&mut input, &resolved, &edges).expect("header override must apply");

        assert_eq!(input.headers[0].value, "42");
    }

    #[test]
    fn header_value_override_out_of_range_is_an_error_not_a_panic() {
        let mut input = sample_execute_input();
        let out_of_range = input.headers.len();
        let edges = vec![edge("e1", "n2", &format!("headers[{out_of_range}].value"))];
        let mut resolved = HashMap::new();
        resolved.insert("e1".to_string(), "42".to_string());

        let err = apply_wired_overrides(&mut input, &resolved, &edges)
            .expect_err("an out-of-range header index must error");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn body_override_replaces_body_content() {
        let mut input = sample_execute_input();
        let edges = vec![edge("e1", "n2", "body")];
        let mut resolved = HashMap::new();
        resolved.insert("e1".to_string(), r#"{"replaced":true}"#.to_string());

        apply_wired_overrides(&mut input, &resolved, &edges).expect("body override must apply");

        assert_eq!(
            input.body.expect("body must be set").content.as_deref(),
            Some(r#"{"replaced":true}"#)
        );
    }

    fn single_override(field: &str, value: &str) -> (Vec<FlowEdge>, HashMap<String, String>) {
        let mut resolved = HashMap::new();
        resolved.insert("e1".to_string(), value.to_string());
        (vec![edge("e1", "n2", field)], resolved)
    }

    #[test]
    fn named_header_override_updates_matching_header_case_insensitively() {
        let mut input = sample_execute_input();
        let (edges, resolved) = single_override("headers[x-test].value", "42");

        apply_wired_overrides(&mut input, &resolved, &edges).expect("named override must apply");

        assert_eq!(input.headers.len(), 1);
        assert_eq!(input.headers[0].key, "X-Test");
        assert_eq!(input.headers[0].value, "42");
    }

    #[test]
    fn named_header_override_appends_missing_header() {
        let mut input = sample_execute_input();
        let (edges, resolved) = single_override("headers[Authorization].value", "Bearer abc");

        apply_wired_overrides(&mut input, &resolved, &edges).expect("named override must append");

        assert_eq!(input.headers.len(), 2);
        assert_eq!(input.headers[1].key, "Authorization");
        assert_eq!(input.headers[1].value, "Bearer abc");
        assert!(input.headers[1].enabled);
    }

    #[test]
    fn header_override_enables_a_disabled_header() {
        let mut input = sample_execute_input();
        input.headers[0].enabled = false;
        let (edges, resolved) = single_override("headers[0].value", "42");

        apply_wired_overrides(&mut input, &resolved, &edges).expect("header override must apply");

        assert!(
            input.headers[0].enabled,
            "a wired header must actually be sent"
        );
    }

    #[test]
    fn empty_header_selector_is_an_error() {
        let mut input = sample_execute_input();
        let (edges, resolved) = single_override("headers[].value", "42");

        let err = apply_wired_overrides(&mut input, &resolved, &edges)
            .expect_err("an empty header selector must error");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn body_override_promotes_none_mode_to_json() {
        let mut input = sample_execute_input();
        input.body = Some(Body {
            mode: BodyMode::None,
            content: None,
            form_data: None,
            file_path: None,
        });
        let (edges, resolved) = single_override("body", r#"{"a":1}"#);

        apply_wired_overrides(&mut input, &resolved, &edges).expect("body override must apply");

        let body = input.body.expect("body must be set");
        assert_eq!(body.mode, BodyMode::Json);
        assert_eq!(body.content.as_deref(), Some(r#"{"a":1}"#));
    }

    #[test]
    fn body_override_keeps_text_like_mode() {
        let mut input = sample_execute_input();
        if let Some(body) = input.body.as_mut() {
            body.mode = BodyMode::Xml;
        }
        let (edges, resolved) = single_override("body", "<a/>");

        apply_wired_overrides(&mut input, &resolved, &edges).expect("body override must apply");

        assert_eq!(input.body.expect("body must be set").mode, BodyMode::Xml);
    }

    #[test]
    fn body_override_into_form_body_is_an_error_not_a_silent_noop() {
        let mut input = sample_execute_input();
        input.body = Some(Body {
            mode: BodyMode::FormUrlEncoded,
            content: None,
            form_data: Some(Vec::new()),
            file_path: None,
        });
        let (edges, resolved) = single_override("body", "x");

        let err = apply_wired_overrides(&mut input, &resolved, &edges)
            .expect_err("a form body ignores content, so a wire into it must error");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn unrecognized_target_field_is_an_error_not_a_silent_noop() {
        let mut input = sample_execute_input();
        let edges = vec![edge("e1", "n2", "auth.token")];
        let mut resolved = HashMap::new();
        resolved.insert("e1".to_string(), "x".to_string());

        let err = apply_wired_overrides(&mut input, &resolved, &edges)
            .expect_err("an unrecognized target_field must error, not silently do nothing");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    use rocket_flow::{Flow, FlowRepository};

    struct FakeFlowRepository {
        flows: std::sync::Mutex<HashMap<(String, String), Flow>>,
    }
    impl FakeFlowRepository {
        fn new() -> Self {
            Self {
                flows: std::sync::Mutex::new(HashMap::new()),
            }
        }
        fn with_flow(self, collection: &str, flow: Flow) -> Self {
            self.flows
                .lock()
                .expect("lock FakeFlowRepository")
                .insert((collection.to_string(), flow.name.clone()), flow);
            self
        }
    }
    impl FlowRepository for FakeFlowRepository {
        fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
            Ok(self
                .flows
                .lock()
                .expect("lock FakeFlowRepository")
                .keys()
                .filter(|(c, _)| c == collection)
                .map(|(_, name)| name.clone())
                .collect())
        }
        fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
            self.flows
                .lock()
                .expect("lock FakeFlowRepository")
                .get(&(collection.to_string(), name.to_string()))
                .cloned()
                .ok_or_else(|| DomainError::NotFound(format!("{collection}/{name}")))
        }
        fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
            self.flows
                .lock()
                .expect("lock FakeFlowRepository")
                .insert((collection.to_string(), flow.name.clone()), flow.clone());
            Ok(())
        }
        fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
            self.flows
                .lock()
                .expect("lock FakeFlowRepository")
                .remove(&(collection.to_string(), name.to_string()));
            Ok(())
        }
    }

    fn service_with_flow(flow: Flow) -> FlowExecutionService {
        FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", flow)),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullEventPublisher),
        )
    }

    fn linear_flow() -> Flow {
        Flow {
            name: "auth-flow".to_string(),
            nodes: vec![
                FlowNode {
                    id: "a".to_string(),
                    kind: FlowNodeKind::Input {
                        label: "Username".to_string(),
                        value: VariableValue::simple("bob"),
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                FlowNode {
                    id: "b".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "Result".to_string(),
                    },
                    position: NodePosition { x: 100.0, y: 0.0 },
                },
            ],
            edges: vec![FlowEdge {
                id: "e1".to_string(),
                source_node_id: "a".to_string(),
                target_node_id: "b".to_string(),
                target_field: "value".to_string(),
                expression: "response.body".to_string(),
                source_handle: rocket_flow::handle::RESULT.to_string(),
            }],
            callback_host: None,
        }
    }

    fn cyclic_flow() -> Flow {
        Flow {
            name: "cyclic".to_string(),
            nodes: vec![
                FlowNode {
                    id: "a".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "A".to_string(),
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                FlowNode {
                    id: "b".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "B".to_string(),
                    },
                    position: NodePosition { x: 100.0, y: 0.0 },
                },
            ],
            edges: vec![
                FlowEdge {
                    id: "e1".to_string(),
                    source_node_id: "a".to_string(),
                    target_node_id: "b".to_string(),
                    target_field: "value".to_string(),
                    expression: "response.body".to_string(),
                    source_handle: rocket_flow::handle::RESULT.to_string(),
                },
                FlowEdge {
                    id: "e2".to_string(),
                    source_node_id: "b".to_string(),
                    target_node_id: "a".to_string(),
                    target_field: "value".to_string(),
                    expression: "response.body".to_string(),
                    source_handle: rocket_flow::handle::RESULT.to_string(),
                },
            ],
            callback_host: None,
        }
    }

    #[test]
    fn load_ordered_nodes_returns_dependency_order_for_a_valid_flow() {
        let service = service_with_flow(linear_flow());
        let (flow, order) = service
            .load_ordered_nodes("my-api", "auth-flow")
            .expect("valid flow must load and sort");
        assert_eq!(flow.name, "auth-flow");
        assert_eq!(order, vec!["a".to_string(), "b".to_string()]);
    }

    #[tokio::test]
    async fn a_run_reports_the_captured_value_of_output_and_input_nodes() {
        let service = service_with_flow(linear_flow());
        let exec = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("bob"),
            }),
        );

        let summary = service
            .run(&exec, run_input("auth-flow"))
            .await
            .expect("run must succeed");

        let step_for = |id: &str| {
            summary
                .steps
                .iter()
                .find(|s| s.node_id == id)
                .expect("step must be recorded")
        };
        assert_eq!(
            step_for("b").value.as_deref(),
            Some("bob"),
            "the Output node must report its captured value"
        );
        assert_eq!(
            step_for("a").value.as_deref(),
            Some("bob"),
            "an Input node reports its resolved value"
        );
    }

    #[test]
    fn load_ordered_nodes_rejects_a_cyclic_flow() {
        let service = service_with_flow(cyclic_flow());
        let err = service
            .load_ordered_nodes("my-api", "cyclic")
            .expect_err("a cyclic flow must not load for execution");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn load_ordered_nodes_propagates_unknown_flow_name() {
        let service = service_with_flow(linear_flow());
        let err = service
            .load_ordered_nodes("my-api", "does-not-exist")
            .expect_err("an unknown flow name must error");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn cancel_on_unknown_run_id_is_a_harmless_noop() {
        let service = service_with_flow(linear_flow());
        service.cancel("no-such-run-id"); // must not panic
    }

    fn request_flow_node(id: &str, url: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Request {
                debug: false,
                repeat_until: None,
                label: format!("Node {id}"),
                source: RequestSource::Inline {
                    request: InlineRequestData {
                        method: "get".to_string(),
                        url: url.to_string(),
                        headers: Vec::new(),
                        body: None,
                    },
                },
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn wait_node(id: &str, name: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::WaitForCallback {
                label: format!("Wait {id}"),
                name: name.to_string(),
                timeout_ms: 1000,
                accept_when: None,
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn service_with_listener(
        flow: Flow,
        fake: &Arc<crate::test_doubles::FakeCallbackListener>,
    ) -> FlowExecutionService {
        service_with_flow(flow).with_callback_listener(Box::new(Arc::clone(fake)))
    }

    fn wait_node_with(id: &str, timeout_ms: u64, accept_when: Option<&str>) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::WaitForCallback {
                label: format!("Wait {id}"),
                name: "payment".to_string(),
                timeout_ms,
                accept_when: accept_when.map(str::to_string),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn event_call(event: &str) -> crate::callback_listener::ReceivedCall {
        crate::callback_listener::ReceivedCall {
            method: "POST".to_string(),
            path: "/cb/0".to_string(),
            query: Vec::new(),
            headers: Vec::new(),
            body: format!(r#"{{"event":"{event}","orderId":42}}"#),
        }
    }

    /// `register -> wait (Run when)`.
    fn register_then_wait(wait: FlowNode) -> Flow {
        Flow {
            name: "cb".to_string(),
            nodes: vec![
                request_flow_node("reg", "https://api.example.com/register"),
                wait,
            ],
            edges: vec![trigger_edge("e1", "reg", handle::RESULT, "w")],
            callback_host: None,
        }
    }

    struct FixedResponseExecutor {
        status: u16,
    }
    #[async_trait]
    impl HttpExecutor for FixedResponseExecutor {
        async fn execute(&self, _request: &HttpRequest) -> DomainResult<HttpResponse> {
            Ok(HttpResponse {
                status: self.status,
                status_text: "OK".into(),
                headers: vec![],
                body: r#"{"value":"ok"}"#.into(),
                duration_ms: 5,
                ttfb_ms: 2,
                size_bytes: 15,
            })
        }
    }

    /// Returns a 500 for `failing_url` and a 200 for everything else — used
    /// to prove a failure on one node does not affect an unrelated node's
    /// own HTTP outcome, which a single shared status (`exec_with_status`)
    /// cannot express since it applies the same status to every request.
    struct UrlAwareExecutor {
        failing_url: String,
    }
    #[async_trait]
    impl HttpExecutor for UrlAwareExecutor {
        async fn execute(&self, request: &HttpRequest) -> DomainResult<HttpResponse> {
            let status = if request.url == self.failing_url {
                500
            } else {
                200
            };
            Ok(HttpResponse {
                status,
                status_text: "status".into(),
                headers: vec![],
                body: r#"{"value":"ok"}"#.into(),
                duration_ms: 5,
                ttfb_ms: 2,
                size_bytes: 15,
            })
        }
    }

    fn exec_failing_for_url(failing_url: &str) -> RequestExecutionService {
        RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(UrlAwareExecutor {
                failing_url: failing_url.to_string(),
            }),
            Box::new(NullHistoryRepo),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(FixedJsonqEngine {
            value: serde_json::json!("ok"),
        }))
    }

    fn exec_with_status(status: u16) -> RequestExecutionService {
        RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(FixedResponseExecutor { status }),
            Box::new(NullHistoryRepo),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(FixedJsonqEngine {
            value: serde_json::json!("ok"),
        }))
    }

    #[tokio::test]
    async fn empty_flow_completes_immediately_with_no_steps() {
        let service = service_with_flow(Flow {
            name: "empty".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            callback_host: None,
        });
        let exec = exec_with_status(200);

        let summary = service
            .run(
                &exec,
                RunFlowInput {
                    collection: "my-api".to_string(),
                    flow_name: "empty".to_string(),
                    environment_name: None,
                    global_env_name: None,
                },
            )
            .await
            .expect("empty flow must run cleanly");

        assert!(summary.steps.is_empty());
    }

    #[tokio::test]
    async fn single_request_node_executes_and_reports_success() {
        let service = service_with_flow(Flow {
            name: "one-node".to_string(),
            nodes: vec![request_flow_node("a", "https://api.example.com/ping")],
            edges: Vec::new(),
            callback_host: None,
        });
        let exec = exec_with_status(200);

        let summary = service
            .run(
                &exec,
                RunFlowInput {
                    collection: "my-api".to_string(),
                    flow_name: "one-node".to_string(),
                    environment_name: None,
                    global_env_name: None,
                },
            )
            .await
            .expect("run must succeed");

        assert_eq!(summary.steps.len(), 1);
        assert_eq!(summary.steps[0].node_id, "a");
        assert_eq!(
            summary.steps[0].status,
            rocket_shared::events::FlowNodeStatus::Success
        );
        assert_eq!(summary.steps[0].status_code, Some(200));
    }

    #[tokio::test]
    async fn unknown_flow_name_errors_before_publishing_started() {
        let service = service_with_flow(linear_flow());
        let exec = exec_with_status(200);

        let err = service
            .run(
                &exec,
                RunFlowInput {
                    collection: "my-api".to_string(),
                    flow_name: "does-not-exist".to_string(),
                    environment_name: None,
                    global_env_name: None,
                },
            )
            .await
            .expect_err("unknown flow name must error");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn cancelling_before_the_run_starts_stops_it_immediately() {
        let service = service_with_flow(Flow {
            name: "two-nodes".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: Vec::new(),
            callback_host: None,
        });
        let exec = exec_with_status(200);
        // Cancel before run() is even called is not directly expressible (run_id
        // is generated inside run()); instead this test exercises the same
        // cancellation flag path by cancelling a run id it already knows the
        // service will not have registered, confirming no panic and normal
        // completion — the true mid-run cancellation race is covered by an
        // integration-level test once Plan 07 exposes cancel_flow_run over IPC.
        service.cancel("irrelevant-run-id");
        let summary = service
            .run(
                &exec,
                RunFlowInput {
                    collection: "my-api".to_string(),
                    flow_name: "two-nodes".to_string(),
                    environment_name: None,
                    global_env_name: None,
                },
            )
            .await
            .expect("run must still complete normally");
        assert_eq!(summary.steps.len(), 2);
    }

    #[tokio::test]
    async fn failed_node_skips_only_its_downstream_dependents() {
        // a (fails, 500) -> b (depends on a)      -- b must be Skipped
        // c (independent, succeeds)               -- c must still run
        let flow = Flow {
            name: "skip-cascade".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
                request_flow_node("c", "https://api.example.com/c"),
            ],
            edges: vec![FlowEdge {
                id: "e1".to_string(),
                source_node_id: "a".to_string(),
                target_node_id: "b".to_string(),
                target_field: "url".to_string(),
                expression: "response.body".to_string(),
                source_handle: rocket_flow::handle::RESULT.to_string(),
            }],
            callback_host: None,
        };
        let service = service_with_flow(flow);
        let exec = exec_failing_for_url("https://api.example.com/a");

        let summary = service
            .run(
                &exec,
                RunFlowInput {
                    collection: "my-api".to_string(),
                    flow_name: "skip-cascade".to_string(),
                    environment_name: None,
                    global_env_name: None,
                },
            )
            .await
            .expect("run must complete even with a failed node");

        let status_of = |id: &str| {
            summary
                .steps
                .iter()
                .find(|s| s.node_id == id)
                .expect("step must be recorded for this node id")
                .status
        };
        assert_eq!(status_of("a"), FlowNodeStatus::Failed);
        assert_eq!(status_of("b"), FlowNodeStatus::Skipped);
        assert_eq!(status_of("c"), FlowNodeStatus::Success);
        assert_eq!(
            summary
                .steps
                .iter()
                .filter(|s| s.status == FlowNodeStatus::Failed)
                .count(),
            1
        );
        assert_eq!(
            summary
                .steps
                .iter()
                .filter(|s| s.status == FlowNodeStatus::Skipped)
                .count(),
            1
        );
    }

    // ---- Whole-plan review: cancellation, fan-out, wire failure, secrets --

    use crate::test_doubles::{
        FakeSecretManagerRepo, FakeSecretStore, FakeVaultSecretFetcher, RecordingExecutor,
        RecordingPublisher, SharedExecutor, SharedPublisher, StaticEnvRepo,
    };

    fn wire(id: &str, from: &str, to: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: from.to_string(),
            target_node_id: to.to_string(),
            target_field: "url".to_string(),
            expression: "response.body".to_string(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        }
    }

    fn run_input(flow_name: &str) -> RunFlowInput {
        RunFlowInput {
            collection: "my-api".to_string(),
            flow_name: flow_name.to_string(),
            environment_name: None,
            global_env_name: None,
        }
    }

    #[tokio::test]
    async fn a_run_resolves_a_global_env_placeholder_in_an_inline_requests_url() {
        let mut global_env = rocket_environment::Environment::new("shared-global");
        global_env.set_variable(rocket_environment::Variable::new("ORG_ID", "acme"));

        let executor = RecordingExecutor::new();
        let exec = RequestExecutionService::new(
            Box::new(StaticEnvRepo(global_env)),
            Arc::new(SharedExecutor(Arc::clone(&executor))),
            Box::new(NullHistoryRepo),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let flow = Flow {
            name: "global-env-flow".to_string(),
            nodes: vec![request_flow_node("a", "https://api.example.com/{{ORG_ID}}")],
            edges: Vec::new(),
            callback_host: None,
        };
        let service = service_with_flow(flow);

        let mut input = run_input("global-env-flow");
        input.global_env_name = Some("shared-global".to_string());
        let summary = service.run(&exec, input).await.expect("run must succeed");

        assert_eq!(summary.steps[0].status_code, Some(200));
        assert_eq!(
            executor.sent_urls(),
            vec!["https://api.example.com/acme".to_string()],
            "the flow-executed request must resolve {{ORG_ID}} against the global environment"
        );
    }

    // ---- Input node variable scope ----------------------------------------

    fn env_with(vars: &[(&str, &str)]) -> rocket_environment::Environment {
        let mut env = rocket_environment::Environment::new("dev");
        for (k, v) in vars {
            env.set_variable(rocket_environment::Variable::new(*k, *v));
        }
        env
    }

    fn collection_var(key: &str, value: &str) -> rocket_collection::CollectionVariable {
        rocket_collection::CollectionVariable {
            key: key.to_string(),
            value: value.to_string(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        }
    }

    /// Execution service with an environment repo, collection variables, and
    /// a script engine that answers from the response body (routes: whether
    /// the body equals `acme`).
    fn scoped_exec(
        env: rocket_environment::Environment,
        collection_vars: Vec<rocket_collection::CollectionVariable>,
    ) -> RequestExecutionService {
        let settings = CollectionSettings {
            variables: collection_vars,
            ..Default::default()
        };
        RequestExecutionService::new(
            Box::new(StaticEnvRepo(env)),
            Arc::new(SharedExecutor(RecordingExecutor::new())),
            Box::new(NullHistoryRepo),
            Box::new(FakeCollectionRepo::new().with_settings(settings)),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(scripted(vec![
            (
                "!!(",
                Scripted::FromResponse(|r| {
                    serde_json::json!(r.map(|r| r.body == "acme").unwrap_or(false))
                }),
            ),
            (
                "response.body",
                Scripted::FromResponse(|r| {
                    serde_json::json!(r.map(|r| r.body.clone()).unwrap_or_default())
                }),
            ),
        ]))
    }

    fn input_node_with(id: &str, v: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Input {
                label: id.to_string(),
                value: VariableValue::simple(v),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn output_node_named(id: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Output {
                label: id.to_string(),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    /// Runs `input -> out` and returns the Output step's value.
    async fn output_of_input(
        text: &str,
        exec: &RequestExecutionService,
        environment: Option<&str>,
        global: Option<&str>,
    ) -> Option<String> {
        let flow = Flow {
            name: "scope".to_string(),
            nodes: vec![input_node_with("in", text), output_node_named("out")],
            edges: vec![FlowEdge {
                target_field: "value".to_string(),
                ..wire("e1", "in", "out")
            }],
            callback_host: None,
        };
        let mut input = run_input("scope");
        input.environment_name = environment.map(str::to_string);
        input.global_env_name = global.map(str::to_string);
        let summary = service_with_flow(flow).run(exec, input).await.expect("run");
        step_of(&summary, "out").value.clone()
    }

    #[tokio::test]
    async fn an_input_resolves_an_environment_variable_for_an_output() {
        let exec = scoped_exec(env_with(&[("ENV_VAR", "acme")]), Vec::new());

        let value = output_of_input("{{ENV_VAR}}", &exec, Some("dev"), None).await;

        assert_eq!(value.as_deref(), Some("acme"));
    }

    #[tokio::test]
    async fn an_input_resolves_a_global_variable_for_an_output() {
        let exec = scoped_exec(env_with(&[("ORG", "acme")]), Vec::new());

        let value = output_of_input("{{ORG}}", &exec, None, Some("shared")).await;

        assert_eq!(value.as_deref(), Some("acme"));
    }

    #[tokio::test]
    async fn an_input_still_resolves_a_collection_variable() {
        let exec = scoped_exec(env_with(&[]), vec![collection_var("COL", "acme")]);

        let value = output_of_input("{{COL}}", &exec, Some("dev"), None).await;

        assert_eq!(value.as_deref(), Some("acme"));
    }

    #[tokio::test]
    async fn an_environment_variable_beats_a_collection_variable_in_an_input() {
        let exec = scoped_exec(
            env_with(&[("KEY", "acme")]),
            vec![collection_var("KEY", "collection")],
        );

        let value = output_of_input("{{KEY}}", &exec, Some("dev"), None).await;

        assert_eq!(value.as_deref(), Some("acme"));
    }

    #[tokio::test]
    async fn an_if_fed_by_an_input_routes_on_the_resolved_environment_value() {
        let flow = Flow {
            name: "if-env".to_string(),
            nodes: vec![
                input_node_with("in", "{{ENV_VAR}}"),
                if_node("check", "response.body === 'acme'"),
                output_node_named("yes"),
                output_node_named("no"),
            ],
            edges: vec![
                input_edge("e1", "in", "check"),
                trigger_edge("e2", "check", handle::TRUE, "yes"),
                trigger_edge("e3", "check", handle::FALSE, "no"),
            ],
            callback_host: None,
        };
        let exec = scoped_exec(env_with(&[("ENV_VAR", "acme")]), Vec::new());
        let mut input = run_input("if-env");
        input.environment_name = Some("dev".to_string());

        let summary = service_with_flow(flow)
            .run(&exec, input)
            .await
            .expect("run");

        assert_eq!(
            step_of(&summary, "check").branch.as_deref(),
            Some(handle::TRUE)
        );
    }

    /// Builds an execution service around a shared `RecordingExecutor` and a
    /// script engine that resolves every wire to `wire_value`.
    fn recording_exec(
        executor: &Arc<RecordingExecutor>,
        engine: Box<dyn ScriptEngine>,
    ) -> RequestExecutionService {
        RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(SharedExecutor(Arc::clone(executor))),
            Box::new(NullHistoryRepo),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(engine)
    }

    fn fixed_wire(value: &str) -> Box<dyn ScriptEngine> {
        Box::new(FixedJsonqEngine {
            value: serde_json::json!(value),
        })
    }

    fn service_with_publisher(
        flow: Flow,
        publisher: &Arc<RecordingPublisher>,
    ) -> FlowExecutionService {
        FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", flow)),
            Box::new(FakeCollectionRepo::new()),
            Box::new(SharedPublisher(Arc::clone(publisher))),
        )
    }

    /// Returns `(node_count, failed_count, skipped_count)` from the single
    /// `FlowRunFinished` event.
    fn finished_counts(publisher: &RecordingPublisher) -> (usize, usize, usize) {
        let finished: Vec<(usize, usize, usize)> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowRunFinished {
                    node_count,
                    failed_count,
                    skipped_count,
                    ..
                } => Some((node_count, failed_count, skipped_count)),
                _ => None,
            })
            .collect();
        assert_eq!(finished.len(), 1, "exactly one FlowRunFinished per run");
        finished[0]
    }

    fn status_of(summary: &FlowRunSummary, id: &str) -> FlowNodeStatus {
        summary
            .steps
            .iter()
            .find(|s| s.node_id == id)
            .unwrap_or_else(|| panic!("no step recorded for node '{id}'"))
            .status
    }

    #[tokio::test]
    async fn empty_flow_publishes_started_then_finished_with_zero_counts() {
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            Flow {
                name: "empty".to_string(),
                nodes: Vec::new(),
                edges: Vec::new(),
                callback_host: None,
            },
            &publisher,
        );
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary = service.run(&exec, run_input("empty")).await.expect("run");

        assert_eq!(summary.stopped_reason, "completed");
        let events = publisher.events();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0],
            DomainEvent::FlowRunStarted { total_nodes: 0, .. }
        ));
        assert_eq!(finished_counts(&publisher), (0, 0, 0));
    }

    #[test]
    fn publish_progress_sends_the_nodes_ids_and_message() {
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            Flow {
                name: "empty".to_string(),
                nodes: Vec::new(),
                edges: Vec::new(),
                callback_host: None,
            },
            &publisher,
        );
        let (_handle, cancel) = cancel_pair();
        let ctx = NodeRunContext {
            run_id: "run-1".to_string(),
            node_id: "poll".to_string(),
            cancel,
        };

        service.publish_progress(&ctx, Some(3), Some(30), "attempt 3/30".to_string());

        let events = publisher.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::FlowStepProgress {
                run_id,
                node_id,
                attempt,
                max_attempts,
                message,
            } => {
                assert_eq!(run_id, "run-1");
                assert_eq!(node_id, "poll");
                assert_eq!(*attempt, Some(3));
                assert_eq!(*max_attempts, Some(30));
                assert_eq!(message, "attempt 3/30");
            }
            other => panic!("expected FlowStepProgress, got {other:?}"),
        }
    }

    /// Publisher that marks the run cancelled once `cancel_after` step events
    /// have been published — simulating a `cancel()` that lands mid-run.
    struct CancelAfterSteps {
        cancel_after: usize,
        seen: Mutex<usize>,
        cancelled: Arc<Mutex<HashSet<String>>>,
    }
    impl EventPublisher for CancelAfterSteps {
        fn publish(&self, event: DomainEvent) {
            if let DomainEvent::FlowStepCompleted { run_id, .. } = event {
                let mut seen = self.seen.lock().expect("lock seen");
                *seen += 1;
                if *seen == self.cancel_after {
                    self.cancelled
                        .lock()
                        .expect("lock cancelled")
                        .insert(run_id);
                }
            }
        }
    }

    #[tokio::test]
    async fn cancelling_mid_run_keeps_completed_steps_and_runs_nothing_further() {
        let flow = Flow {
            name: "three".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
                request_flow_node("c", "https://api.example.com/c"),
            ],
            edges: Vec::new(),
            callback_host: None,
        };
        let cancelled: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        let mut service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", flow)),
            Box::new(FakeCollectionRepo::new()),
            Box::new(CancelAfterSteps {
                cancel_after: 1,
                seen: Mutex::new(0),
                cancelled: Arc::clone(&cancelled),
            }),
        );
        // Share one registry between the service and the canceller.
        service.cancelled = Arc::clone(&cancelled);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary = service.run(&exec, run_input("three")).await.expect("run");

        assert_eq!(summary.stopped_reason, "cancelled");
        assert_eq!(
            summary.steps.len(),
            1,
            "only the node before the cancel is recorded"
        );
        assert_eq!(summary.steps[0].node_id, "a");
        assert_eq!(summary.steps[0].status, FlowNodeStatus::Success);
        assert_eq!(
            executor.sent_urls(),
            vec!["https://api.example.com/a".to_string()]
        );
        assert!(
            !cancelled.lock().expect("lock").contains(&summary.run_id),
            "a finished run must not leak its id in the cancel registry"
        );
        assert!(
            service.in_flight.lock().expect("lock").is_empty(),
            "a finished run must be deregistered from in_flight"
        );
    }

    #[tokio::test]
    async fn cancel_is_checked_before_a_skipped_node_too() {
        // a fails, so b is Skipped. Cancelling after a's step must stop the run
        // before b's Skipped step is even recorded.
        let flow = Flow {
            name: "fail-then-cancel".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![wire("e1", "a", "b")],
            callback_host: None,
        };
        let cancelled: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        let mut service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", flow)),
            Box::new(FakeCollectionRepo::new()),
            Box::new(CancelAfterSteps {
                cancel_after: 1,
                seen: Mutex::new(0),
                cancelled: Arc::clone(&cancelled),
            }),
        );
        service.cancelled = Arc::clone(&cancelled);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary = service
            .run(&exec, run_input("fail-then-cancel"))
            .await
            .expect("run");

        assert_eq!(summary.stopped_reason, "cancelled");
        assert_eq!(summary.steps.len(), 1);
        assert_eq!(summary.steps[0].status, FlowNodeStatus::Failed);
    }

    /// Publisher that triggers the run's cancel handle as soon as `node_id`
    /// starts, so the cancel lands while that node is executing.
    struct CancelOnStart {
        node_id: &'static str,
        handles: Arc<Mutex<HashMap<String, CancelHandle>>>,
    }
    impl EventPublisher for CancelOnStart {
        fn publish(&self, event: DomainEvent) {
            if let DomainEvent::FlowStepStarted { run_id, node_id } = event {
                if node_id == self.node_id {
                    if let Some(handle) = self.handles.lock().expect("lock handles").get(&run_id) {
                        handle.cancel();
                    }
                }
            }
        }
    }

    fn service_cancelling_on_start(flow: Flow, node_id: &'static str) -> FlowExecutionService {
        let handles: Arc<Mutex<HashMap<String, CancelHandle>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let mut service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", flow)),
            Box::new(FakeCollectionRepo::new()),
            Box::new(CancelOnStart {
                node_id,
                handles: Arc::clone(&handles),
            }),
        );
        // Share one handle registry between the service and the canceller.
        service.cancel_handles = handles;
        service
    }

    #[tokio::test]
    async fn cancel_during_a_node_stops_the_run_after_it() {
        let flow = Flow {
            name: "two".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: Vec::new(),
            callback_host: None,
        };
        let service = service_cancelling_on_start(flow, "a");
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary = service.run(&exec, run_input("two")).await.expect("run");

        assert_eq!(summary.stopped_reason, "cancelled");
        assert_eq!(summary.steps.len(), 1, "b must not run after the cancel");
        assert_eq!(summary.steps[0].node_id, "a");
        assert_eq!(
            summary.steps[0].status,
            FlowNodeStatus::Success,
            "a request that finished keeps its real result"
        );
        assert_eq!(
            executor.sent_urls(),
            vec!["https://api.example.com/a".to_string()]
        );
    }

    #[tokio::test]
    async fn a_node_that_fails_during_a_cancel_reports_cancelled() {
        // b's wire script errors while the run is being cancelled.
        let flow = Flow {
            name: "wired".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![wire("e1", "a", "b")],
            callback_host: None,
        };
        let service = service_cancelling_on_start(flow, "b");
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, Box::new(ErrorJsonqEngine));

        let summary = service.run(&exec, run_input("wired")).await.expect("run");

        assert_eq!(summary.stopped_reason, "cancelled");
        let b = step_of(&summary, "b");
        assert_eq!(b.status, FlowNodeStatus::Failed);
        assert_eq!(b.error.as_deref(), Some("cancelled"));
    }

    #[test]
    fn cancel_triggers_the_runs_signal() {
        let service = service_with_flow(Flow {
            name: "x".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            callback_host: None,
        });
        let (handle, signal) = cancel_pair();
        service
            .in_flight
            .lock()
            .expect("lock in_flight")
            .insert("r1".to_string());
        service
            .cancel_handles
            .lock()
            .expect("lock handles")
            .insert("r1".to_string(), handle);

        service.cancel("r1");

        assert!(signal.is_cancelled());
    }

    #[tokio::test]
    async fn finished_runs_leave_no_cancel_handle() {
        let flow = Flow {
            name: "two".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: Vec::new(),
            callback_host: None,
        };
        // One run that completes and one that is cancelled.
        for cancel_on in [None, Some("a")] {
            let service = match cancel_on {
                Some(node) => service_cancelling_on_start(flow.clone(), node),
                None => service_with_flow(flow.clone()),
            };
            let executor = RecordingExecutor::new();
            let exec = recording_exec(&executor, fixed_wire("x"));

            service.run(&exec, run_input("two")).await.expect("run");

            assert!(
                service.cancel_handles.lock().expect("lock").is_empty(),
                "a finished run must drop its cancel handle"
            );
            assert!(service.in_flight.lock().expect("lock").is_empty());
            assert!(service.cancelled.lock().expect("lock").is_empty());
        }
    }

    #[tokio::test]
    async fn fan_out_gives_every_dependent_the_same_captured_output() {
        // a -> b and a -> c. Both dependents wire their URL from a's output,
        // so both must see it, not only the first dependent in order.
        let flow = Flow {
            name: "fan-out".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
                request_flow_node("c", "https://api.example.com/c"),
            ],
            edges: vec![wire("e1", "a", "b"), wire("e2", "a", "c")],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service.run(&exec, run_input("fan-out")).await.expect("run");

        for id in ["a", "b", "c"] {
            assert_eq!(
                status_of(&summary, id),
                FlowNodeStatus::Success,
                "node {id}"
            );
        }
        assert_eq!(
            executor.sent_urls(),
            vec![
                "https://api.example.com/a".to_string(),
                "https://api.example.com/wired".to_string(),
                "https://api.example.com/wired".to_string(),
            ]
        );
        assert_eq!(finished_counts(&publisher), (3, 0, 0));
    }

    #[tokio::test]
    async fn failure_skips_every_transitive_dependent_with_exact_counts() {
        // Diamond a -> {b, c} -> d, plus an unrelated node e. a fails, so
        // b, c and d are Skipped (d only once), and e still runs.
        let flow = Flow {
            name: "diamond".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
                request_flow_node("c", "https://api.example.com/c"),
                request_flow_node("d", "https://api.example.com/d"),
                request_flow_node("e", "https://api.example.com/e"),
            ],
            edges: vec![
                wire("e1", "a", "b"),
                wire("e2", "a", "c"),
                wire("e3", "b", "d"),
                wire("e4", "c", "d"),
            ],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service.run(&exec, run_input("diamond")).await.expect("run");

        assert_eq!(summary.steps.len(), 5, "every node gets exactly one step");
        assert_eq!(status_of(&summary, "a"), FlowNodeStatus::Failed);
        for id in ["b", "c", "d"] {
            assert_eq!(
                status_of(&summary, id),
                FlowNodeStatus::Skipped,
                "node {id}"
            );
        }
        assert_eq!(status_of(&summary, "e"), FlowNodeStatus::Success);
        assert_eq!(
            executor.sent_urls(),
            vec![
                "https://api.example.com/a".to_string(),
                "https://api.example.com/e".to_string(),
            ],
            "skipped nodes must never be sent"
        );
        assert_eq!(finished_counts(&publisher), (5, 1, 3));
    }

    #[tokio::test]
    async fn skipped_step_reports_upstream_failed_in_summary_and_event() {
        // a -> b; a fails (500), so b is skipped because upstream failed.
        let flow = Flow {
            name: "chain".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![wire("e1", "a", "b")],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service.run(&exec, run_input("chain")).await.expect("run");

        let b = summary
            .steps
            .iter()
            .find(|s| s.node_id == "b")
            .expect("step for b");
        assert_eq!(b.status, FlowNodeStatus::Skipped);
        assert_eq!(b.skip_reason, Some(FlowSkipReason::UpstreamFailed));
        assert_eq!(b.error, None, "skip_reason replaces the old error text");
        assert_eq!(b.branch, None);

        let b_event = publisher
            .events()
            .into_iter()
            .find_map(|e| match e {
                DomainEvent::FlowStepCompleted {
                    node_id,
                    status,
                    error,
                    skip_reason,
                    branch,
                    ..
                } if node_id == "b" => Some((status, error, skip_reason, branch)),
                _ => None,
            })
            .expect("FlowStepCompleted for b");
        assert_eq!(
            b_event,
            (
                FlowNodeStatus::Skipped,
                None,
                Some(FlowSkipReason::UpstreamFailed),
                None
            )
        );
    }

    #[tokio::test]
    async fn failed_and_successful_steps_have_no_skip_reason_or_branch() {
        // a fails, c is independent and succeeds.
        let flow = Flow {
            name: "mixed".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("c", "https://api.example.com/c"),
            ],
            edges: vec![],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service.run(&exec, run_input("mixed")).await.expect("run");

        for step in &summary.steps {
            assert_eq!(step.skip_reason, None, "node {}", step.node_id);
            assert_eq!(step.branch, None, "node {}", step.node_id);
        }
        assert_eq!(status_of(&summary, "a"), FlowNodeStatus::Failed);
        assert_eq!(status_of(&summary, "c"), FlowNodeStatus::Success);
    }

    #[tokio::test]
    async fn run_finished_reports_zero_not_taken_for_failure_skips() {
        let flow = Flow {
            name: "chain".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![wire("e1", "a", "b")],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        service.run(&exec, run_input("chain")).await.expect("run");

        let not_taken: Vec<usize> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowRunFinished {
                    not_taken_count, ..
                } => Some(not_taken_count),
                _ => None,
            })
            .collect();
        assert_eq!(not_taken, vec![0]);
        assert_eq!(finished_counts(&publisher), (2, 1, 1));
    }

    #[test]
    fn a_met_poll_is_a_success_even_on_a_non_2xx_response() {
        let mut out = sample_response_output();
        out.response.status = 404;
        let node = request_flow_node("job", "https://api.example.com/job");
        let executed = ExecutedNode {
            output: CapturedOutput::Request(Box::new(out)),
            chosen_exit: handle::RESULT.to_string(),
            poll: Some(crate::flow_poll::PollStats {
                attempts: 7,
                elapsed_ms: 14_200,
            }),
            reported_value: None,
        };

        let step = result_to_step("job", Some(&node), &Ok(executed));

        assert_eq!(step.status, FlowNodeStatus::Success);
        assert_eq!(step.status_code, Some(404));
        assert_eq!(step.duration_ms, Some(14_200));
        assert_eq!(step.attempts, Some(7));
        assert_eq!(step.error, None);
    }

    #[test]
    fn a_plain_request_step_has_no_attempts() {
        let node = request_flow_node("r", "https://api.example.com/r");
        let executed =
            ExecutedNode::plain(CapturedOutput::Request(Box::new(sample_response_output())));
        let step = result_to_step("r", Some(&node), &Ok(executed));
        assert_eq!(step.attempts, None);
        let json = serde_json::to_value(&step).expect("serialize");
        assert!(json.get("attempts").is_none(), "None attempts are omitted");
    }

    #[test]
    fn step_attempts_serialize_camel_case_and_reach_the_event() {
        let step = FlowStepResult {
            attempts: Some(3),
            ..failed_step("job", "x".into())
        };
        let json = serde_json::to_value(&step).expect("serialize");
        assert_eq!(json["attempts"], 3);
        match step_completed_event("run", &step) {
            DomainEvent::FlowStepCompleted { attempts, .. } => assert_eq!(attempts, Some(3)),
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn flow_step_result_serializes_skip_reason_camel_key_snake_value() {
        let step = FlowStepResult {
            node_id: "b".into(),
            status: FlowNodeStatus::Skipped,
            status_code: None,
            duration_ms: None,
            error: None,
            value: None,
            skip_reason: Some(FlowSkipReason::BranchNotTaken),
            branch: None,
            debug_request: None,
            attempts: None,
            exchange: None,
            logs: Vec::new(),
        };
        let json = serde_json::to_value(&step).expect("serialize");
        assert_eq!(json["skipReason"], "branch_not_taken");
        assert!(json.get("skip_reason").is_none());
        assert!(json.get("branch").is_none(), "None branch is omitted");

        let routed = FlowStepResult {
            skip_reason: None,
            branch: Some("true".into()),
            status: FlowNodeStatus::Success,
            ..step
        };
        let json = serde_json::to_value(&routed).expect("serialize");
        assert_eq!(json["branch"], "true");
        assert!(
            json.get("skipReason").is_none(),
            "None skipReason is omitted"
        );

        let back: FlowStepResult = serde_json::from_value(serde_json::json!({
            "nodeId": "x", "status": "success", "statusCode": null,
            "durationMs": null, "error": null
        }))
        .expect("deserialize pre-Phase-2 summary step");
        assert_eq!(back.skip_reason, None);
        assert_eq!(back.branch, None);
    }

    #[tokio::test]
    async fn wire_expression_failure_fails_the_node_and_skips_its_dependents() {
        // a -> b -> c, plus an unrelated node d. Every wire expression throws,
        // so b fails on its own wire, c is Skipped, and a and d succeed.
        let flow = Flow {
            name: "wire-fail".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
                request_flow_node("c", "https://api.example.com/c"),
                request_flow_node("d", "https://api.example.com/d"),
            ],
            edges: vec![wire("e1", "a", "b"), wire("e2", "b", "c")],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, Box::new(ErrorJsonqEngine));

        let summary = service
            .run(&exec, run_input("wire-fail"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "a"), FlowNodeStatus::Success);
        assert_eq!(status_of(&summary, "b"), FlowNodeStatus::Failed);
        assert_eq!(status_of(&summary, "c"), FlowNodeStatus::Skipped);
        assert_eq!(status_of(&summary, "d"), FlowNodeStatus::Success);
        let b = summary
            .steps
            .iter()
            .find(|s| s.node_id == "b")
            .expect("step for b");
        assert!(
            b.error
                .as_deref()
                .is_some_and(|e| e.contains("ReferenceError")),
            "the wire's script error must be reported, got {:?}",
            b.error
        );
        assert!(
            !executor
                .sent_urls()
                .contains(&"https://api.example.com/b".to_string()),
            "a node whose wire failed must not be sent"
        );
        assert_eq!(finished_counts(&publisher), (4, 1, 1));
    }

    #[tokio::test]
    async fn output_node_with_two_incoming_wires_fails_instead_of_dropping_one() {
        let input_node = |id: &str, v: &str| FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Input {
                label: id.to_string(),
                value: VariableValue::simple(v),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        };
        let flow = Flow {
            name: "two-into-output".to_string(),
            nodes: vec![
                input_node("x", "1"),
                input_node("y", "2"),
                FlowNode {
                    id: "out".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "Out".to_string(),
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
            ],
            edges: vec![
                FlowEdge {
                    target_field: "value".to_string(),
                    ..wire("e1", "x", "out")
                },
                FlowEdge {
                    target_field: "value".to_string(),
                    ..wire("e2", "y", "out")
                },
            ],
            callback_host: None,
        };
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("1"));

        let summary = service
            .run(&exec, run_input("two-into-output"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "out"), FlowNodeStatus::Failed);
    }

    #[tokio::test]
    async fn output_node_ignores_a_trigger_edge_when_counting_wires() {
        let flow = Flow {
            name: "output-trigger".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
                FlowNode {
                    id: "out".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "Out".to_string(),
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
            ],
            edges: vec![
                edge_from("e1", "a", handle::RESULT, "out", "value", "response.body"),
                edge_from("e2", "b", handle::RESULT, "out", handle::TRIGGER, ""),
            ],
            callback_host: None,
        };
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("wired"));

        let summary = service
            .run(&exec, run_input("output-trigger"))
            .await
            .expect("run");

        let out = step_of(&summary, "out");
        assert_eq!(out.status, FlowNodeStatus::Success);
        assert_eq!(out.value.as_deref(), Some("wired"));
    }

    #[tokio::test]
    async fn resolves_external_secrets_once_per_run_not_once_per_request_node() {
        let mut env = Environment::new("prod");
        env.external_secrets
            .push(rocket_environment::ExternalSecretBinding {
                alias: "payments".to_string(),
                connection_id: "conn-1".to_string(),
                vault_name: "prod-vault".to_string(),
                secret_names: vec![rocket_environment::ExternalSecretRef {
                    name: "apiKey".to_string(),
                    secret_id: "sec-1".to_string(),
                }],
            });
        let connection = rocket_environment::SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Prod RocketVault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
        };
        let mut values = HashMap::new();
        values.insert("sec-1".to_string(), "sk-secret".to_string());
        let fetcher = FakeVaultSecretFetcher::new(values);
        let executor = RecordingExecutor::new();
        let exec = RequestExecutionService::new(
            Box::new(StaticEnvRepo(env)),
            Arc::new(SharedExecutor(Arc::clone(&executor))),
            Box::new(NullHistoryRepo),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo(connection)),
            Arc::new(FakeSecretStore("client-secret".to_string())),
            Arc::clone(&fetcher) as Arc<dyn rocket_environment::VaultSecretFetcher>,
        )
        .with_script_engine(fixed_wire("x"));

        let url = |n: &str| format!("https://api.example.com/{n}?key={{{{payments.apiKey}}}}");
        let flow = Flow {
            name: "secrets".to_string(),
            nodes: vec![
                request_flow_node("a", &url("a")),
                request_flow_node("b", &url("b")),
                request_flow_node("c", &url("c")),
            ],
            edges: Vec::new(),
            callback_host: None,
        };
        let service = service_with_flow(flow);
        let mut input = run_input("secrets");
        input.environment_name = Some("prod".to_string());

        let summary = service.run(&exec, input).await.expect("run");

        assert_eq!(fetcher.call_count(), 1, "one vault fetch for the whole run");
        assert_eq!(summary.steps.len(), 3);
        let sent = executor.sent_urls();
        assert_eq!(sent.len(), 3);
        for url in &sent {
            assert!(
                url.ends_with("?key=sk-secret"),
                "secret not substituted in {url}"
            );
        }
    }

    #[tokio::test]
    async fn step_started_is_published_before_step_completed_and_never_for_a_skipped_node() {
        let flow = Flow {
            name: "two-nodes".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![wire("e1", "a", "b")],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", flow)),
            Box::new(FakeCollectionRepo::new()),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
        );
        let exec = exec_failing_for_url("https://api.example.com/a");

        service
            .run(&exec, run_input("two-nodes"))
            .await
            .expect("run must complete even with a failed node");

        let events = publisher.events();
        let started_ids: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepStarted { node_id, .. } => Some(node_id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            started_ids,
            vec!["a".to_string()],
            "node b is Skipped (its dependency a failed) and must never get a started event"
        );

        let started_idx = events
            .iter()
            .position(
                |e| matches!(e, DomainEvent::FlowStepStarted { node_id, .. } if node_id == "a"),
            )
            .expect("started event for a must exist");
        let completed_idx = events
            .iter()
            .position(
                |e| matches!(e, DomainEvent::FlowStepCompleted { node_id, .. } if node_id == "a"),
            )
            .expect("completed event for a must exist");
        assert!(
            started_idx < completed_idx,
            "started must publish strictly before completed"
        );
    }

    // ---- Phase 2: outcome-driven run loop --------------------------------

    fn edge_from(
        id: &str,
        from: &str,
        exit: &str,
        to: &str,
        field: &str,
        expression: &str,
    ) -> FlowEdge {
        FlowEdge {
            source_handle: exit.to_string(),
            target_field: field.to_string(),
            expression: expression.to_string(),
            ..wire(id, from, to)
        }
    }

    fn step_of<'s>(summary: &'s FlowRunSummary, id: &str) -> &'s FlowStepResult {
        summary
            .steps
            .iter()
            .find(|s| s.node_id == id)
            .unwrap_or_else(|| panic!("no step recorded for node '{id}'"))
    }

    fn not_taken_count(publisher: &RecordingPublisher) -> usize {
        publisher
            .events()
            .into_iter()
            .find_map(|e| match e {
                DomainEvent::FlowRunFinished {
                    not_taken_count, ..
                } => Some(not_taken_count),
                _ => None,
            })
            .expect("a FlowRunFinished event")
    }

    #[tokio::test]
    async fn upstream_failed_skips_carry_a_skip_reason_and_no_error_text() {
        // Phase 1 regression: same statuses as before, reason now structured.
        let flow = Flow {
            name: "diamond2".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
                request_flow_node("d", "https://api.example.com/d"),
            ],
            edges: vec![
                wire("e1", "a", "b"),
                edge_from("e2", "b", handle::RESULT, "d", "body", "response.body"),
            ],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service
            .run(&exec, run_input("diamond2"))
            .await
            .expect("run");

        for id in ["b", "d"] {
            let s = step_of(&summary, id);
            assert_eq!(s.status, FlowNodeStatus::Skipped, "node {id}");
            assert_eq!(
                s.skip_reason,
                Some(FlowSkipReason::UpstreamFailed),
                "node {id}"
            );
            assert_eq!(s.error, None, "node {id}");
        }
        assert_eq!(finished_counts(&publisher), (3, 1, 2));
        assert_eq!(not_taken_count(&publisher), 0);
    }

    #[tokio::test]
    async fn trigger_edge_gates_but_never_overrides_a_field() {
        let flow = Flow {
            name: "trigger".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![edge_from(
                "t1",
                "a",
                handle::RESULT,
                "b",
                handle::TRIGGER,
                "",
            )],
            callback_host: None,
        };
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service.run(&exec, run_input("trigger")).await.expect("run");

        assert_eq!(status_of(&summary, "b"), FlowNodeStatus::Success);
        assert_eq!(
            executor.sent_urls(),
            vec![
                "https://api.example.com/a".to_string(),
                "https://api.example.com/b".to_string(),
            ],
            "a trigger must not write into any field of b"
        );
    }

    #[tokio::test]
    async fn trigger_from_a_failed_node_skips_the_target_as_upstream_failed() {
        let flow = Flow {
            name: "trigger-fail".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![edge_from(
                "t1",
                "a",
                handle::RESULT,
                "b",
                handle::TRIGGER,
                "",
            )],
            callback_host: None,
        };
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary = service
            .run(&exec, run_input("trigger-fail"))
            .await
            .expect("run");

        assert_eq!(
            step_of(&summary, "b").skip_reason,
            Some(FlowSkipReason::UpstreamFailed)
        );
    }

    #[tokio::test]
    async fn two_unconditional_wires_into_one_field_now_fail_the_target() {
        // Intentional Phase 1 change (spec §5.5, user decision): Phase 1
        // silently used the last wire; now the node fails with a clear error.
        let flow = Flow {
            name: "ambiguous".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
                request_flow_node("c", "https://api.example.com/c"),
            ],
            edges: vec![wire("e1", "a", "c"), wire("e2", "b", "c")],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service
            .run(&exec, run_input("ambiguous"))
            .await
            .expect("run");

        let c = step_of(&summary, "c");
        assert_eq!(c.status, FlowNodeStatus::Failed);
        assert_eq!(c.error.as_deref(), Some("field 'url' has 2 live inputs"));
        assert_eq!(executor.sent_urls().len(), 2, "c must never be sent");
    }

    #[test]
    fn load_ordered_nodes_rejects_a_structurally_invalid_flow() {
        // V1: an If node needs exactly one `input` edge; this one has none.
        // Acyclic on purpose, so the rejection comes from `validate`'s
        // structural rules, not from `topological_sort`.
        let mut flow = linear_flow();
        flow.nodes.push(FlowNode {
            id: "lonely_if".to_string(),
            kind: FlowNodeKind::If {
                label: "Lonely".to_string(),
                condition: "true".to_string(),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        });
        flow.name = "invalid".to_string();
        let service = service_with_flow(flow);

        let err = service
            .load_ordered_nodes("my-api", "invalid")
            .expect_err("an invalid flow must not load for execution");

        assert!(matches!(err, DomainError::InvalidInput(ref m) if m.contains("not runnable")));
    }

    // ---- Phase 2: If / Switch routing -------------------------------------

    use rocket_flow::SwitchCase;

    fn if_node(id: &str, condition: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::If {
                label: id.to_string(),
                condition: condition.to_string(),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    /// `cases` are `(case id, matches)`; the label equals the id.
    fn switch_node(id: &str, value: &str, cases: &[(&str, &str)]) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Switch {
                label: id.to_string(),
                value: value.to_string(),
                cases: cases
                    .iter()
                    .map(|(case_id, matches)| SwitchCase {
                        id: case_id.to_string(),
                        label: case_id.to_string(),
                        matches: matches.to_string(),
                    })
                    .collect(),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn input_edge(id: &str, from: &str, to: &str) -> FlowEdge {
        edge_from(id, from, handle::RESULT, to, handle::INPUT, "")
    }

    fn trigger_edge(id: &str, from: &str, exit: &str, to: &str) -> FlowEdge {
        edge_from(id, from, exit, to, handle::TRIGGER, "")
    }

    /// login -> if(check) -> true: yes, false: no.
    fn if_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![
                request_flow_node("login", "https://api.example.com/login"),
                if_node("check", "response.status === 200"),
                request_flow_node("yes", "https://api.example.com/yes"),
                request_flow_node("no", "https://api.example.com/no"),
            ],
            edges: vec![
                input_edge("e1", "login", "check"),
                trigger_edge("e2", "check", handle::TRUE, "yes"),
                trigger_edge("e3", "check", handle::FALSE, "no"),
            ],
            callback_host: None,
        }
    }

    #[tokio::test]
    async fn if_true_runs_the_true_exit_and_marks_the_false_exit_not_taken() {
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(if_flow("if-true"), &publisher);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service.run(&exec, run_input("if-true")).await.expect("run");

        let check = step_of(&summary, "check");
        assert_eq!(check.status, FlowNodeStatus::Success);
        assert_eq!(check.branch.as_deref(), Some(handle::TRUE));
        assert_eq!(check.status_code, None, "a routing node has no HTTP status");
        assert_eq!(status_of(&summary, "yes"), FlowNodeStatus::Success);
        let no = step_of(&summary, "no");
        assert_eq!(no.status, FlowNodeStatus::Skipped);
        assert_eq!(no.skip_reason, Some(FlowSkipReason::BranchNotTaken));
        assert_eq!(
            executor.sent_urls(),
            vec![
                "https://api.example.com/login".to_string(),
                "https://api.example.com/yes".to_string(),
            ]
        );
        assert_eq!(finished_counts(&publisher), (4, 0, 1));
        assert_eq!(not_taken_count(&publisher), 1);
        let completed_branch = publisher.events().into_iter().find_map(|e| match e {
            DomainEvent::FlowStepCompleted {
                node_id, branch, ..
            } if node_id == "check" => branch,
            _ => None,
        });
        assert_eq!(completed_branch.as_deref(), Some(handle::TRUE));
    }

    #[tokio::test]
    async fn if_false_runs_the_false_exit_only() {
        let service = service_with_flow(if_flow("if-false"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(false)))]),
        );

        let summary = service
            .run(&exec, run_input("if-false"))
            .await
            .expect("run");

        assert_eq!(
            step_of(&summary, "check").branch.as_deref(),
            Some(handle::FALSE)
        );
        assert_eq!(
            step_of(&summary, "yes").skip_reason,
            Some(FlowSkipReason::BranchNotTaken)
        );
        assert_eq!(status_of(&summary, "no"), FlowNodeStatus::Success);
    }

    #[tokio::test]
    async fn if_condition_error_fails_the_node_and_skips_both_exits_as_upstream_failed() {
        let service = service_with_flow(if_flow("if-error"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Throw("ReferenceError: nope"))]),
        );

        let summary = service
            .run(&exec, run_input("if-error"))
            .await
            .expect("run");

        let check = step_of(&summary, "check");
        assert_eq!(check.status, FlowNodeStatus::Failed);
        assert!(check
            .error
            .as_deref()
            .is_some_and(|m| m.contains("ReferenceError")));
        for id in ["yes", "no"] {
            assert_eq!(
                step_of(&summary, id).skip_reason,
                Some(FlowSkipReason::UpstreamFailed),
                "node {id}"
            );
        }
    }

    #[tokio::test]
    async fn if_condition_that_is_not_a_boolean_string_fails_the_node() {
        let service = service_with_flow(if_flow("if-weird"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!("maybe")))]),
        );

        let summary = service
            .run(&exec, run_input("if-weird"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "check"), FlowNodeStatus::Failed);
    }

    #[tokio::test]
    async fn if_observes_a_non_2xx_request_and_routes_on_it() {
        // Spec §6.3.1: login gets a 401 and stays failed. The If still sees
        // the 401 response and routes to `false`. Login's plain dependent is
        // skipped as upstream_failed.
        let mut flow = if_flow("if-after-401");
        flow.nodes
            .push(request_flow_node("plain", "https://api.example.com/plain"));
        flow.edges.push(wire("e4", "login", "plain"));
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/login", 401);
        // The condition answers from the response it is given, so a `false`
        // result proves the If saw the real 401 and not a missing capture.
        let exec = recording_exec(
            &executor,
            scripted(vec![(
                "!!(",
                Scripted::FromResponse(|r| {
                    serde_json::json!(r.map(|r| r.status == 200).unwrap_or(true))
                }),
            )]),
        );

        let summary = service
            .run(&exec, run_input("if-after-401"))
            .await
            .expect("run");

        let login = step_of(&summary, "login");
        assert_eq!(login.status, FlowNodeStatus::Failed);
        assert_eq!(login.status_code, Some(401));
        let check = step_of(&summary, "check");
        assert_eq!(check.status, FlowNodeStatus::Success);
        assert_eq!(check.branch.as_deref(), Some(handle::FALSE));
        assert_eq!(status_of(&summary, "no"), FlowNodeStatus::Success);
        assert_eq!(
            step_of(&summary, "yes").skip_reason,
            Some(FlowSkipReason::BranchNotTaken)
        );
        assert_eq!(
            step_of(&summary, "plain").skip_reason,
            Some(FlowSkipReason::UpstreamFailed),
            "only routing inputs observe a failure"
        );
        assert_eq!(finished_counts(&publisher), (5, 1, 2));
        assert_eq!(not_taken_count(&publisher), 1);
        assert!(!executor.sent_urls().iter().any(|u| u.contains("/plain")));
    }

    #[tokio::test]
    async fn if_after_a_transport_error_is_skipped_as_upstream_failed() {
        // A status of 0 makes `RecordingExecutor` fail with a transport error.
        // With no response captured there is nothing to observe.
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(if_flow("if-after-transport"), &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/login", 0);
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service
            .run(&exec, run_input("if-after-transport"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "login"), FlowNodeStatus::Failed);
        for id in ["check", "yes", "no"] {
            assert_eq!(
                step_of(&summary, id).skip_reason,
                Some(FlowSkipReason::UpstreamFailed),
                "node {id}"
            );
        }
        assert_eq!(finished_counts(&publisher), (4, 1, 3));
    }

    #[tokio::test]
    async fn a_wire_leaving_an_if_exit_reads_the_ifs_input_response() {
        // login answers 201; the wire out of `true` echoes the status it sees.
        // A synthetic capture would report 200, so 201 proves pass-through.
        let mut flow = if_flow("pass-through");
        flow.edges[1] = edge_from("e2", "check", handle::TRUE, "yes", "url", "response.status");
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/login", 201);
        let exec = recording_exec(
            &executor,
            scripted(vec![
                ("!!(", Scripted::Value(serde_json::json!(true))),
                (
                    "response.status",
                    Scripted::FromResponse(|r| {
                        let status = r.map(|r| r.status).unwrap_or_default();
                        serde_json::json!(format!("https://api.example.com/from-{status}"))
                    }),
                ),
            ]),
        );

        let summary = service
            .run(&exec, run_input("pass-through"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "yes"), FlowNodeStatus::Success);
        assert!(executor
            .sent_urls()
            .contains(&"https://api.example.com/from-201".to_string()));
    }

    #[tokio::test]
    async fn if_fed_by_an_input_node_passes_its_value_through() {
        let flow = Flow {
            name: "input-if".to_string(),
            nodes: vec![
                FlowNode {
                    id: "in".to_string(),
                    kind: FlowNodeKind::Input {
                        label: "Plan".to_string(),
                        value: VariableValue::simple("pro"),
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                if_node("check", "response.body === 'pro'"),
                FlowNode {
                    id: "out".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "Out".to_string(),
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
            ],
            edges: vec![
                input_edge("e1", "in", "check"),
                edge_from("e2", "check", handle::TRUE, "out", "value", "response.body"),
            ],
            callback_host: None,
        };
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![
                ("!!(", Scripted::Value(serde_json::json!(true))),
                (
                    "response.body",
                    Scripted::FromResponse(|r| {
                        serde_json::json!(r.map(|r| r.body.clone()).unwrap_or_default())
                    }),
                ),
            ]),
        );

        let summary = service
            .run(&exec, run_input("input-if"))
            .await
            .expect("run");

        assert_eq!(step_of(&summary, "out").value.as_deref(), Some("pro"));
    }

    /// login -> switch(plan) -> free: f, pro: p, default: d.
    fn switch_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![
                request_flow_node("login", "https://api.example.com/login"),
                switch_node(
                    "plan",
                    "response.body.plan",
                    &[("free", "free"), ("pro", "pro")],
                ),
                request_flow_node("f", "https://api.example.com/f"),
                request_flow_node("p", "https://api.example.com/p"),
                request_flow_node("d", "https://api.example.com/d"),
            ],
            edges: vec![
                input_edge("e1", "login", "plan"),
                trigger_edge("e2", "plan", &handle::case_handle("free"), "f"),
                trigger_edge("e3", "plan", &handle::case_handle("pro"), "p"),
                trigger_edge("e4", "plan", handle::DEFAULT, "d"),
            ],
            callback_host: None,
        }
    }

    async fn run_switch(name: &str, value: serde_json::Value) -> FlowRunSummary {
        let service = service_with_flow(switch_flow(name));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("String(", Scripted::Value(value))]),
        );
        service.run(&exec, run_input(name)).await.expect("run")
    }

    #[tokio::test]
    async fn switch_routes_to_the_matching_case_only() {
        let summary = run_switch("sw-pro", serde_json::json!("pro")).await;

        assert_eq!(
            step_of(&summary, "plan").branch,
            Some(handle::case_handle("pro"))
        );
        assert_eq!(status_of(&summary, "p"), FlowNodeStatus::Success);
        for id in ["f", "d"] {
            assert_eq!(
                step_of(&summary, id).skip_reason,
                Some(FlowSkipReason::BranchNotTaken),
                "node {id}"
            );
        }
    }

    #[tokio::test]
    async fn switch_without_a_match_routes_to_default() {
        let summary = run_switch("sw-default", serde_json::json!("enterprise")).await;

        assert_eq!(
            step_of(&summary, "plan").branch.as_deref(),
            Some(handle::DEFAULT)
        );
        assert_eq!(status_of(&summary, "d"), FlowNodeStatus::Success);
    }

    #[tokio::test]
    async fn switch_null_value_routes_to_a_case_matching_null() {
        let mut flow = switch_flow("sw-null");
        if let FlowNodeKind::Switch { cases, .. } = &mut flow.nodes[1].kind {
            cases[0].matches = "null".to_string();
        }
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("String(", Scripted::Value(serde_json::Value::Null))]),
        );

        let summary = service.run(&exec, run_input("sw-null")).await.expect("run");

        assert_eq!(
            step_of(&summary, "plan").branch,
            Some(handle::case_handle("free"))
        );
    }

    #[tokio::test]
    async fn switch_routes_a_numeric_value_to_its_string_case() {
        let mut flow = switch_flow("sw-num");
        if let FlowNodeKind::Switch { cases, .. } = &mut flow.nodes[1].kind {
            cases[1].matches = "200".to_string();
        }
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("String(", Scripted::Value(serde_json::json!(200)))]),
        );

        let summary = service.run(&exec, run_input("sw-num")).await.expect("run");

        assert_eq!(
            step_of(&summary, "plan").branch,
            Some(handle::case_handle("pro"))
        );
    }

    #[tokio::test]
    async fn switch_observes_a_non_2xx_response_and_routes_on_it() {
        let mut flow = switch_flow("sw-401");
        if let FlowNodeKind::Switch { cases, .. } = &mut flow.nodes[1].kind {
            cases[1].matches = "401".to_string();
        }
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/login", 401);
        let exec = recording_exec(
            &executor,
            scripted(vec![(
                "String(",
                Scripted::FromResponse(|r| {
                    serde_json::json!(r.map(|r| r.status).unwrap_or_default())
                }),
            )]),
        );

        let summary = service.run(&exec, run_input("sw-401")).await.expect("run");

        let login = step_of(&summary, "login");
        assert_eq!(login.status, FlowNodeStatus::Failed);
        assert_eq!(login.status_code, Some(401));
        let plan = step_of(&summary, "plan");
        assert_eq!(plan.status, FlowNodeStatus::Success);
        assert_eq!(plan.branch, Some(handle::case_handle("pro")));
        assert_eq!(status_of(&summary, "p"), FlowNodeStatus::Success);
    }

    /// Spec §6.3 case 1 and case 2 in one graph (false taken):
    /// login -> check; true -> profile, false -> refresh;
    /// profile.body & refresh.body -> save (merge);
    /// config.url + profile.header -> call (accidental join).
    fn join_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![
                request_flow_node("login", "https://api.example.com/login"),
                if_node("check", "response.status === 200"),
                request_flow_node("profile", "https://api.example.com/profile"),
                request_flow_node("refresh", "https://api.example.com/refresh"),
                request_flow_node("save", "https://api.example.com/save"),
                request_flow_node("config", "https://api.example.com/config"),
                request_flow_node("call", "https://api.example.com/call"),
            ],
            edges: vec![
                input_edge("e1", "login", "check"),
                trigger_edge("e2", "check", handle::TRUE, "profile"),
                trigger_edge("e3", "check", handle::FALSE, "refresh"),
                edge_from(
                    "e4",
                    "profile",
                    handle::RESULT,
                    "save",
                    "body",
                    "response.body",
                ),
                edge_from(
                    "e5",
                    "refresh",
                    handle::RESULT,
                    "save",
                    "body",
                    "response.body",
                ),
                edge_from(
                    "e6",
                    "config",
                    handle::RESULT,
                    "call",
                    "url",
                    "response.body",
                ),
                edge_from(
                    "e7",
                    "profile",
                    handle::RESULT,
                    "call",
                    "headers[Authorization].value",
                    "response.body",
                ),
            ],
            callback_host: None,
        }
    }

    #[tokio::test]
    async fn per_field_join_merges_alternatives_and_skips_a_missing_field() {
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(join_flow("join"), &publisher);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(false)))]),
        );

        let summary = service.run(&exec, run_input("join")).await.expect("run");

        assert_eq!(
            status_of(&summary, "save"),
            FlowNodeStatus::Success,
            "case 1 merge"
        );
        assert_eq!(
            step_of(&summary, "call").skip_reason,
            Some(FlowSkipReason::BranchNotTaken),
            "case 2: the Authorization field has no live input"
        );
        let sent = executor.sent_urls();
        assert_eq!(
            sent.iter().filter(|u| u.contains("/save")).count(),
            1,
            "the merge node runs exactly once"
        );
        assert!(!sent.iter().any(|u| u.contains("/call")));
        assert_eq!(not_taken_count(&publisher), 2, "profile and call");
    }

    #[tokio::test]
    async fn a_failed_arm_poisons_the_join_even_when_the_other_arm_was_not_taken() {
        let service = service_with_flow(join_flow("join-fail"));
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/profile", 500);
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service
            .run(&exec, run_input("join-fail"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "profile"), FlowNodeStatus::Failed);
        assert_eq!(
            step_of(&summary, "refresh").skip_reason,
            Some(FlowSkipReason::BranchNotTaken)
        );
        assert_eq!(
            step_of(&summary, "save").skip_reason,
            Some(FlowSkipReason::UpstreamFailed)
        );
    }

    #[tokio::test]
    async fn dependents_of_a_not_taken_node_are_not_taken_too() {
        let mut flow = if_flow("transitive");
        flow.nodes.push(request_flow_node(
            "after_no",
            "https://api.example.com/after",
        ));
        flow.edges.push(wire("e4", "no", "after_no"));
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service
            .run(&exec, run_input("transitive"))
            .await
            .expect("run");

        assert_eq!(
            step_of(&summary, "after_no").skip_reason,
            Some(FlowSkipReason::BranchNotTaken)
        );
    }

    #[tokio::test]
    async fn several_live_triggers_into_one_node_run_it_once() {
        let mut flow = if_flow("two-triggers");
        flow.nodes
            .push(request_flow_node("both", "https://api.example.com/both"));
        flow.edges
            .push(trigger_edge("e4", "check", handle::TRUE, "both"));
        flow.edges
            .push(trigger_edge("e5", "login", handle::RESULT, "both"));
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service
            .run(&exec, run_input("two-triggers"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "both"), FlowNodeStatus::Success);
    }

    #[tokio::test]
    async fn cancelling_before_a_routing_node_records_nothing_for_it() {
        let cancelled: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        let mut service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", if_flow("cancel-if"))),
            Box::new(FakeCollectionRepo::new()),
            Box::new(CancelAfterSteps {
                cancel_after: 1,
                seen: Mutex::new(0),
                cancelled: Arc::clone(&cancelled),
            }),
        );
        service.cancelled = Arc::clone(&cancelled);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service
            .run(&exec, run_input("cancel-if"))
            .await
            .expect("run");

        assert_eq!(summary.stopped_reason, "cancelled");
        assert_eq!(summary.steps.len(), 1);
        assert_eq!(summary.steps[0].node_id, "login");
    }

    #[tokio::test]
    async fn step_started_is_never_published_for_a_not_taken_node() {
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(if_flow("started-if"), &publisher);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        service
            .run(&exec, run_input("started-if"))
            .await
            .expect("run");

        let started: Vec<String> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepStarted { node_id, .. } => Some(node_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            started,
            vec!["login".to_string(), "check".to_string(), "yes".to_string()]
        );
    }

    // ---- Real script engine ----------------------------------------------
    // The scripted fakes above never run JS, so they cannot notice when the
    // `response` object a Flow expression uses is missing from the engine.

    fn real_engine_service() -> RequestExecutionService {
        service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(rocket_infra::scripting::DenoScriptEngine::new()),
        )
    }

    #[tokio::test]
    async fn real_engine_route_status_check_sees_response_status() {
        let svc = real_engine_service();
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .evaluate_flow_route_expression(
                "my-api",
                &output,
                "response.status === 200",
                FlowCoercion::Bool,
                &HashSet::new(),
            )
            .await
            .result
            .expect("response.status must be defined");

        assert_eq!(value, "true");
    }

    #[tokio::test]
    async fn real_engine_wire_reads_json_body_field() {
        let svc = real_engine_service();
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "response.body.token", &HashSet::new())
            .await
            .result
            .expect("response.body must be parsed JSON");

        assert_eq!(value, "abc123");
    }

    fn payment_call(body: &str) -> crate::callback_listener::ReceivedCall {
        crate::callback_listener::ReceivedCall {
            method: "POST".to_string(),
            path: "/cb/abc".to_string(),
            query: vec![("id".to_string(), "7".to_string())],
            headers: vec![("x-event".to_string(), "payment.completed".to_string())],
            body: body.to_string(),
        }
    }

    #[tokio::test]
    async fn real_engine_callback_condition_sees_method_path_query_headers_and_json_body() {
        let svc = real_engine_service();
        let value = svc
            .evaluate_flow_callback_condition(
                "my-api",
                &payment_call(r#"{"event":"payment.completed","orderId":42}"#),
                "request.method === 'POST' && request.path === '/cb/abc' \
                 && request.query.id === '7' \
                 && request.headers['x-event'] === 'payment.completed' \
                 && request.body.orderId === 42",
                &HashSet::new(),
            )
            .await
            .result
            .expect("the condition must evaluate");
        assert_eq!(value, "true");
    }

    #[tokio::test]
    async fn real_engine_callback_condition_is_false_when_it_does_not_match() {
        let svc = real_engine_service();
        let value = svc
            .evaluate_flow_callback_condition(
                "my-api",
                &payment_call(r#"{"event":"payment.pending"}"#),
                "request.body.event === 'payment.completed'",
                &HashSet::new(),
            )
            .await
            .result
            .expect("the condition must evaluate");
        assert_eq!(value, "false");
    }

    #[tokio::test]
    async fn real_engine_callback_condition_gets_a_text_body_as_a_string() {
        let svc = real_engine_service();
        let value = svc
            .evaluate_flow_callback_condition(
                "my-api",
                &payment_call("status=done&id=7"),
                "request.body === 'status=done&id=7'",
                &HashSet::new(),
            )
            .await
            .result
            .expect("a text body must not break evaluation");
        assert_eq!(value, "true");
    }

    #[tokio::test]
    async fn real_engine_callback_condition_reports_a_script_error() {
        let svc = real_engine_service();
        let outcome = svc
            .evaluate_flow_callback_condition(
                "my-api",
                &payment_call("{}"),
                "request.body.missing.deeper === 1",
                &HashSet::new(),
            )
            .await;
        assert!(
            outcome.result.is_err(),
            "a thrown TypeError must be an error"
        );
    }

    #[tokio::test]
    async fn real_engine_wire_reads_plain_text_input_value() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("hello"));

        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "response.body", &HashSet::new())
            .await
            .result
            .expect("a plain-text body must stay a string");

        assert_eq!(value, "hello");
    }

    #[tokio::test]
    async fn real_engine_switch_style_string_of_body_field() {
        let mut out = sample_response_output();
        out.response.body = r#"{"plan":"pro"}"#.into();
        let svc = real_engine_service();
        let output = CapturedOutput::Request(Box::new(out));

        let value = svc
            .evaluate_flow_route_expression(
                "my-api",
                &output,
                "response.body.plan",
                FlowCoercion::Str,
                &HashSet::new(),
            )
            .await
            .result
            .expect("switch expression must resolve");

        assert_eq!(value, "pro");
    }

    #[tokio::test]
    async fn real_engine_exposes_status_text_headers_and_duration() {
        let mut out = sample_response_output();
        out.response.headers = vec![Header::new("X-Id", "7")];
        let svc = real_engine_service();
        let output = CapturedOutput::Request(Box::new(out));

        let value = svc
            .evaluate_flow_route_expression(
                "my-api",
                &output,
                "response.statusText + response.duration_ms + JSON.stringify(response.headers)",
                FlowCoercion::Str,
                &HashSet::new(),
            )
            .await
            .result
            .expect("all response fields must be defined");

        assert!(value.starts_with("OK10"), "unexpected value: {value}");
        assert!(value.contains("X-Id"), "unexpected value: {value}");
    }

    #[tokio::test]
    async fn real_engine_wire_runs_a_multi_line_body_with_return() {
        let svc = real_engine_service();
        let output = CapturedOutput::Request(Box::new(sample_response_output()));
        let value = svc
            .resolve_flow_wire_expression(
                "my-api",
                &output,
                "const t = response.body.token;\nreturn 'Bearer ' + t;",
                &HashSet::new(),
            )
            .await
            .result
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
                &HashSet::new(),
            )
            .await
            .result
            .expect_err("a thrown SyntaxError at run time must fail the wire");
        assert!(err.to_string().contains("SyntaxError"), "got {err}");
    }

    #[tokio::test]
    async fn real_engine_wire_keeps_quotes_backticks_and_newlines() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("x"));
        let value = svc
            .resolve_flow_wire_expression(
                "my-api",
                &output,
                "`a\"b` + '\\n' + \"c'd\"",
                &HashSet::new(),
            )
            .await
            .result
            .expect("special characters must survive");
        assert_eq!(value, "a\"b\nc'd");
    }

    #[tokio::test]
    async fn real_engine_wire_object_literal_is_an_expression() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("x"));
        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "{ a: 1 }", &HashSet::new())
            .await
            .result
            .expect("an object literal is an expression");
        assert_eq!(value, r#"{"a":1}"#);
    }

    #[tokio::test]
    async fn real_engine_wire_trailing_semicolon_still_returns_the_value() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("hello"));
        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "response.body;", &HashSet::new())
            .await
            .result
            .expect("a trailing semicolon must not lose the value");
        assert_eq!(value, "hello");
    }

    #[tokio::test]
    async fn real_engine_wire_trailing_line_comment_is_safe() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("hello"));
        let value = svc
            .resolve_flow_wire_expression(
                "my-api",
                &output,
                "response.body // note",
                &HashSet::new(),
            )
            .await
            .result
            .expect("a trailing comment must not swallow the wrapper");
        assert_eq!(value, "hello");
    }

    #[tokio::test]
    async fn real_engine_wire_body_without_return_is_an_error() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("hello"));
        let err = svc
            .resolve_flow_wire_expression(
                "my-api",
                &output,
                "const a = 1;\nconsole.log(a);",
                &HashSet::new(),
            )
            .await
            .result
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
                &HashSet::new(),
            )
            .await
            .result
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
                "const p = response.body.token;\nreturn p;",
                FlowCoercion::Str,
                &HashSet::new(),
            )
            .await
            .result
            .expect("a Switch body must run");
        assert_eq!(value, "abc123");
    }

    #[tokio::test]
    async fn real_engine_reports_a_syntax_error_in_both_forms() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("x"));
        let err = svc
            .resolve_flow_wire_expression("my-api", &output, "return (", &HashSet::new())
            .await
            .result
            .expect_err("broken source must fail");
        assert!(err.to_string().contains("SyntaxError"), "got {err}");
    }

    #[tokio::test]
    async fn real_engine_wire_returns_console_logs() {
        let svc = real_engine_service();
        let output = CapturedOutput::Value(VariableValue::simple("hello"));
        let outcome = svc
            .resolve_flow_wire_expression(
                "my-api",
                &output,
                "console.log('seen', response.body);\nreturn 1;",
                &HashSet::new(),
            )
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
            .resolve_flow_wire_expression(
                "my-api",
                &output,
                "console.warn('before');\nthrow new Error('boom');",
                &HashSet::new(),
            )
            .await;
        assert!(outcome.result.is_err());
        assert_eq!(outcome.logs.len(), 1);
        assert_eq!(outcome.logs[0].level, FlowLogLevel::Warn);
    }

    /// Builds `hello -> out` whose edge carries `expression`, runs it with
    /// the real engine, and returns the summary plus the recorded events.
    async fn run_scripted_output(expression: &str) -> (FlowRunSummary, Vec<DomainEvent>) {
        let mut edge = wire("e1", "hello", "out");
        edge.expression = expression.to_string();
        let flow = Flow {
            name: "logs".to_string(),
            nodes: vec![input_node_with("hello", "hello"), output_node_named("out")],
            edges: vec![edge],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let exec = real_engine_service();
        let summary = service.run(&exec, run_input("logs")).await.expect("run");
        (summary, publisher.events())
    }

    #[tokio::test]
    async fn a_wire_log_of_a_secret_variable_is_redacted() {
        let mut env = rocket_environment::Environment::new("dev");
        let mut token = rocket_environment::Variable::new("TOKEN", "s3cr3t-token-value");
        token.secret = true;
        env.set_variable(token);
        let exec = RequestExecutionService::new(
            Box::new(StaticEnvRepo(env)),
            Arc::new(NullExecutor),
            Box::new(NullHistoryRepo),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(rocket_infra::scripting::DenoScriptEngine::new()));
        let mut edge = wire("e1", "in", "out");
        edge.expression = "console.log(response.body);\nreturn response.body;".to_string();
        let flow = Flow {
            name: "secret".to_string(),
            nodes: vec![input_node_with("in", "{{TOKEN}}"), output_node_named("out")],
            edges: vec![edge],
            callback_host: None,
        };
        let mut input = run_input("secret");
        input.environment_name = Some("dev".to_string());

        let summary = service_with_flow(flow)
            .run(&exec, input)
            .await
            .expect("run");

        let step = step_of(&summary, "out");
        assert_eq!(step.logs.len(), 1);
        assert!(!step.logs[0].message.contains("s3cr3t-token-value"));
        assert!(step.logs[0].message.contains("••••••"));
    }

    fn completed_logs(events: &[DomainEvent], node: &str) -> Vec<FlowLogEntry> {
        events
            .iter()
            .find_map(|e| match e {
                DomainEvent::FlowStepCompleted { node_id, logs, .. } if node_id == node => {
                    Some(logs.clone())
                }
                _ => None,
            })
            .expect("FlowStepCompleted for node")
    }

    #[tokio::test]
    async fn a_run_reports_wire_console_logs_on_the_step_and_its_event() {
        let (summary, events) =
            run_scripted_output("console.log('out');\nreturn response.body;").await;
        let step = step_of(&summary, "out");
        assert_eq!(step.status, FlowNodeStatus::Success);
        assert_eq!(step.logs.len(), 1);
        assert!(step.logs[0].message.contains("out"));
        assert_eq!(completed_logs(&events, "out"), step.logs);
    }

    #[tokio::test]
    async fn a_failed_step_keeps_its_console_logs() {
        let (summary, events) =
            run_scripted_output("console.log('x');\nthrow new Error('no');").await;
        let step = step_of(&summary, "out");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert_eq!(step.logs.len(), 1);
        assert!(step.logs[0].message.contains('x'));
        assert_eq!(completed_logs(&events, "out"), step.logs);
    }

    // ---- Request debug mode -----------------------------------------------

    fn debug_request_node(debug: bool) -> FlowNode {
        FlowNode {
            id: "r".to_string(),
            kind: FlowNodeKind::Request {
                debug,
                repeat_until: None,
                label: "Login".to_string(),
                source: RequestSource::Inline {
                    request: InlineRequestData {
                        method: "post".to_string(),
                        url: "{{base}}/login".to_string(),
                        headers: vec![InlineHeader {
                            name: "X-Key".to_string(),
                            value: "{{secret}}".to_string(),
                        }],
                        body: Some(r#"{"k":"{{secret}}"}"#.to_string()),
                    },
                },
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    /// Runs one Request node against an environment with a plain `base` and a
    /// secret `secret` variable. Returns the summary and the recorded events.
    async fn run_debug_node(
        debug: bool,
        base: &str,
        status: u16,
    ) -> (FlowRunSummary, Vec<DomainEvent>) {
        let mut env = rocket_environment::Environment::new("dev");
        env.set_variable(rocket_environment::Variable::new("base", base));
        let mut secret = rocket_environment::Variable::new("secret", "sekret-value");
        secret.secret = true;
        env.set_variable(secret);
        let executor = RecordingExecutor::new();
        executor.set_status("/login", status);
        let exec = RequestExecutionService::new(
            Box::new(StaticEnvRepo(env)),
            Arc::new(SharedExecutor(executor)),
            Box::new(NullHistoryRepo),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let flow = Flow {
            name: "dbg".to_string(),
            nodes: vec![debug_request_node(debug)],
            edges: Vec::new(),
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let mut input = run_input("dbg");
        input.environment_name = Some("dev".to_string());
        let summary = service.run(&exec, input).await.expect("run");
        (summary, publisher.events())
    }

    fn completed_debug(events: &[DomainEvent], node: &str) -> Option<Box<FlowDebugRequest>> {
        events
            .iter()
            .find_map(|e| match e {
                DomainEvent::FlowStepCompleted {
                    node_id,
                    debug_request,
                    ..
                } if node_id == node => Some(debug_request.clone()),
                _ => None,
            })
            .expect("FlowStepCompleted for node")
    }

    #[tokio::test]
    async fn a_debug_request_node_reports_the_masked_request_on_the_step_and_event() {
        let (summary, events) = run_debug_node(true, "https://x.test", 200).await;
        let step = step_of(&summary, "r");
        let debug = step.debug_request.as_ref().expect("debug record");
        assert_eq!(debug.url, "https://x.test/login");
        let key = debug
            .headers
            .iter()
            .find(|h| h.key == "X-Key")
            .expect("X-Key header");
        assert_eq!(key.value, "••••••");
        assert_eq!(debug.body.as_deref(), Some(r#"{"k":"••••••"}"#));
        assert_eq!(debug.response.as_ref().map(|r| r.status), Some(200));
        assert_eq!(
            completed_debug(&events, "r").map(|d| *d),
            step.debug_request
        );
    }

    #[tokio::test]
    async fn a_request_node_without_debug_has_no_debug_record() {
        let (summary, events) = run_debug_node(false, "https://x.test", 200).await;
        assert_eq!(step_of(&summary, "r").debug_request, None);
        assert!(completed_debug(&events, "r").is_none());
    }

    #[tokio::test]
    async fn a_request_without_debug_still_reports_a_masked_exchange() {
        let (summary, events) = run_debug_node(false, "https://x.test", 200).await;
        let step = step_of(&summary, "r");
        assert_eq!(
            step.debug_request, None,
            "Debug mode still owns debug_request"
        );
        let exchange = step.exchange.as_ref().expect("exchange record");
        assert_eq!(exchange.url, "https://x.test/login");
        let key = exchange
            .headers
            .iter()
            .find(|h| h.key == "X-Key")
            .expect("X-Key header");
        assert_eq!(key.value, "••••••");
        assert_eq!(exchange.body.as_deref(), Some(r#"{"k":"••••••"}"#));
        assert_eq!(exchange.response.as_ref().map(|r| r.status), Some(200));
        let event_exchange = events.iter().find_map(|e| match e {
            DomainEvent::FlowStepCompleted {
                node_id, exchange, ..
            } if node_id == "r" => Some(exchange.clone()),
            _ => None,
        });
        assert_eq!(event_exchange.flatten().map(|b| *b), step.exchange);
    }

    #[tokio::test]
    async fn a_request_that_fails_to_send_reports_its_exchange_with_the_error() {
        let (summary, _) = run_debug_node(false, "https://x.test", 0).await;
        let exchange = step_of(&summary, "r").exchange.clone().expect("exchange");
        assert!(exchange.response.is_none());
        assert!(
            exchange
                .error
                .as_deref()
                .is_some_and(|e| e.contains("connection refused")),
            "got {:?}",
            exchange.error
        );
    }

    #[tokio::test]
    async fn a_polled_request_reports_the_last_attempts_exchange() {
        let executor = SequenceExecutor::new(vec![(404, "{}"), (404, "{}"), (200, r#"{"ok":1}"#)]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 5, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.debug_request, None);
        let response = step
            .exchange
            .clone()
            .and_then(|e| e.response)
            .expect("exchange response");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, r#"{"ok":1}"#);
    }

    #[tokio::test]
    async fn an_accepted_callback_reports_the_call_as_its_exchange() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.completed"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary =
            service_with_listener(register_then_wait(wait_node_with("w", 60_000, None)), &fake)
                .run(&exec, run_input("cb"))
                .await
                .expect("run");

        let exchange = step_of(&summary, "w").exchange.clone().expect("exchange");
        assert_eq!(exchange.method, "POST");
        assert_eq!(exchange.url, "/cb/0");
        let response = exchange.response.expect("response");
        assert!(response.body.contains("payment.completed"));
    }

    #[tokio::test]
    async fn an_if_step_has_no_exchange() {
        let flow = Flow {
            name: "if-env".to_string(),
            nodes: vec![
                input_node_with("in", "{{ENV_VAR}}"),
                if_node("check", "response.body === 'acme'"),
                output_node_named("yes"),
                output_node_named("no"),
            ],
            edges: vec![
                input_edge("e1", "in", "check"),
                trigger_edge("e2", "check", handle::TRUE, "yes"),
                trigger_edge("e3", "check", handle::FALSE, "no"),
            ],
            callback_host: None,
        };
        let exec = scoped_exec(env_with(&[("ENV_VAR", "acme")]), Vec::new());
        let mut input = run_input("if-env");
        input.environment_name = Some("dev".to_string());

        let summary = service_with_flow(flow)
            .run(&exec, input)
            .await
            .expect("run");

        let check = step_of(&summary, "check");
        assert_eq!(check.branch.as_deref(), Some(handle::TRUE));
        assert!(check.exchange.is_none(), "an If step has no exchange");
    }

    #[tokio::test]
    async fn a_given_up_poll_reports_the_last_attempts_exchange() {
        let executor = SequenceExecutor::new(vec![
            (404, r#"{"n":1}"#),
            (404, r#"{"n":2}"#),
            (404, r#"{"n":3}"#),
        ]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 3, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        let response = step
            .exchange
            .clone()
            .and_then(|e| e.response)
            .expect("exchange response");
        assert_eq!(response.status, 404);
        assert_eq!(response.body, r#"{"n":3}"#);
    }

    #[tokio::test]
    async fn debug_mode_keeps_a_full_debug_record_and_a_capped_exchange() {
        let limit = crate::flow_debug::EXCHANGE_BODY_LIMIT;
        let body: &'static str = Box::leak("a".repeat(limit + 10).into_boxed_str());
        let executor = SequenceExecutor::new(vec![(200, body)]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 3, 10_000), true);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        let debug = step
            .debug_request
            .clone()
            .and_then(|d| d.response)
            .expect("debug response");
        assert_eq!(debug.body.len(), limit + 10, "debug_request is not capped");
        assert!(!debug.truncated);
        let exchange = step
            .exchange
            .clone()
            .and_then(|e| e.response)
            .expect("exchange response");
        assert_eq!(exchange.body.len(), limit);
        assert!(exchange.truncated);
    }

    #[tokio::test]
    async fn an_input_reports_a_secret_masked_but_passes_it_on_raw() {
        let mut env = env_with(&[]);
        let mut key = rocket_environment::Variable::new("apiKey", "sk-live-123456");
        key.secret = true;
        env.set_variable(key);
        let exec = scoped_exec(env, Vec::new());
        let flow = Flow {
            name: "scope".to_string(),
            nodes: vec![
                input_node_with("in", "{{apiKey}}"),
                output_node_named("out"),
            ],
            edges: vec![FlowEdge {
                target_field: "value".to_string(),
                ..wire("e1", "in", "out")
            }],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let mut input = run_input("scope");
        input.environment_name = Some("dev".to_string());

        let summary = service_with_publisher(flow, &publisher)
            .run(&exec, input)
            .await
            .expect("run");

        assert_eq!(
            step_of(&summary, "in").value.as_deref(),
            Some(crate::redaction::REDACTED)
        );
        let event_value = publisher.events().iter().find_map(|e| match e {
            DomainEvent::FlowStepCompleted { node_id, value, .. } if node_id == "in" => {
                Some(value.clone())
            }
            _ => None,
        });
        assert_eq!(
            event_value.flatten().as_deref(),
            Some(crate::redaction::REDACTED)
        );
        assert_eq!(
            step_of(&summary, "out").value.as_deref(),
            Some("sk-live-123456"),
            "the downstream wire still gets the real value"
        );
    }

    #[tokio::test]
    async fn input_and_output_steps_have_no_exchange() {
        let service = service_with_flow(linear_flow());
        let exec = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("bob"),
            }),
        );
        let summary = service
            .run(&exec, run_input("auth-flow"))
            .await
            .expect("run");
        for step in &summary.steps {
            assert!(step.exchange.is_none(), "{} has an exchange", step.node_id);
        }
    }

    #[tokio::test]
    async fn a_debug_request_node_that_fails_to_send_keeps_the_request_and_error() {
        let (summary, events) = run_debug_node(true, "https://x.test", 0).await;
        let step = step_of(&summary, "r");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        let debug = step.debug_request.as_ref().expect("debug record");
        assert_eq!(debug.url, "https://x.test/login");
        assert!(debug.response.is_none());
        assert!(
            debug
                .error
                .as_deref()
                .is_some_and(|e| e.contains("connection refused")),
            "got {:?}",
            debug.error
        );
        assert_eq!(
            completed_debug(&events, "r").map(|d| *d),
            step.debug_request
        );
    }

    // ---- Repeat until -------------------------------------------------------

    use crate::test_doubles::{InMemoryHistoryRepo, SharedHistoryRepo};
    use rocket_flow::RepeatUntil;

    /// Answers each send with the next `(status, body)` in `script` and keeps
    /// answering with the last one after that. Status 0 fails the send.
    struct SequenceExecutor {
        script: Vec<(u16, &'static str)>,
        sent: std::sync::Mutex<usize>,
    }
    impl SequenceExecutor {
        fn new(script: Vec<(u16, &'static str)>) -> Arc<Self> {
            Arc::new(Self {
                script,
                sent: std::sync::Mutex::new(0),
            })
        }
        fn sent_count(&self) -> usize {
            *self.sent.lock().expect("lock")
        }
    }
    #[async_trait]
    impl HttpExecutor for SequenceExecutor {
        async fn execute(&self, _request: &HttpRequest) -> DomainResult<HttpResponse> {
            let index = {
                let mut sent = self.sent.lock().expect("lock");
                *sent += 1;
                *sent - 1
            };
            let (status, body) = self.script[index.min(self.script.len() - 1)];
            if status == 0 {
                return Err(DomainError::Http("connection refused".into()));
            }
            Ok(HttpResponse {
                status,
                status_text: "X".into(),
                headers: vec![],
                body: body.into(),
                duration_ms: 1,
                ttfb_ms: 1,
                size_bytes: body.len(),
            })
        }
    }

    fn poll_exec(
        executor: &Arc<SequenceExecutor>,
        engine: Box<dyn ScriptEngine>,
        history: &Arc<InMemoryHistoryRepo>,
    ) -> RequestExecutionService {
        RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::clone(executor) as Arc<dyn HttpExecutor>,
            Box::new(SharedHistoryRepo(Arc::clone(history))),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(engine)
    }

    fn repeat(
        condition: &str,
        interval_ms: u64,
        max_attempts: u32,
        timeout_ms: u64,
    ) -> RepeatUntil {
        RepeatUntil {
            condition: condition.to_string(),
            interval_ms,
            max_attempts,
            timeout_ms,
        }
    }

    /// One polling Request node, id `job`.
    fn poll_flow(repeat_until: RepeatUntil, debug_on: bool) -> Flow {
        let mut node = request_flow_node("job", "https://api.example.com/job");
        if let FlowNodeKind::Request {
            repeat_until: slot,
            debug,
            ..
        } = &mut node.kind
        {
            *slot = Some(repeat_until);
            *debug = debug_on;
        }
        Flow {
            name: "poll".to_string(),
            nodes: vec![node],
            edges: Vec::new(),
            callback_host: None,
        }
    }

    /// A scripted engine whose Bool conditions are true when the response
    /// status is `ok_status`.
    fn status_condition(ok_status: u16) -> Box<dyn ScriptEngine> {
        // `FromResponse` takes a fn pointer, so each status needs its own fn.
        fn is_200(r: Option<&rocket_http::HttpResponse>) -> serde_json::Value {
            serde_json::json!(r.map(|r| r.status == 200).unwrap_or(false))
        }
        fn is_404(r: Option<&rocket_http::HttpResponse>) -> serde_json::Value {
            serde_json::json!(r.map(|r| r.status == 404).unwrap_or(false))
        }
        let f = match ok_status {
            200 => is_200 as fn(Option<&rocket_http::HttpResponse>) -> serde_json::Value,
            404 => is_404,
            other => panic!("no condition fn for {other}"),
        };
        scripted(vec![("!!(", Scripted::FromResponse(f))])
    }

    #[tokio::test]
    async fn poll_succeeds_on_the_attempt_where_the_condition_holds() {
        let executor = SequenceExecutor::new(vec![(404, "{}"), (404, "{}"), (200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 5, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Success);
        assert_eq!(step.status_code, Some(200));
        assert_eq!(step.attempts, Some(3));
        assert_eq!(executor.sent_count(), 3);
        assert_eq!(history.saved_count(), 1, "only the final attempt is kept");
    }

    #[tokio::test]
    async fn poll_gives_up_after_max_attempts() {
        let executor = SequenceExecutor::new(vec![(404, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 3, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        let error = step.error.clone().expect("an error");
        // Like every node error, it carries the `DomainError` prefix.
        assert!(
            error.contains("condition not met after 3 attempts ("),
            "got: {error}"
        );
        assert_eq!(executor.sent_count(), 3);
        // A failed poll still reports its last response and attempt count.
        assert_eq!(step.status_code, Some(404));
        assert!(step.duration_ms.is_some(), "a failed poll has a duration");
        assert_eq!(step.attempts, Some(3));
    }

    #[tokio::test]
    async fn poll_saves_one_history_entry_even_when_it_gives_up() {
        let executor = SequenceExecutor::new(vec![(404, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 3, 10_000), false);

        service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        assert_eq!(history.saved_count(), 1);
    }

    #[tokio::test]
    async fn poll_gives_up_at_the_deadline() {
        let executor = SequenceExecutor::new(vec![(404, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        // 100 ms apart, 250 ms deadline: sends at about 0, 100, 200 and 250 ms.
        let flow = poll_flow(repeat("response.status === 200", 100, 1000, 250), false);
        let started = std::time::Instant::now();

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert!(step
            .error
            .as_deref()
            .unwrap_or("")
            .contains("condition not met after "));
        let sent = executor.sent_count();
        assert!((2..=6).contains(&sent), "sent {sent} times");
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(history.saved_count(), 1);
    }

    #[tokio::test]
    async fn poll_condition_true_on_a_non_2xx_response_succeeds() {
        let executor = SequenceExecutor::new(vec![(404, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(404), &history);
        let flow = poll_flow(repeat("response.status === 404", 100, 5, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Success);
        assert_eq!(step.status_code, Some(404));
        assert_eq!(step.attempts, Some(1));
    }

    #[tokio::test]
    async fn poll_condition_script_error_fails_at_once() {
        let executor = SequenceExecutor::new(vec![(200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Throw("ReferenceError: nope"))]),
            &history,
        );
        let flow = poll_flow(repeat("response.body.done", 100, 5, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert!(step.error.as_deref().unwrap_or("").contains("nope"));
        assert_eq!(executor.sent_count(), 1, "a script error is not retried");
        assert_eq!(step.status_code, Some(200));
        assert!(step.duration_ms.is_some(), "a failed poll has a duration");
        assert_eq!(step.attempts, Some(1));
    }

    #[tokio::test]
    async fn poll_condition_script_error_saves_the_attempt_to_history() {
        let executor = SequenceExecutor::new(vec![(200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Throw("ReferenceError: nope"))]),
            &history,
        );
        let flow = poll_flow(repeat("response.body.done", 100, 5, 10_000), false);

        service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        assert_eq!(
            history.saved_count(),
            1,
            "the attempt that got a response is kept"
        );
    }

    #[tokio::test]
    async fn poll_send_error_fails_at_once() {
        let executor = SequenceExecutor::new(vec![(0, "")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 5, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        // No response came back, so there is nothing to report.
        assert_eq!(step.status_code, None);
        assert_eq!(step.attempts, None);
        assert_eq!(executor.sent_count(), 1, "a send error is not retried");
        assert_eq!(
            history.saved_count(),
            0,
            "a send without a response saves nothing"
        );
    }

    #[tokio::test]
    async fn poll_publishes_progress_for_each_attempt() {
        let executor = SequenceExecutor::new(vec![(404, "{}"), (404, "{}"), (200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            poll_flow(repeat("response.status === 200", 100, 5, 10_000), false),
            &publisher,
        );

        service.run(&exec, run_input("poll")).await.expect("run");

        let progress: Vec<(Option<u32>, String)> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepProgress {
                    node_id,
                    attempt,
                    message,
                    ..
                } if node_id == "job" => Some((attempt, message)),
                _ => None,
            })
            .collect();
        assert_eq!(
            progress,
            vec![
                (Some(1), "attempt 1/5".to_string()),
                (Some(2), "attempt 2/5".to_string()),
                (Some(3), "attempt 3/5".to_string()),
            ]
        );

        // Started comes first, then every progress event, then Completed.
        let order: Vec<&'static str> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepStarted { node_id, .. } if node_id == "job" => Some("started"),
                DomainEvent::FlowStepProgress { node_id, .. } if node_id == "job" => {
                    Some("progress")
                }
                DomainEvent::FlowStepCompleted { node_id, .. } if node_id == "job" => {
                    Some("completed")
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            order,
            vec!["started", "progress", "progress", "progress", "completed"]
        );
    }

    #[tokio::test]
    async fn poll_stops_between_attempts_when_cancelled() {
        let executor = SequenceExecutor::new(vec![(404, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let publisher = RecordingPublisher::new();
        // A 30 s interval: without Stop the test would hang for 30 s.
        let service = service_with_publisher(
            poll_flow(repeat("response.status === 200", 30_000, 5, 60_000), false),
            &publisher,
        );
        let started = std::time::Instant::now();

        let run = service.run(&exec, run_input("poll"));
        let stop = async {
            loop {
                let events = publisher.events();
                let run_id = events.iter().find_map(|e| match e {
                    DomainEvent::FlowRunStarted { run_id, .. } => Some(run_id.clone()),
                    _ => None,
                });
                let polling = events.iter().any(|e| {
                    matches!(e, DomainEvent::FlowStepProgress { node_id, .. } if node_id == "job")
                });
                if let (Some(id), true) = (run_id, polling) {
                    service.cancel(&id);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        };
        let (summary, ()) = tokio::join!(run, stop);
        let summary = summary.expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert_eq!(step.error.as_deref(), Some("cancelled"));
        assert_eq!(summary.stopped_reason, "cancelled");
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(executor.sent_count(), 1);
        // A stopped poll still reports the response it got.
        assert_eq!(step.status_code, Some(404));
        assert!(step.duration_ms.is_some(), "a stopped poll has a duration");
        assert_eq!(step.attempts, Some(1));
        let completed = publisher
            .events()
            .into_iter()
            .find_map(|e| match e {
                DomainEvent::FlowStepCompleted {
                    node_id,
                    status_code,
                    duration_ms,
                    attempts,
                    ..
                } if node_id == "job" => Some((status_code, duration_ms.is_some(), attempts)),
                _ => None,
            })
            .expect("a completed event");
        assert_eq!(completed, (Some(404), true, Some(1)));
        assert_eq!(
            history.saved_count(),
            1,
            "Stop after a response keeps that attempt"
        );
    }

    #[tokio::test]
    async fn poll_keeps_going_after_a_pre_request_script_error() {
        let executor = SequenceExecutor::new(vec![(404, "{}"), (200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec_events = RecordingPublisher::new();
        // The pre-request script throws on every attempt; the condition
        // holds on a 200.
        fn is_200(r: Option<&rocket_http::HttpResponse>) -> serde_json::Value {
            serde_json::json!(r.map(|r| r.status == 200).unwrap_or(false))
        }
        let exec = RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::clone(&executor) as Arc<dyn HttpExecutor>,
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(SharedPublisher(Arc::clone(&exec_events))),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(scripted(vec![
            ("// pre", Scripted::Throw("pre-request boom")),
            ("!!(", Scripted::FromResponse(is_200)),
        ]));
        let mut request = Request::new("Job", HttpMethod::Get, "https://api.example.com/job");
        request.pre_request_script = Some("// pre".into());
        let mut node = saved_flow_node("job", "job.yml");
        if let FlowNodeKind::Request { repeat_until, .. } = &mut node.kind {
            *repeat_until = Some(repeat("response.status === 200", 100, 5, 10_000));
        }
        let service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow(
                "my-api",
                Flow {
                    name: "poll".to_string(),
                    nodes: vec![node],
                    edges: Vec::new(),
                    callback_host: None,
                },
            )),
            Box::new(FakeCollectionRepo::new().with_request("my-api", "job.yml", request)),
            Box::new(NullEventPublisher),
        );

        let summary = service.run(&exec, run_input("poll")).await.expect("run");

        let script_errors: Vec<String> = exec_events
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::ScriptError { phase, message, .. } if phase == "before-request" => {
                    Some(message)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            script_errors,
            vec![
                "pre-request boom".to_string(),
                "pre-request boom".to_string()
            ],
            "each attempt records its script error"
        );
        let step = step_of(&summary, "job");
        assert_eq!(
            step.status,
            FlowNodeStatus::Success,
            "error: {:?}",
            step.error
        );
        assert_eq!(
            step.attempts,
            Some(2),
            "a script error does not stop the poll"
        );
        assert_eq!(executor.sent_count(), 2);
    }

    #[tokio::test]
    async fn poll_debug_record_shows_the_last_attempt() {
        let executor = SequenceExecutor::new(vec![(404, "{}"), (200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 5, 10_000), true);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let debug = step_of(&summary, "job")
            .debug_request
            .clone()
            .expect("debug record");
        assert_eq!(debug.response.expect("a response").status, 200);
    }

    #[tokio::test]
    async fn real_engine_poll_waits_for_a_body_field() {
        let executor = SequenceExecutor::new(vec![
            (200, r#"{"status":"pending"}"#),
            (200, r#"{"status":"done"}"#),
        ]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(
            &executor,
            Box::new(rocket_infra::scripting::DenoScriptEngine::new()),
            &history,
        );
        let flow = poll_flow(
            repeat(r#"response.body.status === "done""#, 100, 5, 10_000),
            false,
        );

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(
            step.status,
            FlowNodeStatus::Success,
            "error: {:?}",
            step.error
        );
        assert_eq!(step.attempts, Some(2));
    }

    #[test]
    fn with_callback_listener_replaces_the_default_listener() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let _service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new()),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullEventPublisher),
        )
        .with_callback_listener(Box::new(Arc::clone(&fake)));
        assert_eq!(fake.opened_count(), 0, "building the service opens nothing");
    }

    #[tokio::test]
    async fn a_request_url_resolves_the_callback_variable() {
        let flow = Flow {
            name: "cb".to_string(),
            nodes: vec![
                request_flow_node(
                    "reg",
                    "https://api.example.com/register?cb={{callback.payment}}",
                ),
                wait_node("w", "payment"),
            ],
            edges: vec![trigger_edge("e1", "reg", handle::RESULT, "w")],
            callback_host: None,
        };
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        service_with_listener(flow, &fake)
            .run(&exec, run_input("cb"))
            .await
            .expect("run");

        assert_eq!(
            executor.sent_urls(),
            vec!["https://api.example.com/register?cb=http://fake:1/cb/0".to_string()]
        );
    }

    #[tokio::test]
    async fn two_wait_nodes_get_their_own_urls() {
        let flow = Flow {
            name: "two".to_string(),
            nodes: vec![
                wait_node("w1", "first"),
                wait_node("w2", "second"),
                request_flow_node(
                    "reg",
                    "https://api.example.com/r?a={{callback.first}}&b={{callback.second}}",
                ),
            ],
            edges: Vec::new(),
            callback_host: None,
        };
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        service_with_listener(flow, &fake)
            .run(&exec, run_input("two"))
            .await
            .expect("run");

        assert_eq!(fake.opened_count(), 2);
        assert_eq!(
            executor.sent_urls(),
            vec!["https://api.example.com/r?a=http://fake:1/cb/0&b=http://fake:1/cb/1".to_string()]
        );
    }

    #[tokio::test]
    async fn a_flow_without_wait_nodes_opens_no_listener() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        service_with_listener(linear_flow(), &fake)
            .run(&exec, run_input("auth-flow"))
            .await
            .expect("run");

        assert_eq!(fake.opened_count(), 0);
    }

    #[tokio::test]
    async fn the_flow_callback_host_is_passed_to_the_listener() {
        let flow = Flow {
            name: "host".to_string(),
            nodes: vec![wait_node("w", "payment")],
            edges: Vec::new(),
            callback_host: Some("host.docker.internal".to_string()),
        };
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        service_with_listener(flow, &fake)
            .run(&exec, run_input("host"))
            .await
            .expect("run");

        assert_eq!(fake.hosts(), vec![Some("host.docker.internal".to_string())]);
    }

    #[tokio::test]
    async fn every_endpoint_is_closed_when_the_run_ends() {
        let flow = Flow {
            name: "close".to_string(),
            nodes: vec![wait_node("w", "payment")],
            edges: Vec::new(),
            callback_host: None,
        };
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        service_with_listener(flow, &fake)
            .run(&exec, run_input("close"))
            .await
            .expect("run");

        assert!(fake.is_closed(0));
    }

    #[tokio::test]
    async fn a_listener_failure_fails_the_run_before_it_starts() {
        let flow = Flow {
            name: "fail".to_string(),
            nodes: vec![wait_node("w", "payment")],
            edges: Vec::new(),
            callback_host: None,
        };
        let fake = crate::test_doubles::FakeCallbackListener::failing("port in use");
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher)
            .with_callback_listener(Box::new(Arc::clone(&fake)));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let err = service
            .run(&exec, run_input("fail"))
            .await
            .expect_err("the run must fail");

        let message = err.to_string();
        assert!(
            message.contains("could not open callback listener"),
            "got: {message}"
        );
        assert!(message.contains("port in use"), "got: {message}");
        assert!(
            publisher.events().is_empty(),
            "no FlowRunStarted or any other event for a run that never started"
        );
        assert!(
            !publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::FlowRunStarted { .. })),
            "a run that never started must not publish FlowRunStarted"
        );
        assert!(service.in_flight.lock().expect("lock").is_empty());
        assert!(service.cancelled.lock().expect("lock").is_empty());
        assert!(service.cancel_handles.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn a_call_before_the_nodes_turn_is_accepted() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        // Delivered when the endpoint opens, before `reg` even runs.
        fake.queue_on_open(event_call("payment.completed"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary =
            service_with_listener(register_then_wait(wait_node_with("w", 60_000, None)), &fake)
                .run(&exec, run_input("cb"))
                .await
                .expect("run");

        let step = step_of(&summary, "w");
        assert_eq!(step.status, FlowNodeStatus::Success, "{:?}", step.error);
        assert_eq!(step.status_code, Some(200));
        assert_eq!(step.value.as_deref(), Some("POST"));
        assert!(fake.is_closed(0), "the endpoint closes after a success");
    }

    #[tokio::test]
    async fn a_call_during_the_wait_is_accepted_and_progress_is_reported() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            register_then_wait(wait_node_with("w", 60_000, None)),
            &publisher,
        )
        .with_callback_listener(Box::new(Arc::clone(&fake)));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let (summary, ()) = tokio::join!(service.run(&exec, run_input("cb")), async {
            fake.wait_opened(1).await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            fake.sender(0)
                .send(event_call("payment.completed"))
                .await
                .expect("send");
        });
        let summary = summary.expect("run");

        assert_eq!(status_of(&summary, "w"), FlowNodeStatus::Success);
        let progress: Vec<String> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepProgress {
                    node_id, message, ..
                } if node_id == "w" => Some(message),
                _ => None,
            })
            .collect();
        assert!(!progress.is_empty(), "the wait reports progress");
        assert!(
            progress[0].starts_with("waiting… ") && progress[0].ends_with("0 ignored call(s)"),
            "got: {progress:?}"
        );
    }

    #[tokio::test]
    async fn accept_when_skips_calls_that_do_not_match() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.pending"));
        fake.queue_on_open(event_call("payment.completed"));
        let executor = RecordingExecutor::new();
        // The generated accept_when script contains `const request`; this
        // rule answers from the carried call body.
        let exec = recording_exec(
            &executor,
            scripted(vec![(
                "const request",
                Scripted::FromResponse(|r| {
                    serde_json::json!(r.is_some_and(|r| r.body.contains("payment.completed")))
                }),
            )]),
        );
        let flow = register_then_wait(wait_node_with(
            "w",
            60_000,
            Some("request.body.event === 'payment.completed'"),
        ));

        let summary = service_with_listener(flow, &fake)
            .run(&exec, run_input("cb"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "w"), FlowNodeStatus::Success);
    }

    #[tokio::test]
    async fn timeout_fails_the_node_and_reports_ignored_calls() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.pending"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![(
                "const request",
                Scripted::Value(serde_json::json!(false)),
            )]),
        );
        let flow = register_then_wait(wait_node_with("w", 1000, Some("request.body.ok")));

        let summary = service_with_listener(flow, &fake)
            .run(&exec, run_input("cb"))
            .await
            .expect("run");

        let step = step_of(&summary, "w");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert_eq!(
            step.error.as_deref(),
            Some("Invalid input: no matching callback within 1s (1 ignored)")
        );
        assert!(fake.is_closed(0), "the endpoint closes after a failure");
    }

    #[tokio::test]
    async fn an_accept_when_script_error_fails_the_node_at_once() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.completed"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![(
                "const request",
                Scripted::Throw("ReferenceError: nope"),
            )]),
        );
        let flow = register_then_wait(wait_node_with("w", 60_000, Some("nope.ok")));

        let started = std::time::Instant::now();
        let summary = service_with_listener(flow, &fake)
            .run(&exec, run_input("cb"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "w"), FlowNodeStatus::Failed);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "no 60 s wait"
        );
    }

    #[tokio::test]
    async fn stop_during_the_wait_ends_the_run_promptly() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            register_then_wait(wait_node_with("w", 60_000, None)),
            &publisher,
        )
        .with_callback_listener(Box::new(Arc::clone(&fake)));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let started = std::time::Instant::now();
        let (summary, ()) = tokio::join!(service.run(&exec, run_input("cb")), async {
            fake.wait_opened(1).await;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let run_id = publisher
                .events()
                .into_iter()
                .find_map(|e| match e {
                    DomainEvent::FlowRunStarted { run_id, .. } => Some(run_id),
                    _ => None,
                })
                .expect("the run started");
            service.cancel(&run_id);
        });
        let summary = summary.expect("run");

        assert_eq!(summary.stopped_reason, "cancelled");
        let step = step_of(&summary, "w");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert_eq!(step.error.as_deref(), Some("cancelled"));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert!(fake.is_closed(0), "the endpoint closes after a cancel");
    }

    #[tokio::test]
    async fn a_skipped_wait_node_still_closes_its_endpoint() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        executor.set_status("register", 500);
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary =
            service_with_listener(register_then_wait(wait_node_with("w", 60_000, None)), &fake)
                .run(&exec, run_input("cb"))
                .await
                .expect("run");

        assert_eq!(status_of(&summary, "w"), FlowNodeStatus::Skipped);
        assert!(fake.is_closed(0));
    }

    #[tokio::test]
    async fn a_downstream_wire_reads_the_callback_body() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.completed"));
        let flow = Flow {
            name: "cb".to_string(),
            nodes: vec![wait_node_with("w", 60_000, None), output_node_named("out")],
            edges: vec![edge_from(
                "e1",
                "w",
                handle::RESULT,
                "out",
                "value",
                "response.body.orderId",
            )],
            callback_host: None,
        };

        let summary = service_with_listener(flow, &fake)
            .run(&real_engine_service(), run_input("cb"))
            .await
            .expect("run");

        assert_eq!(step_of(&summary, "out").value.as_deref(), Some("42"));
    }

    #[tokio::test]
    async fn a_failed_second_open_closes_the_first_endpoint() {
        let flow = Flow {
            name: "partial".to_string(),
            nodes: vec![wait_node("w1", "first"), wait_node("w2", "second")],
            edges: Vec::new(),
            callback_host: None,
        };
        let fake = crate::test_doubles::FakeCallbackListener::failing_after(1, "port in use");
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let err = service_with_listener(flow, &fake)
            .run(&exec, run_input("partial"))
            .await
            .expect_err("the run must fail before it starts");

        assert!(err.to_string().contains("port in use"), "got: {err}");
        assert_eq!(fake.opened_count(), 1, "the first endpoint opened");
        assert!(fake.is_closed(0), "the first endpoint closes again");
        assert!(executor.sent_urls().is_empty(), "no node ran");
    }

    #[tokio::test]
    async fn a_closed_endpoint_fails_the_wait_instead_of_spinning() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));
        let service =
            service_with_listener(register_then_wait(wait_node_with("w", 60_000, None)), &fake);

        let started = std::time::Instant::now();
        let (summary, ()) = tokio::join!(service.run(&exec, run_input("cb")), async {
            fake.wait_opened(1).await;
            fake.hang_up(0);
        });
        let summary = summary.expect("run");

        let step = step_of(&summary, "w");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert!(
            step.error.as_deref().is_some_and(|e| e.contains("closed")),
            "got: {:?}",
            step.error
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "no 60 s wait"
        );
    }

    // ---- Phase 3: Transform ------------------------------------------------

    #[test]
    fn required_coercion_guards_against_undefined() {
        let script = flow_script("return response.body;", FlowCoercion::Required).expect("wrapper");
        assert!(
            script.contains("return __requireValue(fn(response));"),
            "got: {script}"
        );
        assert!(script.contains("script returned no value"), "got: {script}");
    }

    #[test]
    fn other_coercions_do_not_call_the_guard() {
        for coercion in [FlowCoercion::Raw, FlowCoercion::Bool, FlowCoercion::Str] {
            let script = flow_script("1", coercion).expect("wrapper");
            assert!(!script.contains("__requireValue(fn"), "got: {script}");
        }
    }

    async fn transform_result(answer: Scripted) -> FlowScriptOutcome {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            scripted(vec![("__requireValue(", answer)]),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));
        svc.evaluate_flow_transform_script("my-api", &output, "return 1;", &HashSet::new())
            .await
    }

    #[tokio::test]
    async fn transform_script_returns_a_string_as_is() {
        let outcome = transform_result(Scripted::Value(serde_json::json!("abc"))).await;
        assert_eq!(outcome.result.expect("script result"), "abc");
    }

    #[tokio::test]
    async fn transform_script_returns_an_object_as_compact_json() {
        let outcome =
            transform_result(Scripted::Value(serde_json::json!({"a": 1, "b": [2]}))).await;
        assert_eq!(outcome.result.expect("script result"), r#"{"a":1,"b":[2]}"#);
    }

    #[tokio::test]
    async fn transform_script_turns_null_into_the_text_null() {
        let outcome = transform_result(Scripted::Value(serde_json::Value::Null)).await;
        assert_eq!(outcome.result.expect("script result"), "null");
    }

    #[tokio::test]
    async fn transform_script_keeps_zero_and_false() {
        let zero = transform_result(Scripted::Value(serde_json::json!(0))).await;
        assert_eq!(zero.result.expect("script result"), "0");
        let no = transform_result(Scripted::Value(serde_json::json!(false))).await;
        assert_eq!(no.result.expect("script result"), "false");
    }

    #[tokio::test]
    async fn transform_script_reports_a_script_error() {
        let outcome = transform_result(Scripted::Throw("script returned no value")).await;
        assert!(matches!(
            outcome.result,
            Err(DomainError::InvalidInput(ref m)) if m.contains("script returned no value")
        ));
    }

    fn transform_node(id: &str, script: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Transform {
                label: id.to_string(),
                script: script.to_string(),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    /// in("pro") -> t -> out (value).
    fn transform_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![
                input_node_with("in", "pro"),
                transform_node("t", "return response.body.toUpperCase();"),
                output_node_named("out"),
            ],
            edges: vec![
                input_edge("e1", "in", "t"),
                edge_from("e2", "t", handle::RESULT, "out", "value", "response.body"),
            ],
            callback_host: None,
        }
    }

    /// Answers the Transform wrapper with `answer` and any wire from the body.
    fn transform_engine(answer: Scripted) -> Box<dyn ScriptEngine> {
        scripted(vec![
            ("__requireValue(", answer),
            (
                "response.body",
                Scripted::FromResponse(|r| {
                    serde_json::json!(r.map(|r| r.body.clone()).unwrap_or_default())
                }),
            ),
        ])
    }

    async fn run_transform(name: &str, answer: Scripted) -> FlowRunSummary {
        let service = service_with_flow(transform_flow(name));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, transform_engine(answer));
        service.run(&exec, run_input(name)).await.expect("run")
    }

    #[tokio::test]
    async fn transform_reshapes_its_input_for_a_downstream_output() {
        let summary = run_transform("tf-ok", Scripted::Value(serde_json::json!("PRO"))).await;

        assert_eq!(status_of(&summary, "t"), FlowNodeStatus::Success);
        assert_eq!(step_of(&summary, "t").value.as_deref(), Some("PRO"));
        assert_eq!(step_of(&summary, "out").value.as_deref(), Some("PRO"));
    }

    #[tokio::test]
    async fn transform_reports_an_object_result_as_compact_json() {
        let summary = run_transform(
            "tf-obj",
            Scripted::Value(serde_json::json!({"plan": "pro"})),
        )
        .await;

        assert_eq!(
            step_of(&summary, "t").value.as_deref(),
            Some(r#"{"plan":"pro"}"#)
        );
    }

    #[tokio::test]
    async fn a_throwing_transform_fails_and_skips_its_dependents() {
        let summary = run_transform("tf-throw", Scripted::Throw("ReferenceError: nope")).await;

        assert_eq!(status_of(&summary, "t"), FlowNodeStatus::Failed);
        assert!(step_of(&summary, "t")
            .error
            .as_deref()
            .is_some_and(|e| e.contains("ReferenceError")));
        assert_eq!(status_of(&summary, "out"), FlowNodeStatus::Skipped);
        assert_eq!(
            step_of(&summary, "out").skip_reason,
            Some(FlowSkipReason::UpstreamFailed)
        );
    }

    #[tokio::test]
    async fn transform_that_returns_nothing_fails_with_a_clear_error() {
        let summary = run_transform("tf-none", Scripted::Throw("script returned no value")).await;

        assert_eq!(status_of(&summary, "t"), FlowNodeStatus::Failed);
        assert!(step_of(&summary, "t")
            .error
            .as_deref()
            .is_some_and(|e| e.contains("script returned no value")));
    }

    #[tokio::test]
    async fn a_transform_reports_a_secret_masked_but_passes_it_on_raw() {
        let mut env = env_with(&[]);
        let mut key = rocket_environment::Variable::new("apiKey", "sk-live-123456");
        key.secret = true;
        env.set_variable(key);
        let exec = scoped_exec(env, Vec::new());
        let flow = Flow {
            name: "tf-secret".to_string(),
            nodes: vec![
                input_node_with("in", "{{apiKey}}"),
                transform_node("t", "response.body"),
                output_node_named("out"),
            ],
            edges: vec![
                input_edge("e1", "in", "t"),
                edge_from("e2", "t", handle::RESULT, "out", "value", "response.body"),
            ],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let mut input = run_input("tf-secret");
        input.environment_name = Some("dev".to_string());

        let summary = service_with_publisher(flow, &publisher)
            .run(&exec, input)
            .await
            .expect("run");

        assert_eq!(
            step_of(&summary, "t").value.as_deref(),
            Some(crate::redaction::REDACTED)
        );
        let event_value = publisher.events().iter().find_map(|e| match e {
            DomainEvent::FlowStepCompleted { node_id, value, .. } if node_id == "t" => {
                Some(value.clone())
            }
            _ => None,
        });
        assert_eq!(
            event_value.flatten().as_deref(),
            Some(crate::redaction::REDACTED)
        );
        assert_eq!(
            step_of(&summary, "out").value.as_deref(),
            Some("sk-live-123456"),
            "the downstream wire still gets the real value"
        );
    }

    /// in -> check(if) -> true: t -> out.
    fn transform_after_if_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![
                input_node_with("in", "pro"),
                if_node("check", "response.body === 'pro'"),
                transform_node("t", "return response.body;"),
                output_node_named("out"),
            ],
            edges: vec![
                input_edge("e1", "in", "check"),
                edge_from("e2", "check", handle::TRUE, "t", handle::INPUT, ""),
                edge_from("e3", "t", handle::RESULT, "out", "value", "response.body"),
            ],
            callback_host: None,
        }
    }

    #[tokio::test]
    async fn transform_after_a_not_taken_branch_is_skipped() {
        let service = service_with_flow(transform_after_if_flow("tf-branch"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(false)))]),
        );

        let summary = service
            .run(&exec, run_input("tf-branch"))
            .await
            .expect("run");

        for id in ["t", "out"] {
            assert_eq!(
                status_of(&summary, id),
                FlowNodeStatus::Skipped,
                "node {id}"
            );
            assert_eq!(
                step_of(&summary, id).skip_reason,
                Some(FlowSkipReason::BranchNotTaken),
                "node {id}"
            );
        }
    }

    #[tokio::test]
    async fn transform_after_a_taken_branch_runs() {
        let service = service_with_flow(transform_after_if_flow("tf-taken"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![
                ("!!(", Scripted::Value(serde_json::json!(true))),
                ("__requireValue(", Scripted::Value(serde_json::json!("PRO"))),
                (
                    "response.body",
                    Scripted::FromResponse(|r| {
                        serde_json::json!(r.map(|r| r.body.clone()).unwrap_or_default())
                    }),
                ),
            ]),
        );

        let summary = service
            .run(&exec, run_input("tf-taken"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "t"), FlowNodeStatus::Success);
        assert_eq!(step_of(&summary, "out").value.as_deref(), Some("PRO"));
    }

    #[tokio::test]
    async fn one_transform_can_feed_two_consumers() {
        let flow = Flow {
            name: "tf-fan".to_string(),
            nodes: vec![
                input_node_with("in", "pro"),
                transform_node("t", "return response.body;"),
                output_node_named("out1"),
                output_node_named("out2"),
            ],
            edges: vec![
                input_edge("e1", "in", "t"),
                edge_from("e2", "t", handle::RESULT, "out1", "value", "response.body"),
                edge_from("e3", "t", handle::RESULT, "out2", "value", "response.body"),
            ],
            callback_host: None,
        };
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            transform_engine(Scripted::Value(serde_json::json!("SHARED"))),
        );

        let summary = service.run(&exec, run_input("tf-fan")).await.expect("run");

        assert_eq!(step_of(&summary, "out1").value.as_deref(), Some("SHARED"));
        assert_eq!(step_of(&summary, "out2").value.as_deref(), Some("SHARED"));
    }
}
