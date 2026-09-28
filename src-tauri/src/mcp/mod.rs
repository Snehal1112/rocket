//! In-process MCP tool server for ACP agent sessions (Subproject D). One
//! HTTP server instance per ACP session, hosting the 6 tools defined in
//! `tool_server`, guarded by the bearer-token check in `auth`, and tracked
//! by `registry` so the app's exit-sweep machinery can shut every live
//! instance down.
//!
//! `auth` (Task 3) and `registry` (Task 4) do not exist yet as of Task 1 —
//! only `tool_server`'s skeleton is wired in here. Declaring `pub mod auth;`
//! / `pub mod registry;` before those files exist would break
//! `cargo check --workspace`, so those lines are added by the tasks that
//! create the files they name.

pub mod tool_server;
