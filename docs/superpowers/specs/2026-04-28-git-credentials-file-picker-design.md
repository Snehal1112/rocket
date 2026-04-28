# Git Credentials — SSH Key File Picker & OS Keychain Persistence

**Date:** 2026-04-28
**Status:** Approved

---

## Overview

Replace the plain text `Private Key Path` input in the Git Credentials dialog with a clickable file-picker field. Add OS keychain persistence so credentials survive app restarts. Meet WCAG 2.1 AAA accessibility standards throughout.

---

## Requirements

1. The `Private Key Path` field becomes a read-only clickable area — clicking it opens the native OS file picker starting in `~/.ssh/`, showing all files.
2. On dialog open, a new Tauri command auto-detects the default SSH key for the current OS (`id_ed25519` → `id_rsa` → `id_ecdsa` → `id_dsa` in `~/.ssh/`). If a key is found it pre-fills the field; if not, the field shows an italic placeholder "Click to select a key file…".
3. Previously saved credentials are loaded from the OS keychain when the dialog opens. The key path and passphrase are both restored; auth type is also persisted.
4. On Connect, credentials are saved to the OS keychain before the git operation proceeds.
5. The dialog meets WCAG 2.1 AAA throughout — keyboard navigation, focus indicators, ARIA roles, colour contrast ≥ 7:1, motion preferences respected.

---

## Architecture

### New dependency: `tauri-plugin-keyring`

- Add `tauri-plugin-keyring` to `src-tauri/Cargo.toml`.
- Add `@tauri-apps/plugin-keyring` to `package.json`.
- Register the plugin in `src-tauri/src/lib.rs` alongside existing plugins.
- Keychain service name: `"rocket-api"`. Key: `"git-credentials"`. Value: JSON-serialised credential struct.

### New Rust command: `get_default_ssh_key_path`

Location: `src-tauri/src/commands/git.rs`

```
#[tauri::command]
fn get_default_ssh_key_path() -> Option<String>
```

- Uses `dirs::home_dir()` (already available via the `dirs` crate).
- Probes `~/.ssh/id_ed25519`, `~/.ssh/id_rsa`, `~/.ssh/id_ecdsa`, `~/.ssh/id_dsa` in order.
- Returns the full path of the first file that exists, or `None`.
- On Windows, `dirs::home_dir()` returns the correct `C:\Users\<name>` path — no special casing needed.

### New Rust commands: `save_git_credentials` / `load_git_credentials`

Location: `src-tauri/src/commands/git.rs`

```
#[tauri::command]
fn save_git_credentials(creds: GitCredentialsPayload) -> Result<(), String>

#[tauri::command]  
fn load_git_credentials() -> Result<Option<GitCredentialsPayload>, String>
```

- `GitCredentialsPayload` is a serialisable struct mirroring `GitCredentials` (auth type + all fields including passphrase).
- Both commands delegate to `tauri_plugin_keyring` — service `"rocket-api"`, account `"git-credentials"`.
- `save_git_credentials` serialises to JSON and stores. `load_git_credentials` reads and deserialises, returning `None` if no entry exists yet.
- Passphrase is stored inside the OS-encrypted keychain entry — never touches a file on disk.

### Frontend API: `src/lib/tauri-api.ts`

Add three new exported functions:

```ts
export const getDefaultSshKeyPath = () =>
  invoke<string | null>('get_default_ssh_key_path');

export const saveGitCredentials = (creds: GitCredentials) =>
  invoke<void>('save_git_credentials', { creds });

export const loadGitCredentials = () =>
  invoke<GitCredentials | null>('load_git_credentials');
```

### `GitCredentialsDialog.tsx` changes

- On mount: call `loadGitCredentials()` first; if result is non-null, use it to initialise all fields — no further auto-detection is done (saved credentials always win). If null, call `getDefaultSshKeyPath()` to pre-fill the path field for SSH Key type.
- `Private Key Path` field: replace `<Input>` with a styled `<button>` (shadcn `Button` variant `ghost` or a custom div with `role="button"`). Clicking calls `open()` from `@tauri-apps/plugin-dialog` with `defaultPath` set to the `~/.ssh/` directory. Cancelled picker leaves the field unchanged.
- Empty state: dashed border, italic placeholder text, muted folder icon.
- Filled state: solid border, monospace path text, folder icon at trailing edge.
- `handleConnect`: call `saveGitCredentials(creds)` before `setCredentials(creds)`.

### `git-store.ts` changes

- On `setCollection`: after the existing repo check, call `loadGitCredentials()` and if non-null, call `set({ credentials })`. This means stored credentials are applied automatically when a collection is opened — no dialog needed for subsequent sessions.

---

## Accessibility (WCAG 2.1 AAA)

- The clickable path field is a `<button>` element (not a `div`) so it is natively keyboard-focusable and activatable with Enter/Space.
- Visible focus ring: `focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2` (matches rest of app).
- `aria-label="Select SSH private key file"` on the button.
- `aria-describedby` pointing to the hint text ("Click to select a key file — opens in ~/.ssh/").
- All labels are `<label>` elements properly associated via `htmlFor` / `id`.
- Colour contrast: path text and placeholder text must achieve ≥ 7:1 against the field background (AAA). Use `text-foreground` for filled path and `text-muted-foreground` for placeholder — verify in both light and dark themes.
- No animation on the field itself; folder icon is decorative (`aria-hidden="true"`).
- Error states (e.g. keychain save failure) communicated via `role="alert"` inline message, not colour alone.

---

## Error Handling

| Scenario | Behaviour |
|---|---|
| `get_default_ssh_key_path` returns `null` | Field shows italic placeholder; user must pick a file |
| File picker cancelled | Field unchanged; no error shown |
| `load_git_credentials` fails (keychain locked/unavailable) | Dialog opens with defaults; silent failure (no crash) |
| `save_git_credentials` fails | Show inline `role="alert"` error in dialog; Connect still proceeds with in-memory credentials |
| Selected file does not exist at connect time | Existing libgit2 error surfaces as-is in the Git panel error banner |

---

## What is NOT in scope

- Manual path typing (removed — file picker only).
- Passphrase persistence outside the keychain (passphrase is stored inside the single keychain entry as part of the JSON payload).
- Per-collection credentials (single global credential store for now).
- Keychain unlock UI (OS handles that natively).
