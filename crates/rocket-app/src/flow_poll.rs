//! Repeat until: sends one Request node's request until its condition holds.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use rocket_flow::{handle, RepeatUntil};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{FlowDebugRequest, FlowLiveProgress, FlowLogEntry, FlowPollDetail};

use crate::execution_service::{ExecuteRequestInput, RequestExecutionService};
use crate::flow_debug::{build_debug_request, cap_exchange, sent_masks};
use crate::flow_execution_service::{
    to_flow_logs, CapturedOutput, ExecutedNode, FlowCoercion, FlowExecutionService, NodeRunContext,
    RunFlowInput,
};
use crate::flow_trace::NodeTrace;

/// How a successful repeat-until poll went, for its step result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PollStats {
    pub(crate) attempts: u32,
    /// Time from the first send to the attempt that met the condition.
    pub(crate) elapsed_ms: u64,
}

/// How a failed repeat-until poll went. It is set only once an attempt got a
/// response, so the failed step can still show that response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FailedPollStats {
    pub(crate) attempts: u32,
    /// Time from the first send to the end of the poll.
    pub(crate) elapsed_ms: u64,
    /// Status code of the last response.
    pub(crate) status_code: u16,
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Updates the poll record's elapsed time, when the trace has one.
fn note_elapsed(trace: &mut NodeTrace, elapsed_ms: u64) {
    if let Some(poll) = trace.step.poll.as_mut() {
        poll.elapsed_ms = elapsed_ms;
    }
}

impl FlowExecutionService {
    /// Sends `request_input` until `repeat.condition` is truthy. Each attempt
    /// runs the request's scripts. Only the attempt that ends the poll is
    /// saved to History. A send error with no response or a condition script
    /// error fails at once; giving up fails with "condition not met after …".
    /// Every failure after a response sets `poll_stats`.
    /// `trace.poll` holds the same, plus the last verdict, on every path.
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
        exchange: &mut Option<FlowDebugRequest>,
        poll_stats: &mut Option<FailedPollStats>,
        trace: &mut NodeTrace,
        ctx: &mut NodeRunContext,
    ) -> DomainResult<ExecutedNode> {
        let started = tokio::time::Instant::now();
        let deadline = started + Duration::from_millis(repeat.timeout_ms);
        let interval = Duration::from_millis(repeat.interval_ms);
        let mut attempt: u32 = 0;
        // The previous attempt's History entry, not yet saved.
        let mut pending_history = None;
        trace.step.poll = Some(FlowPollDetail {
            attempts: 0,
            max_attempts: repeat.max_attempts,
            last_status_code: None,
            condition_met: None,
            elapsed_ms: 0,
            timeout_ms: repeat.timeout_ms,
        });
        loop {
            // Stop can land after the pause ended. No new request is sent then.
            if attempt > 0 && ctx.cancel.is_cancelled() {
                if let Some(entry) = &pending_history {
                    exec.save_deferred_history(entry);
                }
                if let Some(stats) = poll_stats.as_mut() {
                    stats.elapsed_ms = millis(started.elapsed());
                }
                note_elapsed(trace, millis(started.elapsed()));
                return Err(DomainError::Internal("cancelled".to_string()));
            }
            attempt += 1;
            if let Some(poll) = trace.step.poll.as_mut() {
                poll.attempts = attempt;
            }
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
            // Each attempt overwrites the record, so the step keeps the last one.
            if let Some(sent) = &sent {
                let error = result.as_ref().err().map(|e| e.to_string());
                let record = build_debug_request(
                    sent,
                    result.as_ref().ok().map(|o| &o.response),
                    error.as_deref(),
                    &sent_masks(
                        secret_values,
                        result.as_ref().ok().map(|o| &o.run_secret_values),
                    ),
                );
                if debug_on {
                    *debug = Some(record.clone());
                }
                *exchange = Some(cap_exchange(record));
            }
            // A send that got no response is not retried. A stat set by an
            // earlier attempt stays, with this attempt counted.
            let mut output = match result {
                Ok(output) => output,
                Err(e) => {
                    if let Some(stats) = poll_stats.as_mut() {
                        stats.attempts = attempt;
                        stats.elapsed_ms = millis(started.elapsed());
                    }
                    note_elapsed(trace, millis(started.elapsed()));
                    return Err(e);
                }
            };
            let status_code = output.response.status;
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
            *poll_stats = Some(FailedPollStats {
                attempts: attempt,
                elapsed_ms: millis(elapsed),
                status_code,
            });
            if let Some(poll) = trace.step.poll.as_mut() {
                poll.last_status_code = Some(status_code);
                poll.condition_met = verdict.as_ref().ok().copied();
                poll.elapsed_ms = millis(elapsed);
            }
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
                            elapsed_ms: millis(elapsed),
                        }),
                        reported_value: None,
                        sent_secret: None,
                    })
                }
                Ok(false) if gives_up => {
                    return Err(DomainError::InvalidInput(format!(
                        "condition not met after {attempt} attempts ({:.1}s)",
                        elapsed.as_secs_f64()
                    )))
                }
                Ok(false) => {
                    // Says why the poll goes on, before it pauses.
                    self.publish_live_progress(
                        ctx,
                        Some(attempt),
                        Some(repeat.max_attempts),
                        format!(
                            "attempt {attempt}/{} · condition false",
                            repeat.max_attempts
                        ),
                        FlowLiveProgress {
                            last_status_code: Some(status_code),
                            condition_met: Some(false),
                            elapsed_ms: Some(millis(elapsed)),
                            remaining_ms: Some(millis(deadline.saturating_duration_since(now))),
                            ..Default::default()
                        },
                    );
                }
            }

            // The pause is cut short so the last attempt lands on the deadline.
            let pause = interval.min(deadline - now);
            if ctx.cancel.sleep(pause).await.is_err() {
                // Stop ends the poll, so History keeps this attempt.
                if let Some(entry) = &history {
                    exec.save_deferred_history(entry);
                }
                if let Some(stats) = poll_stats.as_mut() {
                    stats.elapsed_ms = millis(started.elapsed());
                }
                note_elapsed(trace, millis(started.elapsed()));
                return Err(DomainError::Internal("cancelled".to_string()));
            }
            pending_history = history;
        }
    }
}
