// Application services — orchestration layer

pub mod acp_session_service;
pub mod agent_config_service;
pub mod agent_isolation;
pub mod assertion_evaluator;
pub mod assistant_chip_text;
pub mod callback_listener;
pub(crate) mod client_certificates;
pub mod collection_runner_service;
pub mod collection_service;
pub mod collection_trust;
pub mod contract_service;
pub(crate) mod conversation_secrets;
pub mod conversation_service;
pub mod cookie_service;
#[cfg(test)]
mod duplicate_request_e2e_tests;
pub mod env_audit;
pub mod environment_service;
pub mod execution_service;
pub mod export_service;
pub mod flow_auth;
pub(crate) mod flow_callbacks;
pub(crate) mod flow_cancel;
pub(crate) mod flow_debug;
pub mod flow_execution_service;
pub(crate) mod flow_partial;
pub(crate) mod flow_poll;
mod flow_references;
#[cfg(test)]
mod flow_request_ref_e2e_tests;
pub(crate) mod flow_routing;
pub(crate) mod flow_run_cache;
pub(crate) mod flow_run_id;
pub mod flow_service;
pub(crate) mod flow_trace;
pub(crate) mod flow_wait;
pub mod git_service;
pub mod graphql_document;
pub mod graphql_request;
pub mod graphql_schema;
pub mod graphql_subscription;
pub mod grpc_service;
pub mod history_service;
pub mod load_test_service;
pub mod mcp_read_views;
pub mod mcp_tool_service;
pub mod oauth2_service;
pub mod proposal_service;
pub mod proxy_settings_service;
pub(crate) mod redaction;
pub mod request_guard;
pub mod runner_sequence;
pub mod secret_manager_service;
pub mod security_audit_service;
mod session_plan;
pub mod template_service;
#[cfg(test)]
pub(crate) mod test_doubles;
pub(crate) mod vault_certificates;
pub mod vault_secret_resolution;
pub mod websocket_service;
pub mod workspace_service;

pub use acp_session_service::{
    AcpSessionService, McpHttpServerCredentials, NoopSessionCleanup, SessionCleanup,
};
pub use agent_config_service::{process_env_reader, AgentConfigService, EnvReader};
pub use agent_isolation::{
    isolation_meta, SessionIsolation, ISOLATION_ENV_CONFIG_DIR, ROCKET_ASSISTANT_SYSTEM_PROMPT,
};
pub use assistant_chip_text::{
    ChipKind, ChipResource, ResponseChipHeader, ResponseChipInput, ResponseChipRequest,
    ResponseChipTest, CHIP_TEXT_LIMIT_BYTES,
};
pub use callback_listener::{CallbackEndpoint, CallbackListener, NoCallbackListener, ReceivedCall};
pub use collection_runner_service::{
    CollectionRunnerService, RunCollectionInput, RunStepResult, RunStepStatus, RunSummary,
    StoppedReason,
};
pub use collection_service::CollectionService;
pub use collection_trust::{
    effective_capabilities, Capability, CollectionTrustService, TrustStatus,
};
pub use contract_service::ContractService;
pub use conversation_service::{
    ConversationInput, ConversationService, MAX_INPUT_TEXT_BYTES, MAX_SESSION_PINS,
};
pub use cookie_service::CookieService;
pub use environment_service::EnvironmentService;
pub use execution_service::websocket_resolution::{
    WebSocketConnectInput, WebSocketScope, WebSocketSendInput,
};
pub use execution_service::{
    ExecuteRequestInput, ExecuteRequestOutput, ProcessEnv, RequestExecutionService,
};
pub use export_service::{ExportFormat, ExportService};
pub use flow_auth::{
    FetchContext, FlowAuthTokens, FlowTokenFetcher, NoTokenFetcher, OAuth2ServiceFetcher,
    SuppliedToken,
};
pub use flow_callbacks::FlowRunCallback;
pub use flow_execution_service::{
    CapturedOutput, FlowExecutionService, FlowRunOptions, FlowRunSummary, FlowStepResult,
    RunFlowInput,
};
pub use flow_partial::PartialRun;
pub use flow_references::{FlowReferenceUpdater, FlowRewriteReport};
pub use flow_service::FlowService;
pub use git_service::GitAppService;
pub use graphql_request::ExecuteGraphQlInput;
pub use graphql_subscription::{
    resolve_graphql_subscription, to_websocket_url, GraphQlSubscribeInput,
    GraphQlSubscriptionService, GraphQlSubscriptionStart,
};
pub use grpc_service::{GrpcExecuteInput, GrpcService};
pub use history_service::HistoryService;
pub use load_test_service::LoadTestService;
pub use mcp_read_views::{
    CollectionBrief, FolderChildView, FolderView, HistoryBrief, MaskedBody, MaskedEnvironment,
    MaskedFolderSettings, MaskedPair, MaskedRequest, MaskedSettings, MaskedVariable,
};
pub use mcp_tool_service::{
    AssistantMode, McpRunResult, McpToolService, OUTLINE_RESOURCE_URI,
    WORKSPACE_ASSISTANT_INSTRUCTIONS,
};
pub use oauth2_service::OAuth2Service;
pub use proposal_service::ProposalService;
pub use proxy_settings_service::{PasswordChange, ProxySettingsService, ProxySettingsView};
pub use runner_sequence::{build_step_input, flatten_run_set, RunItem};
pub use secret_manager_service::SecretManagerService;
pub use security_audit_service::SecurityAuditService;
pub use session_plan::{
    session_plan, PlanInput, PlannedFile, SessionPlan, GEMINI_FIRST_PROMPT_REMINDER,
    NON_CLAUDE_TOOL_NOTICE, ROCKET_MCP_TOOL_NAMES,
};
pub use template_service::TemplateService;
pub use vault_secret_resolution::resolve_vault_secret_value;
pub use websocket_service::WebSocketService;
pub use workspace_service::WorkspaceService;
