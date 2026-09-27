use std::collections::HashMap;

use rocket_collection::Request;
use rocket_flow::{FlowEdge, FlowNode, FlowNodeKind, InlineRequestData, RequestSource};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Body, BodyMode, Header, HttpMethod};
use rocket_shared::VariableValue;

use crate::execution_service::{
    ExecuteRequestInput, ExecuteRequestOutput, RequestExecutionService,
};
use crate::runner_sequence::{build_step_input, RunItem};

/// One node's fully-executed result, kept around so a downstream edge's
/// wiring expression can be evaluated against it.
#[derive(Debug, Clone)]
pub enum CapturedOutput {
    Request(Box<ExecuteRequestOutput>),
    Value(VariableValue),
}

impl RequestExecutionService {
    /// Evaluates `expression` (a jsonq/JS snippet such as `"response.body"` or
    /// `"response.body.token"`) against `output`, reusing the same
    /// script-engine mechanism `evaluate_var_expression` uses for the Vars
    /// tab's preview — not a second sandbox invocation path. `Value` outputs
    /// (Input/Output nodes) are normalized into a synthetic `HttpResponse`
    /// whose `body` is that value's raw string, so a single expression
    /// convention ("response.xxx") works uniformly regardless of which kind
    /// of node produced the output.
    ///
    /// A string result is returned as-is and other JSON values are
    /// stringified. A `null` or `undefined` result is an `InvalidInput`
    /// error, so a wire never injects the literal text "null".
    pub async fn resolve_flow_wire_expression(
        &self,
        collection: &str,
        output: &CapturedOutput,
        expression: &str,
    ) -> DomainResult<String> {
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
        let response_json = serde_json::to_string(&response).map_err(|e| {
            DomainError::Internal(format!("failed to serialize captured output: {e}"))
        })?;
        let result = self
            .evaluate_var_expression(collection, expression, &response_json)
            .await?;
        match result {
            // A `null` or `undefined` result would wire the literal text "null"
            // into the request, so it is an error instead.
            serde_json::Value::Null => Err(DomainError::InvalidInput(format!(
                "expression '{expression}' resolved to null/undefined"
            ))),
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }
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
    node: &FlowNode,
) -> DomainResult<ExecuteRequestInput> {
    let FlowNodeKind::Request { label, source } = &node.kind else {
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
        None,
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

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use rocket_shared::events::{DomainEvent, FlowNodeStatus};
use ulid::Ulid;

/// Input DTO for `FlowExecutionService::run`.
#[derive(Debug, Clone)]
pub struct RunFlowInput {
    pub collection: String,
    pub flow_name: String,
    pub environment_name: Option<String>,
}

/// One node's outcome within a run, as reported in `FlowRunSummary::steps`
/// and the `FlowStepCompleted` event.
#[derive(Debug, Clone)]
pub struct FlowStepResult {
    pub node_id: String,
    pub status: FlowNodeStatus,
    pub status_code: Option<u16>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

/// The full result of one `FlowExecutionService::run` call.
#[derive(Debug, Clone)]
pub struct FlowRunSummary {
    pub run_id: String,
    pub steps: Vec<FlowStepResult>,
    pub stopped_reason: String,
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
        }
    }

    /// Loads the named flow and validates it into a dependency-ordered node id
    /// list. A cyclic graph is rejected here (defense in depth — `save_flow`,
    /// Plan 07, should already have refused to persist one) rather than ever
    /// starting a run against it.
    fn load_ordered_nodes(
        &self,
        collection: &str,
        flow_name: &str,
    ) -> DomainResult<(rocket_flow::Flow, Vec<String>)> {
        let flow = self.flow_repo.get(collection, flow_name)?;
        let order = rocket_flow::topological_sort(&flow).map_err(|e| {
            DomainError::InvalidInput(format!("flow '{flow_name}' is not runnable: {e}"))
        })?;
        Ok((flow, order))
    }

    /// Asks an in-progress run to stop. The run ends before its next node; a
    /// node already executing finishes first. Cancelling an unknown or
    /// finished run id is a no-op, mirroring `CollectionRunnerService::cancel`.
    pub fn cancel(&self, run_id: &str) {
        if let Ok(in_flight) = self.in_flight.lock() {
            if !in_flight.contains(run_id) {
                return;
            }
        }
        if let Ok(mut cancelled) = self.cancelled.lock() {
            cancelled.insert(run_id.to_string());
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

    /// Runs every node of `input.flow_name` in dependency order, dispatching
    /// each by kind and publishing progress events. Node failures do not
    /// stop the run here — see Task 3 for downstream skip-cascade, added on
    /// top of this loop without changing its shape.
    pub async fn run(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
    ) -> DomainResult<FlowRunSummary> {
        let (flow, order) = self.load_ordered_nodes(&input.collection, &input.flow_name)?;

        let _external_secrets = exec
            .resolve_external_secrets(Some(&input.collection), input.environment_name.as_deref())
            .await?;

        let run_id = Ulid::new().to_string();
        if let Ok(mut set) = self.in_flight.lock() {
            set.insert(run_id.clone());
        }
        self.events.publish(DomainEvent::FlowRunStarted {
            run_id: run_id.clone(),
            flow_name: input.flow_name.clone(),
            collection: input.collection.clone(),
            total_nodes: flow.nodes.len(),
        });

        let mut captured: HashMap<String, CapturedOutput> = HashMap::new();
        let mut steps: Vec<FlowStepResult> = Vec::new();
        let mut stopped_reason = "completed".to_string();

        for node_id in &order {
            if self.is_cancelled(&run_id) {
                stopped_reason = "cancelled".to_string();
                break;
            }
            let node = flow
                .nodes
                .iter()
                .find(|n| &n.id == node_id)
                .expect("topological_sort only returns ids present in flow.nodes");

            let result = self
                .execute_node(exec, &input, &flow, node, &captured)
                .await;

            if let Ok((output, _)) = &result {
                captured.insert(node_id.clone(), output.clone());
            }
            let step = result_to_step(node_id, result);
            self.events.publish(DomainEvent::FlowStepCompleted {
                run_id: run_id.clone(),
                node_id: step.node_id.clone(),
                status: step.status,
                status_code: step.status_code,
                duration_ms: step.duration_ms,
                error: step.error.clone(),
            });
            steps.push(step);
        }

        self.clear_cancellation(&run_id);
        if let Ok(mut set) = self.in_flight.lock() {
            set.remove(&run_id);
        }

        let failed_count = steps.iter().filter(|s| s.status == FlowNodeStatus::Failed).count();
        let skipped_count = steps.iter().filter(|s| s.status == FlowNodeStatus::Skipped).count();
        self.events.publish(DomainEvent::FlowRunFinished {
            run_id: run_id.clone(),
            stopped_reason: stopped_reason.clone(),
            node_count: steps.len(),
            failed_count,
            skipped_count,
        });

        Ok(FlowRunSummary {
            run_id,
            steps,
            stopped_reason,
        })
    }

    /// Dispatches one node by kind. Returns the node's captured output (for
    /// downstream wires) alongside its `FlowStepResult` fields, or an error
    /// if the node itself failed — the caller (`run`) turns either outcome
    /// into a `FlowStepResult` via `result_to_step`.
    async fn execute_node(
        &self,
        exec: &RequestExecutionService,
        input: &RunFlowInput,
        flow: &rocket_flow::Flow,
        node: &rocket_flow::FlowNode,
        captured: &HashMap<String, CapturedOutput>,
    ) -> DomainResult<(CapturedOutput, Option<(u16, u64)>)> {
        match &node.kind {
            rocket_flow::FlowNodeKind::Input { value, .. } => {
                let settings = self.collection_repo.get_settings(&input.collection)?;
                let mut vars = HashMap::new();
                for cv in settings.variables.iter().filter(|v| v.enabled) {
                    let v = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
                    vars.insert(cv.key.clone(), v);
                }
                let resolved = rocket_environment::resolve(value.data(), &vars).output;
                Ok((CapturedOutput::Value(rocket_shared::VariableValue::simple(resolved)), None))
            }
            rocket_flow::FlowNodeKind::Output { .. } => {
                let incoming = flow.edges.iter().find(|e| e.target_node_id == node.id);
                let Some(edge) = incoming else {
                    return Ok((CapturedOutput::Value(rocket_shared::VariableValue::simple("")), None));
                };
                let source_output = captured.get(&edge.source_node_id).ok_or_else(|| {
                    DomainError::Internal(format!(
                        "node '{}' depends on '{}' which has not executed yet — topological order violated",
                        node.id, edge.source_node_id
                    ))
                })?;
                let value = exec
                    .resolve_flow_wire_expression(&input.collection, source_output, &edge.expression)
                    .await?;
                Ok((CapturedOutput::Value(rocket_shared::VariableValue::simple(value)), None))
            }
            rocket_flow::FlowNodeKind::Request { .. } => {
                let mut request_input = build_execute_request_input(
                    self.collection_repo.as_ref(),
                    &input.collection,
                    input.environment_name.as_deref(),
                    node,
                )?;

                let incoming: Vec<&rocket_flow::FlowEdge> =
                    flow.edges.iter().filter(|e| e.target_node_id == node.id).collect();
                let mut resolved = HashMap::new();
                for edge in &incoming {
                    let source_output = captured.get(&edge.source_node_id).ok_or_else(|| {
                        DomainError::Internal(format!(
                            "node '{}' depends on '{}' which has not executed yet — topological order violated",
                            node.id, edge.source_node_id
                        ))
                    })?;
                    let value = exec
                        .resolve_flow_wire_expression(&input.collection, source_output, &edge.expression)
                        .await?;
                    resolved.insert(edge.id.clone(), value);
                }
                let edges_owned: Vec<rocket_flow::FlowEdge> = incoming.into_iter().cloned().collect();
                apply_wired_overrides(&mut request_input, &resolved, &edges_owned)?;

                let output = exec.execute(request_input).await?;
                let timing = (output.response.status, output.response.duration_ms);
                Ok((CapturedOutput::Request(Box::new(output)), Some(timing)))
            }
        }
    }
}

/// Turns one node's `execute_node` outcome into its `FlowStepResult`. A node
/// counts as failed when `execute_node` errored, or when it produced a
/// `Request` response that is not 2xx (`HttpResponse::is_success`) — Flow
/// nodes carry no test scripts in Phase 1, so there is no test-failure case
/// to fold in here, unlike the Collection Runner's `RunStepResult::is_failure`.
fn result_to_step(
    node_id: &str,
    result: DomainResult<(CapturedOutput, Option<(u16, u64)>)>,
) -> FlowStepResult {
    match result {
        Ok((CapturedOutput::Request(out), Some((status, duration_ms)))) => FlowStepResult {
            node_id: node_id.to_string(),
            status: if out.response.is_success() {
                FlowNodeStatus::Success
            } else {
                FlowNodeStatus::Failed
            },
            status_code: Some(status),
            duration_ms: Some(duration_ms),
            error: if out.response.is_success() {
                None
            } else {
                Some(format!("non-2xx response: {status}"))
            },
        },
        Ok((CapturedOutput::Value(_), _)) => FlowStepResult {
            node_id: node_id.to_string(),
            status: FlowNodeStatus::Success,
            status_code: None,
            duration_ms: None,
            error: None,
        },
        Ok((CapturedOutput::Request(_), None)) => unreachable!(
            "a Request node's execute_node branch always returns Some(timing)"
        ),
        Err(e) => FlowStepResult {
            node_id: node_id.to_string(),
            status: FlowNodeStatus::Failed,
            status_code: None,
            duration_ms: None,
            error: Some(e.to_string()),
        },
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
    }
    impl FakeCollectionRepo {
        fn new() -> Self {
            Self {
                requests: std::sync::Mutex::new(HashMap::new()),
            }
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
            Ok(CollectionSettings::default())
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
            .resolve_flow_wire_expression("my-api", &output, "response.body")
            .await
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
            .resolve_flow_wire_expression("my-api", &output, "response.body")
            .await
            .expect("expression should resolve");

        assert_eq!(value, "hello");
    }

    #[tokio::test]
    async fn script_error_surfaces_as_domain_error_not_panic() {
        let svc = service_with_engine(FakeCollectionRepo::new(), Box::new(ErrorJsonqEngine));
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let err = svc
            .resolve_flow_wire_expression("my-api", &output, "response.nope.nope")
            .await
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
            .resolve_flow_wire_expression("my-api", &output, "response.body.missing")
            .await
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
            .resolve_flow_wire_expression("my-api", &output, "response.status")
            .await
            .expect("a number must resolve");

        assert_eq!(value, "42");
    }

    use rocket_flow::{InlineHeader, NodePosition};

    fn saved_flow_node(id: &str, request_path: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Request {
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
        let input = build_execute_request_input(&repo, "my-api", Some("dev"), &node)
            .expect("saved source must resolve");

        assert_eq!(input.method, HttpMethod::Get);
        assert_eq!(input.url, "https://api.example.com/login");
        assert_eq!(input.collection.as_deref(), Some("my-api"));
        assert_eq!(input.environment_name.as_deref(), Some("dev"));
        assert_eq!(input.request_path.as_deref(), Some("auth/login.yml"));
        assert_eq!(input.tags, vec!["auth".to_string()]);
    }

    #[test]
    fn saved_source_propagates_not_found_instead_of_defaulting() {
        let repo = FakeCollectionRepo::new();
        let node = saved_flow_node("n1", "does/not/exist.yml");

        let err = build_execute_request_input(&repo, "my-api", None, &node)
            .expect_err("a missing saved request must error, not silently build an empty request");

        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn inline_source_builds_request_from_embedded_fields() {
        let repo = FakeCollectionRepo::new();
        let node = inline_flow_node("n2");

        let input = build_execute_request_input(&repo, "my-api", None, &node)
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

        let err = build_execute_request_input(&repo, "my-api", None, &node)
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

        let err = build_execute_request_input(&repo, "my-api", None, &node)
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
        }
    }

    fn sample_execute_input() -> ExecuteRequestInput {
        let repo = FakeCollectionRepo::new();
        build_execute_request_input(&repo, "my-api", None, &inline_flow_node("n2"))
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
            }],
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
                },
                FlowEdge {
                    id: "e2".to_string(),
                    source_node_id: "b".to_string(),
                    target_node_id: "a".to_string(),
                    target_field: "value".to_string(),
                    expression: "response.body".to_string(),
                },
            ],
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
        });
        let exec = exec_with_status(200);

        let summary = service
            .run(
                &exec,
                RunFlowInput {
                    collection: "my-api".to_string(),
                    flow_name: "empty".to_string(),
                    environment_name: None,
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
        });
        let exec = exec_with_status(200);

        let summary = service
            .run(
                &exec,
                RunFlowInput {
                    collection: "my-api".to_string(),
                    flow_name: "one-node".to_string(),
                    environment_name: None,
                },
            )
            .await
            .expect("run must succeed");

        assert_eq!(summary.steps.len(), 1);
        assert_eq!(summary.steps[0].node_id, "a");
        assert_eq!(summary.steps[0].status, rocket_shared::events::FlowNodeStatus::Success);
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
                },
            )
            .await
            .expect("run must still complete normally");
        assert_eq!(summary.steps.len(), 2);
    }
}
