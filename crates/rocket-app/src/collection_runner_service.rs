//! Collection Runner — runs a folder's or collection's requests in sequence and
//! honours the `rok.runner.*` scripting API.
//!
//! The runner is an orchestration layer, not a second execution engine: it
//! drives the same phase methods on `RequestExecutionService` that a single
//! send does, one phase at a time, so it can act on `skip_request` before the
//! send and on `next_request` after every phase that ran (spec §4).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use rocket_collection::CollectionRepository;
use rocket_scripting::{ExecutionMode, NextRequest};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::execution_service::{ExecuteRequestInput, RequestExecutionService};
use crate::runner_sequence::{build_step_input, flatten_run_set, RunItem};

/// Hard cap on executed steps in one run. `setNextRequest` can form a cycle
/// (A → B → A); without a cap the run would never end.
const MAX_RUN_STEPS: usize = 1_000;

/// Input for one run. IPC DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCollectionInput {
    pub collection: String,
    /// Folder to run, relative to the collection root, using on-disk directory
    /// names. `None` or `""` runs the whole collection.
    #[serde(default)]
    pub folder_path: Option<String>,
    #[serde(default)]
    pub environment_name: Option<String>,
    #[serde(default)]
    pub global_env_name: Option<String>,
    /// Stop at the first failed step. Defaults to `false` — Bruno's `--bail` is
    /// opt-in too (spec §8.3).
    #[serde(default)]
    pub stop_on_failure: bool,
    /// Opt-in per-workspace SSRF guard policy, applied to every step's
    /// BeforeRequest mutations the same way a single send applies it
    /// (Item 6's request-mutation host guard). Defaults to fully permissive,
    /// same as `ExecuteRequestInput::request_guard_policy`.
    #[serde(default)]
    pub request_guard_policy: rocket_workspace::RequestGuardPolicy,
}

/// Outcome of a single step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunStepStatus {
    /// The request was sent and every phase that applies ran.
    Completed,
    /// A before-request script called `rok.runner.skipRequest()`.
    Skipped,
    /// The request could not be dispatched, or the run could not continue past it.
    Error,
}

impl RunStepStatus {
    /// Wire string used in `DomainEvent::RunnerStepCompleted.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            RunStepStatus::Completed => "completed",
            RunStepStatus::Skipped => "skipped",
            RunStepStatus::Error => "error",
        }
    }
}

/// One row of the run summary. IPC DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStepResult {
    /// Position in the executed-step stream, starting at 0.
    pub index: usize,
    pub item_name: String,
    pub request_path: String,
    pub status: RunStepStatus,
    /// `None` for a skipped or errored step.
    pub status_code: Option<u16>,
    pub duration_ms: u64,
    pub test_pass_count: usize,
    pub test_fail_count: usize,
    /// Uncaught script exception, if any.
    pub script_error: Option<String>,
    /// Transport or sequencing error, if any.
    pub error: Option<String>,
}

impl RunStepResult {
    /// A step counts as failed when it errored, returned a non-2xx status, or
    /// had at least one failing test (spec §7). A skipped step never fails.
    pub fn is_failure(&self) -> bool {
        match self.status {
            RunStepStatus::Error => true,
            RunStepStatus::Skipped => false,
            RunStepStatus::Completed => {
                self.test_fail_count > 0
                    || self.status_code.map(|s| !(200..300).contains(&s)).unwrap_or(true)
            }
        }
    }
}

/// Why a run ended. IPC DTO.
///
/// The container `rename_all` only renames variants — each struct variant
/// carries its own `rename_all` so its fields are camelCase on the wire too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StoppedReason {
    /// Every item in the run set ran.
    Completed,
    /// A script called `rok.runner.setNextRequest(null)`.
    StoppedByScript,
    /// `stop_on_failure` was set and this step failed.
    #[serde(rename_all = "camelCase")]
    StoppedOnFailure { item_name: String },
    /// `rok.runner.setNextRequest(name)` named an item that is not in the run set.
    #[serde(rename_all = "camelCase")]
    UnknownNextRequest { item_name: String, next_request: String },
    /// `stop_collection_run` was called for this run.
    Cancelled,
    /// `MAX_RUN_STEPS` executed steps were reached — almost certainly a
    /// `setNextRequest` cycle.
    #[serde(rename_all = "camelCase")]
    StepLimitReached { limit: usize },
}

impl StoppedReason {
    /// Wire string used in `DomainEvent::RunnerFinished.stopped_reason`.
    pub fn as_str(&self) -> &'static str {
        match self {
            StoppedReason::Completed => "completed",
            StoppedReason::StoppedByScript => "stoppedByScript",
            StoppedReason::StoppedOnFailure { .. } => "stoppedOnFailure",
            StoppedReason::UnknownNextRequest { .. } => "unknownNextRequest",
            StoppedReason::Cancelled => "cancelled",
            StoppedReason::StepLimitReached { .. } => "stepLimitReached",
        }
    }
}

/// Full result of one run. IPC DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub run_id: String,
    pub collection: String,
    pub folder_path: Option<String>,
    pub steps: Vec<RunStepResult>,
    pub stopped_reason: StoppedReason,
}

/// What one step reported back to the run loop.
struct StepOutcome {
    result: RunStepResult,
    /// Last `next_request` any phase of this step set.
    next_request: Option<NextRequest>,
}

/// Runs a folder's or collection's requests in sequence.
///
/// Holds no execution machinery of its own — `run()` takes the
/// `RequestExecutionService` to drive, the same way `LoadTestService::run`
/// does, so both are constructed independently in the DI layer.
pub struct CollectionRunnerService {
    collection_repo: Box<dyn CollectionRepository>,
    events: Box<dyn EventPublisher>,
    /// Run ids that have been asked to stop. Shared behind an `Arc` so a test
    /// (and, later, any other holder) can flip a run to cancelled mid-flight.
    cancelled: Arc<Mutex<HashSet<String>>>,
    /// Run ids currently executing. `cancel()` only inserts into `cancelled`
    /// for an id present here, so a `stop_collection_run` call for an unknown
    /// or already-finished run id does not leak an entry into `cancelled`
    /// forever — a run only ever removes its own id, never anyone else's.
    in_flight: Arc<Mutex<HashSet<String>>>,
}

impl CollectionRunnerService {
    pub fn new(
        collection_repo: Box<dyn CollectionRepository>,
        events: Box<dyn EventPublisher>,
    ) -> Self {
        Self {
            collection_repo,
            events,
            cancelled: Arc::new(Mutex::new(HashSet::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Replaces the cancellation registry. Test seam — it lets a test hold the
    /// same registry the run loop reads and cancel a run while it is in flight.
    #[cfg(test)]
    pub(crate) fn with_cancellations(mut self, cancelled: Arc<Mutex<HashSet<String>>>) -> Self {
        self.cancelled = cancelled;
        self
    }

    /// Runs every request in the target folder or collection, in order.
    ///
    /// Returns once the run ends. Progress is also streamed as
    /// `RunnerStarted` / `RunnerStepCompleted` / `RunnerFinished` events.
    pub async fn run(
        &self,
        exec: &RequestExecutionService,
        input: RunCollectionInput,
    ) -> DomainResult<RunSummary> {
        let collection = self.collection_repo.get(&input.collection)?;
        let items = flatten_run_set(&collection, input.folder_path.as_deref())?;
        let run_id = Ulid::new().to_string();
        if let Ok(mut set) = self.in_flight.lock() {
            set.insert(run_id.clone());
        }

        self.events.publish(DomainEvent::RunnerStarted {
            run_id: run_id.clone(),
            collection: input.collection.clone(),
            folder_path: input.folder_path.clone(),
            total_steps: items.len(),
        });

        let mut steps: Vec<RunStepResult> = Vec::new();
        let mut carried_runtime: HashMap<String, String> = HashMap::new();
        let mut cursor = 0usize;
        let mut stopped_reason = StoppedReason::Completed;

        while cursor < items.len() {
            if self.is_cancelled(&run_id) {
                stopped_reason = StoppedReason::Cancelled;
                break;
            }
            if steps.len() >= MAX_RUN_STEPS {
                stopped_reason = StoppedReason::StepLimitReached { limit: MAX_RUN_STEPS };
                break;
            }

            let item = &items[cursor];
            let outcome = self
                .run_step(exec, &input, item, steps.len(), &mut carried_runtime)
                .await;
            let mut result = outcome.result;

            // Resolve the jump before publishing so an unknown target is part
            // of the step the frontend sees (spec §4).
            let mut jump_to: Option<usize> = None;
            let mut stop_after = false;
            match outcome.next_request {
                Some(NextRequest::Stop) => {
                    stopped_reason = StoppedReason::StoppedByScript;
                    stop_after = true;
                }
                Some(NextRequest::Name(ref name)) => {
                    match items.iter().position(|i| &i.name == name) {
                        Some(idx) => jump_to = Some(idx),
                        None => {
                            result.status = RunStepStatus::Error;
                            result.error = Some(format!(
                                "rok.runner.setNextRequest('{name}') — no request named '{name}' in this run"
                            ));
                            stopped_reason = StoppedReason::UnknownNextRequest {
                                item_name: result.item_name.clone(),
                                next_request: name.clone(),
                            };
                            stop_after = true;
                        }
                    }
                }
                None => {}
            }

            let failed = result.is_failure();
            self.publish_step(&run_id, &result);
            let item_name = result.item_name.clone();
            steps.push(result);

            if stop_after {
                break;
            }
            if failed && input.stop_on_failure {
                stopped_reason = StoppedReason::StoppedOnFailure { item_name };
                break;
            }
            cursor = match jump_to {
                Some(idx) => idx,
                None => cursor + 1,
            };
        }

        self.clear_cancellation(&run_id);
        if let Ok(mut set) = self.in_flight.lock() {
            set.remove(&run_id);
        }

        let failed_count = steps.iter().filter(|s| s.is_failure()).count();
        self.events.publish(DomainEvent::RunnerFinished {
            run_id: run_id.clone(),
            stopped_reason: stopped_reason.as_str().to_string(),
            step_count: steps.len(),
            failed_count,
        });

        Ok(RunSummary {
            run_id,
            collection: input.collection,
            folder_path: input.folder_path,
            steps,
            stopped_reason,
        })
    }

    /// Asks an in-progress run to stop. The run ends before its next step; a
    /// step already in flight finishes first. Cancelling an unknown or finished
    /// run id is a no-op — it does not insert into `cancelled` at all, so a
    /// stale or mistyped run id can never leak an entry there forever.
    pub fn cancel(&self, run_id: &str) {
        let is_in_flight = self
            .in_flight
            .lock()
            .map(|set| set.contains(run_id))
            .unwrap_or(false);
        if !is_in_flight {
            return;
        }
        if let Ok(mut set) = self.cancelled.lock() {
            set.insert(run_id.to_string());
        }
    }

    /// Runs one step: before-request, then — unless the script skipped it —
    /// send, after-response, tests, and the shared finish work (events,
    /// history, assertions).
    async fn run_step(
        &self,
        exec: &RequestExecutionService,
        input: &RunCollectionInput,
        item: &RunItem,
        index: usize,
        carried_runtime: &mut HashMap<String, String>,
    ) -> StepOutcome {
        let step_input: ExecuteRequestInput = build_step_input(
            item,
            &input.collection,
            input.environment_name.as_deref(),
            input.global_env_name.as_deref(),
            input.request_guard_policy.clone(),
        );

        let mut state = match exec.begin_phases(&step_input) {
            Ok(state) => state,
            Err(e) => {
                return StepOutcome {
                    result: error_step(index, item, e.to_string()),
                    next_request: None,
                }
            }
        };
        // Runtime variables set by earlier steps stay readable (spec §8.2).
        state.seed_runtime(carried_runtime);

        // run_before_request_phase can fail — the SSRF guard (Item 6) returns
        // Err when a script's req.setUrl() mutation targets a blocked host.
        // Treat that exactly like a transport error: this step errors, the
        // run continues (unless stop_on_failure is set), it is not a crash.
        if let Err(e) = exec
            .run_before_request_phase(&step_input, ExecutionMode::Runner, &mut state)
            .await
        {
            *carried_runtime = state.var_ctx.runtime.clone();
            let mut result = error_step(index, item, e.to_string());
            result.script_error = state.script_error.clone();
            return StepOutcome {
                next_request: state.next_request.clone(),
                result,
            };
        }

        if state.skip_request {
            // Nothing was sent, so after-response and tests have no response to
            // run against — the step ends here (spec §4). `finish_phases` never
            // runs either, so publish this step's console output directly.
            exec.publish_console(&item.name, &state.console);
            *carried_runtime = state.var_ctx.runtime.clone();
            return StepOutcome {
                next_request: state.next_request.clone(),
                result: RunStepResult {
                    index,
                    item_name: item.name.clone(),
                    request_path: item.request_path.clone(),
                    status: RunStepStatus::Skipped,
                    status_code: None,
                    duration_ms: 0,
                    test_pass_count: 0,
                    test_fail_count: 0,
                    script_error: state.script_error.clone(),
                    error: None,
                },
            };
        }

        let response = match exec.send_request(&state).await {
            Ok(response) => response,
            Err(e) => {
                *carried_runtime = state.var_ctx.runtime.clone();
                let mut result = error_step(index, item, e.to_string());
                result.script_error = state.script_error.clone();
                return StepOutcome {
                    next_request: state.next_request.clone(),
                    result,
                };
            }
        };

        exec.run_after_response_phase(&step_input, ExecutionMode::Runner, &response, &mut state)
            .await;
        exec.run_tests_phase(&step_input, ExecutionMode::Runner, &response, &mut state)
            .await;
        let output = exec.finish_phases(&step_input, response, &mut state).await;

        *carried_runtime = state.var_ctx.runtime.clone();

        let passed = output
            .test_results
            .iter()
            .filter(|t| t.status == rocket_scripting::TestStatus::Passed)
            .count();
        let failed = output.test_results.len() - passed;

        StepOutcome {
            next_request: state.next_request.clone(),
            result: RunStepResult {
                index,
                item_name: item.name.clone(),
                request_path: item.request_path.clone(),
                status: RunStepStatus::Completed,
                status_code: Some(output.response.status),
                duration_ms: output.response.duration_ms,
                test_pass_count: passed,
                test_fail_count: failed,
                script_error: output.script_error.clone(),
                error: None,
            },
        }
    }

    fn publish_step(&self, run_id: &str, result: &RunStepResult) {
        self.events.publish(DomainEvent::RunnerStepCompleted {
            run_id: run_id.to_string(),
            index: result.index,
            item_name: result.item_name.clone(),
            request_path: result.request_path.clone(),
            status: result.status.as_str().to_string(),
            status_code: result.status_code,
            duration_ms: result.duration_ms,
            test_pass_count: result.test_pass_count,
            test_fail_count: result.test_fail_count,
            script_error: result.script_error.clone(),
            error: result.error.clone(),
        });
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
}

/// An errored step with no response and no tests.
fn error_step(index: usize, item: &RunItem, message: String) -> RunStepResult {
    RunStepResult {
        index,
        item_name: item.name.clone(),
        request_path: item.request_path.clone(),
        status: RunStepStatus::Error,
        status_code: None,
        duration_ms: 0,
        test_pass_count: 0,
        test_fail_count: 0,
        script_error: None,
        error: Some(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::{
        InMemoryCollectionRepo, InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo,
        ProgrammableEngine, RecordingExecutor, RecordingPublisher, SharedCollectionRepo,
        SharedEngine, SharedExecutor, SharedHistoryRepo, SharedPublisher,
    };
    use rocket_collection::{Collection, Request};
    use rocket_scripting::ScriptResult;
    use rocket_shared::types::HttpMethod;
    use std::sync::Arc;

    fn req(name: &str, file: &str) -> Request {
        let mut r = Request::new(name, HttpMethod::Get, format!("https://api.test/{file}"));
        r.file_name = Some(file.to_string());
        // Every phase has a script so the engine is always consulted.
        r.pre_request_script = Some("// pre".into());
        r.post_response_script = Some("// post".into());
        r.tests = Some("// tests".into());
        r
    }

    /// root: [First, Second, Third]
    fn three_step_collection() -> Collection {
        let mut collection = Collection::new("my-api");
        collection.root.add_request(req("First", "first.yml"));
        collection.root.add_request(req("Second", "second.yml"));
        collection.root.add_request(req("Third", "third.yml"));
        collection
    }

    fn sample_run_input() -> RunCollectionInput {
        RunCollectionInput {
            collection: "my-api".into(),
            folder_path: None,
            environment_name: None,
            global_env_name: None,
            stop_on_failure: false,
            request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
        }
    }

    struct Harness {
        runner: CollectionRunnerService,
        exec: RequestExecutionService,
        executor: Arc<RecordingExecutor>,
        engine: Arc<ProgrammableEngine>,
        history: Arc<InMemoryHistoryRepo>,
        publisher: Arc<RecordingPublisher>,
    }

    fn harness(collection: Collection, engine: Arc<ProgrammableEngine>, executor: Arc<RecordingExecutor>) -> Harness {
        let repo = InMemoryCollectionRepo::new(collection);
        let history = InMemoryHistoryRepo::new();
        let publisher = RecordingPublisher::new();

        let exec = RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(SharedExecutor(Arc::clone(&executor))),
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(NullCookieRepo),
            Box::new(rocket_shared::events::NullEventPublisher),
        )
        .with_script_engine(Box::new(SharedEngine(Arc::clone(&engine))));

        let runner = CollectionRunnerService::new(
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
        );

        Harness { runner, exec, executor, engine, history, publisher }
    }

    #[tokio::test]
    async fn runs_every_request_in_order() {
        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(
            h.executor.sent_urls(),
            vec![
                "https://api.test/first.yml".to_string(),
                "https://api.test/second.yml".to_string(),
                "https://api.test/third.yml".to_string(),
            ]
        );
        let names: Vec<&str> = summary.steps.iter().map(|s| s.item_name.as_str()).collect();
        assert_eq!(names, vec!["First", "Second", "Third"]);
        assert_eq!(summary.stopped_reason, StoppedReason::Completed);
        assert!(summary.steps.iter().all(|s| s.status == RunStepStatus::Completed));
    }

    #[tokio::test]
    async fn every_script_phase_reports_runner_execution_mode() {
        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        let modes = h.engine.modes();
        assert_eq!(modes.len(), 9, "3 requests x 3 phases");
        assert!(modes.iter().all(|m| m == "runner"), "got {modes:?}");
    }

    #[tokio::test]
    async fn each_step_still_lands_in_history() {
        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        h.runner.run(&h.exec, sample_run_input()).await.expect("run");
        assert_eq!(h.history.saved_count(), 3);
    }

    #[tokio::test]
    async fn runtime_variables_carry_forward_to_later_steps() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "after-response",
            ScriptResult {
                runtime_vars: std::collections::HashMap::from([(
                    "TOKEN".to_string(),
                    serde_json::json!("abc123"),
                )]),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        // The before-request phase of step 2 must already see step 1's write.
        let calls = h.engine.calls();
        let reads = h.engine.runtime_reads();
        let idx = calls
            .iter()
            .position(|c| c == "Second|before-request")
            .expect("second step ran");
        assert_eq!(reads[idx].get("TOKEN"), Some(&"abc123".to_string()));
    }

    #[tokio::test]
    async fn publishes_started_step_and_finished_events() {
        use rocket_shared::events::DomainEvent;

        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");
        let events = h.publisher.events();

        assert!(matches!(
            events.first(),
            Some(DomainEvent::RunnerStarted { total_steps: 3, .. })
        ));
        let step_events: Vec<&DomainEvent> = events
            .iter()
            .filter(|e| matches!(e, DomainEvent::RunnerStepCompleted { .. }))
            .collect();
        assert_eq!(step_events.len(), 3);
        assert!(matches!(
            events.last(),
            Some(DomainEvent::RunnerFinished { step_count: 3, failed_count: 0, .. })
        ));
        assert!(events.iter().all(|e| match e {
            DomainEvent::RunnerStarted { run_id, .. }
            | DomainEvent::RunnerStepCompleted { run_id, .. }
            | DomainEvent::RunnerFinished { run_id, .. } => run_id == &summary.run_id,
            _ => true,
        }));
    }

    #[tokio::test]
    async fn unknown_collection_is_not_found() {
        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        let mut input = sample_run_input();
        input.collection = "missing".into();
        let err = h.runner.run(&h.exec, input).await.expect_err("must fail");
        assert!(matches!(err, rocket_shared::error::DomainError::NotFound(_)));
    }

    // These two lock the IPC DTO wire shapes the frontend plan is written
    // against. Unlike `DomainEvent`, these are camelCase all the way down.
    #[test]
    fn stopped_reason_wire_shape_is_camel_case() {
        let reason = StoppedReason::UnknownNextRequest {
            item_name: "First".into(),
            next_request: "Nowhere".into(),
        };
        let json = serde_json::to_string(&reason).expect("serialize");
        assert_eq!(
            json,
            r#"{"kind":"unknownNextRequest","itemName":"First","nextRequest":"Nowhere"}"#
        );
    }

    #[test]
    fn run_step_status_wire_shape_is_camel_case() {
        assert_eq!(
            serde_json::to_string(&RunStepStatus::Completed).expect("serialize"),
            r#""completed""#
        );
        assert_eq!(
            serde_json::to_string(&RunStepStatus::Skipped).expect("serialize"),
            r#""skipped""#
        );
        assert_eq!(
            serde_json::to_string(&RunStepStatus::Error).expect("serialize"),
            r#""error""#
        );
    }

    #[tokio::test]
    async fn runner_step_applies_the_request_guard_policy_to_before_request_mutations() {
        // The runner must apply the same SSRF guard to every step's
        // BeforeRequest mutations as a single send would (Item 6). Without
        // threading request_guard_policy through build_step_input, this
        // mutation would be applied unchecked.
        use rocket_scripting::RequestMutations;

        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "before-request",
            ScriptResult {
                request_mutations: Some(RequestMutations {
                    url: Some("http://169.254.169.254/latest/meta-data/".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let mut input = sample_run_input();
        input.request_guard_policy = rocket_workspace::RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };

        let summary = h.runner.run(&h.exec, input).await.expect("run");

        assert_eq!(summary.steps[0].status, RunStepStatus::Error);
        let err = summary.steps[0].error.as_ref().expect("guard error message");
        assert!(err.contains("169.254.169.254"), "unexpected message: {err}");
        // The run must continue past the blocked step (stop_on_failure defaults
        // to false) rather than aborting the whole run.
        assert_eq!(summary.steps.len(), 3);
        assert_eq!(summary.steps[1].status, RunStepStatus::Completed);
    }

    #[tokio::test]
    async fn skip_request_makes_no_http_call_and_runs_no_later_phase() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "Second",
            "before-request",
            ScriptResult { skip_request: true, ..Default::default() },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(
            h.executor.sent_urls(),
            vec![
                "https://api.test/first.yml".to_string(),
                "https://api.test/third.yml".to_string(),
            ],
            "the skipped request must never reach the executor"
        );
        assert_eq!(summary.steps[1].status, RunStepStatus::Skipped);
        assert_eq!(summary.steps[1].status_code, None);
        assert!(
            !h.engine.calls().contains(&"Second|after-response".to_string()),
            "a skipped step has no response, so no later phase may run"
        );
        assert!(!h.engine.calls().contains(&"Second|tests".to_string()));
        // Only the two sent requests are history-worthy.
        assert_eq!(h.history.saved_count(), 2);
    }

    #[tokio::test]
    async fn set_next_request_from_tests_phase_jumps_the_run() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "tests",
            ScriptResult {
                next_request: Some(NextRequest::Name("Third".into())),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        let names: Vec<&str> = summary.steps.iter().map(|s| s.item_name.as_str()).collect();
        assert_eq!(names, vec!["First", "Third"], "Second must be jumped over");
    }

    #[tokio::test]
    async fn skip_request_combined_with_set_next_request_honours_the_jump() {
        // Bruno cannot do this (usebruno/bruno#5831) because it only reads
        // setNextRequest from post-response scripts. Rocket checks after every
        // phase that ran, so both calls in one before-request script work.
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "before-request",
            ScriptResult {
                skip_request: true,
                next_request: Some(NextRequest::Name("Third".into())),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        let names: Vec<&str> = summary.steps.iter().map(|s| s.item_name.as_str()).collect();
        assert_eq!(names, vec!["First", "Third"]);
        assert_eq!(summary.steps[0].status, RunStepStatus::Skipped);
        assert_eq!(h.executor.sent_urls(), vec!["https://api.test/third.yml".to_string()]);
    }

    #[tokio::test]
    async fn later_phase_wins_when_two_phases_set_next_request() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "before-request",
            ScriptResult {
                next_request: Some(NextRequest::Name("Second".into())),
                ..Default::default()
            },
        );
        engine.on(
            "First",
            "tests",
            ScriptResult {
                next_request: Some(NextRequest::Name("Third".into())),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        let names: Vec<&str> = summary.steps.iter().map(|s| s.item_name.as_str()).collect();
        assert_eq!(names, vec!["First", "Third"], "the last phase that ran wins");
    }

    #[tokio::test]
    async fn set_next_request_null_stops_the_run() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "after-response",
            ScriptResult { next_request: Some(NextRequest::Stop), ..Default::default() },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), 1);
        assert_eq!(summary.stopped_reason, StoppedReason::StoppedByScript);
        assert_eq!(h.executor.sent_urls().len(), 1);
    }

    #[tokio::test]
    async fn unknown_next_request_records_an_error_and_stops() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "tests",
            ScriptResult {
                next_request: Some(NextRequest::Name("Nowhere".into())),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), 1);
        assert_eq!(summary.steps[0].status, RunStepStatus::Error);
        let error = summary.steps[0].error.as_deref().expect("error recorded on the step");
        assert!(error.contains("Nowhere"), "got {error}");
        assert_eq!(
            summary.stopped_reason,
            StoppedReason::UnknownNextRequest {
                item_name: "First".into(),
                next_request: "Nowhere".into(),
            }
        );
    }

    #[tokio::test]
    async fn a_next_request_cycle_stops_at_the_step_limit() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "tests",
            ScriptResult {
                next_request: Some(NextRequest::Name("Second".into())),
                ..Default::default()
            },
        );
        engine.on(
            "Second",
            "tests",
            ScriptResult {
                next_request: Some(NextRequest::Name("First".into())),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), MAX_RUN_STEPS);
        assert_eq!(
            summary.stopped_reason,
            StoppedReason::StepLimitReached { limit: MAX_RUN_STEPS }
        );
    }

    /// Publisher that cancels the run as soon as it sees the Nth
    /// `RunnerStepCompleted` event, by writing straight into the shared
    /// cancellation registry the service was built with.
    struct CancelAfterSteps {
        cancel_after: usize,
        seen: Mutex<usize>,
        cancelled: Arc<Mutex<HashSet<String>>>,
    }

    impl EventPublisher for CancelAfterSteps {
        fn publish(&self, event: DomainEvent) {
            if let DomainEvent::RunnerStepCompleted { run_id, .. } = &event {
                let mut seen = self.seen.lock().expect("lock");
                *seen += 1;
                if *seen >= self.cancel_after {
                    self.cancelled.lock().expect("lock").insert(run_id.clone());
                }
            }
        }
    }

    #[tokio::test]
    async fn a_failed_step_does_not_stop_the_run_by_default() {
        let executor = RecordingExecutor::new();
        executor.set_status("second.yml", 500);
        let h = harness(three_step_collection(), ProgrammableEngine::new(), executor);
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), 3, "every remaining item still runs");
        assert_eq!(summary.stopped_reason, StoppedReason::Completed);
        assert!(summary.steps[1].is_failure());
        assert_eq!(summary.steps[1].status_code, Some(500));
    }

    #[tokio::test]
    async fn stop_on_failure_ends_the_run_at_the_first_failure() {
        let executor = RecordingExecutor::new();
        executor.set_status("second.yml", 500);
        let h = harness(three_step_collection(), ProgrammableEngine::new(), executor);
        let mut input = sample_run_input();
        input.stop_on_failure = true;
        let summary = h.runner.run(&h.exec, input).await.expect("run");

        assert_eq!(summary.steps.len(), 2);
        assert_eq!(
            summary.stopped_reason,
            StoppedReason::StoppedOnFailure { item_name: "Second".into() }
        );
    }

    #[tokio::test]
    async fn a_failing_test_counts_as_a_step_failure() {
        use rocket_scripting::{TestResult, TestStatus};

        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "tests",
            ScriptResult {
                test_results: vec![
                    TestResult { name: "ok".into(), status: TestStatus::Passed, error: None },
                    TestResult {
                        name: "nope".into(),
                        status: TestStatus::Failed,
                        error: Some("expected 200".into()),
                    },
                ],
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps[0].test_pass_count, 1);
        assert_eq!(summary.steps[0].test_fail_count, 1);
        assert!(summary.steps[0].is_failure());
        assert_eq!(summary.steps.len(), 3, "continue-on-failure is the default");
    }

    #[tokio::test]
    async fn a_transport_error_is_an_error_step_and_the_run_continues() {
        // Status 0 makes the recording executor fail the send.
        let executor = RecordingExecutor::new();
        executor.set_status("second.yml", 0);
        let h = harness(three_step_collection(), ProgrammableEngine::new(), executor);
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), 3);
        assert_eq!(summary.steps[1].status, RunStepStatus::Error);
        assert!(summary.steps[1].error.is_some());
        assert_eq!(summary.stopped_reason, StoppedReason::Completed);
    }

    #[tokio::test]
    async fn cancelling_mid_run_stops_before_the_next_step() {
        // Build the runner by hand so the test and the canceller share one
        // cancellation registry; the publisher cancels once step 1 reports.
        let collection = three_step_collection();
        let repo = InMemoryCollectionRepo::new(collection);
        let executor = RecordingExecutor::new();
        let engine = ProgrammableEngine::new();
        let cancelled: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        let exec = RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(SharedExecutor(Arc::clone(&executor))),
            Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(NullCookieRepo),
            Box::new(rocket_shared::events::NullEventPublisher),
        )
        .with_script_engine(Box::new(SharedEngine(Arc::clone(&engine))));

        let runner = CollectionRunnerService::new(
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(CancelAfterSteps {
                cancel_after: 1,
                seen: Mutex::new(0),
                cancelled: Arc::clone(&cancelled),
            }),
        )
        .with_cancellations(Arc::clone(&cancelled));

        let summary = runner.run(&exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), 1, "the run stops before step 2 starts");
        assert_eq!(summary.stopped_reason, StoppedReason::Cancelled);
        assert_eq!(executor.sent_urls(), vec!["https://api.test/first.yml".to_string()]);
        assert!(
            !cancelled.lock().expect("lock").contains(&summary.run_id),
            "a finished run must not leak its id in the registry"
        );
    }

    #[tokio::test]
    async fn cancelling_an_unknown_run_id_is_a_no_op() {
        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        h.runner.cancel("not-a-real-run");
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");
        assert_eq!(summary.stopped_reason, StoppedReason::Completed);
        assert_eq!(summary.steps.len(), 3);
    }
}
