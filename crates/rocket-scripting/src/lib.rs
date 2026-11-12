// The async_trait macro triggers this lint on newer toolchains.
#![allow(clippy::double_must_use)]
pub mod assertion_evaluator;
pub mod context;
pub mod engine;
pub mod host;
pub mod phase;
pub mod result;
pub mod script_cookie;

pub use assertion_evaluator::{assertion_outcomes, evaluate_assertions};
pub use context::{ExecutionMode, IterationInfo, SandboxMode, ScriptContext, ScriptFileScope};
pub use engine::ScriptEngine;
pub use host::{
    HostCookieOp, HostCookieResult, HostError, HostRequest, HostResponse, HostRunOutcome,
    HostRunRequest, HostScopes, PendingWrites, ScriptHost,
};
pub use phase::ScriptPhase;
pub use result::{
    AssertionOutcome, CollectionVarWrite, ConsoleEntry, ConsoleLevel, EnvVarWrite, HeaderMutation,
    NextRequest, RequestMutations, ScriptResult, TestResult, TestStatus,
};
pub use script_cookie::{
    build_cookie, cookie_id, matching_cookies, to_entry, CookieEntry, CookieInput, UrlScope,
};
