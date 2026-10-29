//! Tests for the app command ACL: only the local page of the `main` window may call app commands.

use std::collections::BTreeSet;

use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::webview::InvokeRequest;
use tauri::{WebviewUrl, WebviewWindowBuilder};

const REMOTE_URL: &str = "https://idp.example.com/authorize";

/// Reads the command list: one name per line, skipping blank and `#` lines.
fn parse_command_file(text: &str) -> BTreeSet<String> {
    command_lines(text).map(str::to_string).collect()
}

/// Returns the non-blank, non-comment lines of the command list in file order.
fn command_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// Reads the command names out of the `generate_handler![...]` block in the given source.
fn parse_generate_handler(source: &str) -> BTreeSet<String> {
    let marker = "generate_handler![";
    let start = source
        .find(marker)
        .expect("lib.rs has a generate_handler block")
        + marker.len();
    let len = source[start..]
        .find(']')
        .expect("generate_handler block is closed");
    // Drop line comments first so a trailing comment cannot swallow the next entry.
    source[start..start + len]
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| entry.rsplit("::").next())
        .map(str::to_string)
        .collect()
}

#[test]
fn parse_generate_handler_keeps_entry_after_trailing_comment() {
    let source = "generate_handler![\n    foo, // note\n    bar,\n]";
    let names = parse_generate_handler(source);
    assert!(names.contains("foo") && names.contains("bar"), "got {names:?}");
}

#[test]
fn app_commands_file_matches_generate_handler() {
    let file = parse_command_file(include_str!("../app-commands.txt"));
    let handler = parse_generate_handler(include_str!("lib.rs"));
    let missing: Vec<_> = handler.difference(&file).collect();
    let extra: Vec<_> = file.difference(&handler).collect();
    assert!(missing.is_empty(), "add to app-commands.txt: {missing:?}");
    assert!(extra.is_empty(), "remove from app-commands.txt: {extra:?}");
}

#[test]
fn app_commands_file_is_sorted_and_unique() {
    let lines: Vec<_> = command_lines(include_str!("../app-commands.txt")).collect();
    let canonical: Vec<_> = lines
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(
        lines, canonical,
        "app-commands.txt must be sorted with no duplicates"
    );
}

/// Builds a mock app with the real ACL context and one stateless command.
fn app() -> tauri::App<tauri::test::MockRuntime> {
    tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            crate::commands::git::list_ssh_key_paths
        ])
        .build(tauri::generate_context!())
        .expect("mock app builds")
}

/// Creates a window with the given label on the local app page.
fn local_window(
    app: &tauri::App<tauri::test::MockRuntime>,
    label: &str,
) -> tauri::WebviewWindow<tauri::test::MockRuntime> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .build()
        .expect("window builds")
}

/// Calls `list_ssh_key_paths` from the window as if the page at `url` sent it.
fn invoke(
    webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    url: &str,
) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
    tauri::test::get_ipc_response(
        webview,
        InvokeRequest {
            cmd: "list_ssh_key_paths".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: url.parse().expect("test url parses"),
            body: InvokeBody::default(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
}

/// Asserts the call failed with Tauri's ACL rejection message.
fn assert_acl_rejected(result: &Result<tauri::ipc::InvokeResponseBody, serde_json::Value>) {
    let err = result.as_ref().expect_err("expected an ACL rejection");
    assert!(err.to_string().contains("not allowed"), "got {err}");
}

#[test]
fn main_window_local_page_can_invoke_app_commands() {
    let app = app();
    let main = local_window(&app, "main");
    let url = main.url().expect("main window has a url").to_string();
    let result = invoke(&main, &url);
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[test]
fn main_window_release_origin_can_invoke_app_commands() {
    let app = app();
    let main = local_window(&app, "main");
    let result = invoke(&main, "tauri://localhost/index.html");
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[test]
fn remote_page_in_oauth_window_cannot_invoke_app_commands() {
    let app = app();
    let oauth = local_window(&app, "oauth2-auth");
    let result = invoke(&oauth, REMOTE_URL);
    assert_acl_rejected(&result);
}

#[test]
fn local_page_in_oauth_window_cannot_invoke_app_commands() {
    let app = app();
    let main = local_window(&app, "main");
    let local_url = main.url().expect("main window has a url").to_string();
    let oauth = local_window(&app, "oauth2-auth");
    let result = invoke(&oauth, &local_url);
    assert_acl_rejected(&result);
}

#[test]
fn remote_page_in_main_window_cannot_invoke_app_commands() {
    let app = app();
    let main = local_window(&app, "main");
    let result = invoke(&main, "https://idp.example.com/");
    assert_acl_rejected(&result);
}
