//! In-process MCP tool server for ACP agent sessions (Subproject D). One
//! HTTP server instance per ACP session, hosting the 6 tools defined in
//! `tool_server`, guarded by the bearer-token check in `auth`, and tracked
//! by `registry` so the app's exit-sweep machinery can shut every live
//! instance down.
//!
pub mod auth;
pub mod registry;
pub mod tool_server;
