# Flow Request Debug Mode — Design

**Date:** 2026-09-29.
**Branch:** `worktree-flow-phase2-branching`.
**Scope:** backend (rocket-flow, rocket-app, rocket-shared) and frontend.
**Related issues:** #43 (normal-send Console leaks secrets) and #44 (backend query params not resolved) are separate and out of scope.

## 1. Problem and goal

A Flow Request node's wire script only shows its value before `{{vars}}` are resolved. When a request returns 400, the user cannot see what was actually sent: the resolved URL, headers and body, after variables, vault secrets, wire overrides and the pre-request script.

**Goal:** a per-node **Debug mode**. When it is on, each run adds one Console row per debug node showing the request as sent and the response it got, with secrets masked.

## 2. Decisions

| Question | Decision |
|---|---|
| Where the toggle lives | The Request card's **⋮** becomes a menu: **Edit properties** and a checkable **Debug mode**. A lucide `Bug` badge on the card shows it is on. |
| Where the output goes | One **HTTP row in the Console** per debug node (the same expandable row a normal send makes): request headers and body plus response status, duration, headers and body. A text log line was offered and declined. |
| What "the request" means | The request as sent: after `{{var}}` resolution, vault secrets, collection auth merge, wire overrides and pre-request script mutations. |
| Masking | Two layers, see §4. |

## 3. Persistence

`FlowNodeKind::Request` gains `debug: bool` with `#[serde(default, skip_serializing_if = "is_false")]`. Old flow files load with `debug = false`, and a node without debug serializes exactly as before. The TS mirror gains `debug?: boolean`. No camelCase rename (persistence struct; the key is one word).

## 4. Backend

### 4.1 Capturing the sent request
`RequestExecutionService` gains a Flow-facing variant of `execute_with_external_secrets` that records a clone of `PhaseState.http_request` right after the before-request phase, into an out-parameter. `execute_with_external_secrets` delegates to it and discards the capture, so every other caller is unchanged. Because the capture happens before `send_request`, a network failure still has the request.

### 4.2 Building the debug record
For a Request node with `debug = true`, the Flow builds a `FlowDebugRequest`:
- `method`, and `url` = the resolved URL with the enabled query params appended the way the executor does (`reqwest_executor.rs:241-251`).
- `headers`: the resolved headers, plus one synthesized auth line: `Authorization: Bearer ••••••` (Bearer), `Authorization: Basic ••••••` (Basic), `<key>: ••••••` (API key in a header) or `Auth: API key in query "<key>"` (API key in the query), and `Auth: <type>` for OAuth2, AWS SigV4, WSSE, Digest and NTLM. The auth value is never shown.
- `body`: the resolved text body, or form fields as `key=value` lines. No body gives `None`.
- The response when there is one: `status`, `status_text`, `duration_ms`, `size_bytes`, response `headers` and `body`. A send failure gives no response and the error message.

### 4.3 Masking (two layers)
1. **By value.** Every string (URL, header values, body, response headers and body) is passed through one shared `redact_secrets(text, &secret_values)` that replaces each secret value with `••••••`, longest value first so a secret that is a prefix of another cannot leave a partial leak. The secret set is the one Flow script logs already use (secret env, collection and global vars plus external vault secrets; the `MIN_REDACTION_LEN = 6` floor applies). The history URL redaction reuses the same helper.
2. **By header name.** The values of `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie` and `X-Api-Key` (case-insensitive) are always `••••••`, on the request and the response, because value masking cannot see tokens from non-secret variables, OAuth2 tokens or Basic base64.

### 4.4 Transport
`FlowStepResult` and `DomainEvent::FlowStepCompleted` gain `debug_request: Option<FlowDebugRequest>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` (camelCase `debugRequest` on the summary DTO, snake_case on the event, as each side already does). It is filled through an out-parameter, like `logs`, so a failed step keeps it. It is set only when the node's `debug` is true.

## 5. Frontend

### 5.1 Console
When the final run summary arrives, each step with `debugRequest` becomes one `addHttpEntry` row: request method, URL, headers, body, and the response fields (a send failure uses status `0` and status text `Error`, like the existing error path in `execute-request.ts`). The row's name is `<flow name> › <node label>`: `HttpConsoleEntry` gains an optional `requestName`, shown in the row when present (normal sends leave it unset and look as before). Logs come from the summary only, as for script logs.

### 5.2 The ⋮ menu and badge
- `NodeMenuButton` on a Request node becomes a shadcn `DropdownMenu`: **Edit properties** (the current action) and a `DropdownMenuCheckboxItem` **Debug mode** that flips `kind.debug` through `updateNodeKind`, marking the tab unsaved. Other node kinds keep today's single-action button.
- The trigger keeps `nodrag nokey`. `DropdownMenuContent` carries `nokey`, because it is portalled and React Flow deletes a selected node on Backspace otherwise.
- A lucide `Bug` icon (amber, `aria-label='Debug mode on'`, `data-testid='request-node-debug-badge'`) sits in the card title while debug is on.
- The Request properties panel also shows a **Debug mode** switch, so the setting is discoverable there too.

## 6. Out of scope
- Fixing #43 and #44.
- Headers the HTTP client adds itself (`User-Agent`, `Content-Length`, a `Content-Type` derived from the body mode). The Console row does not show them; this is a known limit.
- A debug node whose pre-request script fails produces no record, because nothing was sent.
- Debug mode for the normal request tab or the collection runner.

## 7. Testing
- **Backend:** old flow YAML round-trips unchanged and `debug: true` round-trips; `redact_secrets` masks longest-first and honours the floor; header-name masking on request and response; a Flow run with a debug node records the resolved method, URL with query params, headers with the auth pseudo-line, body and response; a secret env var used in the URL, a header and the body is masked everywhere; a send failure still records the request; `debug = false` records nothing.
- **Frontend:** the ⋮ menu offers Edit properties and Debug mode; toggling flips `kind.debug` and marks the tab dirty; the badge shows only when on; Backspace with the menu open deletes nothing; a summary step with `debugRequest` adds one Console HTTP row with the right name and fields; a step without it adds none.
- **Manual:** in `yarn tauri dev`, turn on Debug mode on the Login node, run, and read the Console row for a 400.
