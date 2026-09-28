//! In-process MCP tool server for ACP agent sessions (Subproject D). One
//! HTTP server instance per ACP session, hosting the 6 tools defined in
//! `tool_server`, guarded by the bearer-token check in `auth`, and tracked
//! by `registry` so the app's exit-sweep machinery can shut every live
//! instance down.
//!
//! `registry` (Task 4) does not exist yet as of Task 3 — only
//! `tool_server`'s skeleton and `auth`'s middleware are wired in here.
//! Declaring `pub mod registry;` before that file exists would break
//! `cargo check --workspace`, so that line is added by the task that
//! creates it.

pub mod auth;
pub mod tool_server;
