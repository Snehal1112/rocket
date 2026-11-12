use std::collections::BTreeMap;

use rocket_app::{
    CollectionRunnerService, IterationDataService, LoadedIterationData, RequestExecutionService,
    RunCollectionInput, RunSummary,
};
use rocket_collection::DataFileSource;
use rocket_shared::error::DomainError;
use tauri::State;

/// The parsed iteration data sent to the frontend.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IterationDataDto {
    pub columns: Vec<String>,
    pub rows: Vec<BTreeMap<String, String>>,
    /// `perRun` or `savedDefault`.
    pub source: String,
    pub file_name: String,
}

impl From<LoadedIterationData> for IterationDataDto {
    fn from(loaded: LoadedIterationData) -> Self {
        Self {
            columns: loaded.data.columns,
            rows: loaded.data.rows,
            source: match loaded.source {
                DataFileSource::PerRun => "perRun",
                DataFileSource::SavedDefault => "savedDefault",
            }
            .to_string(),
            file_name: loaded.file_name,
        }
    }
}

/// Reads and parses the data file for a run: the per-run path, else the saved default.
/// Returns `None` when neither is set.
#[tauri::command]
pub async fn load_iteration_data(
    collection: String,
    per_run_path: Option<String>,
    service: State<'_, IterationDataService>,
) -> Result<Option<IterationDataDto>, DomainError> {
    Ok(service
        .load(&collection, per_run_path.as_deref())?
        .map(IterationDataDto::from))
}

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
