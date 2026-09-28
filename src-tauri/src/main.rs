// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Hidden startup mode: an ACP agent spawns this exact binary again with
    // this flag when it needs the Stdio MCP transport (AcpSessionService::
    // start_session, Task 3, builds this command line). It must never reach
    // normal Tauri bootstrap below — there is no window or webview in this
    // mode, only a stdio<->HTTP forwarding loop.
    if std::env::args().any(|arg| arg == "--acp-mcp-stdio-bridge") {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to start the Tokio runtime for the MCP stdio bridge");
        if let Err(e) = runtime.block_on(rocket_lib::mcp::stdio_bridge::run_stdio_bridge()) {
            eprintln!("acp-mcp-stdio-bridge failed: {e}");
            std::process::exit(1);
        }
        return;
    }

    rocket_lib::run()
}
