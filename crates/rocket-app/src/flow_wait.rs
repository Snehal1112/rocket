//! Execution of a Wait for callback node: turning an accepted call into
//! the node's output, and waiting for that call.

use std::collections::HashSet;
use std::time::Duration;

use rocket_flow::FlowNode;
use rocket_http::HttpResponse;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::FlowLogEntry;
use rocket_shared::types::Header;
use tokio::time::{interval, sleep_until, Instant};

use crate::callback_listener::ReceivedCall;
use crate::execution_service::{ExecuteRequestOutput, RequestExecutionService};
use crate::flow_callbacks::RunCallbacks;
use crate::flow_execution_service::{
    CapturedOutput, ExecutedNode, FlowExecutionService, NodeRunContext, RunFlowInput,
};

/// The node's output for an accepted call, shaped like a response so a
/// downstream wire reads it as `response.body` / `response.headers`.
/// `status_text` carries the call's method (for example `POST`), so the
/// step and `response.statusText` can say what was received.
pub(crate) fn callback_output(call: &ReceivedCall, duration_ms: u64) -> ExecuteRequestOutput {
    ExecuteRequestOutput {
        response: HttpResponse {
            status: 200,
            status_text: call.method.clone(),
            headers: call
                .headers
                .iter()
                .map(|(key, value)| Header {
                    key: key.clone(),
                    value: value.clone(),
                    enabled: true,
                    description: None,
                })
                .collect(),
            body: call.body.clone(),
            duration_ms,
            ttfb_ms: duration_ms,
            size_bytes: call.body.len(),
        },
        test_results: Vec::new(),
        console_entries: Vec::new(),
        script_error: None,
        deferred_history: None,
    }
}

impl FlowExecutionService {
    /// Waits for the first call to this node's endpoint that passes
    /// `accept_when`. Fails on timeout, on an `accept_when` script error,
    /// or when the run is cancelled.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn wait_for_callback(
        &self,
        exec: &RequestExecutionService,
        input: &RunFlowInput,
        node: &FlowNode,
        timeout_ms: u64,
        accept_when: Option<&str>,
        secret_values: &HashSet<String>,
        logs: &mut Vec<FlowLogEntry>,
        ctx: &mut NodeRunContext,
        callbacks: &mut RunCallbacks,
    ) -> DomainResult<ExecutedNode> {
        let endpoint = callbacks.endpoint_mut(&node.id).ok_or_else(|| {
            DomainError::Internal(format!("no callback endpoint for node '{}'", node.id))
        })?;
        let started = Instant::now();
        let deadline = started + Duration::from_millis(timeout_ms);
        // The first tick fires at once, so progress shows as soon as the node waits.
        let mut ticker = interval(Duration::from_secs(1));
        let mut ignored: u32 = 0;

        loop {
            tokio::select! {
                biased;
                _ = ctx.cancel.cancelled() => {
                    return Err(DomainError::Internal("cancelled".into()));
                }
                _ = sleep_until(deadline) => {
                    let secs = timeout_ms as f64 / 1000.0;
                    return Err(DomainError::InvalidInput(format!(
                        "no matching callback within {secs}s ({ignored} ignored)"
                    )));
                }
                call = endpoint.calls.recv() => {
                    let Some(call) = call else {
                        return Err(DomainError::Internal(format!(
                            "the callback endpoint of node '{}' closed",
                            node.id
                        )));
                    };
                    if let Some(source) = accept_when {
                        let outcome = exec
                            .evaluate_flow_callback_condition(
                                &input.collection,
                                &call,
                                source,
                                secret_values,
                            )
                            .await;
                        logs.extend(outcome.logs);
                        match outcome.result?.as_str() {
                            "true" => {}
                            "false" => {
                                ignored += 1;
                                continue;
                            }
                            other => {
                                return Err(DomainError::InvalidInput(format!(
                                    "accept_when of node '{}' evaluated to '{other}', expected true or false",
                                    node.id
                                )))
                            }
                        }
                    }
                    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                    return Ok(ExecutedNode::plain(CapturedOutput::Request(Box::new(
                        callback_output(&call, duration_ms),
                    ))));
                }
                _ = ticker.tick() => {
                    let left = deadline.saturating_duration_since(Instant::now()).as_secs();
                    self.publish_progress(
                        ctx,
                        None,
                        None,
                        format!("waiting… {left}s left · {ignored} ignored call(s)"),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::callback_listener::ReceivedCall;

    #[test]
    fn callback_output_turns_a_call_into_a_200_response() {
        let call = ReceivedCall {
            method: "PUT".to_string(),
            path: "/cb/abc".to_string(),
            query: Vec::new(),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: r#"{"orderId":42}"#.to_string(),
        };

        let out = callback_output(&call, 3100);

        assert_eq!(out.response.status, 200);
        assert_eq!(out.response.status_text, "PUT", "the method is reported here");
        assert_eq!(out.response.body, r#"{"orderId":42}"#);
        assert_eq!(out.response.duration_ms, 3100);
        assert_eq!(out.response.size_bytes, 14);
        assert_eq!(out.response.headers.len(), 1);
        assert_eq!(out.response.headers[0].key, "content-type");
        assert_eq!(out.response.headers[0].value, "application/json");
        assert!(out.deferred_history.is_none());
        assert!(out.script_error.is_none());
    }
}
