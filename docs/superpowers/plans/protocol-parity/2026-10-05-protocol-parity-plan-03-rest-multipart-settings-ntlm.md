# Protocol parity, Plan 03: Multipart files, request settings and proxy, NTLM

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make multipart file parts and the binary body work from the UI and fail loudly instead of silently dropping files, honor the per-request `encodeUrl` and `maxRedirects` settings, add an app-level proxy setting, and implement NTLM authentication.

**Architecture:** Multipart and binary bodies are fixed in `ReqwestExecutor::apply_body` plus the frontend body mapping and a new `FormDataEditor`. The executor's client construction is refactored once (`ClientKey` for the cache, `ClientBuild` for the builder) so the proxy, the redirect limit and NTLM's single connection fit without a growing positional argument list. The proxy is an app-level setting (`~/.rocket-api/proxy.yml`, password in the OS keychain) held in a shared handle that the executor reads on every request. NTLM message building is pure code in `rocket-http`; the executor performs the three-step handshake on one dedicated connection.

**Tech Stack:** Rust (`rocket-http`, `rocket-infra`, `rocket-app`, `src-tauri`), reqwest multipart and proxy, hyper (test server), md4, React + TypeScript, Vitest.

**Spec:** [../../specs/opencollection-spec-reference.md](../../specs/opencollection-spec-reference.md) (`MultipartFormBody` parts with `contentType`, `FileBody`, request `settings: { encodeUrl, timeout, followRedirects, maxRedirects }`, `AuthNtlm`). Audit summary (verified against the code on 2026-10-05):

- Multipart: `apply_body` (`crates/rocket-infra/src/reqwest_executor.rs:196-208`) builds file parts with `Part::bytes(..).file_name(..)`, ignores `FormDataEntry.content_type`, and wraps both the path validation and the file read in `if ... .is_ok()`, so a missing or unreadable file, or one outside the workspace, is silently dropped and the request goes out without it. Text parts also ignore `content_type`.
- AUDIT CORRECTION (larger than reported): the frontend cannot produce file parts or a binary body at all. `toApiBody` (`src/lib/execute-request.ts:158-170`) hard-codes `entryType: 'text'` for every form row and returns `{ mode, content }` for binary, dropping `BodyState.filePath`. The TS `Body` type has no `filePath` and `FormDataEntry` has no `contentType`. `mapApiRequestToState` (`src/lib/pane-utils.ts:17-31`) discards `entryType`, `contentType` and `filePath` on load, and `BodyEditor` shows the multipart rows with the plain `KeyValueEditor`, which has no file picker. `buildRequestSavePayload` goes through `toApiBody`, so a save also drops them. The backend persistence (`conversions/body.rs`) is already correct. This plan fixes the whole chain.
- `RequestSettings.encodeUrl` is stored and round-trips (`pane-utils.ts:58`, `request-save-mapper.ts:40`) but nothing reads it: `RequestOptions` has no such field and `request_options_from` (`runner_sequence.rs:153`) skips it. `maxRedirects` has the same shape: `RequestOptions.max_redirects` exists, `request_options_from` maps it, but `sendRequest`, `executeRunnerEntry` and `LoadTestDialog` never send it (they send only `followRedirects`, `timeoutMs`, `verifySsl`, each building the options object separately). Any `max_redirects` also forces the executor to build a fresh client per request, because the shared cache is keyed only on redirect and TLS flags.
- AUDIT NOTE on proxy: "no proxy support" is half right. reqwest reads `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` from the environment by default, so a terminal-launched app already honors them. There is no in-app setting, and a desktop-launched app usually has no such variables. The OAuth2 token requests (`ReqwestTokenClientProvider`, the client-credentials fetch inside a send) build their own clients and are NOT covered by the new setting in this plan; they keep reqwest's environment-based default. Call this out in the review.
- NTLM: `apply_auth` returns `InvalidInput("NTLM authentication is not supported yet")` (`reqwest_executor.rs`, `Auth::Ntlm` arm). The UI shows a read-only note and lists NTLM only while a request already uses it (`auth-type-options.ts`).
- SUSPECTED, VERIFY FIRST (Task 2, Step 6): the frontend sends the URL with its query string AND the parsed `queryParams` (`handleUrlChange` fills `queryParams` from the URL, `resolveRequestFieldsForPath` returns both), and the executor appends every enabled `queryParams` entry to the URL, so a query typed in the URL bar looks like it is sent twice. The plan has a reproduction step before the fix.

## Global Constraints

- Never apply `#[serde(rename_all = "camelCase")]` to persistence structs. `ProxySettings` is persisted (`proxy.yml`) so it has no rename attribute; the IPC DTOs in `src-tauri` do.
- Production code never panics on bad input: no `unwrap()` and no `expect()` outside tests.
- Secrets (proxy password, NTLM password, NTLM hashes) never appear in logs or error messages, and the proxy password never reaches `proxy.yml`.
- Always pass `-j4` to `cargo test` and `cargo check`. Never run `cargo test --workspace`; `cargo check -j4 --workspace --tests` is allowed.
- Commit with conventional commits using the `dev-workflow-skills:1-git-commit` skill, staging by explicit path only (several sessions can share this repo; run `git status` first).
- UI: shadcn/ui primitives and `lucide-react` only. `SingleLineEditor` (CodeMirror) for single-line variable-aware fields, Monaco for multi-line. Zustand: narrow selectors only.
- Auth is being worked on elsewhere (flow Auth node, "inherit from parent"). Task 3 adds NTLM to the request and collection auth lists only. Do not touch `src/lib/flow-auth*.ts`, `src/components/flow/**` or `AUTH_NODE_TYPE_OPTIONS`.
- New dependencies: `mime_guess = "2"` in `rocket-infra` (already in `Cargo.lock` through reqwest) and `md4 = "0.10"` in `rocket-http` (new to the lock file, so the first build needs registry access).
- State at the start of this plan, from Plans 01 and 02: `HttpMethod` is not `Copy`; `build_client_with_identity` takes a trailing `cookies: Option<Arc<RepoCookieStore>>` and `get_or_build_client` takes `use_cookies: bool`; `HttpResponse` has `is_binary` and `body_base64`. If the code differs, adapt the edits below to what is there, keeping the intent.

## Review Focus

- A multipart file that cannot be read, that is outside the workspace, or whose content type is not a valid MIME type must fail the request with a message naming the form field, and no request may be sent (Task 1 tests `unreadable_file_part_fails_the_request_and_sends_nothing`, `file_part_outside_the_workspace_is_rejected`, `invalid_part_content_type_names_the_field`).
- A file part sends the entry's `content_type`, else a type guessed from the file extension, else `application/octet-stream` (Task 1 tests).
- `maxRedirects` must reach the backend without making the executor rebuild a client per request (Task 2 tests `max_redirects_does_not_defeat_the_client_cache` and `maxRedirects` mapping in `toApiOptions`).
- Proxy credentials must never be stored in `proxy.yml`, never be echoed in an error, and a URL with embedded credentials must be rejected (Task 2 tests `rejects_credentials_in_the_url`, `password_never_reaches_the_settings_file`).
- NTLM must send message 1 and message 3 on the same TCP connection, send the body only with message 3, and return a plain 401 (not an error) when the credentials are rejected (Task 3 tests `ntlm_handshake_uses_one_connection`, `ntlm_sends_the_body_only_with_message_three`, `wrong_credentials_return_the_401`).

---

## Task 1: Multipart file parts, content types, surfaced errors, and the UI chain

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml` (`mime_guess = "2"`)
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (`apply_body`, `BodyMode::Binary` and `BodyMode::FormData` arms; new test module)
- Modify: `src/types/pane-types.ts` (`KeyValueEntry`), `src/lib/tauri-api.ts` (`Body`, `FormDataEntry`, lines 24-35)
- Modify: `src/lib/execute-request.ts` (`toApiBody`, line 155)
- Modify: `src/lib/pane-utils.ts` (body mapping, lines 17-31)
- Create: `src/components/request/FormDataEditor.tsx`, `src/components/request/__tests__/FormDataEditor.test.tsx`
- Modify: `src/components/request/BodyEditor.tsx` (multipart branch, line 83), `src/lib/curl-generator.ts` (`binary` case)
- Test: `src/lib/__tests__/execute-request.test.ts`, `src/lib/__tests__/pane-utils.test.ts`, `src/lib/__tests__/curl-generator.test.ts`

**Interfaces:**
- Consumes: `rocket_shared::types::{Body, BodyMode, FormDataEntry, FormDataType}` (`FormDataEntry.content_type: Option<String>`), `ReqwestExecutor::with_allowed_base`.
- Produces (Rust): no signature change. Behavior: `apply_body` returns `Err(DomainError::InvalidInput(..))` naming the form field for an unreadable, out-of-workspace or badly typed file part, and for a binary body with no file selected.
- Produces (TS):
  - `KeyValueEntry` gains `entryType?: 'text' | 'file'` and `contentType?: string` (used by multipart rows only).
  - `tauri-api` `Body.filePath?: string`, `FormDataEntry.contentType?: string`.
  - `toApiBody` emits `entryType: 'file'` and `contentType` for multipart rows, and `{ mode: 'binary', filePath }` for a binary body.
  - `FormDataEditor({ entries, onChange, variableContext, onNavigateToSource })`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`: a multipart part is `{ name, type: "text" | "file", value, contentType?, disabled? }` and a file body is `{ type: "file", data: [{ filePath, contentType, selected }] }`.

- [ ] **Step 2: Write the failing executor tests**

Add `mime_guess = "2"` to the `[dependencies]` of `crates/rocket-infra/Cargo.toml`. Append to `crates/rocket-infra/src/reqwest_executor.rs`:

```rust
#[cfg(test)]
mod multipart_tests {
    use super::*;
    use rocket_shared::types::{FormDataEntry, FormDataType, HttpMethod};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn file_entry(key: &str, path: &str, content_type: Option<&str>) -> FormDataEntry {
        FormDataEntry {
            key: key.into(),
            value: path.into(),
            entry_type: FormDataType::File,
            enabled: true,
            content_type: content_type.map(str::to_string),
            description: None,
        }
    }

    fn text_entry(key: &str, value: &str, content_type: Option<&str>) -> FormDataEntry {
        FormDataEntry {
            key: key.into(),
            value: value.into(),
            entry_type: FormDataType::Text,
            enabled: true,
            content_type: content_type.map(str::to_string),
            description: None,
        }
    }

    fn multipart(entries: Vec<FormDataEntry>) -> Body {
        Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(entries),
            file_path: None,
        }
    }

    async fn server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        server
    }

    async fn sent_body(server: &MockServer) -> String {
        let received = server.received_requests().await.expect("recorded");
        String::from_utf8_lossy(&received[0].body).to_ascii_lowercase()
    }

    fn write(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> String {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).expect("write fixture");
        path.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn file_part_uses_the_entry_content_type() {
        let server = server().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(&dir, "a.bin", b"payload");
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![file_entry("doc", &path, Some("application/x-custom"))]));
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let body = sent_body(&server).await;
        assert!(body.contains("name=\"doc\"; filename=\"a.bin\""), "{body}");
        assert!(body.contains("content-type: application/x-custom"), "{body}");
        assert!(body.contains("payload"));
    }

    #[tokio::test]
    async fn file_part_guesses_the_type_from_the_extension_or_falls_back_to_octet_stream() {
        let server = server().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let png = write(&dir, "pic.png", b"png-bytes");
        let blob = write(&dir, "data.zzqq", b"blob-bytes");
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![
            file_entry("img", &png, None),
            file_entry("raw", &blob, Some("  ")),
        ]));
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let body = sent_body(&server).await;
        assert!(body.contains("content-type: image/png"), "{body}");
        assert!(body.contains("content-type: application/octet-stream"), "{body}");
    }

    #[tokio::test]
    async fn text_part_honors_its_content_type() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![
            text_entry("meta", "{\"a\":1}", Some("application/json")),
            text_entry("plain", "hello", None),
        ]));
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let body = sent_body(&server).await;
        assert!(body.contains("content-type: application/json"), "{body}");
        assert!(body.contains("hello"));
    }

    #[tokio::test]
    async fn unreadable_file_part_fails_the_request_and_sends_nothing() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![file_entry("doc", "/definitely/not/here.txt", None)]));
        let err = ReqwestExecutor::new().execute(&req).await.expect_err("must fail");
        let text = err.to_string();
        assert!(text.contains("doc"), "the message must name the field: {text}");
        assert!(
            server.received_requests().await.expect("recorded").is_empty(),
            "no request may go out without the file"
        );
    }

    #[tokio::test]
    async fn file_part_outside_the_workspace_is_rejected() {
        let server = server().await;
        let workspace = tempfile::tempdir().expect("workspace");
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        let path = write(&elsewhere, "secret.txt", b"nope");
        let exec = ReqwestExecutor::with_allowed_base(Arc::new(Mutex::new(
            workspace.path().to_path_buf(),
        )));
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![file_entry("doc", &path, None)]));
        let err = exec.execute(&req).await.expect_err("must fail");
        assert!(err.to_string().contains("outside the workspace"), "{err}");
        assert!(err.to_string().contains("doc"), "{err}");
        assert!(server.received_requests().await.expect("recorded").is_empty());
    }

    #[tokio::test]
    async fn invalid_part_content_type_names_the_field() {
        let server = server().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(&dir, "a.txt", b"x");
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![file_entry("doc", &path, Some("not a mime"))]));
        let err = ReqwestExecutor::new().execute(&req).await.expect_err("must fail");
        assert!(err.to_string().contains("doc"), "{err}");
    }

    #[tokio::test]
    async fn binary_body_without_a_file_is_an_error() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(Body {
            mode: BodyMode::Binary,
            content: None,
            form_data: None,
            file_path: None,
        });
        let err = ReqwestExecutor::new().execute(&req).await.expect_err("must fail");
        assert!(err.to_string().contains("no file"), "{err}");
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -j4 -p rocket-infra multipart_tests`
Expected: FAIL: the content-type tests see no `content-type` on the part, and the error tests get a successful send instead of an error.

- [ ] **Step 4: Implement the executor changes**

In `apply_body` (`reqwest_executor.rs`), replace the `BodyMode::Binary` arm's `if let Some(file_path) = &body.file_path { ... }` so a missing path is an error:

```rust
            BodyMode::Binary => {
                let Some(file_path) = body.file_path.as_deref().filter(|p| !p.trim().is_empty())
                else {
                    return Err(DomainError::InvalidInput(
                        "The binary body has no file selected".into(),
                    ));
                };
                let path = std::path::Path::new(file_path);
                self.validate_file_path(path)?;
                let data = std::fs::read(path)
                    .map_err(|e| DomainError::InvalidInput(format!("Cannot read file {file_path}: {e}")))?;

                if !has_explicit_content_type {
                    // Detect content type from the file extension.
                    let content_type = mime_guess::from_path(path)
                        .first_or_octet_stream()
                        .to_string();
                    builder = builder.header("Content-Type", content_type);
                }
                builder = builder.body(data);
            }
```

and replace the whole `BodyMode::FormData` arm with:

```rust
            BodyMode::FormData => {
                // Multipart form: every part carries its own content type, and a file part that
                // cannot be sent fails the request instead of being dropped.
                if let Some(entries) = &body.form_data {
                    use reqwest::multipart;
                    let mut form = multipart::Form::new();
                    for entry in entries.iter().filter(|e| e.enabled) {
                        let declared = entry
                            .content_type
                            .as_deref()
                            .map(str::trim)
                            .filter(|c| !c.is_empty());
                        let part = match entry.entry_type {
                            rocket_shared::types::FormDataType::File => {
                                let path = std::path::Path::new(&entry.value);
                                self.validate_file_path(path).map_err(|e| {
                                    DomainError::InvalidInput(format!(
                                        "Form field {}: {e}",
                                        entry.key
                                    ))
                                })?;
                                let bytes = std::fs::read(path).map_err(|e| {
                                    DomainError::InvalidInput(format!(
                                        "Form field {}: cannot read file {}: {e}",
                                        entry.key, entry.value
                                    ))
                                })?;
                                let file_name = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                let mime = declared.map(str::to_string).unwrap_or_else(|| {
                                    mime_guess::from_path(path)
                                        .first_or_octet_stream()
                                        .to_string()
                                });
                                multipart::Part::bytes(bytes)
                                    .file_name(file_name)
                                    .mime_str(&mime)
                                    .map_err(|_| {
                                        DomainError::InvalidInput(format!(
                                            "Form field {}: {mime} is not a valid content type",
                                            entry.key
                                        ))
                                    })?
                            }
                            rocket_shared::types::FormDataType::Text => {
                                let part = multipart::Part::text(entry.value.clone());
                                match declared {
                                    Some(mime) => part.mime_str(mime).map_err(|_| {
                                        DomainError::InvalidInput(format!(
                                            "Form field {}: {mime} is not a valid content type",
                                            entry.key
                                        ))
                                    })?,
                                    None => part,
                                }
                            }
                        };
                        form = form.part(entry.key.clone(), part);
                    }
                    builder = builder.multipart(form);
                }
            }
```

- [ ] **Step 5: Run to verify the executor passes**

Run: `cargo test -j4 -p rocket-infra multipart_tests reqwest_executor`
Expected: PASS. The existing `binary_body_applies_content_type_from_extension` test keeps passing: `mime_guess` returns the same types for `.json`, `.xml`, `.png`, `.jpg`, `.gif`, `.pdf`, `.zip`, and `application/octet-stream` for unknown extensions. If a pre-existing test expects `application/xml` for `.xml`, `mime_guess` may return `text/xml`; update that expectation, since either is a valid type.

- [ ] **Step 6: Write the failing frontend tests**

Append to `src/lib/__tests__/execute-request.test.ts`, inside the existing `describe('toApiBody', ...)` block created in Plan 01 (or a new `describe('toApiBody multipart', ...)` if it is absent):

```ts
  it('sends file rows and content types for multipart bodies', () => {
    const out = toApiBody(
      {
        mode: 'formdata',
        content: '',
        formData: [
          { id: '1', key: 'meta', value: '{{v}}', enabled: true, contentType: 'application/json' },
          { id: '2', key: 'doc', value: '/tmp/a.png', enabled: true, entryType: 'file' },
          { id: '3', key: 'off', value: 'x', enabled: false },
        ],
      },
      (s) => s.replace('{{v}}', '1'),
    );
    expect(out).toEqual({
      mode: 'formdata',
      formData: [
        {
          key: 'meta',
          value: '1',
          entryType: 'text',
          enabled: true,
          contentType: 'application/json',
        },
        { key: 'doc', value: '/tmp/a.png', entryType: 'file', enabled: true },
      ],
    });
  });

  it('keeps urlencoded rows as plain text rows', () => {
    const out = toApiBody(
      {
        mode: 'formurlencoded',
        content: '',
        formData: [{ id: '1', key: 'a', value: 'b', enabled: true, entryType: 'file' }],
      },
      (s) => s,
    );
    expect(out?.formData?.[0].entryType).toBe('text');
  });

  it('sends the chosen file of a binary body', () => {
    expect(
      toApiBody(
        { mode: 'binary', content: '', formData: [], filePath: '/tmp/{{n}}.bin', fileName: 'x' },
        (s) => s.replace('{{n}}', 'blob'),
      ),
    ).toEqual({ mode: 'binary', filePath: '/tmp/blob.bin' });
  });
```

Append to `src/lib/__tests__/pane-utils.test.ts` (it already imports `mapApiRequestToState` after Plan 01):

```ts
describe('mapApiRequestToState body', () => {
  const base = {
    uid: 'u',
    name: 'n',
    method: 'POST',
    url: 'https://h.test/up',
    headers: [],
    auth: { authType: 'none' as const },
  };

  it('keeps the type and content type of multipart rows', () => {
    const state = mapApiRequestToState({
      ...base,
      body: {
        mode: 'formdata',
        formData: [
          { key: 'doc', value: '/tmp/a.png', entryType: 'file', enabled: true, contentType: 'image/png' },
        ],
      },
    });
    expect(state.body.formData[0]).toMatchObject({
      key: 'doc',
      entryType: 'file',
      contentType: 'image/png',
    });
  });

  it('restores the file of a binary body and its display name', () => {
    const state = mapApiRequestToState({
      ...base,
      body: { mode: 'binary', filePath: '/tmp/dir/blob.bin' },
    });
    expect(state.body.filePath).toBe('/tmp/dir/blob.bin');
    expect(state.body.fileName).toBe('blob.bin');
  });
});
```

Append to `src/lib/__tests__/curl-generator.test.ts`:

```ts
  it('sends a binary body from its file path', () => {
    const cmd = generateCurlCommand(
      baseResolved({ body: { mode: 'binary', filePath: '/tmp/blob.bin' } }),
      'POST',
    );
    expect(cmd).toContain('--data-binary @/tmp/blob.bin');
  });
```

Create `src/components/request/__tests__/FormDataEditor.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { KeyValueEntry } from '@/types/pane-types';

// CodeMirror does not run in jsdom, so the variable-aware field is replaced by a plain input.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    placeholder,
  }: {
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
  }) => (
    <input aria-label={placeholder} value={value} onChange={(e) => onChange(e.target.value)} />
  ),
}));

const open = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: (...a: unknown[]) => open(...a) }));

import { FormDataEditor } from '../FormDataEditor';

const textRow: KeyValueEntry = { id: '1', key: 'name', value: 'x', enabled: true };
const fileRow: KeyValueEntry = {
  id: '2',
  key: 'doc',
  value: '/tmp/dir/report.pdf',
  enabled: true,
  entryType: 'file',
};

describe('FormDataEditor', () => {
  beforeEach(() => vi.clearAllMocks());

  it('shows a file row with the file name and a text row with an editor', () => {
    render(<FormDataEditor entries={[textRow, fileRow]} onChange={vi.fn()} />);
    expect(screen.getByDisplayValue('x')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Choose file for row 2' })).toHaveTextContent(
      'report.pdf',
    );
  });

  it('switches a row to a file row and clears its value', () => {
    const onChange = vi.fn();
    render(<FormDataEditor entries={[textRow]} onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Field type for row 1: Text' }));
    expect(onChange).toHaveBeenLastCalledWith([{ ...textRow, entryType: 'file', value: '' }]);
  });

  it('stores the path chosen in the dialog', async () => {
    open.mockResolvedValue('/home/me/pic.png');
    const onChange = vi.fn();
    render(<FormDataEditor entries={[{ ...fileRow, value: '' }]} onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Choose file for row 2' }));
    await waitFor(() =>
      expect(onChange).toHaveBeenLastCalledWith([
        { ...fileRow, value: '/home/me/pic.png' },
      ]),
    );
  });

  it('edits the content type of a row', () => {
    const onChange = vi.fn();
    render(<FormDataEditor entries={[fileRow]} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Content type for row 1'), {
      target: { value: 'application/pdf' },
    });
    expect(onChange).toHaveBeenLastCalledWith([{ ...fileRow, contentType: 'application/pdf' }]);
  });

  it('adds an empty text row', () => {
    const onChange = vi.fn();
    render(<FormDataEditor entries={[]} onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: /add field/i }));
    expect(onChange.mock.calls[0][0]).toHaveLength(1);
    expect(onChange.mock.calls[0][0][0]).toMatchObject({ key: '', value: '', enabled: true });
  });
});
```

In the file-row test above the row index in the aria-label is 1-based by position (`row 2` is the second entry), matching how `KeyValueEditor` numbers its rows.

- [ ] **Step 7: Run to verify failure**

Run: `yarn test execute-request pane-utils curl-generator FormDataEditor`
Expected: the new tests FAIL (`FormDataEditor` not found; `toApiBody` sends `entryType: 'text'` and drops `filePath`).

- [ ] **Step 8: Implement the frontend chain**

`src/types/pane-types.ts` `KeyValueEntry`: add

```ts
  /** Multipart rows only: whether `value` is text or the path of a file to upload. */
  entryType?: 'text' | 'file';
  /** Multipart rows only: the part's Content-Type. Empty means "decide automatically". */
  contentType?: string;
```

`src/lib/tauri-api.ts`: add `contentType?: string;` to `FormDataEntry` and `filePath?: string;` to `Body` (after `formData`).

`src/lib/execute-request.ts`: replace `toApiBody` with

```ts
export function toApiBody(body: BodyState, resolve = (s: string) => s): Body | undefined {
  if (body.mode === 'none') return undefined;
  if (body.mode === 'formdata' || body.mode === 'formurlencoded') {
    return {
      mode: body.mode,
      formData: body.formData
        .filter((e) => e.enabled)
        .map((e) => {
          // Only multipart can carry files and part content types.
          const isMultipart = body.mode === 'formdata';
          const contentType = isMultipart ? e.contentType?.trim() : undefined;
          return {
            key: resolve(e.key),
            value: resolve(e.value),
            entryType: isMultipart && e.entryType === 'file' ? ('file' as const) : ('text' as const),
            enabled: e.enabled,
            ...(contentType ? { contentType } : {}),
          };
        }),
    };
  }
  if (body.mode === 'binary') {
    return { mode: 'binary', filePath: body.filePath ? resolve(body.filePath) : undefined };
  }
  return { mode: body.mode as Body['mode'], content: resolve(body.content) };
}
```

`src/lib/pane-utils.ts`: in `mapApiRequestToState`, replace the `body = { ... }` literal with

```ts
    body = {
      mode: req.body.mode as BodyState['mode'],
      content: req.body.content ?? '',
      formData: (req.body.formData ?? []).map((entry) => ({
        id: crypto.randomUUID(),
        key: entry.key,
        value: entry.value,
        enabled: entry.enabled,
        ...(entry.entryType === 'file' ? { entryType: 'file' as const } : {}),
        ...(entry.contentType ? { contentType: entry.contentType } : {}),
      })),
      filePath: req.body.filePath,
      fileName: req.body.filePath?.split(/[\\/]/).pop(),
    };
```

`src/lib/curl-generator.ts` `binary` case: replace its body with

```ts
    case 'binary': {
      const file = body.filePath ?? body.content;
      if (!file) return { lines: [] };
      return { lines: [`--data-binary ${shellQuote(`@${file}`)}`] };
    }
```

Create `src/components/request/FormDataEditor.tsx`:

```tsx
import { open } from '@tauri-apps/plugin-dialog';
import { FileUp, Plus, Type, X } from 'lucide-react';
import { useCallback } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { KeyValueEntry } from '@/types/pane-types';

interface FormDataEditorProps {
  entries: KeyValueEntry[];
  onChange: (entries: KeyValueEntry[]) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

// Multipart rows: each is text or a file, with an optional Content-Type for the part.
export function FormDataEditor({
  entries,
  onChange,
  variableContext,
  onNavigateToSource,
}: FormDataEditorProps) {
  const updateEntry = useCallback(
    (id: string, patch: Partial<KeyValueEntry>) =>
      onChange(entries.map((e) => (e.id === id ? { ...e, ...patch } : e))),
    [entries, onChange],
  );

  const pickFile = useCallback(
    async (id: string) => {
      const result = await open({ multiple: false, title: 'Select file for form field' });
      if (typeof result === 'string') updateEntry(id, { value: result });
    },
    [updateEntry],
  );

  const addEntry = useCallback(
    () => onChange([...entries, { id: crypto.randomUUID(), key: '', value: '', enabled: true }]),
    [entries, onChange],
  );

  return (
    <div className='space-y-2'>
      {entries.map((entry, idx) => {
        const row = idx + 1;
        const isFile = entry.entryType === 'file';
        return (
          <div key={entry.id} className='flex items-center gap-2'>
            <Checkbox
              checked={entry.enabled}
              onCheckedChange={(checked) => updateEntry(entry.id, { enabled: !!checked })}
              aria-label={`${entry.enabled ? 'Disable' : 'Enable'} ${entry.key || 'unnamed'}`}
            />
            <Input
              aria-label={`Key for row ${row}`}
              placeholder='Field name'
              value={entry.key}
              onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
              className='min-w-0 flex-1 font-mono text-xs'
            />
            <Button
              variant='outline'
              size='sm'
              className='h-8 w-16 shrink-0 text-xs'
              aria-label={`Field type for row ${row}: ${isFile ? 'File' : 'Text'}`}
              onClick={() =>
                updateEntry(entry.id, { entryType: isFile ? 'text' : 'file', value: '' })
              }
            >
              {isFile ? (
                <FileUp className='mr-1 h-3 w-3' aria-hidden='true' />
              ) : (
                <Type className='mr-1 h-3 w-3' aria-hidden='true' />
              )}
              {isFile ? 'File' : 'Text'}
            </Button>
            <div className='min-w-0 flex-1'>
              {isFile ? (
                <Button
                  variant='outline'
                  size='sm'
                  className='h-8 w-full justify-start truncate text-xs'
                  aria-label={`Choose file for row ${row}`}
                  onClick={() => pickFile(entry.id)}
                >
                  {entry.value ? (entry.value.split(/[\\/]/).pop() ?? entry.value) : 'Choose file'}
                </Button>
              ) : (
                <SingleLineEditor
                  placeholder='Value'
                  value={entry.value}
                  onChange={(next) => updateEntry(entry.id, { value: next })}
                  className='text-xs'
                  variableContext={variableContext}
                  onNavigateToSource={onNavigateToSource}
                />
              )}
            </div>
            <Input
              aria-label={`Content type for row ${row}`}
              placeholder='auto'
              value={entry.contentType ?? ''}
              onChange={(e) => updateEntry(entry.id, { contentType: e.target.value })}
              className='w-36 shrink-0 font-mono text-xs'
            />
            <Button
              variant='ghost'
              size='icon'
              className='h-7 w-7'
              aria-label={`Remove ${entry.key || 'unnamed'}`}
              onClick={() => onChange(entries.filter((e) => e.id !== entry.id))}
            >
              <X className='h-3.5 w-3.5' />
            </Button>
          </div>
        );
      })}
      <Button variant='ghost' size='sm' onClick={addEntry} className='text-xs'>
        <Plus className='mr-1 h-3.5 w-3.5' />
        Add Field
      </Button>
    </div>
  );
}
```

In the "stores the path chosen in the dialog" test the file row's `Choose file` button text is the placeholder when the value is empty; the accessible name still comes from `aria-label`, so the query works.

`src/components/request/BodyEditor.tsx`: add `import { FormDataEditor } from './FormDataEditor';` and replace the `body.mode === 'formdata'` branch (the `KeyValueEditor` at line 83) with:

```tsx
      {body.mode === 'formdata' && (
        <FormDataEditor
          entries={body.formData}
          onChange={setFormData}
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      )}
```

(`BodyEditor.test.tsx` mocks `@tauri-apps/plugin-dialog`, so the new import is safe there.)

- [ ] **Step 9: Run all checks**

Run: `yarn test execute-request pane-utils curl-generator FormDataEditor BodyEditor request-save-mapper RequestPanel`
Expected: PASS.

Run: `yarn tsc --noEmit && yarn check`
Expected: PASS.

Run: `cargo test -j4 -p rocket-infra multipart_tests && cargo check -j4 --workspace --tests`
Expected: PASS.

Manual check (needs `yarn tauri dev`): create a POST request to `https://httpbin.org/post`, choose Form Data, add a file row, pick a PNG, send, and confirm `files.<field>` in the echo starts with `data:image/png`. Save, close and reopen the request: the file row, its path and any content type must come back. Repeat with a Binary body.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, staging by explicit path (every file listed under Files for this task, plus `Cargo.lock` if it changed). Suggested subject: `fix(http): send multipart files with content types and fail on unreadable files`.

---

## Task 2: encodeUrl, maxRedirects and an app-level proxy setting

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-http/src/proxy.rs`; Modify: `crates/rocket-http/src/lib.rs`, `crates/rocket-http/src/request.rs` (`encode_url`)
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (client refactor, `encodeUrl`, proxy), `crates/rocket-infra/src/lib.rs`, `crates/rocket-infra/src/secret_store.rs` (`new_proxy`)
- Create: `crates/rocket-infra/src/fs_proxy_settings_repo.rs`
- Create: `crates/rocket-app/src/proxy_settings_service.rs`; Modify: `crates/rocket-app/src/lib.rs`, `crates/rocket-app/src/runner_sequence.rs` (`request_options_from`, line 153)
- Create: `src-tauri/src/commands/proxy.rs`; Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` (executor at line 321, `manage` at about line 505, handler list at about line 624)
- Modify: `src/lib/tauri-api.ts` (`RequestOptions`, proxy commands), `src/lib/execute-request.ts` (`toApiOptions`, sendRequest options, query handling), `src/lib/runner-execute.ts`, `src/components/request/LoadTestDialog.tsx`
- Create: `src/components/settings/ProxySettingsDialog.tsx`, `src/components/settings/__tests__/ProxySettingsDialog.test.tsx`; Modify: `src/components/title-bar/TitleBar.tsx`
- Test: `src/lib/__tests__/execute-request.test.ts`, `src/components/request/__tests__/LoadTestDialog.test.tsx`, `src/lib/__tests__/runner-execute.test.ts`

**Interfaces:**
- Produces (`rocket-http`):
  - `RequestOptions.encode_url: bool` (serde default `true`).
  - `proxy::{ProxyMode, ProxySettings, ResolvedProxy, SharedProxy, ProxySettingsRepository, new_shared_proxy}`: `ProxyMode` is `System` (default), `None` or `Custom`; `ProxySettings { mode, http_proxy: Option<String>, https_proxy: Option<String>, no_proxy: Option<String>, username: Option<String> }` with `validate(&self) -> DomainResult<()>`; `ResolvedProxy { settings: ProxySettings, password: Option<Zeroizing<String>>, generation: u64 }` (`Clone`, `Default`, redacting `Debug`); `SharedProxy = Arc<RwLock<ResolvedProxy>>`; `ProxySettingsRepository { fn load(&self) -> DomainResult<ProxySettings>; fn save(&self, &ProxySettings) -> DomainResult<()> }`.
- Produces (`rocket-infra`): `FsProxySettingsRepo::new(path: PathBuf)`, `KeyringSecretStore::new_proxy()`, `ReqwestExecutor::with_proxy(self, shared: SharedProxy) -> Self`, and internal `ClientKey` / `ClientBuild` / `build_client(ClientBuild)`.
- Produces (`rocket-app`): `ProxySettingsService::new(repo: Box<dyn ProxySettingsRepository>, secrets: Arc<dyn SecretStore>, shared: SharedProxy) -> Self` (infallible), `get(&self) -> DomainResult<ProxySettingsView>`, `save(&self, settings: ProxySettings, password: PasswordChange) -> DomainResult<()>`; `PasswordChange { Keep, Clear, Set(String) }`; `ProxySettingsView { settings: ProxySettings, has_password: bool }`.
- Produces (IPC): commands `get_proxy_settings` and `save_proxy_settings` (`settings`, `password: { action: 'keep' | 'clear' | 'set', value? }`).
- Produces (TS): `toApiOptions(settings: RequestSettings | undefined): RequestOptions`; `RequestOptions.maxRedirects?`, `RequestOptions.encodeUrl?`; `getProxySettings`, `saveProxySettings`; `ProxySettingsDialog({ open, onOpenChange })`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`: `settings.encodeUrl`, `followRedirects` and `maxRedirects` are `bool | number | "inherit"`; `"inherit"` falls back to the default.

- [ ] **Step 2: Refactor the client construction (behavior-preserving)**

This step changes structure only. The existing executor tests must stay green before and after.

Run first: `cargo test -j4 -p rocket-infra reqwest_executor`
Expected: PASS (baseline).

In `crates/rocket-infra/src/reqwest_executor.rs` add, above `impl ReqwestExecutor`:

```rust
/// What decides which cached client serves a request. Everything else (headers, body,
/// query, timeout, auth) is applied per request on the request builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ClientKey {
    follow_redirects: bool,
    verify_ssl: bool,
    use_cookies: bool,
    max_redirects: Option<u32>,
    /// Bumped by the proxy service on every save, so a changed proxy never reuses an old client.
    proxy_generation: u64,
}

impl ClientKey {
    fn plain(follow_redirects: bool, verify_ssl: bool) -> Self {
        Self {
            follow_redirects,
            verify_ssl,
            use_cookies: false,
            max_redirects: None,
            proxy_generation: 0,
        }
    }
}

/// Everything needed to build one client.
struct ClientBuild {
    key: ClientKey,
    identity: Option<ClientIdentity>,
    cookies: Option<Arc<RepoCookieStore>>,
    proxy: rocket_http::ResolvedProxy,
    /// One idle connection and HTTP/1 only, for handshakes that belong to a connection (NTLM).
    single_connection: bool,
}

impl ClientBuild {
    fn plain(follow_redirects: bool, verify_ssl: bool, max_redirects: Option<u32>) -> Self {
        let mut key = ClientKey::plain(follow_redirects, verify_ssl);
        key.max_redirects = max_redirects;
        Self {
            key,
            identity: None,
            cookies: None,
            proxy: rocket_http::ResolvedProxy::default(),
            single_connection: false,
        }
    }
}
```

Change the cache to `clients: Mutex<HashMap<ClientKey, Client>>`, add the field `proxy: Option<rocket_http::SharedProxy>` (initialize `None` in both constructors) and this method:

```rust
    /// Reads requests through `shared`, which the proxy service updates when settings change.
    pub fn with_proxy(mut self, shared: rocket_http::SharedProxy) -> Self {
        self.proxy = Some(shared);
        self
    }

    fn current_proxy(&self) -> rocket_http::ResolvedProxy {
        match &self.proxy {
            // A poisoned lock only means a writer panicked; the value is still a whole proxy.
            Some(shared) => shared.read().unwrap_or_else(|e| e.into_inner()).clone(),
            None => rocket_http::ResolvedProxy::default(),
        }
    }
```

Replace `get_or_build_client` with:

```rust
    fn get_or_build_client(
        &self,
        key: ClientKey,
        cookies: Option<Arc<RepoCookieStore>>,
        proxy: rocket_http::ResolvedProxy,
    ) -> DomainResult<Client> {
        // The cache only holds clients, so a poisoned lock is safe to recover.
        let mut cache = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = cache.get(&key) {
            // reqwest::Client::clone is cheap, it is an Arc inside.
            return Ok(c.clone());
        }
        let client = build_client(ClientBuild {
            key,
            identity: None,
            cookies,
            proxy,
            single_connection: false,
        })?;
        cache.insert(key, client.clone());
        Ok(client)
    }
```

Replace `build_client_impl` and `build_client_with_identity` with one function (keep `build_client_impl` as a thin wrapper because tests call it):

```rust
fn build_client_impl(
    follow_redirects: bool,
    verify_ssl: bool,
    max_redirects: Option<u32>,
) -> DomainResult<Client> {
    build_client(ClientBuild::plain(follow_redirects, verify_ssl, max_redirects))
}

/// Builds a client, presenting the identity as the TLS client certificate when there is one.
///
/// A client offers its identity to every host it connects to, so with an identity the redirect
/// policy stops at a redirect that leaves the certificate's domain. The 3xx response is then
/// returned, and the user can send the request to the new host on purpose.
fn build_client(spec: ClientBuild) -> DomainResult<Client> {
    let ClientBuild {
        key,
        identity,
        cookies,
        proxy,
        single_connection,
    } = spec;
    let limit = key.max_redirects.unwrap_or(10) as usize;
    let redirect_policy = if !key.follow_redirects {
        redirect::Policy::none()
    } else if let Some(scope) = identity.as_ref().map(|i| i.certificate.clone()) {
        redirect::Policy::custom(move |attempt| {
            // A custom policy replaces the limit, so it is checked here like `limited` does.
            if attempt.previous().len() > limit {
                attempt.error("too many redirects")
            } else if rocket_http::client_cert::certificate_covers(&scope, attempt.url().as_str()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        })
    } else {
        redirect::Policy::limited(limit)
    };

    let mut builder = Client::builder()
        .redirect(redirect_policy)
        .danger_accept_invalid_certs(!key.verify_ssl);
    if let Some(identity) = identity {
        builder = builder.identity(identity.identity);
    }
    if let Some(store) = cookies {
        builder = builder.cookie_provider(store);
    }
    if single_connection {
        builder = builder.pool_max_idle_per_host(1).http1_only();
    }
    builder = apply_proxy(builder, &proxy)?;
    builder
        .build()
        .map_err(|e| DomainError::Http(e.to_string()))
}

/// Applies the app's proxy setting. `System` leaves reqwest's default, which reads the
/// `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` environment variables.
fn apply_proxy(
    builder: reqwest::ClientBuilder,
    proxy: &rocket_http::ResolvedProxy,
) -> DomainResult<reqwest::ClientBuilder> {
    use rocket_http::ProxyMode;
    match proxy.settings.mode {
        ProxyMode::System => Ok(builder),
        ProxyMode::None => Ok(builder.no_proxy()),
        ProxyMode::Custom => {
            // The custom proxies replace the environment ones.
            let mut builder = builder.no_proxy();
            let no_proxy = proxy
                .settings
                .no_proxy
                .as_deref()
                .and_then(reqwest::NoProxy::from_string);
            let entries = [
                ("HTTP", proxy.settings.http_proxy.as_deref(), false),
                ("HTTPS", proxy.settings.https_proxy.as_deref(), true),
            ];
            for (label, url, https) in entries {
                let Some(url) = url.map(str::trim).filter(|u| !u.is_empty()) else {
                    continue;
                };
                // The URL is never echoed: it is user input and could hold credentials.
                let created = if https {
                    reqwest::Proxy::https(url)
                } else {
                    reqwest::Proxy::http(url)
                };
                let mut p = created.map_err(|_| {
                    DomainError::InvalidInput(format!("The {label} proxy URL is not valid"))
                })?;
                if let (Some(user), Some(password)) =
                    (proxy.settings.username.as_deref(), proxy.password.as_ref())
                {
                    p = p.basic_auth(user, password.as_str());
                }
                builder = builder.proxy(p.no_proxy(no_proxy.clone()));
            }
            Ok(builder)
        }
    }
}
```

Update the callers: `ReqwestTokenClientProvider::client_for` becomes

```rust
        let identity = identity_for_url(certificates, token_url)?;
        let mut spec = ClientBuild::plain(true, verify_ssl, None);
        spec.identity = identity;
        build_client(spec)
```

and `fetch_client_credentials_token` builds its client the same way (`let mut spec = ClientBuild::plain(true, verify_ssl, None); spec.identity = identity; build_client(spec).map_err(...)`). In `execute`, replace the client selection with:

```rust
        let cookies = if request.options.use_cookie_jar {
            self.cookie_store.clone()
        } else {
            None
        };
        let proxy = self.current_proxy();
        let key = ClientKey {
            follow_redirects: request.options.follow_redirects,
            verify_ssl: request.options.verify_ssl,
            use_cookies: cookies.is_some(),
            max_redirects: request.options.max_redirects,
            proxy_generation: proxy.generation,
        };
        let client = if identity.is_some() {
            // The shared client cache is keyed without an identity, so this gets its own client.
            build_client(ClientBuild {
                key,
                identity,
                cookies,
                proxy,
                single_connection: false,
            })?
        } else {
            self.get_or_build_client(key, cookies, proxy)?
        };
```

Update the existing tests that call `get_or_build_client`: replace each `exec.get_or_build_client(a, b, true)` with `exec.get_or_build_client(ClientKey::plain(a, b), None, rocket_http::ResolvedProxy::default())`. This step needs `rocket_http::ResolvedProxy`, which Step 3 creates, so do Step 3 before compiling.

- [ ] **Step 3: Write the failing proxy domain tests**

Create `crates/rocket-http/src/proxy.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn custom(http: Option<&str>, https: Option<&str>) -> ProxySettings {
        ProxySettings {
            mode: ProxyMode::Custom,
            http_proxy: http.map(str::to_string),
            https_proxy: https.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn system_and_none_always_validate() {
        assert!(ProxySettings::default().validate().is_ok());
        assert!(ProxySettings {
            mode: ProxyMode::None,
            ..Default::default()
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn custom_needs_at_least_one_url() {
        assert!(custom(None, None).validate().is_err());
        assert!(custom(Some("  "), None).validate().is_err());
        assert!(custom(Some("http://proxy.corp:8080"), None).validate().is_ok());
        assert!(custom(None, Some("https://proxy.corp:8443")).validate().is_ok());
    }

    #[test]
    fn rejects_credentials_in_the_url_without_echoing_them() {
        let err = custom(Some("http://user:hunter2@proxy.corp:8080"), None)
            .validate()
            .expect_err("credentials in the URL");
        let text = err.to_string();
        assert!(text.contains("username"), "{text}");
        assert!(!text.contains("hunter2") && !text.contains("proxy.corp"), "{text}");
    }

    #[test]
    fn rejects_unsupported_schemes_and_junk() {
        for bad in ["socks5://proxy.corp:1080", "proxy.corp:8080", "not a url", "ftp://p:1"] {
            assert!(custom(Some(bad), None).validate().is_err(), "{bad}");
        }
    }

    #[test]
    fn debug_never_prints_the_password() {
        let resolved = ResolvedProxy {
            settings: ProxySettings::default(),
            password: Some(zeroize::Zeroizing::new("hunter2".to_string())),
            generation: 3,
        };
        let shown = format!("{resolved:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
    }

    #[test]
    fn persistence_format_is_snake_case_and_skips_empty_values() {
        let yaml = serde_yaml::to_string(&custom(Some("http://p:1"), None)).expect("serialize");
        assert!(yaml.contains("mode: custom"), "{yaml}");
        assert!(yaml.contains("http_proxy: http://p:1"), "{yaml}");
        assert!(!yaml.contains("https_proxy"), "{yaml}");
        assert!(!yaml.contains("password"), "{yaml}");
        let back: ProxySettings = serde_yaml::from_str("mode: none\n").expect("minimal file");
        assert_eq!(back.mode, ProxyMode::None);
    }
}
```

`rocket-http` has no `serde_yaml` dependency: add `serde_yaml.workspace = true` under `[dev-dependencies]` in `crates/rocket-http/Cargo.toml` (create the section if absent).

Add to `crates/rocket-http/src/request.rs` tests:

```rust
    #[test]
    fn encode_url_defaults_to_true_and_can_be_turned_off() {
        let on: RequestOptions = serde_json::from_str("{}").expect("deserialize");
        assert!(on.encode_url);
        let off: RequestOptions = serde_json::from_str(r#"{"encodeUrl":false}"#).expect("deserialize");
        assert!(!off.encode_url);
    }
```

Run: `cargo test -j4 -p rocket-http proxy request`
Expected: FAIL to compile (types missing).

- [ ] **Step 4: Implement the proxy domain types and `encode_url`**

Prepend to `crates/rocket-http/src/proxy.rs`:

```rust
//! The app-level proxy setting. Pure data and validation; the executor in `rocket-infra`
//! applies it and `rocket-app` owns the use case of changing it.

use std::fmt;
use std::sync::{Arc, RwLock};

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Where requests go through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    /// reqwest's default: the `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` environment variables.
    #[default]
    System,
    /// Always connect directly.
    None,
    /// Use the URLs below.
    Custom,
}

/// The persisted proxy setting (`proxy.yml`). It never holds the password: that lives in the OS
/// keychain, so this file is safe to read, back up and share.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProxySettings {
    #[serde(default)]
    pub mode: ProxyMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_proxy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub https_proxy: Option<String>,
    /// Comma-separated hosts that bypass the proxy, in the `NO_PROXY` format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_proxy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

impl ProxySettings {
    /// Checks the custom URLs. Errors never repeat the URL, because it is user input.
    pub fn validate(&self) -> DomainResult<()> {
        if self.mode != ProxyMode::Custom {
            return Ok(());
        }
        let urls = [
            ("HTTP", self.http_proxy.as_deref()),
            ("HTTPS", self.https_proxy.as_deref()),
        ];
        let mut any = false;
        for (label, url) in urls {
            let Some(url) = url.map(str::trim).filter(|u| !u.is_empty()) else {
                continue;
            };
            any = true;
            let parsed = reqwest::Url::parse(url).map_err(|_| {
                DomainError::InvalidInput(format!(
                    "The {label} proxy URL is not valid. Use http://host:port or https://host:port"
                ))
            })?;
            if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
                return Err(DomainError::InvalidInput(format!(
                    "The {label} proxy URL must start with http:// or https://"
                )));
            }
            if !parsed.username().is_empty() || parsed.password().is_some() {
                return Err(DomainError::InvalidInput(format!(
                    "The {label} proxy URL must not contain a username or password. \
                     Use the username and password fields instead"
                )));
            }
        }
        if any {
            Ok(())
        } else {
            Err(DomainError::InvalidInput(
                "A custom proxy needs an HTTP or an HTTPS proxy URL".into(),
            ))
        }
    }
}

/// The setting as the executor uses it: the persisted values plus the password from the
/// keychain. Never serialized.
#[derive(Clone, Default)]
pub struct ResolvedProxy {
    pub settings: ProxySettings,
    pub password: Option<Zeroizing<String>>,
    /// Bumped on every change, so clients built for an older setting are never reused.
    pub generation: u64,
}

impl fmt::Debug for ResolvedProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedProxy")
            .field("settings", &self.settings)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("generation", &self.generation)
            .finish()
    }
}

/// The handle the executor reads on every request and the proxy service updates.
pub type SharedProxy = Arc<RwLock<ResolvedProxy>>;

pub fn new_shared_proxy() -> SharedProxy {
    Arc::new(RwLock::new(ResolvedProxy::default()))
}

/// Persistence of the non-secret part of the setting.
pub trait ProxySettingsRepository: Send + Sync {
    /// A missing file is the default setting (`System`), not an error.
    fn load(&self) -> DomainResult<ProxySettings>;
    fn save(&self, settings: &ProxySettings) -> DomainResult<()>;
}
```

In `crates/rocket-http/src/lib.rs` add `pub mod proxy;` and `pub use proxy::{new_shared_proxy, ProxyMode, ProxySettings, ProxySettingsRepository, ResolvedProxy, SharedProxy};`. In `request.rs` add to `RequestOptions`:

```rust
    /// Percent-encode the query parameters of the params table. Off sends them as typed,
    /// which is what a user who pre-encoded a value wants. On by default.
    #[serde(default = "default_true")]
    pub encode_url: bool,
```

and `encode_url: true,` in `Default`.

Run: `cargo test -j4 -p rocket-http proxy request`
Expected: PASS.

- [ ] **Step 5: Verify the refactor is behavior-preserving**

Run: `cargo test -j4 -p rocket-infra reqwest_executor`
Expected: PASS, same count as the baseline in Step 2. If `executor_builds_different_clients_for_different_options` (four cache entries) fails, the `ClientKey::plain` calls in the updated tests must differ only in `follow_redirects` and `verify_ssl`.

- [ ] **Step 6: Reproduce the suspected duplicate query before fixing it**

Needs `yarn tauri dev` and network access. Send `GET https://httpbin.org/get?a=1` from the UI and read `args` in the response.
- If `args` is `{"a": "1"}`: there is no duplicate in practice. Record that in the commit message, skip Step 7's `duplicate query` test and its implementation, and keep everything else.
- If `args` is `{"a": ["1", "1"]}`: the duplicate is real; continue with the steps below.

- [ ] **Step 7: Write the failing settings tests**

Append to the tests of `crates/rocket-app/src/runner_sequence.rs`:

```rust
    #[test]
    fn step_input_maps_encode_url() {
        use rocket_shared::types::{RequestSettingValue, RequestSettings};

        let mut request = req("Login", "login.yml");
        request.settings = Some(RequestSettings {
            encode_url: Some(RequestSettingValue::Value(false)),
            timeout: None,
            follow_redirects: None,
            max_redirects: None,
            verify_ssl: None,
        });
        let item = RunItem {
            name: request.name.clone(),
            request_path: "login.yml".into(),
            request,
        };
        let input = build_step_input(
            &item,
            "my-api",
            None,
            None,
            rocket_workspace::RequestGuardPolicy::default(),
        );
        assert!(!input.options.encode_url);

        let inherit = RunItem {
            name: "x".into(),
            request_path: "x.yml".into(),
            request: req("x", "x.yml"),
        };
        let input = build_step_input(
            &inherit,
            "my-api",
            None,
            None,
            rocket_workspace::RequestGuardPolicy::default(),
        );
        assert!(input.options.encode_url, "no setting means encoded, the default");
    }
```

Append to `crates/rocket-infra/src/reqwest_executor.rs`:

```rust
#[cfg(test)]
mod encode_url_tests {
    use super::*;
    use rocket_shared::types::{HttpMethod, QueryParam};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn param(key: &str, value: &str) -> QueryParam {
        QueryParam {
            key: key.into(),
            value: value.into(),
            enabled: true,
            description: None,
        }
    }

    async fn query_sent(encode_url: bool, url_path: &str, params: Vec<QueryParam>) -> String {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}{url_path}", server.uri()));
        req.query_params = params;
        req.options.encode_url = encode_url;
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let received = server.received_requests().await.expect("recorded");
        received[0].url.query().unwrap_or_default().to_string()
    }

    #[tokio::test]
    async fn encoded_by_default() {
        let q = query_sent(true, "/p", vec![param("q", "a+b/c:d"), param("r", "50%25")]).await;
        assert_eq!(q, "q=a%2Bb%2Fc%3Ad&r=50%2525");
    }

    #[tokio::test]
    async fn sent_as_typed_when_encoding_is_off() {
        let q = query_sent(false, "/p", vec![param("q", "a+b/c:d"), param("r", "50%25")]).await;
        assert_eq!(q, "q=a+b/c:d&r=50%25");
    }

    #[tokio::test]
    async fn the_urls_own_query_is_kept_in_both_modes() {
        let on = query_sent(true, "/p?x=1", vec![param("y", "2")]).await;
        let off = query_sent(false, "/p?x=1", vec![param("y", "2")]).await;
        assert_eq!(on, "x=1&y=2");
        assert_eq!(off, "x=1&y=2");
    }

    #[tokio::test]
    async fn max_redirects_does_not_defeat_the_client_cache() {
        let exec = ReqwestExecutor::new();
        let key = ClientKey {
            max_redirects: Some(5),
            ..ClientKey::plain(true, true)
        };
        let proxy = rocket_http::ResolvedProxy::default();
        exec.get_or_build_client(key, None, proxy.clone()).expect("first");
        exec.get_or_build_client(key, None, proxy.clone()).expect("second");
        assert_eq!(exec.cache_len(), 1, "the same redirect limit must reuse one client");
        let other = ClientKey {
            max_redirects: Some(2),
            ..key
        };
        exec.get_or_build_client(other, None, proxy).expect("other limit");
        assert_eq!(exec.cache_len(), 2);
    }
}
```

Append a proxy test module to the same file:

```rust
#[cfg(test)]
mod proxy_tests {
    use super::*;
    use rocket_http::{ProxyMode, ProxySettings, ResolvedProxy};
    use rocket_shared::types::HttpMethod;
    use std::sync::RwLock;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn shared(settings: ProxySettings, password: Option<&str>) -> rocket_http::SharedProxy {
        Arc::new(RwLock::new(ResolvedProxy {
            settings,
            password: password.map(|p| zeroize::Zeroizing::new(p.to_string())),
            generation: 1,
        }))
    }

    async fn proxy_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("via proxy"))
            .mount(&server)
            .await;
        server
    }

    fn custom(uri: &str) -> ProxySettings {
        ProxySettings {
            mode: ProxyMode::Custom,
            http_proxy: Some(uri.to_string()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn custom_proxy_carries_the_request() {
        let proxy = proxy_server().await;
        let exec = ReqwestExecutor::new().with_proxy(shared(custom(&proxy.uri()), None));
        let mut req = HttpRequest::new(HttpMethod::Get, "http://upstream.invalid/x");
        req.options.timeout_ms = 5_000;
        let response = exec.execute(&req).await.expect("through the proxy");
        assert_eq!(response.body, "via proxy");
    }

    #[tokio::test]
    async fn proxy_credentials_are_sent_as_basic_auth() {
        let proxy = proxy_server().await;
        let mut settings = custom(&proxy.uri());
        settings.username = Some("u".into());
        let exec = ReqwestExecutor::new().with_proxy(shared(settings, Some("p")));
        let mut req = HttpRequest::new(HttpMethod::Get, "http://upstream.invalid/x");
        req.options.timeout_ms = 5_000;
        exec.execute(&req).await.expect("through the proxy");
        let received = proxy.received_requests().await.expect("recorded");
        let auth = received[0]
            .headers
            .get("proxy-authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert_eq!(auth, "Basic dTpw", "base64 of u:p");
    }

    #[tokio::test]
    async fn no_proxy_hosts_bypass_the_proxy() {
        let proxy = proxy_server().await;
        let mut settings = custom(&proxy.uri());
        settings.no_proxy = Some("upstream.invalid".into());
        let exec = ReqwestExecutor::new().with_proxy(shared(settings, None));
        let mut req = HttpRequest::new(HttpMethod::Get, "http://upstream.invalid/x");
        req.options.timeout_ms = 3_000;
        let err = exec.execute(&req).await.expect_err("direct connection must fail");
        assert!(matches!(err, DomainError::Http(_)), "{err}");
        assert!(proxy.received_requests().await.expect("recorded").is_empty());
    }

    #[tokio::test]
    async fn changing_the_setting_changes_the_client() {
        let proxy = proxy_server().await;
        let handle = shared(ProxySettings::default(), None);
        let exec = ReqwestExecutor::new().with_proxy(Arc::clone(&handle));
        let mut req = HttpRequest::new(HttpMethod::Get, "http://upstream.invalid/x");
        req.options.timeout_ms = 3_000;
        assert!(exec.execute(&req).await.is_err(), "system mode, no proxy: unreachable host");
        {
            let mut write = handle.write().expect("lock");
            write.settings = custom(&proxy.uri());
            write.generation += 1;
        }
        let response = exec.execute(&req).await.expect("now through the proxy");
        assert_eq!(response.body, "via proxy");
    }

    #[tokio::test]
    async fn an_invalid_proxy_url_fails_without_echoing_it() {
        let exec = ReqwestExecutor::new().with_proxy(shared(custom("http://user:hunter2@"), None));
        let req = HttpRequest::new(HttpMethod::Get, "http://upstream.invalid/x");
        let err = exec.execute(&req).await.expect_err("must fail");
        assert!(!err.to_string().contains("hunter2"), "{err}");
    }
}
```

The proxy tests assume the test environment has no `HTTP_PROXY` or `ALL_PROXY` variable set (`changing_the_setting_changes_the_client` expects `System` mode to reach no proxy) and that `*.invalid` names do not resolve.

Run: `cargo test -j4 -p rocket-infra encode_url_tests proxy_tests` and `cargo test -j4 -p rocket-app step_input_maps_encode_url`
Expected: FAIL (query is always encoded; `encode_url` is not mapped; the proxy tests fail until `proxy_server` requests really go through the proxy, which they already can after Step 2, so `custom_proxy_carries_the_request` may already pass, while the credentials and generation tests depend on the same code and should pass too: if all proxy tests pass here, that confirms Step 2's `apply_proxy`).

- [ ] **Step 8: Implement `encodeUrl` and map the setting**

In `execute` (`reqwest_executor.rs`), replace the block that appends the enabled query parameters with:

```rust
        // Merge enabled query params into the URL.
        let mut url = reqwest::Url::parse(&request.url)
            .map_err(|e| DomainError::InvalidInput(format!("Invalid URL: {e}")))?;
        {
            let enabled: Vec<_> = request.query_params.iter().filter(|p| p.enabled).collect();
            // Only touch the query when there are params; query_pairs_mut with no appends
            // sets an empty query string and produces a trailing '?'.
            if !enabled.is_empty() {
                if request.options.encode_url {
                    let mut pairs = url.query_pairs_mut();
                    for p in enabled {
                        pairs.append_pair(&p.key, &p.value);
                    }
                } else {
                    // As typed: reserved characters and existing %-escapes are kept. The URL
                    // parser still escapes what a URL cannot hold, such as a space.
                    let extra = enabled
                        .iter()
                        .map(|p| format!("{}={}", p.key, p.value))
                        .collect::<Vec<_>>()
                        .join("&");
                    let query = match url.query() {
                        Some(existing) if !existing.is_empty() => format!("{existing}&{extra}"),
                        _ => extra,
                    };
                    url.set_query(Some(&query));
                }
            }
        }
```

In `crates/rocket-app/src/runner_sequence.rs` `request_options_from`, add before `options`:

```rust
    if let Some(RequestSettingValue::Value(v)) = settings.encode_url.as_ref() {
        options.encode_url = *v;
    }
```

Run: `cargo test -j4 -p rocket-infra encode_url_tests proxy_tests reqwest_executor` and `cargo test -j4 -p rocket-app runner_sequence`
Expected: PASS.

- [ ] **Step 9: Write the failing proxy storage and service tests**

Create `crates/rocket-infra/src/fs_proxy_settings_repo.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::{ProxyMode, ProxySettings};

    #[test]
    fn a_missing_file_is_the_default_setting() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = FsProxySettingsRepo::new(dir.path().join("proxy.yml"));
        assert_eq!(repo.load().expect("load"), ProxySettings::default());
    }

    #[test]
    fn settings_round_trip_and_never_contain_a_password() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("proxy.yml");
        let repo = FsProxySettingsRepo::new(path.clone());
        let settings = ProxySettings {
            mode: ProxyMode::Custom,
            http_proxy: Some("http://p:8080".into()),
            https_proxy: None,
            no_proxy: Some("localhost".into()),
            username: Some("u".into()),
        };
        repo.save(&settings).expect("save");
        assert_eq!(repo.load().expect("load"), settings);
        let text = std::fs::read_to_string(path).expect("read");
        assert!(!text.to_ascii_lowercase().contains("password"), "{text}");
    }

    #[test]
    fn a_corrupt_file_is_an_error_not_a_silent_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("proxy.yml");
        std::fs::write(&path, "mode: [oops").expect("write");
        assert!(FsProxySettingsRepo::new(path).load().is_err());
    }
}
```

Create `crates/rocket-app/src/proxy_settings_service.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::SecretStore;
    use rocket_http::{new_shared_proxy, ProxyMode};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemSecrets(Mutex<HashMap<(String, String), String>>);

    impl SecretStore for MemSecrets {
        fn get(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
            Ok(self
                .0
                .lock()
                .expect("lock")
                .get(&(scope.to_string(), key.to_string()))
                .cloned())
        }
        fn set(&self, scope: &str, key: &str, value: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock")
                .insert((scope.to_string(), key.to_string()), value.to_string());
            Ok(())
        }
        fn delete(&self, scope: &str, key: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock")
                .remove(&(scope.to_string(), key.to_string()));
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemRepo(Mutex<ProxySettings>);

    impl ProxySettingsRepository for MemRepo {
        fn load(&self) -> DomainResult<ProxySettings> {
            Ok(self.0.lock().expect("lock").clone())
        }
        fn save(&self, settings: &ProxySettings) -> DomainResult<()> {
            *self.0.lock().expect("lock") = settings.clone();
            Ok(())
        }
    }

    fn custom() -> ProxySettings {
        ProxySettings {
            mode: ProxyMode::Custom,
            http_proxy: Some("http://p:8080".into()),
            username: Some("u".into()),
            ..Default::default()
        }
    }

    fn service() -> (ProxySettingsService, rocket_http::SharedProxy, Arc<MemSecrets>) {
        let shared = new_shared_proxy();
        let secrets = Arc::new(MemSecrets::default());
        let svc = ProxySettingsService::new(
            Box::new(MemRepo::default()),
            Arc::clone(&secrets) as Arc<dyn SecretStore>,
            Arc::clone(&shared),
        );
        (svc, shared, secrets)
    }

    #[test]
    fn save_updates_the_shared_handle_and_bumps_the_generation() {
        let (svc, shared, _) = service();
        let before = shared.read().expect("lock").generation;
        svc.save(custom(), PasswordChange::Set("pw".into())).expect("save");
        let now = shared.read().expect("lock");
        assert_eq!(now.settings.mode, ProxyMode::Custom);
        assert_eq!(now.password.as_ref().map(|p| p.as_str()), Some("pw"));
        assert!(now.generation > before);
    }

    #[test]
    fn the_password_goes_to_the_secret_store_only() {
        let (svc, _, secrets) = service();
        svc.save(custom(), PasswordChange::Set("pw".into())).expect("save");
        assert_eq!(
            secrets.get("proxy", "password").expect("get").as_deref(),
            Some("pw")
        );
        let view = svc.get().expect("get");
        assert!(view.has_password);
        assert_eq!(view.settings, custom());
    }

    #[test]
    fn keep_leaves_the_password_and_clear_removes_it() {
        let (svc, shared, secrets) = service();
        svc.save(custom(), PasswordChange::Set("pw".into())).expect("save");
        svc.save(custom(), PasswordChange::Keep).expect("keep");
        assert_eq!(secrets.get("proxy", "password").expect("get").as_deref(), Some("pw"));
        assert!(shared.read().expect("lock").password.is_some());
        svc.save(custom(), PasswordChange::Clear).expect("clear");
        assert_eq!(secrets.get("proxy", "password").expect("get"), None);
        assert!(shared.read().expect("lock").password.is_none());
        assert!(!svc.get().expect("get").has_password);
    }

    #[test]
    fn invalid_settings_change_nothing() {
        let (svc, shared, secrets) = service();
        let bad = ProxySettings {
            mode: ProxyMode::Custom,
            ..Default::default()
        };
        assert!(svc.save(bad, PasswordChange::Set("pw".into())).is_err());
        assert_eq!(secrets.get("proxy", "password").expect("get"), None);
        assert_eq!(shared.read().expect("lock").settings.mode, ProxyMode::System);
    }

    #[test]
    fn startup_publishes_what_was_saved_before() {
        let shared = new_shared_proxy();
        let secrets = Arc::new(MemSecrets::default());
        secrets.set("proxy", "password", "pw").expect("seed");
        let repo = MemRepo::default();
        repo.save(&custom()).expect("seed");
        let _svc = ProxySettingsService::new(
            Box::new(repo),
            secrets as Arc<dyn SecretStore>,
            Arc::clone(&shared),
        );
        let now = shared.read().expect("lock");
        assert_eq!(now.settings, custom());
        assert_eq!(now.password.as_ref().map(|p| p.as_str()), Some("pw"));
    }
}
```

Run: `cargo test -j4 -p rocket-infra fs_proxy_settings_repo` and `cargo test -j4 -p rocket-app proxy_settings_service`
Expected: FAIL to compile (types and modules do not exist yet).

- [ ] **Step 10: Implement the repository, the keychain namespace and the service**

Prepend to `crates/rocket-infra/src/fs_proxy_settings_repo.rs`:

```rust
use std::fs;
use std::path::PathBuf;

use rocket_http::{ProxySettings, ProxySettingsRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

/// Stores the non-secret proxy setting in one YAML file.
pub struct FsProxySettingsRepo {
    path: PathBuf,
}

impl FsProxySettingsRepo {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ProxySettingsRepository for FsProxySettingsRepo {
    fn load(&self) -> DomainResult<ProxySettings> {
        if !self.path.exists() {
            return Ok(ProxySettings::default());
        }
        let text = fs::read_to_string(&self.path)?;
        serde_yaml::from_str(&text)
            .map_err(|e| DomainError::Internal(format!("Failed to parse the proxy settings: {e}")))
    }

    fn save(&self, settings: &ProxySettings) -> DomainResult<()> {
        let yaml = serde_yaml::to_string(settings).map_err(|e| {
            DomainError::Internal(format!("Failed to serialize the proxy settings: {e}"))
        })?;
        atomic_write(&self.path, yaml.as_bytes())?;
        Ok(())
    }
}
```

In `crates/rocket-infra/src/lib.rs` add `pub mod fs_proxy_settings_repo;` and `pub use fs_proxy_settings_repo::FsProxySettingsRepo;` next to the other repo exports. In `crates/rocket-infra/src/secret_store.rs` add next to `new_vault_connections`:

```rust
    /// Backs the proxy password. A separate keychain service from the other namespaces so the
    /// entries can never collide.
    pub fn new_proxy() -> Self {
        Self {
            service: "com.rocketapi.proxy",
        }
    }
```

Prepend to `crates/rocket-app/src/proxy_settings_service.rs`:

```rust
use std::sync::Arc;

use rocket_environment::SecretStore;
use rocket_http::{ProxySettings, ProxySettingsRepository, ResolvedProxy, SharedProxy};
use rocket_shared::error::DomainResult;
use zeroize::Zeroizing;

/// Keychain location of the proxy password.
const SECRET_SCOPE: &str = "proxy";
const SECRET_KEY: &str = "password";

/// What a save does with the stored password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordChange {
    Keep,
    Clear,
    Set(String),
}

/// The setting as shown to the user: never the password, only whether one is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxySettingsView {
    pub settings: ProxySettings,
    pub has_password: bool,
}

/// Owns changing the app-level proxy: validates, stores the settings and the password
/// separately, and publishes the result to the handle the executor reads.
pub struct ProxySettingsService {
    repo: Box<dyn ProxySettingsRepository>,
    secrets: Arc<dyn SecretStore>,
    shared: SharedProxy,
}

impl ProxySettingsService {
    /// Loads what was saved before and publishes it. A file or keychain that cannot be read
    /// must not stop the app from starting, so it falls back to the system setting.
    pub fn new(
        repo: Box<dyn ProxySettingsRepository>,
        secrets: Arc<dyn SecretStore>,
        shared: SharedProxy,
    ) -> Self {
        let svc = Self {
            repo,
            secrets,
            shared,
        };
        let settings = svc.repo.load().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "could not read the proxy settings, using the system proxy");
            ProxySettings::default()
        });
        let password = svc.stored_password();
        svc.publish(settings, password);
        svc
    }

    pub fn get(&self) -> DomainResult<ProxySettingsView> {
        Ok(ProxySettingsView {
            settings: self.repo.load()?,
            has_password: self.stored_password().is_some(),
        })
    }

    pub fn save(&self, settings: ProxySettings, password: PasswordChange) -> DomainResult<()> {
        settings.validate()?;
        match &password {
            PasswordChange::Keep => {}
            PasswordChange::Clear => self.secrets.delete(SECRET_SCOPE, SECRET_KEY)?,
            PasswordChange::Set(value) if value.is_empty() => {
                self.secrets.delete(SECRET_SCOPE, SECRET_KEY)?
            }
            PasswordChange::Set(value) => self.secrets.set(SECRET_SCOPE, SECRET_KEY, value)?,
        }
        self.repo.save(&settings)?;
        let password = self.stored_password();
        self.publish(settings, password);
        Ok(())
    }

    fn stored_password(&self) -> Option<Zeroizing<String>> {
        self.secrets
            .get(SECRET_SCOPE, SECRET_KEY)
            .ok()
            .flatten()
            .filter(|p| !p.is_empty())
            .map(Zeroizing::new)
    }

    fn publish(&self, settings: ProxySettings, password: Option<Zeroizing<String>>) {
        // A poisoned lock only means a writer panicked; the value is still a whole proxy.
        let mut shared = self.shared.write().unwrap_or_else(|e| e.into_inner());
        shared.settings = settings;
        shared.password = password;
        shared.generation += 1;
    }
}
```

In `crates/rocket-app/src/lib.rs` add `pub mod proxy_settings_service;` and `pub use proxy_settings_service::{PasswordChange, ProxySettingsService, ProxySettingsView};` next to the other service exports (follow the file's existing style). `crates/rocket-app/Cargo.toml` needs `zeroize` (add `zeroize = "1"` under `[dependencies]` if it is not there) and `tracing` (already used by `execution_service.rs`).

Run: `cargo test -j4 -p rocket-infra fs_proxy_settings_repo` and `cargo test -j4 -p rocket-app proxy_settings_service`
Expected: PASS.

- [ ] **Step 11: Add the IPC commands and wire everything**

Create `src-tauri/src/commands/proxy.rs`:

```rust
use rocket_app::{PasswordChange, ProxySettingsService, ProxySettingsView};
use rocket_http::{ProxyMode, ProxySettings};
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

/// IPC shape of the proxy setting as shown to the user. The password is never part of it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettingsViewDto {
    pub mode: ProxyMode,
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub no_proxy: Option<String>,
    pub username: Option<String>,
    pub has_password: bool,
}

impl From<ProxySettingsView> for ProxySettingsViewDto {
    fn from(view: ProxySettingsView) -> Self {
        Self {
            mode: view.settings.mode,
            http_proxy: view.settings.http_proxy,
            https_proxy: view.settings.https_proxy,
            no_proxy: view.settings.no_proxy,
            username: view.settings.username,
            has_password: view.has_password,
        }
    }
}

/// IPC shape of the setting being saved. Kept apart from `ProxySettings` so the camelCase
/// rename never reaches `proxy.yml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettingsInputDto {
    pub mode: ProxyMode,
    #[serde(default)]
    pub http_proxy: Option<String>,
    #[serde(default)]
    pub https_proxy: Option<String>,
    #[serde(default)]
    pub no_proxy: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

impl From<ProxySettingsInputDto> for ProxySettings {
    fn from(dto: ProxySettingsInputDto) -> Self {
        let clean = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        Self {
            mode: dto.mode,
            http_proxy: clean(dto.http_proxy),
            https_proxy: clean(dto.https_proxy),
            no_proxy: clean(dto.no_proxy),
            username: clean(dto.username),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum PasswordChangeDto {
    Keep,
    Clear,
    Set { value: String },
}

impl From<PasswordChangeDto> for PasswordChange {
    fn from(dto: PasswordChangeDto) -> Self {
        match dto {
            PasswordChangeDto::Keep => PasswordChange::Keep,
            PasswordChangeDto::Clear => PasswordChange::Clear,
            PasswordChangeDto::Set { value } => PasswordChange::Set(value),
        }
    }
}

#[tauri::command]
pub fn get_proxy_settings(
    svc: State<'_, ProxySettingsService>,
) -> Result<ProxySettingsViewDto, DomainError> {
    svc.get().map(ProxySettingsViewDto::from)
}

#[tauri::command]
pub fn save_proxy_settings(
    settings: ProxySettingsInputDto,
    password: PasswordChangeDto,
    svc: State<'_, ProxySettingsService>,
) -> Result<(), DomainError> {
    svc.save(settings.into(), password.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_never_carries_a_password_field() {
        let dto = ProxySettingsViewDto::from(ProxySettingsView {
            settings: ProxySettings::default(),
            has_password: true,
        });
        let json = serde_json::to_value(dto).expect("serialize");
        assert_eq!(json["hasPassword"], true);
        assert!(json.get("password").is_none());
    }

    #[test]
    fn input_trims_and_drops_empty_values() {
        let dto: ProxySettingsInputDto = serde_json::from_str(
            r#"{"mode":"custom","httpProxy":" http://p:1 ","httpsProxy":"  ","username":""}"#,
        )
        .expect("deserialize");
        let settings = ProxySettings::from(dto);
        assert_eq!(settings.http_proxy.as_deref(), Some("http://p:1"));
        assert_eq!(settings.https_proxy, None);
        assert_eq!(settings.username, None);
    }

    #[test]
    fn password_change_is_a_tagged_action() {
        let keep: PasswordChangeDto = serde_json::from_str(r#"{"action":"keep"}"#).expect("keep");
        assert!(matches!(PasswordChange::from(keep), PasswordChange::Keep));
        let set: PasswordChangeDto =
            serde_json::from_str(r#"{"action":"set","value":"pw"}"#).expect("set");
        assert_eq!(PasswordChange::from(set), PasswordChange::Set("pw".into()));
    }
}
```

In `src-tauri/src/commands/mod.rs` add `pub mod proxy;` (alphabetical, after `pub mod oauth2;`). In `src-tauri/src/lib.rs`: before the executor construction (line 321) add

```rust
            // App-level proxy: the service validates and stores it, the executor reads it.
            let shared_proxy = rocket_http::new_shared_proxy();
            let proxy_svc = rocket_app::ProxySettingsService::new(
                Box::new(rocket_infra::FsProxySettingsRepo::new(data_dir.join("proxy.yml"))),
                Arc::new(rocket_infra::KeyringSecretStore::new_proxy()),
                Arc::clone(&shared_proxy),
            );
```

change the executor to `.with_cookie_repo(...).with_proxy(Arc::clone(&shared_proxy))`, add `app.manage(proxy_svc);` after `app.manage(cookie_svc);`, and register `commands::proxy::get_proxy_settings, commands::proxy::save_proxy_settings,` after `commands::cookies::clear_cookies,`.

Run: `cargo test -j4 -p rocket commands::proxy` (package `rocket`)
Expected: PASS.

Run: `cargo check -j4 --workspace --tests`
Expected: PASS.

- [ ] **Step 12: Write the failing frontend tests**

Append to `src/lib/__tests__/execute-request.test.ts` (extend the import with `toApiOptions`):

```ts
describe('toApiOptions', () => {
  it('maps every request setting the backend honors', () => {
    expect(
      toApiOptions({
        verifySsl: false,
        followRedirects: false,
        maxRedirects: 3,
        timeoutMs: 5000,
        encodeUrl: false,
      }),
    ).toEqual({
      followRedirects: false,
      timeoutMs: 5000,
      verifySsl: false,
      maxRedirects: 3,
      encodeUrl: false,
    });
  });

  it('falls back to the defaults when the request has no settings', () => {
    expect(toApiOptions(undefined)).toEqual({
      followRedirects: true,
      timeoutMs: 30000,
      verifySsl: true,
      maxRedirects: undefined,
      encodeUrl: true,
    });
  });
});
```

If Step 6 showed a real duplicate, also append inside `describe('resolveRequestFieldsForPath', ...)`:

```ts
  it('sends the query once: it travels in queryParams, not in the url', async () => {
    const request = {
      ...baseRequest(),
      url: '{{baseUrl}}/items?a=1&b=2',
      queryParams: [
        { id: 'q1', key: 'a', value: '1', enabled: true },
        { id: 'q2', key: 'b', value: '2', enabled: true },
      ],
    };
    const resolved = await resolveRequestFieldsForPath('demo', 'ping.yml', request);
    expect(resolved.url).toBe('https://collection.example/items');
    expect(resolved.queryParams.map((p) => p.key)).toEqual(['a', 'b']);
  });
```

Add to `src/components/request/__tests__/LoadTestDialog.test.tsx` a mock entry-free assertion in the existing forwarding test pattern:

```tsx
  it('forwards the request settings as options', async () => {
    (runLoadTest as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      totalRequests: 1,
      succeeded: 1,
      failed: 0,
      failedTransport: 0,
      failedStatus: 0,
      minLatencyMs: 1,
      avgLatencyMs: 1,
      p50LatencyMs: 1,
      p95LatencyMs: 1,
      p99LatencyMs: 1,
      maxLatencyMs: 1,
      requestsPerSecond: 1,
      totalDurationMs: 1,
    });
    const request = makeRequest();
    request.settings = { ...request.settings, maxRedirects: 2, encodeUrl: false };
    render(<LoadTestDialog open onOpenChange={noop} request={request} tabId='t1' />);
    fireEvent.click(screen.getByRole('button', { name: /^run$/i }));
    await waitFor(() => expect(runLoadTest).toHaveBeenCalledTimes(1));
    const options = (runLoadTest as unknown as ReturnType<typeof vi.fn>).mock.calls[0][0].options;
    expect(options.maxRedirects).toBe(2);
    expect(options.encodeUrl).toBe(false);
  });
```

Create `src/components/settings/__tests__/ProxySettingsDialog.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const getProxySettings = vi.fn();
const saveProxySettings = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  getProxySettings: () => getProxySettings(),
  saveProxySettings: (...a: unknown[]) => saveProxySettings(...a),
}));
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

import { ProxySettingsDialog } from '../ProxySettingsDialog';

describe('ProxySettingsDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getProxySettings.mockResolvedValue({
      mode: 'custom',
      httpProxy: 'http://proxy.corp:8080',
      username: 'bob',
      hasPassword: true,
    });
    saveProxySettings.mockResolvedValue(undefined);
  });

  it('loads the saved setting and shows that a password is stored without showing it', async () => {
    render(<ProxySettingsDialog open onOpenChange={vi.fn()} />);
    expect(await screen.findByDisplayValue('http://proxy.corp:8080')).toBeInTheDocument();
    expect(screen.getByDisplayValue('bob')).toBeInTheDocument();
    expect(screen.getByLabelText('Proxy password')).toHaveValue('');
    expect(screen.getByText(/a password is saved/i)).toBeInTheDocument();
  });

  it('keeps the stored password when the field is left empty', async () => {
    render(<ProxySettingsDialog open onOpenChange={vi.fn()} />);
    await screen.findByDisplayValue('http://proxy.corp:8080');
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(saveProxySettings).toHaveBeenCalledTimes(1));
    const [settings, password] = saveProxySettings.mock.calls[0];
    expect(settings.mode).toBe('custom');
    expect(password).toEqual({ action: 'keep' });
  });

  it('sends a typed password and can clear the stored one', async () => {
    render(<ProxySettingsDialog open onOpenChange={vi.fn()} />);
    await screen.findByDisplayValue('http://proxy.corp:8080');
    fireEvent.change(screen.getByLabelText('Proxy password'), { target: { value: 'pw' } });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(saveProxySettings).toHaveBeenCalledTimes(1));
    expect(saveProxySettings.mock.calls[0][1]).toEqual({ action: 'set', value: 'pw' });

    saveProxySettings.mockClear();
    fireEvent.click(screen.getByRole('button', { name: /remove saved password/i }));
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(saveProxySettings).toHaveBeenCalledTimes(1));
    expect(saveProxySettings.mock.calls[0][1]).toEqual({ action: 'clear' });
  });

  it('hides the url fields unless the mode is custom', async () => {
    getProxySettings.mockResolvedValue({ mode: 'system', hasPassword: false });
    render(<ProxySettingsDialog open onOpenChange={vi.fn()} />);
    await waitFor(() => expect(getProxySettings).toHaveBeenCalled());
    expect(screen.queryByLabelText('HTTP proxy URL')).toBeNull();
    fireEvent.click(await screen.findByRole('radio', { name: /custom/i }));
    expect(screen.getByLabelText('HTTP proxy URL')).toBeInTheDocument();
  });
});
```

Run: `yarn test execute-request LoadTestDialog ProxySettingsDialog`
Expected: FAIL (`toApiOptions` and `ProxySettingsDialog` do not exist).

- [ ] **Step 13: Implement the frontend**

`src/lib/tauri-api.ts`: add `maxRedirects?: number;` and `encodeUrl?: boolean;` to `RequestOptions`, and append:

```ts
// ============================================================
// Proxy
// ============================================================

export type ProxyMode = 'system' | 'none' | 'custom';

export interface ProxySettings {
  mode: ProxyMode;
  httpProxy?: string;
  httpsProxy?: string;
  noProxy?: string;
  username?: string;
}

/** The saved setting. The password itself is never returned, only whether one is stored. */
export interface ProxySettingsView extends ProxySettings {
  hasPassword: boolean;
}

export type ProxyPasswordChange =
  | { action: 'keep' }
  | { action: 'clear' }
  | { action: 'set'; value: string };

export const getProxySettings = () => invoke<ProxySettingsView>('get_proxy_settings');

export const saveProxySettings = (settings: ProxySettings, password: ProxyPasswordChange) =>
  invoke<void>('save_proxy_settings', { settings, password });
```

`src/lib/execute-request.ts`: add (import `RequestOptions` as a type from `@/lib/tauri-api` and `RequestSettings` from `@/types/pane-types`):

```ts
// The execution options a request's settings ask for. One place builds them, so a single send,
// the collection runner and the load test cannot drift apart.
export function toApiOptions(settings: RequestSettings | undefined): RequestOptions {
  return {
    followRedirects: settings?.followRedirects ?? true,
    timeoutMs: settings?.timeoutMs ?? 30000,
    verifySsl: settings?.verifySsl ?? true,
    maxRedirects: settings?.maxRedirects,
    encodeUrl: settings?.encodeUrl ?? true,
  };
}
```

In `sendRequest` replace the `options: { followRedirects: ..., timeoutMs: ..., verifySsl: ... }` object with `options: toApiOptions(effectiveRequest.settings),`. In `src/lib/runner-execute.ts` replace its `options: { ... }` object with `options: toApiOptions(requestState.settings),` (import `toApiOptions` from `@/lib/execute-request`; add `toApiOptions: vi.fn(() => ({ followRedirects: true, timeoutMs: 0, verifySsl: true }))` to the `vi.mock('@/lib/execute-request', ...)` factory in `runner-execute.test.ts`, and keep its assertions on the other fields). In `LoadTestDialog.tsx` replace its `options: { ... }` object with `options: toApiOptions(request.settings),` (and add `toApiOptions: (s) => ({ followRedirects: true, timeoutMs: 30000, verifySsl: true, maxRedirects: s?.maxRedirects, encodeUrl: s?.encodeUrl })` to the dialog test's `vi.mock('@/lib/execute-request', ...)` factory).

If Step 6 showed a real duplicate, in `resolveRequestFieldsForPath` change `const resolvedUrl = resolve(request.url);` to

```ts
  // The query travels in `queryParams`, so it must not also stay in the url or it is sent twice.
  const resolvedUrl = resolve(request.url).split('?')[0];
```

(Plan 01 already made this a `const` and removed the path param loop. A fragment after the query is dropped too, which a server never sees anyway.)

Create `src/components/settings/ProxySettingsDialog.tsx`:

```tsx
import { Loader2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import {
  getProxySettings,
  type ProxyMode,
  type ProxyPasswordChange,
  saveProxySettings,
} from '@/lib/tauri-api';

interface ProxySettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const MODES: { value: ProxyMode; label: string; hint: string }[] = [
  { value: 'system', label: 'System', hint: 'Use the HTTP_PROXY, HTTPS_PROXY and NO_PROXY variables.' },
  { value: 'none', label: 'None', hint: 'Always connect directly.' },
  { value: 'custom', label: 'Custom', hint: 'Use the proxy URLs below.' },
];

export function ProxySettingsDialog({ open, onOpenChange }: ProxySettingsDialogProps) {
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [mode, setMode] = useState<ProxyMode>('system');
  const [httpProxy, setHttpProxy] = useState('');
  const [httpsProxy, setHttpsProxy] = useState('');
  const [noProxy, setNoProxy] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [hasPassword, setHasPassword] = useState(false);
  const [removePassword, setRemovePassword] = useState(false);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    getProxySettings()
      .then((s) => {
        if (cancelled) return;
        setMode(s.mode);
        setHttpProxy(s.httpProxy ?? '');
        setHttpsProxy(s.httpsProxy ?? '');
        setNoProxy(s.noProxy ?? '');
        setUsername(s.username ?? '');
        setHasPassword(s.hasPassword);
        setPassword('');
        setRemovePassword(false);
      })
      .catch(() => toast.error('Could not load the proxy settings'))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [open]);

  const handleSave = async () => {
    const change: ProxyPasswordChange = password
      ? { action: 'set', value: password }
      : removePassword
        ? { action: 'clear' }
        : { action: 'keep' };
    setSaving(true);
    try {
      await saveProxySettings({ mode, httpProxy, httpsProxy, noProxy, username }, change);
      toast.success('Proxy settings saved');
      onOpenChange(false);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Proxy</DialogTitle>
        </DialogHeader>
        {loading ? (
          <div className='flex justify-center p-6'>
            <Loader2 className='h-4 w-4 animate-spin' aria-label='Loading' />
          </div>
        ) : (
          <div className='space-y-4'>
            <RadioGroup value={mode} onValueChange={(v) => setMode(v as ProxyMode)}>
              {MODES.map((m) => (
                <div key={m.value} className='flex items-start gap-2'>
                  <RadioGroupItem value={m.value} id={`proxy-${m.value}`} className='mt-0.5' />
                  <Label htmlFor={`proxy-${m.value}`} className='flex flex-col gap-0.5'>
                    <span>{m.label}</span>
                    <span className='text-xs font-normal text-muted-foreground'>{m.hint}</span>
                  </Label>
                </div>
              ))}
            </RadioGroup>
            {mode === 'custom' && (
              <div className='space-y-3'>
                <div className='space-y-1.5'>
                  <Label htmlFor='proxy-http'>HTTP proxy URL</Label>
                  <Input
                    id='proxy-http'
                    aria-label='HTTP proxy URL'
                    placeholder='http://proxy.corp:8080'
                    value={httpProxy}
                    onChange={(e) => setHttpProxy(e.target.value)}
                  />
                </div>
                <div className='space-y-1.5'>
                  <Label htmlFor='proxy-https'>HTTPS proxy URL</Label>
                  <Input
                    id='proxy-https'
                    aria-label='HTTPS proxy URL'
                    placeholder='http://proxy.corp:8080'
                    value={httpsProxy}
                    onChange={(e) => setHttpsProxy(e.target.value)}
                  />
                </div>
                <div className='space-y-1.5'>
                  <Label htmlFor='proxy-no'>No proxy for</Label>
                  <Input
                    id='proxy-no'
                    aria-label='No proxy for'
                    placeholder='localhost, .internal.corp'
                    value={noProxy}
                    onChange={(e) => setNoProxy(e.target.value)}
                  />
                </div>
                <div className='grid grid-cols-2 gap-2'>
                  <div className='space-y-1.5'>
                    <Label htmlFor='proxy-user'>Username</Label>
                    <Input
                      id='proxy-user'
                      aria-label='Proxy username'
                      value={username}
                      onChange={(e) => setUsername(e.target.value)}
                    />
                  </div>
                  <div className='space-y-1.5'>
                    <Label htmlFor='proxy-pass'>Password</Label>
                    <Input
                      id='proxy-pass'
                      aria-label='Proxy password'
                      type='password'
                      autoComplete='new-password'
                      placeholder={hasPassword && !removePassword ? 'Unchanged' : ''}
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                    />
                  </div>
                </div>
                {hasPassword && !removePassword && (
                  <p className='flex items-center gap-2 text-xs text-muted-foreground'>
                    A password is saved in the system keychain.
                    <Button
                      variant='link'
                      size='sm'
                      className='h-auto p-0 text-xs'
                      onClick={() => setRemovePassword(true)}
                    >
                      Remove saved password
                    </Button>
                  </p>
                )}
                <p className='text-xs text-muted-foreground'>
                  OAuth 2.0 token requests do not use this setting yet.
                </p>
              </div>
            )}
          </div>
        )}
        <DialogFooter>
          <Button variant='ghost' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={loading || saving}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

`DialogFooter` and `RadioGroup` come from the existing shadcn files; if `dialog.tsx` does not export `DialogFooter`, replace it with a `<div className='flex justify-end gap-2'>`.

`src/components/title-bar/TitleBar.tsx`: add `Globe` to the `lucide-react` import, `import { ProxySettingsDialog } from '@/components/settings/ProxySettingsDialog';`, `const [showProxy, setShowProxy] = useState(false);`, a button after the secret-manager one:

```tsx
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Proxy settings'
          onClick={() => setShowProxy(true)}
        >
          <Globe className='h-4 w-4' aria-hidden='true' />
        </Button>
```

and `<ProxySettingsDialog open={showProxy} onOpenChange={setShowProxy} />` next to the other dialogs.

- [ ] **Step 14: Run all checks**

Run: `yarn test execute-request LoadTestDialog runner-execute ProxySettingsDialog TitleBar`
Expected: PASS.

Run: `yarn tsc --noEmit && yarn check`
Expected: PASS.

Run: `cargo test -j4 -p rocket-http proxy request && cargo test -j4 -p rocket-infra encode_url_tests proxy_tests fs_proxy_settings_repo reqwest_executor && cargo test -j4 -p rocket-app proxy_settings_service runner_sequence && cargo check -j4 --workspace --tests`
Expected: PASS.

Manual check (needs `yarn tauri dev`): in Proxy settings choose Custom with a local proxy (for example `mitmproxy` on `http://127.0.0.1:8080`), save, send a request and confirm it appears in the proxy; set a username and password, confirm `~/.rocket-api/proxy.yml` holds no password and the keychain has an entry for `com.rocketapi.proxy`. Turn "Encode URL" off in a request's Settings tab and confirm a `%20` typed in a query value is sent unchanged. Set Max redirects to 1 against `https://httpbin.org/redirect/3` and confirm the 3xx response is returned.

- [ ] **Step 15: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, staging by explicit path (every file listed under Files for this task, plus `Cargo.lock` if it changed). Consider two commits if the diff is large: `refactor(http): build executor clients from a cache key and a build spec`, then `feat(http): honor encodeUrl and maxRedirects and add an app-level proxy`.

---

## Task 3: NTLM authentication

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-http/Cargo.toml` (`md4 = "0.10"`)
- Create: `crates/rocket-http/src/ntlm_sig.rs`; Modify: `crates/rocket-http/src/lib.rs`
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (`apply_auth` NTLM arm, `execute` send step, client selection, new test module)
- Modify: `src/lib/auth-type-options.ts`, `src/lib/auth-type-defaults.ts`, `src/components/request/AuthEditor.tsx` (NTLM note at about line 205), `src/components/request/RequestPanel.tsx` (`BASE_AUTH_TYPES`), `src/components/collections/CollectionOverviewTab.tsx` (`COLLECTION_AUTH_TYPES`)
- Test: `src/lib/__tests__/auth-type-options.test.ts`, `src/lib/__tests__/auth-type-defaults.test.ts`, `src/components/request/__tests__/AuthEditor.test.tsx`
- Docs: `crates/rocket-infra/CLAUDE.md` ("WSSE, Digest, NTLM" paragraph), `crates/rocket-http/CLAUDE.md` (module map)

**Interfaces:**
- Consumes: `Auth::Ntlm { username, password, domain }` (already resolved for `{{variables}}` by `resolve_auth`, `execution_service.rs:2065`).
- Produces (`rocket-http::ntlm_sig`, NTLMv2 only):
  - `negotiate_message() -> Vec<u8>` (message 1).
  - `parse_challenge(msg: &[u8]) -> Result<NtlmChallenge, String>` with `NtlmChallenge { flags: u32, server_challenge: [u8; 8], target_info: Vec<u8>, timestamp: Option<[u8; 8]> }`.
  - `authenticate_message(challenge: &NtlmChallenge, username: &str, password: &str, domain: &str, workstation: &str, client_nonce: [u8; 8], client_time: u64) -> Vec<u8>` (message 3).
  - `nt_hash(password: &str) -> [u8; 16]`, `ntowfv2(password: &str, username: &str, domain: &str) -> [u8; 16]`.
  - `header_value(message: &[u8]) -> String` (`NTLM <base64>`), `parse_header(value: &str) -> Option<Vec<u8>>`, `find_token(values: &[&str]) -> Option<Vec<u8>>`.
  - `split_account(username: &str, domain: &str) -> (String, String)`, `random_nonce() -> [u8; 8]`, `file_time_now() -> u64`.
- Produces (executor): an NTLM send is three steps on one connection: message 1 without a body, message 3 with the full request. A response that is not a 401, or a 401 without an NTLM challenge, is returned as is.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`: the `AuthNtlm` shape is `{ type: "ntlm", username, password, domain }`.

- [ ] **Step 2: Write the failing `ntlm_sig` tests**

Add `md4 = "0.10"` to `[dependencies]` of `crates/rocket-http/Cargo.toml`. Create `crates/rocket-http/src/ntlm_sig.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
            .collect()
    }

    /// The target information of the [MS-NLMP] section 4.2.4 examples: NbDomainName "Domain",
    /// NbComputerName "Server", then the end marker.
    fn spec_target_info() -> Vec<u8> {
        let mut v = Vec::new();
        for (id, text) in [(2u16, "Domain"), (1u16, "Server")] {
            let name: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
            v.extend(id.to_le_bytes());
            v.extend((name.len() as u16).to_le_bytes());
            v.extend(name);
        }
        v.extend([0, 0, 0, 0]);
        v
    }

    fn spec_challenge() -> NtlmChallenge {
        NtlmChallenge {
            flags: 0xA088_8215,
            server_challenge: [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef],
            target_info: spec_target_info(),
            timestamp: None,
        }
    }

    /// Reads a security buffer (length, max length, offset) at `at` and returns its bytes.
    fn buffer(msg: &[u8], at: usize) -> Vec<u8> {
        let len = u16::from_le_bytes([msg[at], msg[at + 1]]) as usize;
        let off = u32::from_le_bytes([msg[at + 4], msg[at + 5], msg[at + 6], msg[at + 7]]) as usize;
        msg[off..off + len].to_vec()
    }

    #[test]
    fn nt_hash_matches_the_well_known_value() {
        assert_eq!(
            nt_hash("Password").to_vec(),
            unhex("a4f49c406510bdcab6824ee7c30fd852")
        );
    }

    #[test]
    fn ntowfv2_matches_the_spec_example() {
        // [MS-NLMP] 4.2.4.1.1: User "User", Domain "Domain", Password "Password".
        assert_eq!(
            ntowfv2("Password", "User", "Domain").to_vec(),
            unhex("0c868a403bfd7a93a3001ef22ef02e3f")
        );
    }

    #[test]
    fn the_nt_proof_and_lm_response_match_the_spec_example() {
        // [MS-NLMP] 4.2.4.2: client challenge aa..aa, time 0.
        let msg = authenticate_message(
            &spec_challenge(),
            "User",
            "Password",
            "Domain",
            "COMPUTER",
            [0xaa; 8],
            0,
        );
        let lm = buffer(&msg, 12);
        let nt = buffer(&msg, 20);
        assert_eq!(lm, unhex("86c35097ac9cec102554764a57cccc19aaaaaaaaaaaaaaaa"));
        assert_eq!(nt[..16].to_vec(), unhex("68cd0ab851e51c96aabc927bebef6a1c"));
        // The temp blob follows the proof: version 1.1, zeros, time, client nonce, zeros,
        // the server's target information, zeros.
        let blob = &nt[16..];
        assert_eq!(blob[..8].to_vec(), unhex("0101000000000000"));
        assert_eq!(blob[8..16].to_vec(), vec![0u8; 8]);
        assert_eq!(blob[16..24].to_vec(), vec![0xaa; 8]);
        assert!(blob.windows(spec_target_info().len()).any(|w| w == spec_target_info().as_slice()));
    }

    #[test]
    fn message_three_has_a_consistent_layout() {
        let msg = authenticate_message(
            &spec_challenge(),
            "User",
            "Password",
            "Domain",
            "COMPUTER",
            [1; 8],
            0,
        );
        assert_eq!(&msg[..8], b"NTLMSSP\0");
        assert_eq!(u32::from_le_bytes([msg[8], msg[9], msg[10], msg[11]]), 3);
        let utf16 = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect() };
        assert_eq!(buffer(&msg, 28), utf16("Domain"));
        assert_eq!(buffer(&msg, 36), utf16("User"));
        assert_eq!(buffer(&msg, 44), utf16("COMPUTER"));
        assert!(buffer(&msg, 52).is_empty(), "no session key is negotiated");
        let flags = u32::from_le_bytes([msg[60], msg[61], msg[62], msg[63]]);
        assert_ne!(flags & 0x1, 0, "unicode must be set");
        assert_eq!(flags & 0x4000_0000, 0, "key exchange is never claimed");
    }

    #[test]
    fn a_server_timestamp_is_used_and_the_lm_response_is_zeroed() {
        let mut challenge = spec_challenge();
        challenge.timestamp = Some([9; 8]);
        let msg = authenticate_message(&challenge, "u", "p", "d", "w", [2; 8], 12345);
        assert_eq!(buffer(&msg, 12), vec![0u8; 24]);
        let nt = buffer(&msg, 20);
        assert_eq!(nt[16 + 8..16 + 16].to_vec(), vec![9u8; 8], "the server's time, not ours");
    }

    #[test]
    fn message_one_is_a_bare_negotiate() {
        let msg = negotiate_message();
        assert_eq!(&msg[..8], b"NTLMSSP\0");
        assert_eq!(u32::from_le_bytes([msg[8], msg[9], msg[10], msg[11]]), 1);
        assert_eq!(msg.len(), 32);
    }

    fn challenge_bytes() -> Vec<u8> {
        let info = spec_target_info();
        let mut m = Vec::new();
        m.extend(b"NTLMSSP\0");
        m.extend(2u32.to_le_bytes());
        m.extend([0, 0, 0, 0]);
        m.extend(48u32.to_le_bytes());
        m.extend(0xA088_8215u32.to_le_bytes());
        m.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        m.extend([0u8; 8]);
        m.extend((info.len() as u16).to_le_bytes());
        m.extend((info.len() as u16).to_le_bytes());
        m.extend(48u32.to_le_bytes());
        m.extend(&info);
        m
    }

    #[test]
    fn parses_a_challenge() {
        let c = parse_challenge(&challenge_bytes()).expect("parse");
        assert_eq!(c.flags, 0xA088_8215);
        assert_eq!(c.server_challenge, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(c.target_info, spec_target_info());
        assert_eq!(c.timestamp, None);
    }

    #[test]
    fn finds_the_server_timestamp_pair() {
        let mut info = Vec::new();
        info.extend(7u16.to_le_bytes());
        info.extend(8u16.to_le_bytes());
        info.extend([7u8; 8]);
        info.extend([0, 0, 0, 0]);
        let mut m = challenge_bytes();
        m.truncate(40);
        m.extend((info.len() as u16).to_le_bytes());
        m.extend((info.len() as u16).to_le_bytes());
        m.extend(48u32.to_le_bytes());
        m.extend(&info);
        assert_eq!(parse_challenge(&m).expect("parse").timestamp, Some([7; 8]));
    }

    #[test]
    fn rejects_messages_that_are_not_a_usable_challenge() {
        assert!(parse_challenge(b"nope").is_err());
        let mut wrong_type = challenge_bytes();
        wrong_type[8] = 1;
        assert!(parse_challenge(&wrong_type).is_err());
        let mut short = challenge_bytes();
        short.truncate(30);
        assert!(parse_challenge(&short).is_err(), "no target information means NTLMv1 only");
        let mut lying = challenge_bytes();
        lying[44] = 0xff; // target info offset far outside the message
        assert!(parse_challenge(&lying).is_err());
    }

    #[test]
    fn header_helpers_round_trip_and_find_a_token_among_schemes() {
        let msg = negotiate_message();
        let header = header_value(&msg);
        assert!(header.starts_with("NTLM "));
        assert_eq!(parse_header(&header), Some(msg.clone()));
        assert_eq!(parse_header("Basic abc"), None);
        assert_eq!(parse_header("NTLM"), None, "a bare scheme carries no message");
        let values = ["Negotiate", header.as_str(), "Basic realm=x"];
        assert_eq!(find_token(&values), Some(msg));
        assert_eq!(find_token(&["Negotiate, NTLM"]), None);
        let combined = format!("Negotiate, {header}");
        assert!(find_token(&[combined.as_str()]).is_some());
    }

    #[test]
    fn splits_domain_and_user_forms() {
        assert_eq!(split_account("CORP\\bob", ""), ("bob".into(), "CORP".into()));
        assert_eq!(split_account("bob", "CORP"), ("bob".into(), "CORP".into()));
        assert_eq!(split_account("bob@corp.test", ""), ("bob@corp.test".into(), String::new()));
        // An explicit domain wins over a prefix on the user name.
        assert_eq!(split_account("OTHER\\bob", "CORP"), ("OTHER\\bob".into(), "CORP".into()));
    }

    #[test]
    fn file_time_is_after_the_unix_epoch_offset() {
        // 1601-01-01 to 1970-01-01 is 11644473600 seconds, in 100 ns ticks.
        assert!(file_time_now() > 11_644_473_600u64 * 10_000_000);
    }
}
```

If the three spec vectors (`ntowfv2`, the NT proof and the LMv2 response) fail while the layout tests pass, re-check the vectors against [MS-NLMP] section 4.2.4 before changing the implementation: the hashes are the only values here typed from memory of the spec, and the NT hash of `Password` is independent of them.

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -j4 -p rocket-http ntlm_sig`
Expected: FAIL to compile (module not declared, items missing).

- [ ] **Step 4: Implement `ntlm_sig`**

Prepend to `crates/rocket-http/src/ntlm_sig.rs`:

```rust
//! NTLMv2 authentication messages (MS-NLMP), without signing, sealing or key exchange.
//!
//! Pure functions only. The executor in `rocket-infra` runs the handshake: message 1, the
//! server's challenge (message 2) in a 401, then message 3, all on one connection. NTLMv1 and
//! LM-only servers are not supported.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use md4::{Digest as _, Md4};
use rand::RngCore;

type HmacMd5 = Hmac<md5::Md5>;

const SIGNATURE: &[u8; 8] = b"NTLMSSP\0";

const NEGOTIATE_UNICODE: u32 = 0x0000_0001;
const REQUEST_TARGET: u32 = 0x0000_0004;
const NEGOTIATE_NTLM: u32 = 0x0000_0200;
const NEGOTIATE_ALWAYS_SIGN: u32 = 0x0000_8000;
const NEGOTIATE_EXTENDED_SESSIONSECURITY: u32 = 0x0008_0000;
const NEGOTIATE_128: u32 = 0x2000_0000;
const NEGOTIATE_56: u32 = 0x8000_0000;

/// What the client asks for in message 1, and the most it will accept in message 3.
const CLIENT_FLAGS: u32 = NEGOTIATE_UNICODE
    | REQUEST_TARGET
    | NEGOTIATE_NTLM
    | NEGOTIATE_ALWAYS_SIGN
    | NEGOTIATE_EXTENDED_SESSIONSECURITY
    | NEGOTIATE_128
    | NEGOTIATE_56;

/// AV pair id of the server's timestamp.
const AV_TIMESTAMP: u16 = 7;

/// The parts of the server's message 2 that message 3 needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtlmChallenge {
    pub flags: u32,
    pub server_challenge: [u8; 8],
    /// The server's AV pairs, including the end marker, copied into the response.
    pub target_info: Vec<u8>,
    /// The server's `MsvAvTimestamp`, when it sent one.
    pub timestamp: Option<[u8; 8]>,
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

/// HMAC-MD5 over the concatenation of `parts`. HMAC accepts a key of any length, so building
/// the MAC cannot fail; the zero fallback only exists to keep this function free of panics.
fn hmac_md5(key: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let Ok(mut mac) = HmacMd5::new_from_slice(key) else {
        return [0u8; 16];
    };
    for part in parts {
        mac.update(part);
    }
    let mut out = [0u8; 16];
    out.copy_from_slice(&mac.finalize().into_bytes());
    out
}

/// The NT hash: MD4 of the UTF-16LE password.
pub fn nt_hash(password: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    out.copy_from_slice(&Md4::digest(utf16le(password)));
    out
}

/// NTOWFv2: HMAC-MD5 keyed with the NT hash over the upper-cased user name plus the domain.
pub fn ntowfv2(password: &str, username: &str, domain: &str) -> [u8; 16] {
    let identity = utf16le(&format!("{}{}", username.to_uppercase(), domain));
    hmac_md5(&nt_hash(password), &[&identity])
}

/// Message 1: no domain, no workstation.
pub fn negotiate_message() -> Vec<u8> {
    let mut m = Vec::with_capacity(32);
    m.extend_from_slice(SIGNATURE);
    m.extend_from_slice(&1u32.to_le_bytes());
    m.extend_from_slice(&CLIENT_FLAGS.to_le_bytes());
    for _ in 0..2 {
        m.extend_from_slice(&0u16.to_le_bytes());
        m.extend_from_slice(&0u16.to_le_bytes());
        m.extend_from_slice(&32u32.to_le_bytes());
    }
    m
}

fn read_u16(msg: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(msg.get(at..at + 2)?.try_into().ok()?))
}

fn read_u32(msg: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(msg.get(at..at + 4)?.try_into().ok()?))
}

/// Reads the server's message 2.
pub fn parse_challenge(msg: &[u8]) -> Result<NtlmChallenge, String> {
    if msg.len() < 32 || &msg[..8] != SIGNATURE {
        return Err("not an NTLM message".into());
    }
    if read_u32(msg, 8) != Some(2) {
        return Err("not an NTLM challenge message".into());
    }
    let flags = read_u32(msg, 20).ok_or("truncated challenge")?;
    let mut server_challenge = [0u8; 8];
    server_challenge.copy_from_slice(msg.get(24..32).ok_or("truncated challenge")?);
    if msg.len() < 48 {
        return Err(
            "the server sent no target information, so only NTLMv1 is possible, \
             which is not supported"
                .into(),
        );
    }
    let len = read_u16(msg, 40).ok_or("truncated challenge")? as usize;
    let offset = read_u32(msg, 44).ok_or("truncated challenge")? as usize;
    let target_info = msg
        .get(offset..offset.checked_add(len).ok_or("malformed target information")?)
        .ok_or("the target information lies outside the message")?
        .to_vec();
    Ok(NtlmChallenge {
        flags,
        server_challenge,
        timestamp: find_timestamp(&target_info),
        target_info,
    })
}

fn find_timestamp(info: &[u8]) -> Option<[u8; 8]> {
    let mut at = 0;
    while let (Some(id), Some(len)) = (read_u16(info, at), read_u16(info, at + 2)) {
        if id == 0 {
            return None;
        }
        let value = info.get(at + 4..at + 4 + len as usize)?;
        if id == AV_TIMESTAMP && len == 8 {
            let mut out = [0u8; 8];
            out.copy_from_slice(value);
            return Some(out);
        }
        at += 4 + len as usize;
    }
    None
}

/// Message 3 with an NTLMv2 response. `client_time` is a Windows FILETIME, used only when the
/// server sent no timestamp. When it did, that timestamp is used and the LM response is zeroed.
pub fn authenticate_message(
    challenge: &NtlmChallenge,
    username: &str,
    password: &str,
    domain: &str,
    workstation: &str,
    client_nonce: [u8; 8],
    client_time: u64,
) -> Vec<u8> {
    let key = ntowfv2(password, username, domain);
    let time = challenge.timestamp.unwrap_or(client_time.to_le_bytes());

    let mut blob = Vec::with_capacity(32 + challenge.target_info.len());
    blob.extend_from_slice(&[1, 1, 0, 0, 0, 0, 0, 0]);
    blob.extend_from_slice(&time);
    blob.extend_from_slice(&client_nonce);
    blob.extend_from_slice(&[0, 0, 0, 0]);
    blob.extend_from_slice(&challenge.target_info);
    blob.extend_from_slice(&[0, 0, 0, 0]);

    let proof = hmac_md5(&key, &[&challenge.server_challenge, &blob]);
    let mut nt_response = proof.to_vec();
    nt_response.extend_from_slice(&blob);

    let lm_response = if challenge.timestamp.is_some() {
        vec![0u8; 24]
    } else {
        let mut lm = hmac_md5(&key, &[&challenge.server_challenge, &client_nonce]).to_vec();
        lm.extend_from_slice(&client_nonce);
        lm
    };

    let domain_bytes = utf16le(domain);
    let user_bytes = utf16le(username);
    let workstation_bytes = utf16le(workstation);
    // Payload order: domain, user, workstation, LM response, NT response, session key (empty).
    let payload: [&[u8]; 6] = [
        &domain_bytes,
        &user_bytes,
        &workstation_bytes,
        &lm_response,
        &nt_response,
        &[],
    ];
    // Header: signature, type, six security buffers, flags. The payload starts right after it.
    let header_len = 64u32;
    let mut offsets = [0u32; 6];
    let mut next = header_len;
    for (i, part) in payload.iter().enumerate() {
        offsets[i] = next;
        next += part.len() as u32;
    }
    let mut buffer = |m: &mut Vec<u8>, i: usize| {
        let len = payload[i].len() as u16;
        m.extend_from_slice(&len.to_le_bytes());
        m.extend_from_slice(&len.to_le_bytes());
        m.extend_from_slice(&offsets[i].to_le_bytes());
    };

    let mut m = Vec::with_capacity(next as usize);
    m.extend_from_slice(SIGNATURE);
    m.extend_from_slice(&3u32.to_le_bytes());
    // Wire order of the buffers: LM, NT, domain, user, workstation, session key.
    for i in [3, 4, 0, 1, 2, 5] {
        buffer(&mut m, i);
    }
    let flags = (challenge.flags & CLIENT_FLAGS) | NEGOTIATE_UNICODE;
    m.extend_from_slice(&flags.to_le_bytes());
    for part in payload {
        m.extend_from_slice(part);
    }
    m
}

/// `NTLM <base64>` for an `Authorization` header.
pub fn header_value(message: &[u8]) -> String {
    format!("NTLM {}", STANDARD.encode(message))
}

/// The message in an `NTLM <base64>` header value, or `None` for another scheme or a bare
/// `NTLM` with no message.
pub fn parse_header(value: &str) -> Option<Vec<u8>> {
    let value = value.trim();
    let (scheme, rest) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("ntlm") {
        return None;
    }
    STANDARD.decode(rest.trim()).ok().filter(|m| !m.is_empty())
}

/// The NTLM message among `WWW-Authenticate` values. A value may list several schemes.
pub fn find_token(values: &[&str]) -> Option<Vec<u8>> {
    values
        .iter()
        .flat_map(|v| v.split(','))
        .find_map(parse_header)
}

/// Splits `DOMAIN\user` when no domain is given separately. An explicit domain always wins.
pub fn split_account(username: &str, domain: &str) -> (String, String) {
    if domain.is_empty() {
        if let Some((d, u)) = username.split_once('\\') {
            return (u.to_string(), d.to_string());
        }
    }
    (username.to_string(), domain.to_string())
}

pub fn random_nonce() -> [u8; 8] {
    let mut nonce = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut nonce);
    nonce
}

/// The current time as a Windows FILETIME: 100 ns ticks since 1601-01-01.
pub fn file_time_now() -> u64 {
    let now = chrono::Utc::now();
    let secs = now.timestamp().max(0) as u64 + 11_644_473_600;
    secs * 10_000_000 + u64::from(now.timestamp_subsec_nanos()) / 100
}
```

In `crates/rocket-http/src/lib.rs` add `pub mod ntlm_sig;` (after `pub mod load_test;`).

- [ ] **Step 5: Run to verify the `ntlm_sig` tests pass**

Run: `cargo test -j4 -p rocket-http ntlm_sig`
Expected: PASS (11 tests). `find_token(&["Negotiate, NTLM"])` must be `None`: a bare `NTLM` has no space-separated message.

- [ ] **Step 6: Write the failing handshake tests**

Append to `crates/rocket-infra/src/reqwest_executor.rs`:

```rust
#[cfg(test)]
mod ntlm_tests {
    use super::*;
    use base64::Engine as _;
    use http_body_util::{BodyExt, Full};
    use hyper::body::{Bytes, Incoming};
    use hyper::service::service_fn;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;
    use rocket_http::ntlm_sig;
    use rocket_shared::types::HttpMethod;

    /// One log line per request the server saw: (connection id, kind, body length).
    type Log = Arc<Mutex<Vec<(u32, &'static str, usize)>>>;

    fn challenge_message() -> Vec<u8> {
        let name: Vec<u8> = "DOMAIN".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let mut info = Vec::new();
        info.extend(2u16.to_le_bytes());
        info.extend((name.len() as u16).to_le_bytes());
        info.extend(name);
        info.extend([0, 0, 0, 0]);
        let mut m = Vec::new();
        m.extend(b"NTLMSSP\0");
        m.extend(2u32.to_le_bytes());
        m.extend([0, 0, 0, 0]);
        m.extend(48u32.to_le_bytes());
        m.extend(0xA088_8215u32.to_le_bytes());
        m.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        m.extend([0u8; 8]);
        m.extend((info.len() as u16).to_le_bytes());
        m.extend((info.len() as u16).to_le_bytes());
        m.extend(48u32.to_le_bytes());
        m.extend(&info);
        m
    }

    /// A server that speaks NTLM: it answers message 1 with a challenge and accepts message 3
    /// only when it arrives on the same connection as message 1. With `require_ntlm` off it
    /// answers 200 to everything.
    async fn server(require_ntlm: bool) -> (std::net::SocketAddr, Log) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let server_log = Arc::clone(&log);
        tokio::spawn(async move {
            let mut next_id = 0u32;
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                next_id += 1;
                let id = next_id;
                let log = Arc::clone(&server_log);
                tokio::spawn(async move {
                    let service = service_fn(move |req: Request<Incoming>| {
                        let log = Arc::clone(&log);
                        async move {
                            let auth = req
                                .headers()
                                .get("authorization")
                                .and_then(|v| v.to_str().ok())
                                .map(str::to_string);
                            let body_len = req
                                .into_body()
                                .collect()
                                .await
                                .map(|b| b.to_bytes().len())
                                .unwrap_or(0);
                            let message = auth.as_deref().and_then(ntlm_sig::parse_header);
                            let builder = Response::builder();
                            let response = if !require_ntlm {
                                log.lock().expect("log").push((id, "plain", body_len));
                                builder.status(200).body(Full::new(Bytes::from("ok")))
                            } else {
                                match message.as_deref().and_then(|m| m.get(8).copied()) {
                                    Some(1) => {
                                        log.lock().expect("log").push((id, "type1", body_len));
                                        builder
                                            .status(401)
                                            .header(
                                                "www-authenticate",
                                                ntlm_sig::header_value(&challenge_message()),
                                            )
                                            .body(Full::new(Bytes::from("challenge")))
                                    }
                                    Some(3) => {
                                        let after_type1 = log
                                            .lock()
                                            .expect("log")
                                            .iter()
                                            .any(|(i, k, _)| *i == id && *k == "type1");
                                        log.lock().expect("log").push((id, "type3", body_len));
                                        builder
                                            .status(if after_type1 { 200 } else { 401 })
                                            .body(Full::new(Bytes::from("welcome")))
                                    }
                                    _ => {
                                        log.lock().expect("log").push((id, "none", body_len));
                                        builder
                                            .status(401)
                                            .header("www-authenticate", "NTLM")
                                            .body(Full::new(Bytes::from("denied")))
                                    }
                                }
                            };
                            Ok::<_, std::convert::Infallible>(
                                response.unwrap_or_else(|_| Response::new(Full::new(Bytes::new()))),
                            )
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        (addr, log)
    }

    fn ntlm_request(addr: std::net::SocketAddr, method: HttpMethod) -> HttpRequest {
        let mut req = HttpRequest::new(method, format!("http://{addr}/secure"));
        req.auth = Auth::Ntlm {
            username: "user".into(),
            password: "pass".into(),
            domain: "DOMAIN".into(),
        };
        req.options.timeout_ms = 10_000;
        req
    }

    #[tokio::test]
    async fn ntlm_handshake_uses_one_connection() {
        let (addr, log) = server(true).await;
        let response = ReqwestExecutor::new()
            .execute(&ntlm_request(addr, HttpMethod::Get))
            .await
            .expect("send");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "welcome");
        let seen = log.lock().expect("log").clone();
        let kinds: Vec<_> = seen.iter().map(|(_, k, _)| *k).collect();
        assert_eq!(kinds, ["type1", "type3"], "{seen:?}");
        assert_eq!(seen[0].0, seen[1].0, "both messages must share one connection: {seen:?}");
    }

    #[tokio::test]
    async fn ntlm_sends_the_body_only_with_message_three() {
        let (addr, log) = server(true).await;
        let mut req = ntlm_request(addr, HttpMethod::Post);
        req.body = Some(Body {
            mode: BodyMode::Json,
            content: Some("{\"a\":1}".into()),
            form_data: None,
            file_path: None,
        });
        let response = ReqwestExecutor::new().execute(&req).await.expect("send");
        assert_eq!(response.status, 200);
        let seen = log.lock().expect("log").clone();
        assert_eq!(seen[0].1, "type1");
        assert_eq!(seen[0].2, 0, "message 1 must carry no body");
        assert_eq!(seen[1].1, "type3");
        assert_eq!(seen[1].2, 7, "message 3 carries the whole body");
    }

    #[tokio::test]
    async fn a_server_without_ntlm_is_not_forced_through_the_handshake() {
        let (addr, log) = server(false).await;
        let response = ReqwestExecutor::new()
            .execute(&ntlm_request(addr, HttpMethod::Get))
            .await
            .expect("send");
        assert_eq!(response.status, 200);
        assert_eq!(log.lock().expect("log").len(), 1, "no second request after a 200");
    }

    #[tokio::test]
    async fn wrong_credentials_return_the_401() {
        // This server answers the challenge but never accepts message 3 on a new connection:
        // simulate rejection by asking for NTLM and refusing every message 3.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let service = service_fn(|req: Request<Incoming>| async move {
                        let kind = req
                            .headers()
                            .get("authorization")
                            .and_then(|v| v.to_str().ok())
                            .and_then(ntlm_sig::parse_header)
                            .and_then(|m| m.get(8).copied());
                        let mut b = Response::builder().status(401);
                        if kind == Some(1) {
                            b = b.header(
                                "www-authenticate",
                                ntlm_sig::header_value(&challenge_message()),
                            );
                        } else {
                            b = b.header("www-authenticate", "NTLM");
                        }
                        Ok::<_, std::convert::Infallible>(
                            b.body(Full::new(Bytes::from("denied")))
                                .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()))),
                        )
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        let response = ReqwestExecutor::new()
            .execute(&ntlm_request(addr, HttpMethod::Get))
            .await
            .expect("a rejected login is a response, not an error");
        assert_eq!(response.status, 401);
        assert_eq!(response.body, "denied");
    }

    #[tokio::test]
    async fn a_challenge_that_cannot_be_read_is_an_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let service = service_fn(|_req: Request<Incoming>| async move {
                        let junk = base64::engine::general_purpose::STANDARD.encode(b"garbage!");
                        Ok::<_, std::convert::Infallible>(
                            Response::builder()
                                .status(401)
                                .header("www-authenticate", format!("NTLM {junk}"))
                                .body(Full::new(Bytes::new()))
                                .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()))),
                        )
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        let err = ReqwestExecutor::new()
            .execute(&ntlm_request(addr, HttpMethod::Get))
            .await
            .expect_err("must fail");
        assert!(err.to_string().contains("NTLM"), "{err}");
        assert!(!err.to_string().contains("pass"), "{err}");
    }
}
```

- [ ] **Step 7: Run to verify failure**

Run: `cargo test -j4 -p rocket-infra ntlm_tests`
Expected: FAIL: every NTLM request returns the `InvalidInput("NTLM authentication is not supported yet")` error.

- [ ] **Step 8: Implement the handshake**

In `apply_auth` replace the `Auth::Ntlm { .. } => { return Err(...) }` arm with a no-op, matching the other connection-bound schemes:

```rust
        Auth::Ntlm { .. } => {
            // A three-step handshake on one connection, done in `execute`.
        }
```

In `execute`, before `let builder = apply_auth(...)`, keep the first URL for the handshake:

```rust
        // NTLM sends message 1 to this URL, without the request body.
        let ntlm_url = url.clone();
```

Replace the client selection added in Task 2 so NTLM gets a dedicated single-connection client:

```rust
        let single_connection = matches!(request.auth, Auth::Ntlm { .. });
        let client = if identity.is_some() || single_connection {
            // The shared client cache has no identity and no single-connection mode in its key,
            // so these get their own client.
            build_client(ClientBuild {
                key,
                identity,
                cookies,
                proxy,
                single_connection,
            })?
        } else {
            self.get_or_build_client(key, cookies, proxy)?
        };
```

Replace the `let mut response = match &request.auth { ... }.map_err(...)?;` block with a version whose arms each produce a `DomainResult<reqwest::Response>`:

```rust
        let http_error = |e: reqwest::Error| DomainError::Http(e.to_string());
        let mut response = match &request.auth {
            // OAuth1 and AWS signing both sign the final request, so they wait until the body is applied.
            Auth::OAuth1(oauth) => {
                let mut built = builder.build().map_err(|e| {
                    DomainError::Internal(format!("Cannot build request for signing: {e}"))
                })?;
                apply_oauth1(&mut built, &request.method, oauth)?;
                client.execute(built).await.map_err(http_error)?
            }
            Auth::AwsSigV4 {
                access_key,
                secret_key,
                region,
                service,
                session_token,
                profile_name,
            } => {
                let creds = crate::aws_profile::resolve_credentials(
                    access_key,
                    secret_key,
                    region,
                    service,
                    session_token.as_deref(),
                    profile_name.as_deref(),
                )?;
                let mut built = builder.build().map_err(|e| {
                    DomainError::Internal(format!("Cannot build request for signing: {e}"))
                })?;
                apply_aws_sigv4(&mut built, &creds)?;
                client.execute(built).await.map_err(http_error)?
            }
            Auth::Ntlm {
                username,
                password,
                domain,
            } => {
                use rocket_http::ntlm_sig;

                // Message 1 carries no body: the body goes out once, with message 3.
                let mut first = start_builder(ntlm_url).header(
                    reqwest::header::AUTHORIZATION,
                    ntlm_sig::header_value(&ntlm_sig::negotiate_message()),
                );
                if request.options.timeout_ms > 0 {
                    first = first.timeout(Duration::from_millis(request.options.timeout_ms));
                }
                let challenged = first.send().await.map_err(http_error)?;
                let values: Vec<String> = challenged
                    .headers()
                    .get_all(reqwest::header::WWW_AUTHENTICATE)
                    .iter()
                    .filter_map(|v| v.to_str().ok().map(str::to_string))
                    .collect();
                let refs: Vec<&str> = values.iter().map(String::as_str).collect();
                match (challenged.status(), ntlm_sig::find_token(&refs)) {
                    // No challenge: the server did not ask for NTLM, or refuses it. Return it as is.
                    (reqwest::StatusCode::UNAUTHORIZED, Some(token)) => {
                        let challenge = ntlm_sig::parse_challenge(&token).map_err(|e| {
                            DomainError::InvalidInput(format!(
                                "The NTLM challenge could not be read: {e}"
                            ))
                        })?;
                        // The connection returns to the pool only once the body is drained, and
                        // message 3 must reuse it.
                        let next_url = challenged.url().clone();
                        let _ = challenged.bytes().await;
                        let (user, dom) = ntlm_sig::split_account(username, domain);
                        let workstation = std::env::var("COMPUTERNAME")
                            .or_else(|_| std::env::var("HOSTNAME"))
                            .unwrap_or_else(|_| "ROCKET".to_string())
                            .to_uppercase();
                        let message = ntlm_sig::authenticate_message(
                            &challenge,
                            &user,
                            password,
                            &dom,
                            &workstation,
                            ntlm_sig::random_nonce(),
                            ntlm_sig::file_time_now(),
                        );
                        let second = finish_builder(start_builder(next_url))?.header(
                            reqwest::header::AUTHORIZATION,
                            ntlm_sig::header_value(&message),
                        );
                        second.send().await.map_err(http_error)?
                    }
                    _ => challenged,
                }
            }
            _ => builder.send().await.map_err(http_error)?,
        };
```

Notes for the edit: the Digest block that follows keeps working because `response` is a plain `reqwest::Response` as before; `start_builder` and `finish_builder` are the closures already defined above this block; the unused `builder` in the NTLM arm is intentional (it is built for every scheme before the match).

- [ ] **Step 9: Run to verify the handshake passes**

Run: `cargo test -j4 -p rocket-infra ntlm_tests`
Expected: PASS (5 tests). If `ntlm_handshake_uses_one_connection` fails with different connection ids, the first response body was not drained before message 3, or the client is not the single-connection one: check `pool_max_idle_per_host(1)` and `http1_only()` in `build_client`.

Run: `cargo test -j4 -p rocket-infra reqwest_executor`
Expected: PASS (existing OAuth, digest and SigV4 tests unchanged).

- [ ] **Step 10: Write the failing frontend tests**

In `src/lib/__tests__/auth-type-options.test.ts` replace the whole `describe` with (NTLM is now a normal option, so no read-only extras remain):

```ts
import { describe, expect, it } from 'vitest';
import { NTLM_OPTION, OAUTH1_OPTION, withCurrentAuthType } from '@/lib/auth-type-options';

const base = [
  { label: 'None', value: 'none' as const },
  { label: 'Basic', value: 'basic' as const },
];

describe('withCurrentAuthType', () => {
  it('has no read-only extras left: every auth type is a normal option', () => {
    for (const current of ['ntlm', 'oauth1', 'digest', 'basic'] as const) {
      expect(withCurrentAuthType(base, current)).toBe(base);
    }
  });

  it('exports the shared options with their labels', () => {
    expect(NTLM_OPTION).toEqual({ label: 'NTLM', value: 'ntlm' });
    expect(OAUTH1_OPTION).toEqual({ label: 'OAuth 1.0', value: 'oauth1' });
  });
});
```

Append to `src/lib/__tests__/auth-type-defaults.test.ts`:

```ts
  it('defaults ntlm to empty credentials and keeps existing ones', () => {
    expect(authStateForType('ntlm', none).ntlm).toEqual({ username: '', password: '', domain: '' });
    const prev = { authType: 'ntlm', ntlm: { username: 'u', password: 'p', domain: 'D' } } as const;
    expect(authStateForType('ntlm', prev).ntlm).toEqual({ username: 'u', password: 'p', domain: 'D' });
  });
```

In `src/components/request/__tests__/AuthEditor.test.tsx` replace the test `shows a read-only note for ntlm and does not offer fields` with:

```tsx
  it('edits the ntlm username, password and domain', () => {
    const onChange = vi.fn();
    const auth: AuthState = {
      authType: 'ntlm',
      ntlm: { username: 'u', password: 'p', domain: 'CORP' },
    };
    render(<AuthEditor auth={auth} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText('Username'), { target: { value: 'bob' } });
    expect(onChange).toHaveBeenLastCalledWith({
      authType: 'ntlm',
      ntlm: { username: 'bob', password: 'p', domain: 'CORP' },
    });

    fireEvent.change(screen.getByLabelText('Domain'), { target: { value: 'OTHER' } });
    expect(onChange).toHaveBeenLastCalledWith({
      authType: 'ntlm',
      ntlm: { username: 'u', password: 'p', domain: 'OTHER' },
    });
  });
```

- [ ] **Step 11: Run to verify failure**

Run: `yarn test auth-type auth-type-defaults AuthEditor`
Expected: FAIL (`NTLM_OPTION` is not exported, `authStateForType('ntlm')` has no `ntlm`, the editor shows a note).

- [ ] **Step 12: Implement the NTLM UI**

`src/lib/auth-type-options.ts`: add the option and empty the read-only list (keep the mechanism, since a future auth type can use it):

```ts
export const NTLM_OPTION: AuthTypeOption = { label: 'NTLM', value: 'ntlm' };

// Auth types that can be kept and sent but not picked from the list. They show up in the
// selector only while the request already uses one, so its current value always has a label.
// Every type has an editor now, so the list is empty.
const READ_ONLY_OPTIONS: AuthTypeOption[] = [];
```

`src/lib/auth-type-defaults.ts`: add inside `authStateForType`:

```ts
  if (authType === 'ntlm') next.ntlm = prev.ntlm ?? { username: '', password: '', domain: '' };
```

`RequestPanel.tsx`: import `NTLM_OPTION` with `OAUTH1_OPTION` and add `NTLM_OPTION,` after `OAUTH1_OPTION,` in `BASE_AUTH_TYPES`. `CollectionOverviewTab.tsx`: do the same for `COLLECTION_AUTH_TYPES`. `AuthEditor.tsx`: replace the NTLM info card (lines 205-215) with:

```tsx
      {auth.authType === 'ntlm' && (
        <>
          <UserPasswordCard
            username={auth.ntlm?.username ?? ''}
            password={auth.ntlm?.password ?? ''}
            onChange={(patch) =>
              onChange({
                ...auth,
                ntlm: { username: '', password: '', domain: '', ...auth.ntlm, ...patch },
              })
            }
            variableContext={variableContext}
            onNavigateToSource={onNavigateToSource}
          />
          <Card>
            <CardContent className='space-y-1.5 p-4'>
              <Label className='text-xs text-muted-foreground'>Domain</Label>
              <SingleLineEditor
                aria-label='Domain'
                placeholder='Domain (optional)'
                className='text-sm'
                value={auth.ntlm?.domain ?? ''}
                onChange={(domain) =>
                  onChange({
                    ...auth,
                    ntlm: { username: '', password: '', ...auth.ntlm, domain },
                  })
                }
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
              <p className='text-xs text-muted-foreground'>
                Leave the domain empty and type the user as DOMAIN\user, or fill it in separately.
              </p>
            </CardContent>
          </Card>
        </>
      )}
```

- [ ] **Step 13: Update the crate docs**

In `crates/rocket-infra/CLAUDE.md`, replace the last sentence of the "WSSE, Digest, NTLM" paragraph ("NTLM is not implemented yet, so `apply_auth` returns an `InvalidInput` error instead of sending the request unauthenticated.") with: "NTLM (NTLMv2 only, no signing or sealing) is a three-step handshake in `execute`: message 1 without a body, the server's challenge in a 401, then message 3 with the full request, all on one dedicated single-connection HTTP/1 client (`ClientBuild.single_connection`). The first response body is drained so the connection returns to the pool. A response that is not a 401, or a 401 with no NTLM challenge, is returned as is, and a rejected login is the plain 401." Update the same paragraph's first sentence to drop "NTLM" from what `apply_auth` handles, and add: "Cookies: `RepoCookieStore` backs a reqwest cookie provider with the existing `CookieRepository` (see `cookie_store.rs`); `RequestOptions.use_cookie_jar` turns it off per request, and the load test runs with it off. Proxy: `ReqwestExecutor::with_proxy` reads a `SharedProxy` on every request and the client cache key includes its generation. SigV4: signed after the body is applied (`apply_aws_sigv4`), profile credentials come from `aws_profile.rs`."

In `crates/rocket-http/CLAUDE.md` add module-map rows: `ntlm_sig` (NTLMv2 message building and parsing), `path_params` (`substitute_path_params`), `proxy` (`ProxySettings`, `ResolvedProxy`, `SharedProxy`, `ProxySettingsRepository`), and extend the `cookie` row with "RFC 6265 `Set-Cookie` parsing and request cookie selection (`parse_set_cookie`, `cookies_for_request`)" and the `response` row with "`body_from_bytes` splits a body into text or a base64 payload". Replace the `aws_sig` row's text with "`sign_request` and `sign_request_with_payload_hash` (full AWS Signature Version 4)".

- [ ] **Step 14: Run all checks**

Run: `yarn test auth-type auth-type-defaults AuthEditor RequestPanel CollectionOverview`
Expected: PASS.

Run: `yarn tsc --noEmit && yarn check`
Expected: PASS.

Run: `cargo test -j4 -p rocket-http ntlm_sig && cargo test -j4 -p rocket-infra ntlm_tests reqwest_executor && cargo check -j4 --workspace --tests`
Expected: PASS.

Manual check (needs a real NTLM endpoint, for example an IIS site with Windows authentication and `NTLM` as the only provider): create a request with NTLM auth and a `DOMAIN\user` or separate domain, send it, and confirm a 200 instead of a 401. Repeat with a wrong password and confirm a 401 response (not an error toast). Repeat with a POST body.

- [ ] **Step 15: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, staging by explicit path (every file listed under Files for this task, plus `Cargo.lock`). Suggested subject: `feat(http): implement NTLM authentication`.

---

## Next Plan

[Plan 04: SOAP and WSDL import](2026-10-05-protocol-parity-plan-04-soap-wsdl-import.md). It is independent of this plan's code. Chain to it automatically when this one finishes.
