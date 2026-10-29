# App command ACL

Rocket's app commands (the `#[tauri::command]` functions in `src-tauri/src/commands/*`, registered in `generate_handler!` in `src-tauri/src/lib.rs`) can be called only by the local page in the `main` window.

## Why

Tauri 2.10.3 checks app commands only when an app ACL manifest exists (`webview/mod.rs:1802`). Without one, any page in any window can call every command. The OAuth 2 window (`oauth2-auth`, in `src-tauri/src/commands/oauth2.rs`) loads a remote identity provider page, so that page could call every command for up to 120 seconds. The OAuth callback is caught in Rust by `on_navigation`, so the OAuth windows need no IPC.

## The rule

- The command list is `src-tauri/app-commands.txt`: one name per line, sorted, `#` comments allowed.
- A new command goes in both `generate_handler!` (`src-tauri/src/lib.rs`) and `app-commands.txt`. The test `app_commands_file_matches_generate_handler` fails when the two differ.
- `src-tauri/build.rs` reads the list and writes the generated `src-tauri/permissions/main-window-commands.toml`. That file is git-ignored. It holds one permission set, `main-window-commands`.
- A command missing from the list is denied at runtime.

## Which windows get what

- `main` gets the `main-window-commands` set through `src-tauri/capabilities/default.json`. The capability has no `remote` entry, so only the local page can use it.
- `oauth2-auth` and `oauth2-auth-clear` get no app commands.

## Giving a new window access

Add a new capability file for that window, and grant only the set it needs. Never add a `remote` entry without a security review.

## Tests

In `src-tauri/src/ipc_acl_tests.rs`:

- `app_commands_file_matches_generate_handler`
- `app_commands_file_is_sorted_and_unique`
- `main_window_local_page_can_invoke_app_commands`
- `remote_page_in_oauth_window_cannot_invoke_app_commands`
- `local_page_in_oauth_window_cannot_invoke_app_commands`
- `remote_page_in_main_window_cannot_invoke_app_commands`

## Manual checks

Not yet run. These need the desktop app, so they cannot run in CI.

- [ ] `yarn tauri dev`: the app starts. Collections list, a request runs, the environment editor saves and the git panel loads, with no "not allowed" errors in the console.
- [ ] `yarn tauri dev`: OAuth 2 authorization code with the embedded window (system browser off) completes sign-in and stores a token.
- [ ] `yarn tauri build`: the release app starts and runs one request. Release origins differ from dev (`tauri://localhost` vs `http://localhost:1420`).
