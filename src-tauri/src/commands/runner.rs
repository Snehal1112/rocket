use rocket_app::{CollectionRunnerService, RequestExecutionService, RunCollectionInput, RunSummary};
use rocket_shared::error::DomainError;
use tauri::State;

/// Runs every request in a collection or folder, in order.
///
/// Streams `runner-started`, `runner-step-completed` and `runner-finished`
/// events while it runs, and returns the same data as one summary when the run
/// ends — mirroring how `run_load_test_v2_command` streams progress and the
/// frontend reads results from the events.
#[tauri::command]
pub async fn run_collection(
    input: RunCollectionInput,
    runner: State<'_, CollectionRunnerService>,
    exec: State<'_, RequestExecutionService>,
) -> Result<RunSummary, DomainError> {
    runner.run(&exec, input).await
}

/// Asks an in-progress run to stop. The run ends before its next step; the step
/// already in flight finishes first. An unknown or finished run id is a no-op.
#[tauri::command]
pub fn stop_collection_run(
    run_id: String,
    runner: State<'_, CollectionRunnerService>,
) -> Result<(), DomainError> {
    runner.cancel(&run_id);
    Ok(())
}
