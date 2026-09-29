//! Repeat until: sends one Request node's request until its condition holds.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use rocket_flow::{handle, RepeatUntil};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{FlowDebugRequest, FlowLogEntry};

use crate::execution_service::{ExecuteRequestInput, RequestExecutionService};
use crate::flow_debug::build_debug_request;
use crate::flow_execution_service::{
    to_flow_logs, CapturedOutput, ExecutedNode, FlowCoercion, FlowExecutionService, NodeRunContext,
    RunFlowInput,
};

/// How a successful repeat-until poll went, for its step result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PollStats {
    pub(crate) attempts: u32,
    /// Time from the first send to the attempt that met the condition.
    pub(crate) elapsed_ms: u64,
}

impl FlowExecutionService {
    /// Sends `request_input` until `repeat.condition` is truthy. Each attempt
    /// runs the request's scripts. Only the attempt that ends the poll is
    /// saved to History. A send error or a condition script error fails at
    /// once; giving up fails with "condition not met after …".
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_repeat_until(
        &self,
        exec: &RequestExecutionService,
        input: &RunFlowInput,
        request_input: ExecuteRequestInput,
        repeat: &RepeatUntil,
        external_secrets: &HashMap<String, String>,
        secret_values: &HashSet<String>,
        debug_on: bool,
        logs: &mut Vec<FlowLogEntry>,
        debug: &mut Option<FlowDebugRequest>,
        ctx: &mut NodeRunContext,
    ) -> DomainResult<ExecutedNode> {
        let started = tokio::time::Instant::now();
        let deadline = started + Duration::from_millis(repeat.timeout_ms);
        let interval = Duration::from_millis(repeat.interval_ms);
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            self.publish_progress(
                ctx,
                Some(attempt),
                Some(repeat.max_attempts),
                format!("attempt {attempt}/{}", repeat.max_attempts),
            );

            let mut attempt_input = request_input.clone();
            attempt_input.skip_history = true;
            let mut sent = None;
            let result = exec
                .execute_capturing(attempt_input, external_secrets, &mut sent)
                .await;
            if debug_on {
                if let Some(sent) = &sent {
                    let error = result.as_ref().err().map(|e| e.to_string());
                    *debug = Some(build_debug_request(
                        sent,
                        result.as_ref().ok().map(|o| &o.response),
                        error.as_deref(),
                        secret_values,
                    ));
                }
            }
            // A send that got no response is not retried.
            let mut output = result?;
            logs.extend(to_flow_logs(output.console_entries.clone()));
            let history = output.deferred_history.take();
            let captured = CapturedOutput::Request(Box::new(output));

            let outcome = exec
                .evaluate_flow_route_expression(
                    &input.collection,
                    &captured,
                    &repeat.condition,
                    FlowCoercion::Bool,
                    secret_values,
                )
                .await;
            logs.extend(outcome.logs);
            let verdict = outcome.result.and_then(|raw| match raw.as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                other => Err(DomainError::InvalidInput(format!(
                    "repeat-until condition evaluated to '{other}', expected true or false"
                ))),
            });

            let now = tokio::time::Instant::now();
            let elapsed = now - started;
            let gives_up = attempt >= repeat.max_attempts || now >= deadline;
            let ends_poll = !matches!(verdict, Ok(false)) || gives_up;
            if ends_poll {
                if let Some(entry) = &history {
                    exec.save_deferred_history(entry);
                }
            }
            match verdict {
                Err(e) => return Err(e),
                Ok(true) => {
                    return Ok(ExecutedNode {
                        output: captured,
                        chosen_exit: handle::RESULT.to_string(),
                        poll: Some(PollStats {
                            attempts: attempt,
                            elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
                        }),
                    })
                }
                Ok(false) if gives_up => {
                    return Err(DomainError::InvalidInput(format!(
                        "condition not met after {attempt} attempts ({:.1}s)",
                        elapsed.as_secs_f64()
                    )))
                }
                Ok(false) => {}
            }

            // The pause is cut short so the last attempt lands on the deadline.
            let pause = interval.min(deadline - now);
            if ctx.cancel.sleep(pause).await.is_err() {
                // Stop ends the poll, so History keeps this attempt.
                if let Some(entry) = &history {
                    exec.save_deferred_history(entry);
                }
                return Err(DomainError::Internal("cancelled".to_string()));
            }
        }
    }
}
