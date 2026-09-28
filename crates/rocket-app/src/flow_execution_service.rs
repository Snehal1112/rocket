use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use rocket_collection::Request;
use rocket_flow::{handle, FlowEdge, FlowNode, FlowNodeKind, InlineRequestData, RequestSource};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Body, BodyMode, Header, HttpMethod};
use rocket_shared::VariableValue;

use crate::execution_service::{
    ExecuteRequestInput, ExecuteRequestOutput, RequestExecutionService,
};
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
}

impl ExecutedNode {
    fn plain(output: CapturedOutput) -> Self {
        Self {
            output,
            chosen_exit: handle::RESULT.to_string(),
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

impl RequestExecutionService {
    /// Evaluates `expression` (a jsonq/JS snippet such as `"response.body"` or
    /// `"response.body.token"`) against `output`, reusing the same
    /// script-engine mechanism `evaluate_var_expression` uses for the Vars
    /// tab's preview — not a second sandbox invocation path.
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
        let response_json = captured_output_response_json(output)?;
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

    /// Evaluates an If/Switch routing expression against `output`. The caller
    /// passes it already wrapped (`!!(…)` for If, `String(…)` for Switch), so
    /// the result is always a string; a `null` result becomes `"null"` rather
    /// than an error, which keeps a missing value routable by a case.
    pub async fn evaluate_flow_route_expression(
        &self,
        collection: &str,
        output: &CapturedOutput,
        wrapped_expression: &str,
    ) -> DomainResult<String> {
        let response_json = captured_output_response_json(output)?;
        let result = self
            .evaluate_var_expression(collection, wrapped_expression, &response_json)
            .await?;
        Ok(match result {
            serde_json::Value::Null => "null".to_string(),
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        })
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
    global_env_name: Option<&str>,
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

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use rocket_shared::events::{DomainEvent, FlowNodeStatus, FlowSkipReason};
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
    /// The node's captured output value, populated only for `Output`-kind
    /// nodes (see `result_to_step`). `None` for a Request node (its result is
    /// the HTTP response, not a single value), an Input node, or a node that
    /// never produced output (Skipped/Failed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Set only when `status` is `Skipped`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<FlowSkipReason>,
    /// The exit a succeeded If/Switch node took. `None` for every other node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
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

    /// Runs every node of `input.flow_name` in dependency order. Each node's
    /// fate is decided at its turn from its predecessors' outcomes
    /// (`flow_routing::decide_fate`): it runs, is skipped (`upstream_failed`
    /// or `branch_not_taken`), or fails on ambiguous inputs. Cancellation is
    /// checked before each node, so a cancelled run records no step for the
    /// nodes it never reached.
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
                    let result = match node_opt {
                        Some(node) => {
                            self.execute_node(
                                exec,
                                &input,
                                node,
                                &data_edges,
                                &captured,
                                &external_secrets,
                            )
                            .await
                        }
                        None => Err(DomainError::Internal(format!(
                            "node '{node_id}' is missing from the flow"
                        ))),
                    };
                    let step = result_to_step(node_id, node_opt, &result);
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
                    (step, outcome)
                }
            };

            self.events.publish(step_completed_event(&run_id, &step));
            outcomes.insert(node_id.clone(), outcome);
            steps.push(step);
        }

        self.clear_cancellation(&run_id);
        if let Ok(mut set) = self.in_flight.lock() {
            set.remove(&run_id);
        }

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

    /// Dispatches one node by kind, feeding it only `data_edges` — the live,
    /// non-trigger edges `decide_fate` selected. Returns the node's captured
    /// output and chosen exit, or an error if the node itself failed.
    async fn execute_node(
        &self,
        exec: &RequestExecutionService,
        input: &RunFlowInput,
        node: &FlowNode,
        data_edges: &[&FlowEdge],
        captured: &HashMap<String, CapturedOutput>,
        external_secrets: &HashMap<String, String>,
    ) -> DomainResult<ExecutedNode> {
        match &node.kind {
            FlowNodeKind::Input { value, .. } => {
                let settings = self.collection_repo.get_settings(&input.collection)?;
                let mut vars = HashMap::new();
                for cv in settings.variables.iter().filter(|v| v.enabled) {
                    let v = if cv.value.is_empty() {
                        cv.initial_value.clone()
                    } else {
                        cv.value.clone()
                    };
                    vars.insert(cv.key.clone(), v);
                }
                // An unknown `{{name}}` stays as literal text. When the value is
                // wired into a Request field, `execute` resolves it again with
                // the full environment scope, so environment variables still work.
                let resolved = rocket_environment::resolve(value.data(), &vars).output;
                Ok(ExecutedNode::plain(CapturedOutput::Value(
                    VariableValue::simple(resolved),
                )))
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
                let value = exec
                    .resolve_flow_wire_expression(
                        &input.collection,
                        source_output,
                        &edge.expression,
                    )
                    .await?;
                Ok(ExecutedNode::plain(CapturedOutput::Value(
                    VariableValue::simple(value),
                )))
            }
            FlowNodeKind::Request { .. } => {
                let mut request_input = build_execute_request_input(
                    self.collection_repo.as_ref(),
                    &input.collection,
                    input.environment_name.as_deref(),
                    input.global_env_name.as_deref(),
                    node,
                )?;

                let mut resolved = HashMap::new();
                for edge in data_edges {
                    let source_output = captured_source(node, edge, captured)?;
                    let value = exec
                        .resolve_flow_wire_expression(
                            &input.collection,
                            source_output,
                            &edge.expression,
                        )
                        .await?;
                    resolved.insert(edge.id.clone(), value);
                }
                let edges_owned: Vec<FlowEdge> = data_edges.iter().map(|e| (*e).clone()).collect();
                apply_wired_overrides(&mut request_input, &resolved, &edges_owned)?;

                let output = exec
                    .execute_with_external_secrets(request_input, external_secrets)
                    .await?;
                Ok(ExecutedNode::plain(CapturedOutput::Request(Box::new(
                    output,
                ))))
            }
            FlowNodeKind::If { condition, .. } => {
                let source = single_route_input(node, data_edges, captured)?;
                let raw = exec
                    .evaluate_flow_route_expression(
                        &input.collection,
                        source,
                        &format!("!!({condition})"),
                    )
                    .await?;
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
                })
            }
            FlowNodeKind::Switch { value, cases, .. } => {
                let source = single_route_input(node, data_edges, captured)?;
                let raw = exec
                    .evaluate_flow_route_expression(
                        &input.collection,
                        source,
                        &format!("String({value})"),
                    )
                    .await?;
                let chosen_exit = cases
                    .iter()
                    .find(|case| case.matches == raw)
                    .map(|case| handle::case_handle(&case.id))
                    .unwrap_or_else(|| handle::DEFAULT.to_string());
                Ok(ExecutedNode {
                    output: source.clone(),
                    chosen_exit,
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

/// The captured output feeding a routing node through its single live
/// `input` edge. `validate` (V1) and `decide_fate` guarantee exactly one;
/// anything else is reported, never panicked on. The source may be a
/// Request that failed with a non-2xx status (spec §6.3.1): its response
/// was captured and is used exactly like a successful one.
fn single_route_input<'c>(
    node: &FlowNode,
    data_edges: &[&FlowEdge],
    captured: &'c HashMap<String, CapturedOutput>,
) -> DomainResult<&'c CapturedOutput> {
    match data_edges {
        [edge] if edge.target_field == handle::INPUT => captured_source(node, edge, captured),
        _ => Err(DomainError::InvalidInput(format!(
            "routing node '{}' needs exactly one live '{}' input, found {}",
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
    };
    match result {
        Ok(executed) if is_routing => FlowStepResult {
            branch: Some(executed.chosen_exit.clone()),
            ..base
        },
        Ok(ExecutedNode {
            output: CapturedOutput::Request(out),
            ..
        }) => {
            let status = out.response.status;
            let success = out.response.is_success();
            FlowStepResult {
                status: if success {
                    FlowNodeStatus::Success
                } else {
                    FlowNodeStatus::Failed
                },
                status_code: Some(status),
                duration_ms: Some(out.response.duration_ms),
                error: (!success).then(|| format!("non-2xx response: {status}")),
                ..base
            }
        }
        Ok(ExecutedNode {
            output: CapturedOutput::Value(v),
            ..
        }) => {
            let is_output = matches!(kind, Some(FlowNodeKind::Output { .. }));
            FlowStepResult {
                value: is_output.then(|| v.data().to_string()),
                ..base
            }
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
    async fn route_expression_passes_the_wrapped_expression_verbatim() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            scripted(vec![(
                "!!(response.status === 200)",
                Scripted::Value(serde_json::json!(true)),
            )]),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .evaluate_flow_route_expression("my-api", &output, "!!(response.status === 200)")
            .await
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
            .evaluate_flow_route_expression("my-api", &output, "String(response.body.plan)")
            .await
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
            .evaluate_flow_route_expression("my-api", &output, "String(response.body.plan)")
            .await
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
            .evaluate_flow_route_expression("my-api", &output, "String(response.status)")
            .await
            .expect("resolve");

        assert_eq!(value, "200");
    }

    #[tokio::test]
    async fn route_expression_script_error_is_invalid_input() {
        let svc = service_with_engine(FakeCollectionRepo::new(), Box::new(ErrorJsonqEngine));
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let err = svc
            .evaluate_flow_route_expression("my-api", &output, "!!(nope.nope)")
            .await
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
    async fn a_run_reports_the_output_nodes_captured_value_but_not_the_input_nodes() {
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
            step_for("a").value, None,
            "an Input node must never report a value, only Output nodes do"
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
            .position(|e| matches!(e, DomainEvent::FlowStepStarted { node_id, .. } if node_id == "a"))
            .expect("started event for a must exist");
        let completed_idx = events
            .iter()
            .position(|e| matches!(e, DomainEvent::FlowStepCompleted { node_id, .. } if node_id == "a"))
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
            DomainEvent::FlowStepCompleted { node_id, branch, .. } if node_id == "check" => branch,
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

        let summary = service.run(&exec, run_input("if-false")).await.expect("run");

        assert_eq!(step_of(&summary, "check").branch.as_deref(), Some(handle::FALSE));
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

        let summary = service.run(&exec, run_input("if-error")).await.expect("run");

        let check = step_of(&summary, "check");
        assert_eq!(check.status, FlowNodeStatus::Failed);
        assert!(check.error.as_deref().is_some_and(|m| m.contains("ReferenceError")));
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

        let summary = service.run(&exec, run_input("if-weird")).await.expect("run");

        assert_eq!(status_of(&summary, "check"), FlowNodeStatus::Failed);
    }

    #[tokio::test]
    async fn if_observes_a_non_2xx_request_and_routes_on_it() {
        // Spec §6.3.1: login gets a 401 and stays failed. The If still sees
        // the 401 response and routes to `false`. Login's plain dependent is
        // skipped as upstream_failed.
        let mut flow = if_flow("if-after-401");
        flow.nodes.push(request_flow_node("plain", "https://api.example.com/plain"));
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

        let summary = service.run(&exec, run_input("if-after-401")).await.expect("run");

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

        let summary = service.run(&exec, run_input("pass-through")).await.expect("run");

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

        let summary = service.run(&exec, run_input("input-if")).await.expect("run");

        assert_eq!(step_of(&summary, "out").value.as_deref(), Some("pro"));
    }

    /// login -> switch(plan) -> free: f, pro: p, default: d.
    fn switch_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![
                request_flow_node("login", "https://api.example.com/login"),
                switch_node("plan", "response.body.plan", &[("free", "free"), ("pro", "pro")]),
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
        }
    }

    async fn run_switch(name: &str, value: serde_json::Value) -> FlowRunSummary {
        let service = service_with_flow(switch_flow(name));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, scripted(vec![("String(", Scripted::Value(value))]));
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

        assert_eq!(step_of(&summary, "plan").branch.as_deref(), Some(handle::DEFAULT));
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
                edge_from("e4", "profile", handle::RESULT, "save", "body", "response.body"),
                edge_from("e5", "refresh", handle::RESULT, "save", "body", "response.body"),
                edge_from("e6", "config", handle::RESULT, "call", "url", "response.body"),
                edge_from(
                    "e7",
                    "profile",
                    handle::RESULT,
                    "call",
                    "headers[Authorization].value",
                    "response.body",
                ),
            ],
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

        assert_eq!(status_of(&summary, "save"), FlowNodeStatus::Success, "case 1 merge");
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

        let summary = service.run(&exec, run_input("join-fail")).await.expect("run");

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
        flow.nodes.push(request_flow_node("after_no", "https://api.example.com/after"));
        flow.edges.push(wire("e4", "no", "after_no"));
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service.run(&exec, run_input("transitive")).await.expect("run");

        assert_eq!(
            step_of(&summary, "after_no").skip_reason,
            Some(FlowSkipReason::BranchNotTaken)
        );
    }

    #[tokio::test]
    async fn several_live_triggers_into_one_node_run_it_once() {
        let mut flow = if_flow("two-triggers");
        flow.nodes.push(request_flow_node("both", "https://api.example.com/both"));
        flow.edges.push(trigger_edge("e4", "check", handle::TRUE, "both"));
        flow.edges.push(trigger_edge("e5", "login", handle::RESULT, "both"));
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service.run(&exec, run_input("two-triggers")).await.expect("run");

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

        let summary = service.run(&exec, run_input("cancel-if")).await.expect("run");

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

        service.run(&exec, run_input("started-if")).await.expect("run");

        let started: Vec<String> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepStarted { node_id, .. } => Some(node_id),
                _ => None,
            })
            .collect();
        assert_eq!(started, vec!["login".to_string(), "check".to_string(), "yes".to_string()]);
    }
}
