use crate::scripting::local_modules::LocalRoots;
use rocket_environment::VariableContext;
use rocket_http::{HttpRequest, HttpResponse};
use rocket_scripting::{
    CollectionVarWrite, ConsoleEntry, ConsoleLevel, EnvVarWrite, NextRequest, RequestMutations,
    SandboxMode, ScriptPhase, TestResult, TestStatus,
};
use rocket_shared::types::PathParam;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Holds everything ops need to read from the `ScriptContext`.
/// Stored in `deno_core::OpState` as a read-only snapshot.
pub struct ScriptInputState {
    pub phase: ScriptPhase,
    pub variables: VariableContext,
    pub request: HttpRequest,
    pub response: Option<HttpResponse>,
    pub env_name: Option<String>,
    pub execution_mode: String,
    pub execution_platform: String,
    pub request_name: String,
    pub request_tags: Vec<String>,
    pub path_params: Vec<PathParam>,
    /// Allowed roots for local `require()`. `None` when the context has no file
    /// scope or the collection directory is unavailable.
    pub local_roots: Option<LocalRoots>,
    /// Values that must be redacted if they appear in script-emitted
    /// console/test-error text. Copied from `variables.secret_values` when
    /// this state is seeded in `run_script` (engine.rs) — kept as its own
    /// field so ops that only need the redaction list (console/test-fail
    /// ops) don't have to reach through `variables`.
    pub secret_values: HashSet<String>,
    /// Capability level of this run. Decides `rok.isSafeMode()` and `rok.cwd()`.
    pub sandbox_mode: SandboxMode,
    /// Collection display name, empty when unknown.
    pub collection_name: String,
    /// Absolute collection directory, for `rok.cwd()` and `__dirname`.
    pub collection_root: Option<PathBuf>,
}

/// Accumulates all side-effects produced by ops during execution.
/// Stored in `deno_core::OpState` as mutable state.
#[derive(Default)]
pub struct ScriptOutputState {
    pub request_mutations: RequestMutations,
    pub any_request_mutation: bool,
    pub runtime_vars: HashMap<String, serde_json::Value>,
    pub runtime_var_deletes: Vec<String>,
    pub env_var_writes: Vec<EnvVarWrite>,
    pub collection_var_writes: Vec<CollectionVarWrite>,
    pub global_env_var_writes: Vec<EnvVarWrite>,
    pub next_request: Option<NextRequest>,
    pub skip_request: bool,
    pub test_results: Vec<TestResult>,
    pub console_entries: Vec<ConsoleEntry>,
}

impl ScriptOutputState {
    pub fn add_console(&mut self, level: ConsoleLevel, message: String) {
        self.console_entries.push(ConsoleEntry { level, message });
    }

    pub fn add_test_result(&mut self, name: String, passed: bool, error: Option<String>) {
        self.test_results.push(TestResult {
            name,
            status: if passed {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            },
            error,
        });
    }
}
