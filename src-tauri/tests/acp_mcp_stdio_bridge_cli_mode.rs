// Proves the `--acp-mcp-stdio-bridge` flag short-circuits before any Tauri
// bootstrap. Running it with no ROCKET_MCP_PORT/ROCKET_MCP_TOKEN set must
// fail fast with the bridge's own clear error, not hang trying to open a
// display connection the way falling through to `tauri::Builder` startup
// would on a headless machine — a real GUI attempt fails differently (or
// hangs), so this specific fast, specific-message failure is the proxy for
// "no GUI bootstrap happened".
use std::process::Command;

#[test]
fn acp_mcp_stdio_bridge_flag_skips_gui_bootstrap_and_fails_fast_without_env() {
    let exe = env!("CARGO_BIN_EXE_rocket");
    let output = Command::new(exe)
        .arg("--acp-mcp-stdio-bridge")
        .env_remove("ROCKET_MCP_PORT")
        .env_remove("ROCKET_MCP_TOKEN")
        .output()
        .expect("failed to run the rocket binary");

    assert!(
        !output.status.success(),
        "bridge mode with no env vars set must exit non-zero, not hang or launch a GUI"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ROCKET_MCP_PORT"),
        "expected the bridge's own missing-env-var error, got: {stderr}"
    );
}
