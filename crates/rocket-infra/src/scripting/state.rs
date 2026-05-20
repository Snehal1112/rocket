use std::collections::HashMap;
use rocket_http::{HttpRequest, HttpResponse};
use rocket_scripting::{
    CollectionVarWrite, ConsoleEntry, ConsoleLevel, EnvVarWrite,
    NextRequest, RequestMutations, ScriptPhase, TestResult, TestStatus,
};
use rocket_environment::VariableContext;

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
}

/// Accumulates all side-effects produced by ops during execution.
/// Stored in `deno_core::OpState` as mutable state.
#[derive(Default)]
pub struct ScriptOutputState {
    pub request_mutations: RequestMutations,
    pub any_request_mutation: bool,
    pub runtime_vars: HashMap<String, serde_json::Value>,
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
            status: if passed { TestStatus::Passed } else { TestStatus::Failed },
            error,
        });
    }
}
