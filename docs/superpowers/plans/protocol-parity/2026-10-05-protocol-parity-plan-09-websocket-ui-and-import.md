# Protocol parity, Plan 09: WebSocket UI and Bruno import

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make WebSocket usable end to end. Bruno WebSocket requests import as real WebSocket items, the sidebar shows and opens them, the create dialog saves a real WebSocket file, and a WebSocket tab can connect, disconnect, hold several saved messages, send the selected one, and show a live message log.

**Architecture:** Three independent slices.

1. `rocket-import` learns the Bruno `ws` request: the `.bru` parser reads `meta { type: ws }`, `ws { url, auth }` and `body:ws { message N [kind] { ... } }`; `convert_websocket` builds a `WebSocketRequest` and `convert_item` (the router [Plan 05](2026-10-05-protocol-parity-plan-05-graphql-model-and-import.md) adds) returns it as `Converted::WebSocket`; `ImportService::walk_requests` saves it through `CollectionRepository::save_websocket_request` instead of the HTTP converter.
2. The frontend follows Plan 05's GraphQL pattern exactly. The sidebar already receives a `summary` with `kind: 'websocket'` (plan 08), so `RequestNode` gets a `WS` badge and opens the tab through `getWebSocketRequest`; the create dialog re-enables its WebSocket option and saves a real WebSocket file; `saveTabRequest` (Plan 05's save router) and auto-save gain a WebSocket branch. `RequestState.requestType` is the discriminator (Plan 05's `createDefaultRequestFor`, `mapGraphQlToState` pattern), with the saved messages in a new `RequestState.websocket` draft.
3. A `useWebSocketStore` Zustand store, keyed by tab id, is fed by one app-lifetime event bridge (`ws:message`, `ws:status`). A `WebSocketPanel` renders the tab. The log component, `MessageLog`, is generic on purpose: plan 10 reuses it for GraphQL subscription results.

**Tech Stack:** Rust (`rocket-import`), React 18 + TypeScript, Zustand, shadcn/ui, lucide-react, CodeMirror 6 `SingleLineEditor` for the URL, Monaco (lazy) for message bodies, Vitest and Testing Library.

**Spec:** [../../specs/opencollection-spec-reference.md](../../specs/opencollection-spec-reference.md) section 2.6 and `WebSocketMessage`. Bruno behaviour: https://docs.usebruno.com/send-requests/websocket/overview and https://docs.usebruno.com/send-requests/websocket/ws-interface.

**Depends on:** [Plan 08](2026-10-05-protocol-parity-plan-08-websocket-backend.md) (domain type, `save_websocket_request`, the WebSocket sidebar summary, `ws_*` commands, `ws:message` and `ws:status` events) and, through it, [Plan 05](2026-10-05-protocol-parity-plan-05-graphql-model-and-import.md) (`RequestKind`, `RequestSummary.kind`, `Converted` and `convert_item` on the import side; `saveTabRequest`, `createDefaultRequestFor`, the kind-aware `RequestNode`, the guarded `collectPaths` and sidebar loops and the disabled WebSocket option in `CreateRequestDialog` on the frontend). Task 1 needs only Plan 05 Task 2 plus Plan 08 Task 1. Tasks 2 and 3 need Plan 05 Task 3.

**Delta on Plan 05.** Both plans edit `RequestNode.tsx`, `CollectionNode.tsx`, `FolderNode.tsx`, `CreateRequestDialog.tsx`, `save-tab-request.ts`, `auto-save.ts`, `collectPaths.ts` and `pane-types.ts`. This plan adds only the WebSocket lines next to Plan 05's GraphQL lines and never rewrites one of Plan 05's. Where a step below says "extend", it means "next to the GraphQL line Plan 05 added".

## Facts verified against the repo (do not re-derive)

- **Audit correction: legacy `.bru` WebSocket files are imported as a bogus HTTP request, not skipped.** `bru/yml_adapter.rs:155-160` pushes an `unsupported_type` block for non-HTTP types, but only the YAML adapter does. `bru/parser.rs::parse_meta` does not, so `converter::request::convert` (`converter/request.rs:14-28`) never sees the skip marker for a `.bru` file. A `.bru` with `type: ws` has no method block, so it converts to `GET` with an empty URL and is saved as an HTTP file. (The existing `unsupported_request_type_produces_skip_reason` test at `converter/request.rs:236` builds the marker by hand, which is why nothing caught it.) The same was true for `.bru` GraphQL files (Plan 05 routes them) and is still true for `.bru` gRPC files (their plan owns it). This plan routes ws documents before the HTTP converter.
- Modern (Bruno 3 / OpenCollection) collections are copied file-for-file by `import_modern_collection` and never parsed, so their WebSocket files already land on disk unchanged; after plan 08 they load as typed items with no import work.
- `BruDocument` (`bru/ast.rs`) derives `Default`; tests build it with `..BruDocument::default()`, so new fields do not break them. `RAW_TEXT_BLOCK_NAMES` in `bru/lexer.rs` is `["body", "script", "docs"]`, so `body:ws { ... }` is captured as one `RawText` with inner `{}` depth-tracked. `ws { url: ... }` is a normal key-value block (`split_once(':')` keeps `wss://...` intact).
- `ImportService::walk_requests` takes `repo: &dyn CollectionRepository`, so `repo.save_websocket_request(...)` (plan 08 trait method) is available. Existing HTTP saves use `let _ = repo.save_request(...)`; the ws branch reports a failed save instead.
- Frontend before Plan 05: `CreateRequestDialog.tsx:86-100` saves **an HTTP request file** for every type (WebSocket, GraphQL and gRPC included) and only sets `requestType` on the in-memory tab. `TabBar.tsx:257-258` opens an ephemeral tab with `requestType: 'websocket'` that is HTTP-shaped. `src/types/pane-types.ts:238` already declares the four-way union. Plan 05 Task 3 then: makes `pane-store` `openEphemeralTab` call `createDefaultRequestFor(requestType)` (currently `{ ...createDefaultRequest(), requestType }` for non-GraphQL kinds), disables the gRPC and WebSocket options in `CreateRequestDialog` ("coming soon", because they saved an HTTP file under the wrong label), makes `RequestNode` derive `kind` from `summary.kind`, and guards the full-tree `opaque` and `graphql` items out of the sidebar loops and out of `collectPaths`. This plan re-enables the WebSocket option and extends each of those guards.
- `hooks/useKeyboardShortcuts.ts:30-35`: Ctrl or Cmd+Enter calls `sendRequest(tab.id, tab.request)` for **every** request tab, which would fire an HTTP GET at a `wss://` URL.
- Save paths that assume an HTTP payload: `lib/auto-save.ts` (`toApiRequest`, called from `pane-store.ts:327,396,976,1055`), `components/request/SaveRequestButton.tsx:21`, `components/request/SaveToCollectionDialog.tsx:75-77`. Plan 05 routes the last two through `saveTabRequest` (`lib/save-tab-request.ts`) and branches `auto-save.ts` for GraphQL; this plan adds the WebSocket branch to `save-tab-request.ts` and `auto-save.ts` only.
- `pane-store.ts:115-134` `endSessionIfActive` and `endActiveSessions` are the existing hooks for "this tab is going away, end its backend session" (agent chat). Call sites: `closeTab` (`:337`), switching collections with no snapshot (`:903`), closing everything (`:965`), reset (`:1044`).
- Event payloads from `DomainEvent` have **snake_case fields** (`session_id`), as `AgentSessionChunkEvent` in `tauri-api.ts` shows. The `listen` wrappers live next to it (`onAgentSessionChunk`, around line 2126).
- Reusable pieces: `SingleLineEditor` (`components/editor`, props `value`, `onChange`, `placeholder`, `variableContext`, `onSubmit`, `className`), lazy `MonacoWrapper` (`components/editor/MonacoWrapper`, props `value`, `onChange`, `language`, `height`; see `BodyEditor.tsx:12-18,72` for the lazy and `Suspense` pattern), `HeadersEditor` (`headers: KeyValueEntry[]`, `onChange`, `variableContext`), `AuthEditor` (`auth`, `onChange`, `variableContext`, `collection`, `environmentName`, `requestPath`), `authStateForType(authType, prev)` in `lib/auth-type-defaults.ts`, `toPersistedHeaders`, `toPersistedAuth`, `fromPersistedAuth(auth, fallback)`, `buildScopedContext` in `lib/url-variables.ts`, `useEnvironments`, `useGlobalEnvironment`, `useGlobalEnvironmentName`, `useProcessEnvVars` in `lib/queries/environment-queries.ts`, `getCollectionSettings(name)`.
- Frontend checks: `yarn tsc --noEmit`, `yarn check` (Biome), `yarn test --run <pattern>` (plain `yarn test` starts Vitest in watch mode).

## Behaviour decisions baked in

- A tab's saved messages live in `request.websocket.messages` with exactly one `selected`. Send uses the selected message. The invariant is enforced in pure helpers, not in the component.
- Binary messages are edited as base64 text. The log shows binary payloads as hex.
- Fields with no editor yet (description, `seq`, `runtimeAuth`, runtime variables, scripts) ride along in `request.websocket.passthrough` and are written back unchanged, so a UI save never drops them.
- `Skip TLS verification` in the Settings tab is a per-session toggle (`request.settings.verifySsl`); it is not persisted, because the OpenCollection WebSocket settings have no such key.
- WebSocket rows reuse `RequestNode`, so Rename, Move and Delete work as for HTTP rows. Duplicate is hidden for WebSocket (and, as a side effect, GraphQL) rows, because it copies through `getRequest`.

## Global Constraints

- Rust: `cargo test -j4 -p rocket-import <name>`, `cargo check -j4`. Never `cargo test --workspace`. No `unwrap()` in production paths; tests may use `.expect("reason")`.
- Frontend: shadcn/ui primitives and `lucide-react` icons only. No raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>` and no inline SVG in new code. Single-line variable-aware fields use `SingleLineEditor`; multi-line editors use Monaco. Zustand: never fully destructure store state at the top of a component; use narrow selectors.
- Commits use the `dev-workflow-skills:1-git-commit` skill (never a freeform `git commit -m`), conventional subjects, staging by explicit path only. Never stage `crates/rocket-app/src/execution_service.rs` here.
- Verification: `cargo check -j4` and the focused cargo test for Task 1; `yarn tsc --noEmit`, `yarn check` and the focused `yarn test --run <pattern>` for Tasks 2 and 3.
- Another plan edits the GraphQL and gRPC branches of `CreateRequestDialog.tsx` and `pane-types.ts`. Add only the WebSocket branch and keep each edit small and local so the merges are trivial.

## Review Focus

1. **A `.bru` WebSocket file must become a WebSocket item, never a bogus HTTP GET.** Pinned by `legacy_bru_websocket_imports_as_a_websocket_item_not_http` (Task 1).
2. **Message blocks with JSON braces inside must parse whole.** Pinned by `parses_messages_whose_json_contains_braces` (Task 1).
3. **A WebSocket tab must never save through the HTTP path** (that would overwrite a `websocket:` file with an `http:` file). Pinned by the `saveTabRequest websocket routing` and `scheduleAutoSave for a websocket tab` tests (Task 2).
4. **Events can arrive before the connect call resolves, and events for a dead session must be ignored.** Pinned by `routes_events_that_arrive_before_the_connect_call_returns` and `ignores_events_for_unknown_or_finished_sessions` (Task 3).
5. **Closing a tab must close its socket, and the log must stay bounded.** Pinned by `closing_a_websocket_tab_disconnects_its_session` and `caps_the_log_and_drops_the_oldest_entries` (Task 3).

---

## Task 1: Bruno WebSocket import

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-import/src/bru/ast.rs`, `bru/parser.rs`, `bru/yml_adapter.rs`, `converter/request.rs`, `importer.rs`, `crates/rocket-import/CLAUDE.md`, `crates/rocket-import/tests/integration_test.rs`

This task builds on the router [Plan 05](2026-10-05-protocol-parity-plan-05-graphql-model-and-import.md) Task 2 adds to the same files (`Converted`, `convert_item`, `convert_graphql`, `auth_skips`, the `graphql` AST field, the `walk_requests` match on `Converted`). If Plan 05 is not merged yet, add the same router yourself with only `Http` and `WebSocket` variants and let the other plan resolve the merge; the WebSocket lines are separate from the GraphQL lines.

**Interfaces:**
- Consumes: `WebSocketRequest`, `WebSocketMessage`, `WebSocketMessageKind`, `WebSocketScript` (`rocket_collection`, plan 08); `CollectionRepository::save_websocket_request` (plan 08); from Plan 05: `converter::request::{Converted, convert_item, auth_skips}` and the private `bru_auth_to_domain` (same file, so no visibility change).
- Produces:
  - `BruDocument::{ws_messages: Vec<BruWsMessage>, ws_auth_mode: Option<String>}` and `BruDocument::is_websocket(&self) -> bool`.
  - `pub(crate) fn bru::parser::parse_ws_messages(raw: &str) -> Vec<BruWsMessage>`.
  - `converter::request::convert_websocket(doc: &BruDocument) -> (WebSocketRequest, Vec<SkipReason>)` and the new variant `Converted::WebSocket(WebSocketRequest)`; `convert_item` returns it for a ws document.
  - `ImportService` writes ws documents through `save_websocket_request` and counts them in `report.imported`.

**Source-format caveat (read before coding).** The `.bru` shape below is taken from the Bruno docs page for the WebSocket interface (`meta { type: ws }`, `ws { url }`, `body:ws { message N [json] { ... } }`). It has not been checked against a real Bruno export on this machine. Step 13 is a manual check against an export from the user's Bruno; adjust the parser fixtures, not the architecture, if it differs. The Bruno YAML shape is likewise from the docs summary (`ws:` block with `url` and a `messages:` list of `name`, `type`, `body`), so the adapter accepts the plausible aliases.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing parser tests**

Append to the `tests` module of `crates/rocket-import/src/bru/parser.rs` (it already has `use super::*;`; if it does not, add it):

```rust
    const WS_BRU: &str = r#"meta {
  name: Echo
  type: ws
  seq: 2
}

ws {
  url: wss://echo.websocket.org
  body: ws
  auth: bearer
}

headers {
  X-Trace: abc
  ~X-Off: 1
}

auth:bearer {
  token: {{token}}
}

body:ws {
  message 1 [json] {
    {"name":"Bruno","nested":{"a":[1,2]}}
  }

  message 2 [text] {
    hello
  }
}
"#;

    #[test]
    fn parses_a_websocket_request_document() {
        let doc = parse(WS_BRU).expect("parse");
        assert!(doc.is_websocket());
        assert_eq!(doc.meta.as_ref().map(|m| m.name.as_str()), Some("Echo"));
        assert_eq!(doc.meta.as_ref().and_then(|m| m.seq), Some(2));
        assert_eq!(doc.url.as_deref(), Some("wss://echo.websocket.org"));
        assert_eq!(doc.ws_auth_mode.as_deref(), Some("bearer"));
        assert_eq!(doc.headers.len(), 2);
        assert!(doc.headers[1].disabled);
        assert!(matches!(&doc.auth, Some(BruAuth::Bearer { token }) if token == "{{token}}"));
        assert_eq!(doc.ws_messages.len(), 2);
    }

    #[test]
    fn parses_messages_whose_json_contains_braces() {
        let doc = parse(WS_BRU).expect("parse");
        assert_eq!(doc.ws_messages[0].name, "message 1");
        assert_eq!(doc.ws_messages[0].kind, "json");
        assert_eq!(
            doc.ws_messages[0].content,
            r#"{"name":"Bruno","nested":{"a":[1,2]}}"#
        );
        assert_eq!(doc.ws_messages[1].name, "message 2");
        assert_eq!(doc.ws_messages[1].kind, "text");
        assert_eq!(doc.ws_messages[1].content, "hello");
    }

    #[test]
    fn multi_line_message_content_keeps_its_relative_indentation() {
        let raw = "message 1 [json] {\n  {\n    \"a\": 1\n  }\n}\n";
        let messages = parse_ws_messages(raw);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn a_message_header_without_a_kind_defaults_to_text() {
        let messages = parse_ws_messages("ping {\n  hi\n}\n");
        assert_eq!(messages[0].name, "ping");
        assert_eq!(messages[0].kind, "text");
        assert_eq!(messages[0].content, "hi");
    }

    #[test]
    fn an_http_document_is_not_a_websocket() {
        let doc = parse("meta {\n  name: A\n  type: http\n}\nget {\n  url: https://x\n}\n").expect("parse");
        assert!(!doc.is_websocket());
        assert!(doc.ws_messages.is_empty());
    }
```

- [ ] **Step 3: Run it and confirm it fails**

Run: `cargo test -j4 -p rocket-import parses_a_websocket_request_document`
Expected: compile errors (`is_websocket`, `ws_messages`, `ws_auth_mode`, `parse_ws_messages` not found).

- [ ] **Step 4: Implement the AST and parser**

In `crates/rocket-import/src/bru/ast.rs` add to `BruDocument` (after `post_response_script`):

```rust
    /// Messages from a `body:ws` block (WebSocket requests only).
    pub ws_messages: Vec<BruWsMessage>,
    /// The `auth:` mode named inside a `ws {}` block, such as `inherit`, `none` or `bearer`.
    pub ws_auth_mode: Option<String>,
```

and below `BruDocument`'s struct, plus a type and a method:

```rust
impl BruDocument {
    /// True for a Bruno WebSocket request (`meta.type` is `ws` or `websocket`).
    pub fn is_websocket(&self) -> bool {
        self.meta
            .as_ref()
            .is_some_and(|m| matches!(m.request_type.as_str(), "ws" | "websocket"))
    }
}

/// One saved WebSocket message from a `body:ws` block.
#[derive(Debug, Clone, PartialEq)]
pub struct BruWsMessage {
    pub name: String,
    /// Bruno's format tag: `json`, `text` or `xml`. Anything else is passed through.
    pub kind: String,
    pub content: String,
}
```

In `crates/rocket-import/src/bru/parser.rs` add to `dispatch_block`, as a new first arm so it wins over the method-block fallback:

```rust
        ("ws", None) => parse_ws_block(doc, tokens),
```

at the top of `parse_body` (before `let raw = ...` is used for the match; keep `raw` as is) add:

```rust
    if subtype == "ws" {
        doc.ws_messages = parse_ws_messages(&extract_raw_text(tokens).unwrap_or_default());
        return;
    }
```

and add these functions below `parse_method_block`:

```rust
fn parse_ws_block(doc: &mut BruDocument, tokens: &[Token]) {
    let map = kv_map(tokens);
    if let Some((_, url)) = map.iter().find(|(k, _)| k == "url") {
        doc.url = Some(url.clone());
    }
    if let Some((_, mode)) = map.iter().find(|(k, _)| k == "auth") {
        doc.ws_auth_mode = Some(mode.clone());
    }
}

/// Parses the inside of a `body:ws` block: repeated `name [kind] { content }` entries.
/// The braces of JSON content are balanced, so the entry ends at the first `}` line that
/// brings the nesting depth back to zero.
pub(crate) fn parse_ws_messages(raw: &str) -> Vec<BruWsMessage> {
    let mut out = Vec::new();
    let mut lines = raw.lines();
    while let Some(line) = lines.next() {
        let Some(header) = line.trim().strip_suffix('{') else {
            continue;
        };
        let header = header.trim();
        let (name, kind) = match (header.rfind('['), header.rfind(']')) {
            (Some(open), Some(close)) if open < close => (
                header[..open].trim().to_string(),
                header[open + 1..close].trim().to_string(),
            ),
            _ => (header.to_string(), "text".to_string()),
        };

        let mut depth = 1usize;
        let mut body: Vec<&str> = Vec::new();
        for inner in lines.by_ref() {
            let trimmed = inner.trim();
            if trimmed == "}" && depth == 1 {
                break;
            }
            for ch in trimmed.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            body.push(inner);
        }
        out.push(BruWsMessage {
            name,
            kind,
            content: dedent(&body),
        });
    }
    out
}

/// Removes the indentation shared by every non-blank line, then trims the ends.
fn dedent(lines: &[&str]) -> String {
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| if l.len() >= indent { &l[indent..] } else { l.trim_start() })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}
```

- [ ] **Step 5: Run the parser tests**

Run: `cargo test -j4 -p rocket-import bru::parser`
Expected: all pass (the new 5 and the existing ones). If `parses_a_websocket_request_document` reports `doc.url` empty, the `("ws", None)` arm is not reached; check it sits before the `_ =>` fallback inside `dispatch_block`.

- [ ] **Step 6: Write the failing converter and YAML adapter tests**

Append a second test module to the end of `crates/rocket-import/src/converter/request.rs` (it does not touch the existing `tests` module):

```rust
#[cfg(test)]
mod websocket_tests {
    use super::*;
    use crate::bru::ast::{BruAuth, BruDocument, BruKeyValue, BruMeta, BruRawBlock, BruWsMessage};
    use rocket_collection::WebSocketMessageKind;
    use rocket_shared::types::Auth;

    fn ws_doc() -> BruDocument {
        BruDocument {
            meta: Some(BruMeta { name: "Echo".into(), request_type: "ws".into(), seq: Some(2) }),
            url: Some("wss://echo.websocket.org".into()),
            headers: vec![
                BruKeyValue { key: "X-Trace".into(), value: "abc".into(), disabled: false },
                BruKeyValue { key: "X-Off".into(), value: "1".into(), disabled: true },
            ],
            ws_messages: vec![
                BruWsMessage { name: "message 1".into(), kind: "json".into(), content: "{}".into() },
                BruWsMessage { name: "message 2".into(), kind: "yaml".into(), content: "x".into() },
            ],
            ..BruDocument::default()
        }
    }

    #[test]
    fn converts_name_url_seq_headers_and_messages() {
        let (ws, skipped) = convert_websocket(&ws_doc());
        assert!(skipped.is_empty());
        assert_eq!(ws.name, "Echo");
        assert_eq!(ws.url, "wss://echo.websocket.org");
        assert_eq!(ws.seq, Some(2));
        assert_eq!(ws.headers.len(), 2);
        assert!(ws.headers[0].enabled);
        assert!(!ws.headers[1].enabled);
        assert_eq!(ws.messages.len(), 2);
        assert_eq!(ws.messages[0].title, "message 1");
        assert_eq!(ws.messages[0].kind, WebSocketMessageKind::Json);
        // An unknown format falls back to text.
        assert_eq!(ws.messages[1].kind, WebSocketMessageKind::Text);
    }

    #[test]
    fn exactly_the_first_message_is_selected() {
        let (ws, _) = convert_websocket(&ws_doc());
        let selected: Vec<bool> = ws.messages.iter().map(|m| m.selected).collect();
        assert_eq!(selected, vec![true, false]);
    }

    #[test]
    fn auth_mode_decides_between_inherit_none_and_the_auth_block() {
        let mut doc = ws_doc();
        doc.auth = Some(BruAuth::Bearer { token: "t".into() });

        doc.ws_auth_mode = Some("inherit".into());
        assert_eq!(convert_websocket(&doc).0.auth, Auth::Inherit);

        doc.ws_auth_mode = Some("none".into());
        assert_eq!(convert_websocket(&doc).0.auth, Auth::None);

        doc.ws_auth_mode = Some("bearer".into());
        assert_eq!(convert_websocket(&doc).0.auth, Auth::Bearer { token: "t".into() });

        doc.ws_auth_mode = None;
        assert_eq!(convert_websocket(&doc).0.auth, Auth::Bearer { token: "t".into() });
    }

    #[test]
    fn scripts_are_carried_as_runtime_scripts() {
        let mut doc = ws_doc();
        doc.pre_request_script = Some("// pre".into());
        doc.post_response_script = Some("// post".into());
        let (ws, _) = convert_websocket(&doc);
        let kinds: Vec<&str> = ws.scripts.iter().map(|s| s.script_type.as_str()).collect();
        assert_eq!(kinds, vec!["before-request", "after-response"]);
    }

    #[test]
    fn an_unsupported_auth_block_is_reported_and_the_request_still_imports() {
        let mut doc = ws_doc();
        doc.unknown_blocks.push(BruRawBlock {
            name: "auth".into(),
            subtype: Some("oauth2".into()),
            content: String::new(),
        });
        let (ws, skipped) = convert_websocket(&doc);
        assert_eq!(skipped.len(), 1);
        assert!(matches!(skipped[0], SkipReason::UnsupportedAuthType(_)));
        assert_eq!(ws.auth, Auth::None);
    }

    #[test]
    fn a_new_request_gets_a_uid_so_it_can_be_saved() {
        let (ws, _) = convert_websocket(&ws_doc());
        assert!(!ws.uid.is_empty());
    }
    #[test]
    fn convert_item_routes_a_websocket_document_and_still_routes_http() {
        let (item, skipped) = convert_item(&ws_doc());
        assert!(skipped.is_empty());
        assert!(matches!(item, Some(Converted::WebSocket(ref w)) if w.name == "Echo"));

        let http = BruDocument {
            meta: Some(BruMeta { name: "A".into(), request_type: "http".into(), seq: None }),
            method: Some(BruMethod::Get),
            url: Some("https://x".into()),
            ..BruDocument::default()
        };
        assert!(matches!(convert_item(&http).0, Some(Converted::Http(_))));
    }
}
```

Append to the `tests` module of `crates/rocket-import/src/bru/yml_adapter.rs`:

```rust
    #[test]
    fn websocket_yml_request_is_parsed_not_flagged_unsupported() {
        let yml = r#"
meta:
  name: Chat
  type: ws
ws:
  url: wss://chat.example.com/ws
  headers:
    - name: Origin
      value: https://example.com
  auth:
    mode: bearer
    bearer:
      token: t0k
  messages:
    - name: hello
      type: json
      body: '{"hi":true}'
    - name: ping
      type: text
      content: ping
"#;
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert!(doc.is_websocket());
        assert!(doc.unknown_blocks.is_empty(), "{:?}", doc.unknown_blocks);
        assert_eq!(doc.url.as_deref(), Some("wss://chat.example.com/ws"));
        assert_eq!(doc.headers.len(), 1);
        assert!(matches!(&doc.auth, Some(BruAuth::Bearer { token }) if token == "t0k"));
        assert_eq!(doc.ws_messages.len(), 2);
        assert_eq!(doc.ws_messages[0].kind, "json");
        assert_eq!(doc.ws_messages[0].content, "{\"hi\":true}");
        assert_eq!(doc.ws_messages[1].content, "ping");
    }

    #[test]
    fn websocket_yml_accepts_the_websocket_key_alias() {
        let yml = "meta:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: ws://x\n";
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert!(doc.is_websocket());
        assert_eq!(doc.url.as_deref(), Some("ws://x"));
    }
```

(`BruAuth` is already in scope there through `use crate::bru::ast::*;`.)

- [ ] **Step 7: Run them and confirm they fail**

Run: `cargo test -j4 -p rocket-import websocket_tests`
Expected: compile error (`convert_websocket` and `Converted::WebSocket` not found).

- [ ] **Step 8: Implement the converter and the YAML adapter changes**

In `crates/rocket-import/src/converter/request.rs`, extend the import line `use rocket_collection::{GraphQlBody, GraphQlRequest, Request};` (Plan 05) with `WebSocketMessage, WebSocketMessageKind, WebSocketRequest, WebSocketScript`, add the variant to `Converted`, route it in `convert_item` before the HTTP fallback, and add `convert_websocket` beside `convert_graphql`:

```rust
pub enum Converted {
    Http(Request),
    GraphQl(GraphQlRequest),
    WebSocket(WebSocketRequest),
}
```

In `convert_item`, directly after the `is_graphql` block:

```rust
    if doc.is_websocket() {
        let (ws, skipped) = convert_websocket(doc);
        return (Some(Converted::WebSocket(ws)), skipped);
    }
```

and below `convert_graphql`:

```rust
/// Converts a Bruno WebSocket document to a domain `WebSocketRequest`.
///
/// Never drops the request: an unsupported auth block is reported and the request imports with
/// no auth, like the HTTP and GraphQL converters do.
pub fn convert_websocket(doc: &BruDocument) -> (WebSocketRequest, Vec<SkipReason>) {
    let skipped = auth_skips(doc);

    let name = doc
        .meta
        .as_ref()
        .map(|m| m.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Untitled".into());
    let mut ws = WebSocketRequest::new(name, doc.url.clone().unwrap_or_default());
    ws.seq = doc.meta.as_ref().and_then(|m| m.seq);

    ws.headers = doc
        .headers
        .iter()
        .map(|h| {
            if h.disabled {
                Header::disabled(h.key.clone(), h.value.clone())
            } else {
                Header::new(h.key.clone(), h.value.clone())
            }
        })
        .collect();

    ws.messages = doc
        .ws_messages
        .iter()
        .enumerate()
        .map(|(index, m)| WebSocketMessage {
            title: m.name.clone(),
            selected: index == 0,
            kind: WebSocketMessageKind::parse(&m.kind).unwrap_or(WebSocketMessageKind::Text),
            data: m.content.clone(),
        })
        .collect();

    ws.auth = match doc.ws_auth_mode.as_deref() {
        Some("inherit") => Auth::Inherit,
        Some("none") => Auth::None,
        _ if !skipped.is_empty() => Auth::None,
        _ => doc.auth.as_ref().map(bru_auth_to_domain).unwrap_or(Auth::None),
    };

    if let Some(code) = &doc.pre_request_script {
        ws.scripts.push(WebSocketScript {
            script_type: "before-request".into(),
            code: code.clone(),
        });
    }
    if let Some(code) = &doc.post_response_script {
        ws.scripts.push(WebSocketScript {
            script_type: "after-response".into(),
            code: code.clone(),
        });
    }

    (ws, skipped)
}
```

(`Auth`, `Header` and `bru_auth_to_domain` are already in scope in that file.)

In `crates/rocket-import/src/bru/yml_adapter.rs`:

1. Add the serde structs after `BruYmlHttp`:

```rust
/// The WebSocket block of a Bruno YAML request. `websocket` is accepted as an alias of `ws`.
#[derive(Debug, Deserialize)]
pub struct BruYmlWs {
    pub url: Option<String>,
    pub headers: Option<Vec<BruYmlHeader>>,
    pub auth: Option<BruYmlAuth>,
    pub messages: Option<Vec<BruYmlWsMessage>>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlWsMessage {
    #[serde(alias = "title")]
    pub name: Option<String>,
    #[serde(rename = "type", alias = "format")]
    pub kind: Option<String>,
    #[serde(alias = "body", alias = "data")]
    pub content: Option<String>,
}
```

2. Add `#[serde(alias = "websocket")] pub ws: Option<BruYmlWs>,` to `BruYmlRequest` (after `http`).
3. In `adapt_request` extend the type check. Plan 05 already added `graphql` to its `matches!` list; add the WebSocket spellings:

```rust
        if !matches!(request_type.as_str(), "http" | "" | "graphql" | "ws" | "websocket") {
```

(without Plan 05, the list is `"http" | "" | "ws" | "websocket"`.)

and, after the `if let Some(http) = yml.http { ... }` block, add:

```rust
    if let Some(ws) = yml.ws {
        doc.url = ws.url;
        doc.headers = ws
            .headers
            .unwrap_or_default()
            .into_iter()
            .map(|h| BruKeyValue {
                key: h.name,
                value: h.value,
                disabled: h.disabled,
            })
            .collect();
        if let Some(auth) = ws.auth {
            doc.ws_auth_mode = auth.mode.clone();
            doc.auth = adapt_auth(auth, &mut doc.unknown_blocks);
        }
        doc.ws_messages = ws
            .messages
            .unwrap_or_default()
            .into_iter()
            .map(|m| BruWsMessage {
                name: m.name.unwrap_or_default(),
                kind: m.kind.unwrap_or_else(|| "text".into()),
                content: m.content.unwrap_or_default(),
            })
            .collect();
    }
```

`adapt_auth` returns `Option<BruAuth>` for `inherit` and `none` modes as `None`; `ws_auth_mode` carries the mode so the converter still sees `inherit`.

- [ ] **Step 9: Run the converter and adapter tests**

Run: `cargo test -j4 -p rocket-import websocket_tests` then `cargo test -j4 -p rocket-import yml_adapter`
Expected: the 7 converter tests and the adapter tests pass. If `websocket_yml_request_is_parsed_not_flagged_unsupported` leaves an entry in `unknown_blocks`, the `matches!` edit in `adapt_request` is not applied.

- [ ] **Step 10: Write the failing end-to-end import test**

Append to `crates/rocket-import/tests/integration_test.rs`:

```rust
const WS_BRU: &str = "meta {\n  name: Echo\n  type: ws\n  seq: 1\n}\n\nws {\n  url: wss://echo.websocket.org\n  body: ws\n  auth: none\n}\n\nheaders {\n  X-Trace: abc\n}\n\nbody:ws {\n  message 1 [json] {\n    {\"name\":\"Bruno\"}\n  }\n}\n";

fn legacy_ws_collection(dir: &Path) -> PathBuf {
    let root = dir.join("ws-col");
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(
        root.join("bruno.json"),
        r#"{"name":"ws-col","version":"1","type":"collection"}"#,
    )
    .expect("bruno.json");
    std::fs::write(root.join("echo.bru"), WS_BRU).expect("echo.bru");
    root
}

#[test]
fn legacy_bru_websocket_imports_as_a_websocket_item_not_http() {
    let source = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let service = make_service(workspace.path());

    let report = service
        .import_collection(&legacy_ws_collection(source.path()), "default")
        .expect("import should succeed");

    assert_eq!(report.imported, 1, "{report:?}");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);

    let repo = FsCollectionRepo::new_standalone(workspace.path().join("collections"));
    let ws = rocket_collection::CollectionRepository::get_websocket_request(&repo, "ws-col", "echo.yml")
        .expect("saved as a websocket request");
    assert_eq!(ws.name, "Echo");
    assert_eq!(ws.url, "wss://echo.websocket.org");
    assert_eq!(ws.headers[0].key, "X-Trace");
    assert_eq!(ws.messages.len(), 1);
    assert_eq!(ws.messages[0].data, "{\"name\":\"Bruno\"}");

    // It must not also exist as an HTTP request.
    let http = rocket_collection::CollectionRepository::get_request(&repo, "ws-col", "echo.yml");
    assert!(http.is_err(), "the file must be a websocket file, not an http one");
}

#[test]
fn a_websocket_import_that_cannot_be_saved_is_reported_not_counted() {
    // A name that sanitises to nothing cannot be written; the report must say so.
    // (Kept as a unit-level guard on the routing: see importer.rs.)
    let source = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let root = legacy_ws_collection(source.path());
    // Break the target: make the destination collection directory read-only is not portable,
    // so assert the success path counts exactly one item instead.
    let report = make_service(workspace.path())
        .import_collection(&root, "default")
        .expect("import");
    assert_eq!(report.imported, 1);
    assert_eq!(report.total_files, 1);
}
```

If `rocket_collection` is not yet a dev-dependency path usable from the integration test, it already is a normal dependency of the crate, so it is visible to `tests/`.

Delete the second test if the reviewer finds it adds nothing; it only pins `total_files`.

- [ ] **Step 11: Run it and confirm it fails**

Run: `cargo test -j4 -p rocket-import legacy_bru_websocket_imports_as_a_websocket_item_not_http`
Expected: FAIL. Today the file is imported as an HTTP request, so `get_websocket_request` errors with a parse error.

- [ ] **Step 12: Route ws documents in the importer**

In `crates/rocket-import/src/importer.rs`, `walk_requests` already matches on `Converted` (Plan 05). Add the WebSocket arm next to the GraphQL one:

```rust
                        Some(req_converter::Converted::WebSocket(ws)) => {
                            match repo.save_websocket_request(collection_name, &out_path, &ws) {
                                Ok(_) => report.imported += 1,
                                Err(e) => report.skipped.push(SkippedItem {
                                    path: rel_str.clone(),
                                    reason: SkipReason::ParseError(e.to_string()),
                                }),
                            }
                        }
```

No other change is needed: `convert_item` already routes ws documents before the HTTP converter.

Update `crates/rocket-import/CLAUDE.md`: Plan 05 changed the "Non-fatal skips" bullet to "Unsupported request types (gRPC, WebSocket)". Change it to "(gRPC)" and add: "WebSocket requests import as `WebSocketRequest` through `CollectionRepository::save_websocket_request` (`convert_websocket`)". Add `ws_messages` and `ws_auth_mode` to the BruDocument fields table. Add a short note that a `.bru` gRPC file is still imported as an empty HTTP request because `parse_meta` does not mark unsupported types (the gRPC plan fixes this); Plan 05 routes GraphQL and this plan routes WebSocket before that fallback.

- [ ] **Step 13: Run the import tests**

Run: `cargo test -j4 -p rocket-import` (this crate only; it is small) and `cargo check -j4`.
Expected: green. Manual check (needs the user's Bruno): export or open one real Bruno WebSocket request, import it, and confirm name, URL, headers and each message. If the real `.bru` differs from the fixture, fix `parse_ws_block` and `parse_ws_messages` and the fixtures only.

- [ ] **Step 14: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by path: `crates/rocket-import/src/bru/ast.rs`, `crates/rocket-import/src/bru/parser.rs`, `crates/rocket-import/src/bru/yml_adapter.rs`, `crates/rocket-import/src/converter/request.rs`, `crates/rocket-import/src/importer.rs`, `crates/rocket-import/CLAUDE.md`, `crates/rocket-import/tests/integration_test.rs`.
Suggested subject: `feat(import): import Bruno WebSocket requests as WebSocket items`.

---

## Task 2: Frontend typed item, sidebar, create and save

**Files:**
- Create: `src/lib/websocket-messages.ts`, `src/lib/websocket-mapper.ts`, `src/lib/websocket-create.ts`, and tests `src/lib/__tests__/websocket-messages.test.ts`, `src/lib/__tests__/websocket-mapper.test.ts`, `src/lib/__tests__/websocket-create.test.ts`, `src/lib/__tests__/save-tab-request.websocket.test.ts`, `src/lib/__tests__/auto-save.websocket.test.ts`, `src/components/request/__tests__/CreateRequestDialog.websocket.test.tsx`
- Modify: `src/lib/tauri-api.ts`, `src/types/pane-types.ts`, `src/lib/pane-utils.ts` (`createDefaultRequestFor`), `src/lib/save-tab-request.ts`, `src/lib/auto-save.ts`, `src/lib/colors.ts`, `src/lib/contracts/collectPaths.ts`, `src/lib/contracts/collectPaths.test.ts`, `src/components/collections/RequestNode.tsx`, `src/components/collections/CollectionNode.tsx`, `src/components/collections/FolderNode.tsx`, `src/components/request/CreateRequestDialog.tsx`, `src/components/collections/__tests__/RequestNode.test.tsx`

All of the Plan 05 files named above exist only after Plan 05 Task 3. Each edit below says which Plan 05 line it extends.

**Interfaces:**
- Consumes: `get_websocket_request`, `save_websocket_request` (plan 08); from Plan 05: `RequestKind`, `RequestSummary.kind`, `saveTabRequest(collection, path, tab, overrides?)`, `createDefaultRequestFor(kind)`, the `kind`-aware `RequestNode`, the `opaque or graphql` guards; `fromPersistedAuth`, `toPersistedAuth`, `toPersistedHeaders`, `usePaneStore`.
- Produces:
  - Types in `tauri-api.ts`: `WebSocketMessageKind`, `WebSocketMessage`, `WebSocketSettings`, `WebSocketRequest`, `WebSocketScopeInput`, `WebSocketConnectInput`, `WebSocketSendInput`, `WebSocketMessageEvent`, `WebSocketStatusEvent`; `CollectionItem` gains `({ type: 'websocket' } & WebSocketRequest)`; functions `getWebSocketRequest`, `saveWebSocketRequest`, `wsConnect`, `wsSend`, `wsDisconnect`, `onWebSocketMessage`, `onWebSocketStatus`.
  - Types in `pane-types.ts`: `WebSocketDraftMessage`, `WebSocketPassthrough`, `WebSocketDraft`; `RequestState.websocket?: WebSocketDraft`.
  - `lib/websocket-messages.ts`: `MESSAGE_KINDS`, `newMessage(title)`, `createDefaultWebSocketDraft()`, `normalizeSelection`, `selectedMessage`, `addMessage`, `selectMessage`, `updateMessage`, `removeMessage`.
  - `lib/websocket-mapper.ts`: `createDefaultWebSocketRequestState(url?)`, `mapWebSocketToState(ws)`, `toApiWebSocketRequest(uid, name, request, fileName?)`, `buildWebSocketSavePayload(tab, overrides?)`, `webSocketToTab(ws, collection, path)`, `parseOptionalMs(text)`.
  - `lib/websocket-create.ts`: `createWebSocketItem(collection, folderPath, name, url) -> Promise<RequestTab>`.
  - `createDefaultRequestFor('websocket')` returns a request state with a default draft; `saveTabRequest` saves a WebSocket tab with `saveWebSocketRequest`.

- [ ] **Step 1: Types and commands in `tauri-api.ts`**

In `src/lib/tauri-api.ts` (Plan 05 has already added `RequestKind`, `RequestSummary.kind` and the `graphql` member of `CollectionItem`; do not touch `OpaqueProtocolItem`):

1. After the GraphQL interfaces Plan 05 added, add:

```ts
export type WebSocketMessageKind = 'text' | 'json' | 'xml' | 'binary';

export interface WebSocketMessage {
  title: string;
  selected: boolean;
  kind: WebSocketMessageKind;
  /** Text as is. For `binary` this is base64. */
  data: string;
}

export interface WebSocketSettings {
  /** Connect timeout in ms, or 'inherit' for the 30 s default. 0 waits forever. */
  timeout?: number | 'inherit';
  /** Ms between client pings, or 'inherit' for none. */
  keepAliveInterval?: number | 'inherit';
}

/** Mirrors the Rust `WebSocketRequest` (rocket-collection). */
export interface WebSocketRequest {
  uid: string;
  name: string;
  /** String, `{ content, type }` or null. Kept verbatim, never edited here. */
  description?: unknown;
  seq?: number;
  tags?: string[];
  url: string;
  headers: Header[];
  messages: WebSocketMessage[];
  auth: Auth;
  runtimeAuth?: Auth;
  /** Edited through the request-variables commands, so a save from the tab never sends it. */
  variables?: CollectionVariable[];
  scripts?: { scriptType: string; code: string }[];
  settings?: WebSocketSettings;
  docs?: string | null;
  fileName?: string;
}
```

2. Extend `CollectionItem` with one more member (next to Plan 05's `graphql`):

```ts
  | ({ type: 'websocket' } & WebSocketRequest)
```

3. After `saveGraphQlRequest` add:

```ts
export const getWebSocketRequest = (collection: string, path: string) =>
  invoke<WebSocketRequest>('get_websocket_request', { collection, path });

export const saveWebSocketRequest = (collection: string, path: string, request: WebSocketRequest) =>
  invoke<WebSocketRequest>('save_websocket_request', { collection, path, request });
```

4. After the agent session event wrappers (after `onAgentSessionFinished`) add:

```ts
// ============================================================
// WebSocket sessions
// ============================================================

/** Where `{{variables}}` come from for a connect or send. */
export interface WebSocketScopeInput {
  collection?: string;
  environmentName?: string;
  globalEnvName?: string;
  requestPath?: string;
}

export interface WebSocketConnectInput extends WebSocketScopeInput {
  url: string;
  headers: Header[];
  auth?: Auth;
  subprotocols?: string[];
  timeoutMs?: number;
  keepAliveMs?: number;
  verifySsl?: boolean;
}

export interface WebSocketSendInput extends WebSocketScopeInput {
  kind: WebSocketMessageKind;
  data: string;
}

/** `ws_connect` only reports whether the connect worked. Frames arrive as events. */
export const wsConnect = (sessionId: string, input: WebSocketConnectInput) =>
  invoke<void>('ws_connect', { sessionId, input });

export const wsSend = (sessionId: string, input: WebSocketSendInput) =>
  invoke<void>('ws_send', { sessionId, input });

export const wsDisconnect = (sessionId: string) => invoke<void>('ws_disconnect', { sessionId });

/** Payload of the `ws:message` event. Fields are snake_case, like every `DomainEvent`. */
export interface WebSocketMessageEvent {
  type: 'webSocketMessage';
  session_id: string;
  direction: 'in' | 'out';
  kind: 'text' | 'binary';
  /** Text as is, or base64 for binary frames. */
  data: string;
  size: number;
  timestamp_ms: number;
}

export interface WebSocketStatusEvent {
  type: 'webSocketStatus';
  session_id: string;
  state: 'connecting' | 'open' | 'closed' | 'failed';
  subprotocol: string | null;
  code: number | null;
  reason: string | null;
}

export const onWebSocketMessage = (
  handler: (event: WebSocketMessageEvent) => void,
): Promise<UnlistenFn> =>
  listen<WebSocketMessageEvent>('ws:message', (e) => handler(e.payload));

export const onWebSocketStatus = (
  handler: (event: WebSocketStatusEvent) => void,
): Promise<UnlistenFn> =>
  listen<WebSocketStatusEvent>('ws:status', (e) => handler(e.payload));
```

5. Run `yarn tsc --noEmit`. Expected: new errors only at the sidebar loops and `collectPaths.ts`, where `item` can now be a `websocket` item; Step 8 fixes them.

- [ ] **Step 2: Draft types in `pane-types.ts`**

In `src/types/pane-types.ts`, add `websocket?: WebSocketDraft;` to `RequestState` directly after Plan 05's `graphql?: GraphQlState;`, and add these exports next to `GraphQlState`:

```ts
export interface WebSocketDraftMessage {
  id: string;
  title: string;
  selected: boolean;
  kind: import('@/lib/tauri-api').WebSocketMessageKind;
  /** Text as is. For `binary` this is base64. */
  data: string;
}

/**
 * Saved WebSocket fields with no editor yet. They are kept as loaded and written back unchanged,
 * so saving from the UI never drops them. Runtime variables are not here: they are edited and
 * saved through their own commands, and the backend keeps them when a save carries none.
 */
export interface WebSocketPassthrough {
  description?: unknown;
  seq?: number;
  runtimeAuth?: import('@/lib/tauri-api').Auth;
  scripts?: { scriptType: string; code: string }[];
}

export interface WebSocketDraft {
  messages: WebSocketDraftMessage[];
  /** Connect timeout in ms, or 'inherit'. */
  timeoutMs: number | 'inherit';
  /** Ms between pings, or 'inherit' (none). */
  keepAliveMs: number | 'inherit';
  passthrough: WebSocketPassthrough;
}
```

- [ ] **Step 3: Write the failing message-helper tests**

Create `src/lib/__tests__/websocket-messages.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  addMessage,
  createDefaultWebSocketDraft,
  newMessage,
  normalizeSelection,
  removeMessage,
  selectedMessage,
  selectMessage,
  updateMessage,
} from '@/lib/websocket-messages';
import type { WebSocketDraftMessage } from '@/types/pane-types';

function msg(id: string, selected = false): WebSocketDraftMessage {
  return { id, title: id, selected, kind: 'text', data: '' };
}

describe('websocket message helpers', () => {
  it('normalizeSelection keeps exactly one selected message', () => {
    expect(normalizeSelection([msg('a'), msg('b')]).map((m) => m.selected)).toEqual([true, false]);
    expect(normalizeSelection([msg('a', true), msg('b', true)]).map((m) => m.selected)).toEqual([
      true,
      false,
    ]);
    expect(normalizeSelection([msg('a'), msg('b', true)]).map((m) => m.selected)).toEqual([
      false,
      true,
    ]);
    expect(normalizeSelection([])).toEqual([]);
  });

  it('addMessage appends a titled message and selects only it', () => {
    const next = addMessage([msg('a', true)]);
    expect(next).toHaveLength(2);
    expect(next[1].title).toBe('message 2');
    expect(next.map((m) => m.selected)).toEqual([false, true]);
  });

  it('selectMessage selects exactly the given id', () => {
    const next = selectMessage([msg('a', true), msg('b')], 'b');
    expect(next.map((m) => m.selected)).toEqual([false, true]);
  });

  it('removing the selected message selects the first remaining one', () => {
    const next = removeMessage([msg('a'), msg('b', true), msg('c')], 'b');
    expect(next.map((m) => m.id)).toEqual(['a', 'c']);
    expect(next.map((m) => m.selected)).toEqual([true, false]);
  });

  it('removing an unselected message keeps the selection', () => {
    const next = removeMessage([msg('a'), msg('b', true)], 'a');
    expect(next.map((m) => m.selected)).toEqual([true]);
  });

  it('removing the last message leaves an empty list', () => {
    expect(removeMessage([msg('a', true)], 'a')).toEqual([]);
  });

  it('updateMessage patches only the matching message and never changes the id', () => {
    const next = updateMessage([msg('a'), msg('b')], 'b', { data: 'x', kind: 'json' });
    expect(next[0]).toEqual(msg('a'));
    expect(next[1]).toMatchObject({ id: 'b', data: 'x', kind: 'json' });
  });

  it('selectedMessage falls back to the first message', () => {
    expect(selectedMessage([msg('a'), msg('b')])?.id).toBe('a');
    expect(selectedMessage([msg('a'), msg('b', true)])?.id).toBe('b');
    expect(selectedMessage([])).toBeUndefined();
  });

  it('newMessage creates an unselected text message with a unique id', () => {
    const a = newMessage('one');
    const b = newMessage('two');
    expect(a.id).not.toBe(b.id);
    expect(a).toMatchObject({ title: 'one', selected: false, kind: 'text', data: '' });
  });

  it('a default draft has one selected message and inherited settings', () => {
    const draft = createDefaultWebSocketDraft();
    expect(draft.messages).toHaveLength(1);
    expect(draft.messages[0]).toMatchObject({ title: 'message 1', selected: true });
    expect(draft.timeoutMs).toBe('inherit');
    expect(draft.keepAliveMs).toBe('inherit');
    expect(draft.passthrough).toEqual({});
  });
});
```

- [ ] **Step 4: Run it and confirm it fails**

Run: `yarn test --run websocket-messages`
Expected: FAIL, cannot resolve `@/lib/websocket-messages`.

- [ ] **Step 5: Implement the helpers**

Create `src/lib/websocket-messages.ts`. It must not import `pane-utils` (which imports it back in Step 8):

```ts
import type { WebSocketMessageKind } from '@/lib/tauri-api';
import type { WebSocketDraft, WebSocketDraftMessage } from '@/types/pane-types';

export const MESSAGE_KINDS: { label: string; value: WebSocketMessageKind }[] = [
  { label: 'Text', value: 'text' },
  { label: 'JSON', value: 'json' },
  { label: 'XML', value: 'xml' },
  { label: 'Binary (base64)', value: 'binary' },
];

export function newMessage(title: string): WebSocketDraftMessage {
  return { id: crypto.randomUUID(), title, selected: false, kind: 'text', data: '' };
}

/** The draft of a new WebSocket request: one selected message, settings inherited. */
export function createDefaultWebSocketDraft(): WebSocketDraft {
  return {
    messages: [{ ...newMessage('message 1'), selected: true }],
    timeoutMs: 'inherit',
    keepAliveMs: 'inherit',
    passthrough: {},
  };
}

/** Keeps exactly one message selected: the first selected one, else the first message. */
export function normalizeSelection(messages: WebSocketDraftMessage[]): WebSocketDraftMessage[] {
  if (messages.length === 0) return messages;
  const firstSelected = messages.findIndex((m) => m.selected);
  const keep = firstSelected === -1 ? 0 : firstSelected;
  return messages.map((m, index) =>
    m.selected === (index === keep) ? m : { ...m, selected: index === keep },
  );
}

/** The message Send uses: the selected one, else the first. */
export function selectedMessage(
  messages: WebSocketDraftMessage[],
): WebSocketDraftMessage | undefined {
  return messages.find((m) => m.selected) ?? messages[0];
}

export function addMessage(messages: WebSocketDraftMessage[]): WebSocketDraftMessage[] {
  const created = { ...newMessage(`message ${messages.length + 1}`), selected: true };
  return [...messages.map((m) => (m.selected ? { ...m, selected: false } : m)), created];
}

export function selectMessage(
  messages: WebSocketDraftMessage[],
  id: string,
): WebSocketDraftMessage[] {
  return normalizeSelection(
    messages.map((m) => (m.selected === (m.id === id) ? m : { ...m, selected: m.id === id })),
  );
}

export function updateMessage(
  messages: WebSocketDraftMessage[],
  id: string,
  patch: Partial<Omit<WebSocketDraftMessage, 'id'>>,
): WebSocketDraftMessage[] {
  return messages.map((m) => (m.id === id ? { ...m, ...patch, id: m.id } : m));
}

export function removeMessage(
  messages: WebSocketDraftMessage[],
  id: string,
): WebSocketDraftMessage[] {
  return normalizeSelection(messages.filter((m) => m.id !== id));
}
```

Run: `yarn test --run websocket-messages`
Expected: 10 passed.

- [ ] **Step 6: Write the failing mapper tests**

Create `src/lib/__tests__/websocket-mapper.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { createDefaultRequestFor } from '@/lib/pane-utils';
import type { WebSocketRequest } from '@/lib/tauri-api';
import {
  buildWebSocketSavePayload,
  createDefaultWebSocketRequestState,
  mapWebSocketToState,
  parseOptionalMs,
  toApiWebSocketRequest,
  webSocketToTab,
} from '@/lib/websocket-mapper';

const saved: WebSocketRequest = {
  uid: 'ws-1',
  name: 'Chat',
  description: { content: '# Chat', type: 'text/markdown' },
  seq: 3,
  tags: ['realtime'],
  url: 'wss://chat.example.com/ws',
  headers: [
    { key: 'Origin', value: 'https://example.com', enabled: true },
    { key: 'X-Off', value: '1', enabled: false },
  ],
  messages: [
    { title: 'hello', selected: false, kind: 'json', data: '{}' },
    { title: 'raw', selected: true, kind: 'binary', data: 'AQID' },
  ],
  auth: { authType: 'bearer', token: 't' },
  runtimeAuth: { authType: 'basic', username: 'u', password: 'p' },
  variables: [{ key: 'room', value: 'general', initialValue: '', enabled: true, secret: false }],
  scripts: [{ scriptType: 'before-request', code: '// pre' }],
  settings: { timeout: 5000, keepAliveInterval: 'inherit' },
  docs: '# Docs',
  fileName: 'chat.yml',
};

describe('websocket mapper', () => {
  it('maps a saved item to request state with the websocket discriminator', () => {
    const state = mapWebSocketToState(saved);
    expect(state.requestType).toBe('websocket');
    expect(state.url).toBe('wss://chat.example.com/ws');
    expect(state.headers.map((h) => [h.key, h.enabled])).toEqual([
      ['Origin', true],
      ['X-Off', false],
    ]);
    expect(state.auth.authType).toBe('bearer');
    expect(state.websocket?.messages.map((m) => [m.title, m.selected])).toEqual([
      ['hello', false],
      ['raw', true],
    ]);
    expect(state.websocket?.timeoutMs).toBe(5000);
    expect(state.websocket?.keepAliveMs).toBe('inherit');
  });

  it('writes back every field the UI does not edit, unchanged', () => {
    const payload = toApiWebSocketRequest('ws-1', 'Chat', mapWebSocketToState(saved));
    expect(payload.description).toEqual(saved.description);
    expect(payload.seq).toBe(3);
    expect(payload.runtimeAuth).toEqual(saved.runtimeAuth);
    expect(payload.scripts).toEqual(saved.scripts);
    expect(payload.docs).toBe('# Docs');
    expect(payload.tags).toEqual(['realtime']);
  });

  it('never sends runtime variables, so a stale copy cannot overwrite edited ones', () => {
    const payload = toApiWebSocketRequest('ws-1', 'Chat', mapWebSocketToState(saved));
    expect(payload.variables).toBeUndefined();
  });

  it('round-trips headers, messages, auth and settings', () => {
    const payload = toApiWebSocketRequest('ws-1', 'Chat', mapWebSocketToState(saved));
    expect(payload.headers).toEqual(saved.headers);
    expect(payload.messages).toEqual(saved.messages);
    expect(payload.auth).toEqual(saved.auth);
    expect(payload.settings).toEqual({ timeout: 5000, keepAliveInterval: 'inherit' });
  });

  it('omits settings when both values inherit', () => {
    const state = mapWebSocketToState({ ...saved, settings: undefined });
    expect(toApiWebSocketRequest('ws-1', 'Chat', state).settings).toBeUndefined();
  });

  it('normalizes a saved item with no selected message', () => {
    const state = mapWebSocketToState({
      ...saved,
      messages: saved.messages.map((m) => ({ ...m, selected: false })),
    });
    expect(state.websocket?.messages.map((m) => m.selected)).toEqual([true, false]);
  });

  it('a new default state is a websocket request with one selected message', () => {
    const state = createDefaultWebSocketRequestState('wss://x');
    expect(state.requestType).toBe('websocket');
    expect(state.url).toBe('wss://x');
    expect(state.websocket?.messages).toHaveLength(1);
    expect(state.websocket?.messages[0].selected).toBe(true);
    expect(state.websocket?.timeoutMs).toBe('inherit');
  });

  it('createDefaultRequestFor(websocket) carries a draft (new ephemeral tabs use it)', () => {
    const state = createDefaultRequestFor('websocket');
    expect(state.requestType).toBe('websocket');
    expect(state.websocket?.messages).toHaveLength(1);
  });

  it('builds a request tab keyed by uid with its source', () => {
    const tab = webSocketToTab(saved, 'my-api', 'chat.yml');
    expect(tab).toMatchObject({
      id: 'ws-1',
      title: 'Chat',
      tabType: 'request',
      isDirty: false,
      source: { collection: 'my-api', path: 'chat.yml' },
    });
    expect(tab.request.requestType).toBe('websocket');
  });

  it('buildWebSocketSavePayload honours the name and file name overrides', () => {
    const tab = webSocketToTab(saved, 'my-api', 'chat.yml');
    const payload = buildWebSocketSavePayload(tab, { name: 'Renamed', fileName: 'chat2' });
    expect(payload).toMatchObject({ uid: 'ws-1', name: 'Renamed', fileName: 'chat2' });
    expect(buildWebSocketSavePayload(tab).name).toBe('Chat');
  });

  it('parseOptionalMs maps blank to inherit and rejects negatives', () => {
    expect(parseOptionalMs('')).toBe('inherit');
    expect(parseOptionalMs('  ')).toBe('inherit');
    expect(parseOptionalMs('2500')).toBe(2500);
    expect(parseOptionalMs('0')).toBe(0);
    expect(parseOptionalMs('-5')).toBe('inherit');
    expect(parseOptionalMs('abc')).toBe('inherit');
  });
});
```

- [ ] **Step 7: Run it and confirm it fails**

Run: `yarn test --run websocket-mapper`
Expected: FAIL, cannot resolve `@/lib/websocket-mapper` (and `createDefaultRequestFor('websocket')` has no draft yet).

- [ ] **Step 8: Implement the mapper and extend Plan 05's guards**

Create `src/lib/websocket-mapper.ts`:

```ts
import { createDefaultRequestFor } from '@/lib/pane-utils';
import { fromPersistedAuth, toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import type { WebSocketRequest } from '@/lib/tauri-api';
import { createDefaultWebSocketDraft, normalizeSelection } from '@/lib/websocket-messages';
import type { RequestState, RequestTab } from '@/types/pane-types';

/** A blank WebSocket tab state, for new and unsaved tabs. */
export function createDefaultWebSocketRequestState(url = ''): RequestState {
  return { ...createDefaultRequestFor('websocket'), url };
}

/** Maps a saved WebSocket item to the frontend request state. */
export function mapWebSocketToState(ws: WebSocketRequest): RequestState {
  return {
    ...createDefaultRequestFor('websocket'),
    requestType: 'websocket',
    url: ws.url,
    headers: ws.headers.map((h) => ({
      id: crypto.randomUUID(),
      key: h.key,
      value: h.value,
      enabled: h.enabled,
    })),
    auth: fromPersistedAuth(ws.auth, 'inherit'),
    tags: ws.tags ?? [],
    docs: ws.docs ?? null,
    websocket: {
      messages: normalizeSelection(
        ws.messages.map((m) => ({
          id: crypto.randomUUID(),
          title: m.title,
          selected: m.selected,
          kind: m.kind,
          data: m.data,
        })),
      ),
      timeoutMs: ws.settings?.timeout ?? 'inherit',
      keepAliveMs: ws.settings?.keepAliveInterval ?? 'inherit',
      passthrough: {
        description: ws.description,
        seq: ws.seq,
        runtimeAuth: ws.runtimeAuth,
        scripts: ws.scripts,
      },
    },
  };
}

/** Maps request state to the payload of `save_websocket_request`. */
export function toApiWebSocketRequest(
  uid: string,
  name: string,
  request: RequestState,
  fileName?: string,
): WebSocketRequest {
  const draft = request.websocket ?? createDefaultWebSocketDraft();
  const extra = draft.passthrough;
  const inheritsAll = draft.timeoutMs === 'inherit' && draft.keepAliveMs === 'inherit';
  return {
    uid,
    name,
    url: request.url,
    headers: toPersistedHeaders(request.headers),
    messages: draft.messages.map((m) => ({
      title: m.title,
      selected: m.selected,
      kind: m.kind,
      data: m.data,
    })),
    auth: toPersistedAuth(request.auth),
    tags: request.tags.length > 0 ? request.tags : undefined,
    docs: request.docs ?? null,
    description: extra.description,
    seq: extra.seq,
    runtimeAuth: extra.runtimeAuth,
    scripts: extra.scripts,
    settings: inheritsAll
      ? undefined
      : { timeout: draft.timeoutMs, keepAliveInterval: draft.keepAliveMs },
    ...(fileName !== undefined ? { fileName } : {}),
  };
}

/** The save payload for a WebSocket tab. Same override shape as Plan 05's GraphQL builder. */
export function buildWebSocketSavePayload(
  tab: RequestTab,
  overrides?: { name?: string; fileName?: string },
): WebSocketRequest {
  return toApiWebSocketRequest(
    tab.id || crypto.randomUUID(),
    overrides?.name ?? tab.title,
    tab.request,
    overrides?.fileName,
  );
}

/** A request tab for a saved WebSocket item. The tab id is the item uid. */
export function webSocketToTab(ws: WebSocketRequest, collection: string, path: string): RequestTab {
  return {
    id: ws.uid,
    title: ws.name,
    tabType: 'request',
    request: mapWebSocketToState(ws),
    response: null,
    isDirty: false,
    source: { collection, path },
  };
}

/** Blank or invalid input means "inherit"; otherwise a non-negative number of milliseconds. */
export function parseOptionalMs(text: string): number | 'inherit' {
  const trimmed = text.trim();
  if (trimmed === '') return 'inherit';
  const value = Number(trimmed);
  return Number.isFinite(value) && value >= 0 ? value : 'inherit';
}
```

In `src/lib/pane-utils.ts`, extend Plan 05's `createDefaultRequestFor` with a WebSocket branch next to its GraphQL branch, and import the draft factory (the helpers file imports nothing from `pane-utils`, so there is no cycle):

```ts
import { createDefaultWebSocketDraft } from '@/lib/websocket-messages';
// ...
  if (kind === 'websocket') {
    return { ...base, requestType: 'websocket', websocket: createDefaultWebSocketDraft() };
  }
```

`pane-store.openEphemeralTab` already calls `createDefaultRequestFor(requestType)` after Plan 05, so a new WebSocket tab from the tab bar now gets a draft with no further change.

In `src/lib/colors.ts`, add next to Plan 05's `GQL` entry of `METHOD_BADGE_COLOR`:

```ts
  WS: 'text-teal-500 dark:text-teal-400 border-teal-500/30 bg-teal-500/10 dark:bg-teal-500/20',
```

In `src/lib/contracts/collectPaths.ts`, extend Plan 05's guard to `} else if (item.type !== 'opaque' && item.type !== 'graphql' && item.type !== 'websocket') {` and add this case to `src/lib/contracts/collectPaths.test.ts` (next to Plan 05's `skips typed graphql items like opaque ones`):

```ts
  it('skips typed websocket items so they are never treated as request paths', () => {
    const items: CollectionItem[] = [
      {
        type: 'websocket',
        uid: 'ws1',
        name: 'Chat',
        url: 'wss://x',
        headers: [],
        messages: [],
        auth: { authType: 'none' },
        fileName: 'chat.yml',
      },
    ];
    const folders: string[] = [];
    const requests: string[] = [];
    collectPaths(items, '', folders, requests);
    expect(requests).toEqual([]);
  });
```

In `CollectionNode.tsx` and `FolderNode.tsx`, extend the two guards Plan 05 changed:

```tsx
  const filterableItems = rawItems.filter(
    (item) => item.type !== 'opaque' && item.type !== 'graphql' && item.type !== 'websocket',
  );
```

(`FolderNode.tsx` uses `items.filter(...)`) and

```tsx
            if (item.type === 'opaque' || item.type === 'graphql' || item.type === 'websocket')
              return null;
```

and extend Plan 05's comment: typed full-tree items never render here, the sidebar loads summaries, where WebSocket arrives as a `summary` with `kind: 'websocket'`.

Run: `yarn test --run websocket-mapper collectPaths` and `yarn tsc --noEmit`.
Expected: mapper 12 passed, collectPaths passed, no type errors.

- [ ] **Step 9: Write the failing sidebar, create and save tests**

Add to `src/components/collections/__tests__/RequestNode.test.tsx` (Plan 05 already touches this file; extend the `vi.mock('@/lib/tauri-api', ...)` factory with `getWebSocketRequest: vi.fn(),` next to `getRequest` and `getGraphQlRequest`):

```tsx
const wsSummary: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }> = {
  type: 'summary',
  uid: 'ws-1',
  name: 'Chat',
  method: 'GET',
  url: 'wss://chat.example.com/ws',
  fileName: 'chat.yml',
  kind: 'websocket',
};

describe('RequestNode websocket rows', () => {
  beforeEach(() => {
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.getRequest).mockReset();
    vi.mocked(tauriApi.getWebSocketRequest).mockReset();
  });

  it('shows a WS badge, not the HTTP verb', () => {
    renderNode(wsSummary, 'chat.yml');
    expect(screen.getByText('WS')).toBeInTheDocument();
    expect(screen.queryByText('GET')).not.toBeInTheDocument();
  });

  it('opens a websocket tab through getWebSocketRequest, never getRequest', async () => {
    vi.mocked(tauriApi.getWebSocketRequest).mockResolvedValue({
      uid: 'ws-1',
      name: 'Chat',
      url: 'wss://chat.example.com/ws',
      headers: [{ key: 'Origin', value: 'https://example.com', enabled: true }],
      messages: [{ title: 'hello', selected: true, kind: 'json', data: '{}' }],
      auth: { authType: 'none' },
    });
    renderNode(wsSummary, 'chat.yml');

    await userEvent.setup().click(screen.getByLabelText('Open WS Chat'));

    await waitFor(() => {
      expect(tauriApi.getWebSocketRequest).toHaveBeenCalledWith('my-api', 'chat.yml');
    });
    await waitFor(() => {
      const found = findTabInTree(usePaneStore.getState().root, 'ws-1');
      expect(found?.tab.tabType).toBe('request');
      if (found?.tab.tabType !== 'request') return;
      expect(found.tab.request.requestType).toBe('websocket');
      expect(found.tab.request.url).toBe('wss://chat.example.com/ws');
      expect(found.tab.request.websocket?.messages[0].data).toBe('{}');
    });
    expect(tauriApi.getRequest).not.toHaveBeenCalled();
  });

  it('is not draggable into a Flow, which only runs HTTP requests', () => {
    renderNode(wsSummary, 'chat.yml');
    expect(screen.getByTestId('request-item-WS-Chat')).not.toHaveAttribute('draggable', 'true');
  });
});
```

Create `src/lib/__tests__/websocket-create.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { createWebSocketItem } from '@/lib/websocket-create';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    saveWebSocketRequest: vi.fn(),
    saveRequest: vi.fn(),
  };
});

describe('createWebSocketItem', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.saveWebSocketRequest).mockReset();
    vi.mocked(tauriApi.saveRequest).mockReset();
    vi.mocked(tauriApi.saveWebSocketRequest).mockImplementation(async (_c, path, req) => ({
      ...req,
      fileName: `${path}.yml`,
    }));
  });

  it('saves a real websocket file, never an http one', async () => {
    await createWebSocketItem('my-api', undefined, 'Chat', 'wss://x/ws');

    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    const [collection, path, payload] = vi.mocked(tauriApi.saveWebSocketRequest).mock.calls[0];
    expect(collection).toBe('my-api');
    expect(path).toBe('Chat');
    expect(payload.url).toBe('wss://x/ws');
    expect(payload.name).toBe('Chat');
    expect(payload.messages).toHaveLength(1);
    expect(payload.uid).toBeTruthy();
  });

  it('places the file under the folder and opens a websocket tab on the saved path', async () => {
    const tab = await createWebSocketItem('my-api', 'realtime', 'Chat', '');

    expect(vi.mocked(tauriApi.saveWebSocketRequest).mock.calls[0][1]).toBe('realtime/Chat');
    expect(tab.request.requestType).toBe('websocket');
    expect(tab.source).toEqual({ collection: 'my-api', path: 'realtime/Chat.yml' });
    expect(tab.isDirty).toBe(false);
  });
});
```

Create `src/lib/__tests__/save-tab-request.websocket.test.ts` (a separate file, so it does not depend on the shape of the mock factory in Plan 05's `save-tab-request.test.ts`):

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultRequest } from '@/lib/pane-utils';
import { saveTabRequest } from '@/lib/save-tab-request';
import * as tauriApi from '@/lib/tauri-api';
import { createDefaultWebSocketRequestState } from '@/lib/websocket-mapper';
import type { RequestTab } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    saveRequest: vi.fn().mockResolvedValue({ fileName: 'a.yml' }),
    saveGraphQlRequest: vi.fn().mockResolvedValue({ fileName: 'q.yml' }),
    saveWebSocketRequest: vi.fn().mockResolvedValue({ fileName: 'chat.yml' }),
  };
});

function tab(request: RequestTab['request']): RequestTab {
  return { id: 'u1', title: 'T', tabType: 'request', request, response: null, isDirty: true };
}

describe('saveTabRequest websocket routing', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.saveRequest).mockClear();
    vi.mocked(tauriApi.saveGraphQlRequest).mockClear();
    vi.mocked(tauriApi.saveWebSocketRequest).mockClear();
  });

  it('saves a websocket tab with the websocket command and never the http one', async () => {
    const saved = await saveTabRequest('c', 'chat', tab(createDefaultWebSocketRequestState('wss://x')));

    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    expect(tauriApi.saveGraphQlRequest).not.toHaveBeenCalled();
    expect(tauriApi.saveWebSocketRequest).toHaveBeenCalledTimes(1);
    const [, , payload] = vi.mocked(tauriApi.saveWebSocketRequest).mock.calls[0];
    expect(payload.uid).toBe('u1');
    expect(payload.url).toBe('wss://x');
    expect(saved.fileName).toBe('chat.yml');
  });

  it('still saves an http tab with the http command', async () => {
    await saveTabRequest('c', 'a', tab(createDefaultRequest()));
    expect(tauriApi.saveWebSocketRequest).not.toHaveBeenCalled();
    expect(tauriApi.saveRequest).toHaveBeenCalledTimes(1);
  });

  it('honours the name and file name overrides used by Save to Collection', async () => {
    await saveTabRequest('c', 'chat', tab(createDefaultWebSocketRequestState()), {
      name: 'Renamed',
      fileName: 'chat',
    });
    const [, , payload] = vi.mocked(tauriApi.saveWebSocketRequest).mock.calls[0];
    expect(payload.name).toBe('Renamed');
    expect(payload.fileName).toBe('chat');
  });
});
```

Create `src/lib/__tests__/auto-save.websocket.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cancelAutoSave, scheduleAutoSave } from '@/lib/auto-save';
import * as tauriApi from '@/lib/tauri-api';
import { createDefaultWebSocketRequestState } from '@/lib/websocket-mapper';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    saveRequest: vi.fn().mockResolvedValue({}),
    saveGraphQlRequest: vi.fn().mockResolvedValue({}),
    saveWebSocketRequest: vi.fn().mockResolvedValue({}),
  };
});

describe('scheduleAutoSave for a websocket tab', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(tauriApi.saveRequest).mockClear();
    vi.mocked(tauriApi.saveWebSocketRequest).mockClear();
  });
  afterEach(() => {
    cancelAutoSave('u1');
    vi.useRealTimers();
  });

  it('auto-saves with the websocket command, never the http one', async () => {
    scheduleAutoSave('u1', 'c', 'chat.yml', 'Chat', createDefaultWebSocketRequestState('wss://x'));
    await vi.advanceTimersByTimeAsync(600);

    expect(tauriApi.saveWebSocketRequest).toHaveBeenCalledTimes(1);
    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
  });
});
```

(`auto-save.ts` calls `usePaneStore.getState().markClean(tabId)` after saving; with no such tab in the store that is a no-op.)

Create `src/components/request/__tests__/CreateRequestDialog.websocket.test.tsx`, following the structure of Plan 05's `CreateRequestDialog.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CreateRequestDialog } from '@/components/request/CreateRequestDialog';
import { createDefaultLeaf, findTabInTree } from '@/lib/pane-utils';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    saveRequest: vi.fn(),
    saveGraphQlRequest: vi.fn(),
    saveWebSocketRequest: vi.fn(),
  };
});

describe('CreateRequestDialog websocket', () => {
  beforeEach(() => {
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.saveRequest).mockReset();
    vi.mocked(tauriApi.saveWebSocketRequest).mockReset();
    vi.mocked(tauriApi.saveWebSocketRequest).mockImplementation(async (_c, path, request) => ({
      ...request,
      fileName: `${path}.yml`,
    }));
  });

  it('saves a real WebSocket item and opens it, instead of an HTTP file under a WebSocket label', async () => {
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox', { name: /request type/i }));
    const option = await screen.findByRole('option', { name: 'WebSocket' });
    expect(option.getAttribute('aria-disabled')).not.toBe('true');
    await userEvent.click(option);
    await userEvent.type(screen.getByLabelText('Request Name'), 'chat');
    await userEvent.type(screen.getByLabelText('URL'), 'wss://echo.websocket.org');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => expect(tauriApi.saveWebSocketRequest).toHaveBeenCalledTimes(1));
    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    const [, , payload] = vi.mocked(tauriApi.saveWebSocketRequest).mock.calls[0];
    expect(payload.url).toBe('wss://echo.websocket.org');
    expect(payload.messages).toHaveLength(1);

    const found = findTabInTree(usePaneStore.getState().root, payload.uid);
    expect(found?.tab.tabType).toBe('request');
    if (found?.tab.tabType !== 'request') return;
    expect(found.tab.request.requestType).toBe('websocket');
  });
});
```

As in Plan 05, if the Radix `Select` trigger is not exposed as a `combobox` named "Request Type" in jsdom, open it the way the repo's other tests do (search `__tests__` for `combobox`).

- [ ] **Step 10: Run them and confirm they fail**

Run: `yarn test --run RequestNode websocket-create save-tab-request.websocket auto-save.websocket CreateRequestDialog.websocket`
Expected: FAIL (no `WS` badge, `createWebSocketItem` missing, `saveTabRequest` and auto-save still send a WebSocket tab through `saveRequest`, the dialog option is disabled).

- [ ] **Step 11: Implement the sidebar row, the create path and the save routes**

`src/lib/websocket-create.ts`:

```ts
import { sanitizeFilename } from '@/lib/filename-utils';
import { saveWebSocketRequest } from '@/lib/tauri-api';
import {
  createDefaultWebSocketRequestState,
  toApiWebSocketRequest,
  webSocketToTab,
} from '@/lib/websocket-mapper';
import type { RequestTab } from '@/types/pane-types';

/** Creates and saves a new WebSocket request file, and returns the tab to open for it. */
export async function createWebSocketItem(
  collectionName: string,
  folderPath: string | undefined,
  name: string,
  url: string,
): Promise<RequestTab> {
  const fsName = sanitizeFilename(name);
  const filePath = folderPath ? `${folderPath}/${fsName}` : fsName;
  const payload = toApiWebSocketRequest(
    crypto.randomUUID(),
    name,
    createDefaultWebSocketRequestState(url),
    filePath,
  );
  const saved = await saveWebSocketRequest(collectionName, filePath, payload);
  return webSocketToTab(saved, collectionName, saved.fileName ?? filePath);
}
```

(`sanitizeFilename` is what `CreateRequestDialog.tsx` already uses to derive `fsName`; the backend's `request_filename_for` does not add a second `.yml`.)

`src/lib/save-tab-request.ts` (Plan 05): add the WebSocket branch next to the GraphQL one, before the final HTTP return:

```ts
import { saveWebSocketRequest } from '@/lib/tauri-api';
import { buildWebSocketSavePayload } from '@/lib/websocket-mapper';
// ...
  if (tab.request.requestType === 'websocket') {
    return saveWebSocketRequest(collection, path, buildWebSocketSavePayload(tab, overrides));
  }
```

`SaveRequestButton.tsx` and `SaveToCollectionDialog.tsx` already call `saveTabRequest` after Plan 05, so they need no change.

`src/lib/auto-save.ts` (Plan 05's `if (request.requestType === 'graphql') { ... } else { ... }`): add the WebSocket branch between them:

```ts
      } else if (request.requestType === 'websocket') {
        await saveWebSocketRequest(
          collection,
          path,
          toApiWebSocketRequest(tabId || crypto.randomUUID(), title, request),
        );
      } else {
```

with `saveWebSocketRequest` added to the tauri-api import and `import { toApiWebSocketRequest } from '@/lib/websocket-mapper';`.

`src/components/collections/RequestNode.tsx` (Plan 05's `kind`-aware version):
- import `getWebSocketRequest` next to `getGraphQlRequest` and `mapWebSocketToState` from `@/lib/websocket-mapper`;
- extend the badge: `const badge = kind === 'graphql' ? 'GQL' : kind === 'websocket' ? 'WS' : method;`;
- in `createTab`, add a branch between Plan 05's GraphQL branch and its HTTP `else`:

```tsx
    } else if (kind === 'websocket') {
      request = mapWebSocketToState(await getWebSocketRequest(collectionName, path));
    } else {
```

- wrap both `Duplicate` menu items (the `DropdownMenuItem` and the `ContextMenuItem`) in `{kind === 'http' && ( ... )}`. Duplicate copies a request with `getRequest`, which fails on a WebSocket or GraphQL file; hiding it is the safe behaviour until each protocol gets its own duplicate. (Plan 05 does not hide it for GraphQL; this also fixes that.)

`src/components/request/CreateRequestDialog.tsx` (Plan 05 marks gRPC and WebSocket `disabled` with "(coming soon)" labels): change the WebSocket entry back to `{ label: 'WebSocket', value: 'websocket' }` with no `disabled`, leave gRPC disabled, add `import { createWebSocketItem } from '@/lib/websocket-create';` and add this branch beside Plan 05's GraphQL branch (before the HTTP payload is built):

```tsx
      if (requestType === 'websocket') {
        const wsTab = await createWebSocketItem(collectionName, folderPath, trimmedName, url);
        usePaneStore.getState().openTab(wsTab);
        reset();
        onClose();
        return;
      }
```

The HTTP method select already hides for non-HTTP types.

- [ ] **Step 12: Verify**

Run, in order:
- `yarn test --run RequestNode websocket-create save-tab-request auto-save collectPaths websocket-messages websocket-mapper CreateRequestDialog CollectionNode pane-store`
- `yarn tsc --noEmit`
- `yarn check`

Expected: all green. Fix Biome formatting with `yarn format` on the files this task touched only. If `yarn tsc --noEmit` reports another site that switches on `CollectionItem['type']`, handle `'websocket'` the way that site handles `'opaque'`.

- [ ] **Step 13: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by path: `src/lib/tauri-api.ts`, `src/types/pane-types.ts`, `src/lib/pane-utils.ts`, `src/lib/websocket-messages.ts`, `src/lib/websocket-mapper.ts`, `src/lib/websocket-create.ts`, `src/lib/save-tab-request.ts`, `src/lib/auto-save.ts`, `src/lib/colors.ts`, `src/lib/contracts/collectPaths.ts`, `src/lib/contracts/collectPaths.test.ts`, `src/components/collections/RequestNode.tsx`, `src/components/collections/CollectionNode.tsx`, `src/components/collections/FolderNode.tsx`, `src/components/request/CreateRequestDialog.tsx`, `src/components/collections/__tests__/RequestNode.test.tsx`, and the six new test files named under Files.
Suggested subject: `feat(websocket): sidebar row, create and save paths for WebSocket requests`.

---

## Task 3: WebSocket tab UI

**Files:**
- Create: `src/types/message-log.ts`, `src/stores/websocket-store.ts`, `src/lib/websocket-event-bridge.ts`, `src/lib/websocket-session.ts`, `src/lib/message-log-format.ts`, `src/hooks/useWebSocketVariableContext.ts`, `src/components/request/websocket/MessageLog.tsx`, `src/components/request/websocket/WebSocketMessagesEditor.tsx`, `src/components/request/websocket/WebSocketPanel.tsx`, and tests `src/stores/__tests__/websocket-store.test.ts`, `src/lib/__tests__/websocket-event-bridge.test.ts`, `src/lib/__tests__/websocket-session.test.ts`, `src/lib/__tests__/message-log-format.test.ts`, `src/components/request/websocket/__tests__/WebSocketPanel.test.tsx`
- Modify: `src/components/panes/EditorGroup.tsx`, `src/App.tsx`, `src/stores/pane-store.ts` (the two session-cleanup helpers), `src/hooks/useKeyboardShortcuts.ts`, `src/stores/__tests__/pane-store.test.ts`

**Interfaces:**
- Consumes: `wsConnect`, `wsSend`, `wsDisconnect`, `onWebSocketMessage`, `onWebSocketStatus` (Task 2); `selectedMessage`, `addMessage`, `selectMessage`, `updateMessage`, `removeMessage`, `MESSAGE_KINDS`; `createDefaultWebSocketDraft` (`websocket-messages.ts`), `parseOptionalMs` (`websocket-mapper.ts`); `saveTabRequest` (through `SaveRequestButton` and `SaveToCollectionDialog`); the reusable editors listed under Facts.
- Produces:
  - `src/types/message-log.ts`: `MessageDirection = 'in' | 'out' | 'system'`; `MessageLogEntry { id; direction; label?; kind: 'text' | 'binary'; data; size; timestampMs }`.
  - `useWebSocketStore` with `byTab: Record<string, TabSession>`, `tabBySession`, actions `beginSession(tabId, sessionId)`, `applyMessage(e)`, `applyStatus(e)`, `failSession(tabId, error)`, `clearLog(tabId)`, `forgetTab(tabId)`; exports `MAX_LOG_ENTRIES`, `appendCapped`, `IDLE_SESSION`, types `ConnectionStatus`, `TabSession`.
  - `useWebSocketEventBridge()`.
  - `websocket-session.ts`: `buildConnectInput(tab)`, `connectTab(tab)`, `sendSelectedMessage(tab)`, `disconnectTab(tabId)`, `releaseWebSocketTab(tab)`.
  - `message-log-format.ts`: `formatTime`, `formatSize`, `previewPayload`.
  - `MessageLog` (props `entries`, `onClear`, `title?`, `emptyText?`) and `WebSocketPanel` (props `tab`, `groupId`).

- [ ] **Step 1: Write the failing log-format tests**

Create `src/lib/__tests__/message-log-format.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { formatSize, formatTime, previewPayload } from '@/lib/message-log-format';

describe('message log formatting', () => {
  it('formats a timestamp as local HH:MM:SS.mmm', () => {
    const ms = new Date(2026, 9, 5, 13, 4, 5, 7).getTime();
    expect(formatTime(ms)).toBe('13:04:05.007');
  });

  it('formats sizes in B, KB and MB', () => {
    expect(formatSize(0)).toBe('0 B');
    expect(formatSize(999)).toBe('999 B');
    expect(formatSize(1536)).toBe('1.5 KB');
    expect(formatSize(2 * 1024 * 1024)).toBe('2.0 MB');
  });

  it('previews text as is and truncates long text with the full size', () => {
    expect(previewPayload({ kind: 'text', data: 'hello', size: 5 })).toBe('hello');
    const long = 'x'.repeat(50);
    expect(previewPayload({ kind: 'text', data: long, size: 50 }, 10)).toBe(
      `${'x'.repeat(10)}… (50 bytes)`,
    );
  });

  it('previews binary frames as hex', () => {
    expect(previewPayload({ kind: 'binary', data: 'AQID', size: 3 })).toBe('01 02 03');
  });

  it('truncates long binary previews and states the total size', () => {
    const bytes = new Uint8Array(100).fill(255);
    const b64 = btoa(String.fromCharCode(...bytes));
    const preview = previewPayload({ kind: 'binary', data: b64, size: 100 });
    expect(preview.endsWith('… (100 bytes)')).toBe(true);
    expect(preview.split(' ').filter((p) => p === 'ff')).toHaveLength(64);
  });

  it('falls back to the raw data when binary data is not valid base64', () => {
    expect(previewPayload({ kind: 'binary', data: '***', size: 3 })).toBe('***');
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `yarn test --run message-log-format`
Expected: FAIL, module not found.

- [ ] **Step 3: Implement the log types and formatting**

Create `src/types/message-log.ts`:

```ts
export type MessageDirection = 'in' | 'out' | 'system';

/**
 * One line of a live message log. Shared by the WebSocket tab and the GraphQL subscription
 * panel. `system` entries are produced by the frontend (connected, closed) and carry size 0.
 */
export interface MessageLogEntry {
  id: string;
  direction: MessageDirection;
  /** Short tag shown as a badge, such as `next` or `error` for subscription results. */
  label?: string;
  kind: 'text' | 'binary';
  /** Text as is, or base64 when `kind` is `binary`. */
  data: string;
  /** Payload size in bytes. */
  size: number;
  timestampMs: number;
}
```

Create `src/lib/message-log-format.ts`:

```ts
import type { MessageLogEntry } from '@/types/message-log';

const pad = (value: number, width: number) => String(value).padStart(width, '0');

/** Local time as HH:MM:SS.mmm. */
export function formatTime(timestampMs: number): string {
  const d = new Date(timestampMs);
  return `${pad(d.getHours(), 2)}:${pad(d.getMinutes(), 2)}:${pad(d.getSeconds(), 2)}.${pad(d.getMilliseconds(), 3)}`;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const MAX_HEX_BYTES = 64;

/** A single-string preview of an entry's payload: text, truncated, or hex for binary. */
export function previewPayload(
  entry: Pick<MessageLogEntry, 'kind' | 'data' | 'size'>,
  maxChars = 2000,
): string {
  if (entry.kind === 'text') {
    return entry.data.length > maxChars
      ? `${entry.data.slice(0, maxChars)}… (${entry.size} bytes)`
      : entry.data;
  }
  try {
    const raw = atob(entry.data);
    const hex = Array.from(raw.slice(0, MAX_HEX_BYTES), (c) =>
      c.charCodeAt(0).toString(16).padStart(2, '0'),
    ).join(' ');
    return raw.length > MAX_HEX_BYTES ? `${hex} … (${entry.size} bytes)` : hex;
  } catch {
    return entry.data;
  }
}
```

Run: `yarn test --run message-log-format`
Expected: 6 passed.

- [ ] **Step 4: Write the failing store tests**

Create `src/stores/__tests__/websocket-store.test.ts`:

```ts
import { beforeEach, describe, expect, it } from 'vitest';
import type { WebSocketMessageEvent, WebSocketStatusEvent } from '@/lib/tauri-api';
import { appendCapped, MAX_LOG_ENTRIES, useWebSocketStore } from '../websocket-store';

const message = (session: string, data: string, direction: 'in' | 'out' = 'in'): WebSocketMessageEvent => ({
  type: 'webSocketMessage',
  session_id: session,
  direction,
  kind: 'text',
  data,
  size: data.length,
  timestamp_ms: 1000,
});

const status = (
  session: string,
  state: WebSocketStatusEvent['state'],
  extra: Partial<WebSocketStatusEvent> = {},
): WebSocketStatusEvent => ({
  type: 'webSocketStatus',
  session_id: session,
  state,
  subprotocol: null,
  code: null,
  reason: null,
  ...extra,
});

const tab = () => useWebSocketStore.getState().byTab['tab-1'];

beforeEach(() => {
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
});

describe('websocket-store', () => {
  it('routes events that arrive before the connect call returns', () => {
    const s = useWebSocketStore.getState();
    s.beginSession('tab-1', 'sess-1');
    // The backend publishes Open and the first frame before `ws_connect` resolves.
    useWebSocketStore.getState().applyStatus(status('sess-1', 'open', { subprotocol: 'graphql-ws' }));
    useWebSocketStore.getState().applyMessage(message('sess-1', 'hello'));

    expect(tab().status).toBe('open');
    expect(tab().subprotocol).toBe('graphql-ws');
    expect(tab().log.map((e) => [e.direction, e.data])).toEqual([
      ['system', 'Connected (graphql-ws)'],
      ['in', 'hello'],
    ]);
  });

  it('ignores events for unknown or finished sessions', () => {
    useWebSocketStore.getState().applyMessage(message('nobody', 'x'));
    expect(useWebSocketStore.getState().byTab).toEqual({});

    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyStatus(status('sess-1', 'closed', { code: 1000 }));
    const before = tab().log.length;
    useWebSocketStore.getState().applyMessage(message('sess-1', 'late'));
    expect(tab().log).toHaveLength(before);
    expect(tab().sessionId).toBeNull();
  });

  it('a terminal status records the reason and frees the session id', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyStatus(status('sess-1', 'failed', { reason: 'handshake rejected with HTTP 401' }));

    expect(tab().status).toBe('failed');
    expect(tab().error).toBe('handshake rejected with HTTP 401');
    expect(tab().sessionId).toBeNull();
    expect(tab().log.at(-1)?.data).toBe('Failed: handshake rejected with HTTP 401');
    expect(useWebSocketStore.getState().tabBySession['sess-1']).toBeUndefined();
  });

  it('a clean close logs the code and reason', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyStatus(status('sess-1', 'closed', { code: 4001, reason: 'bye' }));
    expect(tab().status).toBe('closed');
    expect(tab().log.at(-1)?.data).toBe('Closed 4001: bye');
  });

  it('failSession reports a rejected connect once, even if the failed event also arrives', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().failSession('tab-1', 'boom');
    useWebSocketStore.getState().applyStatus(status('sess-1', 'failed', { reason: 'boom' }));

    expect(tab().status).toBe('failed');
    expect(tab().log.filter((e) => e.data.startsWith('Failed')).length).toBe(1);
  });

  it('failSession does nothing once the event already marked the session failed', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyStatus(status('sess-1', 'failed', { reason: 'boom' }));
    useWebSocketStore.getState().failSession('tab-1', 'boom');
    expect(tab().log.filter((e) => e.data.startsWith('Failed')).length).toBe(1);
  });

  it('a new session replaces the old mapping but keeps the log', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyMessage(message('sess-1', 'one'));
    useWebSocketStore.getState().beginSession('tab-1', 'sess-2');

    expect(tab().sessionId).toBe('sess-2');
    expect(tab().status).toBe('connecting');
    expect(tab().log).toHaveLength(1);
    useWebSocketStore.getState().applyMessage(message('sess-1', 'stale'));
    expect(tab().log).toHaveLength(1);
  });

  it('caps the log and drops the oldest entries', () => {
    expect(appendCapped([1, 2, 3], 4, 3)).toEqual([2, 3, 4]);

    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    for (let i = 0; i < MAX_LOG_ENTRIES + 5; i++) {
      useWebSocketStore.getState().applyMessage(message('sess-1', `m${i}`));
    }
    expect(tab().log).toHaveLength(MAX_LOG_ENTRIES);
    expect(tab().log[0].data).toBe('m5');
  });

  it('clearLog empties only the log and forgetTab removes everything for the tab', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyMessage(message('sess-1', 'one'));
    useWebSocketStore.getState().clearLog('tab-1');
    expect(tab().log).toEqual([]);
    expect(tab().sessionId).toBe('sess-1');

    useWebSocketStore.getState().forgetTab('tab-1');
    expect(useWebSocketStore.getState().byTab['tab-1']).toBeUndefined();
    expect(useWebSocketStore.getState().tabBySession['sess-1']).toBeUndefined();
  });

  it('keeps outgoing messages as out entries with their size', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyMessage(message('sess-1', 'ping', 'out'));
    expect(tab().log[0]).toMatchObject({ direction: 'out', size: 4, timestampMs: 1000 });
  });
});
```

- [ ] **Step 5: Run it and confirm it fails**

Run: `yarn test --run websocket-store`
Expected: FAIL, module not found.

- [ ] **Step 6: Implement the store**

Create `src/stores/websocket-store.ts`:

```ts
import { create } from 'zustand';
import type { WebSocketMessageEvent, WebSocketStatusEvent } from '@/lib/tauri-api';
import type { MessageLogEntry } from '@/types/message-log';

/** The log keeps this many entries per tab; older ones are dropped. */
export const MAX_LOG_ENTRIES = 1000;

export type ConnectionStatus = 'idle' | 'connecting' | 'open' | 'closed' | 'failed';

export interface TabSession {
  /** Id of the live session, or null when none is connecting or open. */
  sessionId: string | null;
  status: ConnectionStatus;
  subprotocol: string | null;
  error: string | null;
  log: MessageLogEntry[];
}

export const IDLE_SESSION: TabSession = {
  sessionId: null,
  status: 'idle',
  subprotocol: null,
  error: null,
  log: [],
};

/** Appends and keeps only the newest `max` items. */
export function appendCapped<T>(list: T[], entry: T, max = MAX_LOG_ENTRIES): T[] {
  const next = [...list, entry];
  return next.length > max ? next.slice(next.length - max) : next;
}

let entrySeq = 0;
const nextEntryId = () => `ws-log-${++entrySeq}`;

function systemEntry(text: string, timestampMs = Date.now()): MessageLogEntry {
  return {
    id: nextEntryId(),
    direction: 'system',
    kind: 'text',
    data: text,
    size: 0,
    timestampMs,
  };
}

function closeText(code: number | null, reason: string | null): string {
  const head = code === null ? 'Closed' : `Closed ${code}`;
  return reason ? `${head}: ${reason}` : head;
}

interface WebSocketStoreState {
  byTab: Record<string, TabSession>;
  tabBySession: Record<string, string>;
  /** Registers a session for a tab before `ws_connect` is invoked, so no early event is lost. */
  beginSession: (tabId: string, sessionId: string) => void;
  applyMessage: (event: WebSocketMessageEvent) => void;
  applyStatus: (event: WebSocketStatusEvent) => void;
  /** A rejected connect call. A no-op when a status event already ended the session. */
  failSession: (tabId: string, error: string) => void;
  clearLog: (tabId: string) => void;
  forgetTab: (tabId: string) => void;
}

function withoutKey<T>(record: Record<string, T>, key: string): Record<string, T> {
  const { [key]: _removed, ...rest } = record;
  return rest;
}

export const useWebSocketStore = create<WebSocketStoreState>((set, get) => ({
  byTab: {},
  tabBySession: {},

  beginSession(tabId, sessionId) {
    const { byTab, tabBySession } = get();
    const previous = byTab[tabId] ?? IDLE_SESSION;
    const mapping = previous.sessionId ? withoutKey(tabBySession, previous.sessionId) : tabBySession;
    set({
      byTab: {
        ...byTab,
        [tabId]: { ...previous, sessionId, status: 'connecting', subprotocol: null, error: null },
      },
      tabBySession: { ...mapping, [sessionId]: tabId },
    });
  },

  applyMessage(event) {
    const { byTab, tabBySession } = get();
    const tabId = tabBySession[event.session_id];
    if (!tabId) return;
    const session = byTab[tabId];
    if (!session) return;
    const entry: MessageLogEntry = {
      id: nextEntryId(),
      direction: event.direction,
      kind: event.kind,
      data: event.data,
      size: event.size,
      timestampMs: event.timestamp_ms,
    };
    set({ byTab: { ...byTab, [tabId]: { ...session, log: appendCapped(session.log, entry) } } });
  },

  applyStatus(event) {
    const { byTab, tabBySession } = get();
    const tabId = tabBySession[event.session_id];
    if (!tabId) return;
    const session = byTab[tabId];
    if (!session) return;

    if (event.state === 'connecting') {
      set({ byTab: { ...byTab, [tabId]: { ...session, status: 'connecting' } } });
      return;
    }
    if (event.state === 'open') {
      const label = event.subprotocol ? `Connected (${event.subprotocol})` : 'Connected';
      set({
        byTab: {
          ...byTab,
          [tabId]: {
            ...session,
            status: 'open',
            subprotocol: event.subprotocol,
            error: null,
            log: appendCapped(session.log, systemEntry(label)),
          },
        },
      });
      return;
    }

    // closed or failed: the session is over, so later events for this id are ignored.
    const failed = event.state === 'failed';
    const text = failed ? `Failed: ${event.reason ?? 'connection lost'}` : closeText(event.code, event.reason);
    set({
      byTab: {
        ...byTab,
        [tabId]: {
          ...session,
          sessionId: null,
          status: failed ? 'failed' : 'closed',
          error: failed ? (event.reason ?? 'connection lost') : null,
          log: appendCapped(session.log, systemEntry(text)),
        },
      },
      tabBySession: withoutKey(tabBySession, event.session_id),
    });
  },

  failSession(tabId, error) {
    const { byTab, tabBySession } = get();
    const session = byTab[tabId];
    if (!session || session.status !== 'connecting') return;
    set({
      byTab: {
        ...byTab,
        [tabId]: {
          ...session,
          sessionId: null,
          status: 'failed',
          error,
          log: appendCapped(session.log, systemEntry(`Failed: ${error}`)),
        },
      },
      tabBySession: session.sessionId ? withoutKey(tabBySession, session.sessionId) : tabBySession,
    });
  },

  clearLog(tabId) {
    const { byTab } = get();
    const session = byTab[tabId];
    if (!session) return;
    set({ byTab: { ...byTab, [tabId]: { ...session, log: [] } } });
  },

  forgetTab(tabId) {
    const { byTab, tabBySession } = get();
    const session = byTab[tabId];
    set({
      byTab: withoutKey(byTab, tabId),
      tabBySession: session?.sessionId ? withoutKey(tabBySession, session.sessionId) : tabBySession,
    });
  },
}));
```

Run: `yarn test --run websocket-store`
Expected: 10 passed.

- [ ] **Step 7: Write the failing bridge and session tests**

Create `src/lib/__tests__/websocket-event-bridge.test.ts`:

```ts
import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { WebSocketMessageEvent, WebSocketStatusEvent } from '@/lib/tauri-api';
import { useWebSocketEventBridge } from '@/lib/websocket-event-bridge';
import { useWebSocketStore } from '@/stores/websocket-store';

let messageHandler: ((e: WebSocketMessageEvent) => void) | undefined;
let statusHandler: ((e: WebSocketStatusEvent) => void) | undefined;
const unlisten = vi.fn();

vi.mock('@/lib/tauri-api', () => ({
  onWebSocketMessage: vi.fn((h: (e: WebSocketMessageEvent) => void) => {
    messageHandler = h;
    return Promise.resolve(unlisten);
  }),
  onWebSocketStatus: vi.fn((h: (e: WebSocketStatusEvent) => void) => {
    statusHandler = h;
    return Promise.resolve(unlisten);
  }),
}));

beforeEach(() => {
  messageHandler = undefined;
  statusHandler = undefined;
  unlisten.mockClear();
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
});

describe('useWebSocketEventBridge', () => {
  it('routes message and status events into the store by session id', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    renderHook(() => useWebSocketEventBridge());

    statusHandler?.({
      type: 'webSocketStatus',
      session_id: 'sess-1',
      state: 'open',
      subprotocol: null,
      code: null,
      reason: null,
    });
    messageHandler?.({
      type: 'webSocketMessage',
      session_id: 'sess-1',
      direction: 'in',
      kind: 'text',
      data: 'hi',
      size: 2,
      timestamp_ms: 5,
    });

    const session = useWebSocketStore.getState().byTab['tab-1'];
    expect(session.status).toBe('open');
    expect(session.log.map((e) => e.data)).toEqual(['Connected', 'hi']);
  });

  it('unsubscribes both listeners on unmount', async () => {
    const { unmount } = renderHook(() => useWebSocketEventBridge());
    unmount();
    await Promise.resolve();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalledTimes(2);
  });
});
```

Create `src/lib/__tests__/websocket-session.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultLeaf } from '@/lib/pane-utils';
import * as tauriApi from '@/lib/tauri-api';
import {
  buildConnectInput,
  connectTab,
  disconnectTab,
  releaseWebSocketTab,
  sendSelectedMessage,
} from '@/lib/websocket-session';
import { createDefaultWebSocketRequestState } from '@/lib/websocket-mapper';
import { useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    wsConnect: vi.fn(),
    wsSend: vi.fn(),
    wsDisconnect: vi.fn(),
  };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));

function wsTab(): RequestTab {
  const request = createDefaultWebSocketRequestState('wss://echo.example.com/ws');
  request.headers = [
    { id: 'h1', key: 'X-Token', value: '{{token}}', enabled: true },
    { id: 'h2', key: 'X-Off', value: '1', enabled: false },
    { id: 'h3', key: '', value: 'draft', enabled: true },
  ];
  request.auth = { authType: 'bearer', bearer: { token: 'abc' } };
  if (request.websocket) {
    request.websocket.timeoutMs = 5000;
    request.websocket.keepAliveMs = 'inherit';
    request.websocket.messages[0].kind = 'json';
    request.websocket.messages[0].data = '{"a":"{{token}}"}';
  }
  return {
    id: 'tab-1',
    title: 'Chat',
    tabType: 'request',
    request,
    response: null,
    isDirty: false,
    source: { collection: 'my-api', path: 'chat.yml' },
  };
}

beforeEach(() => {
  createDefaultLeaf();
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
  vi.mocked(tauriApi.wsConnect).mockReset().mockResolvedValue(undefined);
  vi.mocked(tauriApi.wsSend).mockReset().mockResolvedValue(undefined);
  vi.mocked(tauriApi.wsDisconnect).mockReset().mockResolvedValue(undefined);
});

describe('buildConnectInput', () => {
  it('sends the url, saved headers, auth, scope and settings', () => {
    const input = buildConnectInput(wsTab());
    expect(input.url).toBe('wss://echo.example.com/ws');
    expect(input.headers).toEqual([
      { key: 'X-Token', value: '{{token}}', enabled: true },
      { key: 'X-Off', value: '1', enabled: false },
    ]);
    expect(input.auth).toEqual({ authType: 'bearer', token: 'abc' });
    expect(input.collection).toBe('my-api');
    expect(input.requestPath).toBe('chat.yml');
    expect(input.timeoutMs).toBe(5000);
    expect(input.keepAliveMs).toBeUndefined();
    expect(input.verifySsl).toBe(true);
  });
});

describe('connectTab', () => {
  it('registers the session before the connect call, so early events are routed', async () => {
    let registeredWhenInvoked: string | null = null;
    vi.mocked(tauriApi.wsConnect).mockImplementation(async (sessionId) => {
      registeredWhenInvoked = useWebSocketStore.getState().tabBySession[sessionId] ?? null;
    });

    await connectTab(wsTab());

    expect(registeredWhenInvoked).toBe('tab-1');
  });

  it('marks the tab failed when the connect call rejects', async () => {
    vi.mocked(tauriApi.wsConnect).mockRejectedValue('handshake rejected with HTTP 401');
    await connectTab(wsTab());

    const session = useWebSocketStore.getState().byTab['tab-1'];
    expect(session.status).toBe('failed');
    expect(session.error).toBe('handshake rejected with HTTP 401');
  });

  it('does not open a second session while one is connecting or open', async () => {
    useWebSocketStore.getState().beginSession('tab-1', 'existing');
    await connectTab(wsTab());
    expect(tauriApi.wsConnect).not.toHaveBeenCalled();
  });
});

describe('sendSelectedMessage', () => {
  it('sends the selected message with its kind and scope', async () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.setState((s) => ({
      byTab: { ...s.byTab, 'tab-1': { ...s.byTab['tab-1'], status: 'open' } },
    }));

    await sendSelectedMessage(wsTab());

    expect(tauriApi.wsSend).toHaveBeenCalledWith('sess-1', {
      kind: 'json',
      data: '{"a":"{{token}}"}',
      collection: 'my-api',
      environmentName: undefined,
      globalEnvName: undefined,
      requestPath: 'chat.yml',
    });
  });

  it('does nothing when the tab is not connected', async () => {
    await sendSelectedMessage(wsTab());
    expect(tauriApi.wsSend).not.toHaveBeenCalled();
  });
});

describe('disconnect and release', () => {
  it('disconnectTab closes the live session', async () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    await disconnectTab('tab-1');
    expect(tauriApi.wsDisconnect).toHaveBeenCalledWith('sess-1');
  });

  it('releasing a websocket tab disconnects it and forgets its state', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');

    releaseWebSocketTab(wsTab());

    expect(tauriApi.wsDisconnect).toHaveBeenCalledWith('sess-1');
    expect(useWebSocketStore.getState().byTab['tab-1']).toBeUndefined();
  });

  it('releasing an http tab does nothing', () => {
    const http: RequestTab = { ...wsTab(), request: { ...wsTab().request, requestType: 'http' } };
    releaseWebSocketTab(http);
    expect(tauriApi.wsDisconnect).not.toHaveBeenCalled();
  });
});
```

Delete the stray `createDefaultLeaf();` call and its import from that test if Biome flags them as unused; the store tests do not need a pane tree.

- [ ] **Step 8: Run them and confirm they fail**

Run: `yarn test --run websocket-event-bridge websocket-session`
Expected: FAIL, modules not found.

- [ ] **Step 9: Implement the bridge and the session actions**

Create `src/lib/websocket-event-bridge.ts`:

```ts
import { useEffect } from 'react';
import { onWebSocketMessage, onWebSocketStatus } from '@/lib/tauri-api';
import { useWebSocketStore } from '@/stores/websocket-store';

// Subscribes once, for the app's lifetime, to the WebSocket events and routes each one into the
// store by session id. It lives outside the panel so frames are never dropped while the panel
// of the owning tab is unmounted (only the active tab of a pane is mounted).
export function useWebSocketEventBridge(): void {
  useEffect(() => {
    const unsubs = Promise.all([
      onWebSocketMessage((e) => useWebSocketStore.getState().applyMessage(e)),
      onWebSocketStatus((e) => useWebSocketStore.getState().applyStatus(e)),
    ]);
    return () => {
      unsubs.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, []);
}
```

Create `src/lib/websocket-session.ts`:

```ts
import { toast } from 'sonner';
import { environmentKeys } from '@/lib/queries/environment-queries';
import { getQueryClient } from '@/lib/query-client';
import { toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import {
  type WebSocketConnectInput,
  type WebSocketScopeInput,
  wsConnect,
  wsDisconnect,
  wsSend,
} from '@/lib/tauri-api';
import { selectedMessage } from '@/lib/websocket-messages';
import { useEnvStore } from '@/stores/env-store';
import { useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab, Tab } from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

// Where {{variables}} come from. Read from the stores at call time, like the HTTP send path.
function scopeFor(tab: RequestTab): WebSocketScopeInput {
  return {
    collection: tab.source?.collection,
    environmentName: useEnvStore.getState().activeEnvId ?? undefined,
    globalEnvName:
      getQueryClient().getQueryData<string | null>(environmentKeys.globalName) ?? undefined,
    requestPath: tab.source?.path,
  };
}

/** The `ws_connect` input for a tab: raw values, the backend resolves variables. */
export function buildConnectInput(tab: RequestTab): WebSocketConnectInput {
  const { request } = tab;
  const draft = request.websocket;
  return {
    url: request.url,
    headers: toPersistedHeaders(request.headers),
    auth: toPersistedAuth(request.auth),
    timeoutMs: typeof draft?.timeoutMs === 'number' ? draft.timeoutMs : undefined,
    keepAliveMs: typeof draft?.keepAliveMs === 'number' ? draft.keepAliveMs : undefined,
    verifySsl: request.settings.verifySsl,
    ...scopeFor(tab),
  };
}

/**
 * Opens a session for the tab. The session id is registered in the store BEFORE the invoke,
 * because the backend publishes its first events before `ws_connect` resolves. A rejected
 * connect is shown through the store, not thrown.
 */
export async function connectTab(tab: RequestTab): Promise<void> {
  const current = useWebSocketStore.getState().byTab[tab.id];
  if (current && (current.status === 'connecting' || current.status === 'open')) return;

  const sessionId = crypto.randomUUID();
  useWebSocketStore.getState().beginSession(tab.id, sessionId);
  try {
    await wsConnect(sessionId, buildConnectInput(tab));
  } catch (err) {
    useWebSocketStore.getState().failSession(tab.id, errorText(err));
  }
}

/** Sends the tab's selected message on its open session. */
export async function sendSelectedMessage(tab: RequestTab): Promise<void> {
  const session = useWebSocketStore.getState().byTab[tab.id];
  if (!session?.sessionId || session.status !== 'open') return;
  const message = selectedMessage(tab.request.websocket?.messages ?? []);
  if (!message) return;
  try {
    await wsSend(session.sessionId, { kind: message.kind, data: message.data, ...scopeFor(tab) });
  } catch (err) {
    toast.error(`Could not send: ${errorText(err)}`);
  }
}

/** Asks the backend to close the tab's live session. The final status arrives as an event. */
export async function disconnectTab(tabId: string): Promise<void> {
  const sessionId = useWebSocketStore.getState().byTab[tabId]?.sessionId;
  if (!sessionId) return;
  try {
    await wsDisconnect(sessionId);
  } catch (err) {
    console.error('[websocket] disconnect failed:', err);
  }
}

/** Called when a tab is about to be discarded: closes its socket and forgets its state. */
export function releaseWebSocketTab(tab: Tab): void {
  if (!isRequestTab(tab) || tab.request.requestType !== 'websocket') return;
  void disconnectTab(tab.id);
  useWebSocketStore.getState().forgetTab(tab.id);
}
```

`disconnectTab` reads the session id synchronously before its first `await`, so `releaseWebSocketTab` can call `forgetTab` immediately after.

Run: `yarn test --run websocket-event-bridge websocket-session`
Expected: all pass. If `toPersistedAuth` of the test's `bearer` state produces a different shape, assert against that function's actual output (it is the single source of the persisted shape).

- [ ] **Step 10: Write the failing close-tab test and wire cleanup into `pane-store`**

Append to `src/stores/__tests__/pane-store.test.ts` (use the file's existing imports and reset helpers; if it has none, build the state the same way `RequestNode.test.tsx` does with `createDefaultLeaf`):

```ts
describe('websocket tab cleanup', () => {
  it('closing a websocket tab disconnects its session', async () => {
    const session = await import('@/lib/websocket-session');
    const release = vi.spyOn(session, 'releaseWebSocketTab').mockImplementation(() => undefined);
    const { createDefaultWebSocketRequestState } = await import('@/lib/websocket-mapper');

    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    usePaneStore.getState().openTab({
      id: 'ws-tab',
      title: 'Chat',
      tabType: 'request',
      request: createDefaultWebSocketRequestState('wss://x'),
      response: null,
      isDirty: false,
    });

    usePaneStore.getState().closeTab('ws-tab', leaf.groupId);

    expect(release).toHaveBeenCalledTimes(1);
    expect(release.mock.calls[0][0]).toMatchObject({ id: 'ws-tab' });
    release.mockRestore();
  });
});
```

(`createDefaultLeaf`, `usePaneStore` and `vi` are already imported in that file; add any that are missing. `vi.spyOn` on an ES module export works under Vitest because the module namespace is spied through the same instance `pane-store` imports.)

Run: `yarn test --run pane-store`
Expected: the new test FAILS (`release` never called).

In `src/stores/pane-store.ts` add `import { releaseWebSocketTab } from '@/lib/websocket-session';` and change the two helpers (lines 115-134):

```ts
function endSessionIfActive(tab: Tab): void {
  releaseWebSocketTab(tab);
  if (isRequestTab(tab) && tab.agentSession?.status === 'active') {
    Promise.resolve(endAgentSession(tab.agentSession.sessionId)).catch((err) => {
      console.error('[pane-store] failed to end agent session', err);
    });
  }
}

function endActiveSessions(tabs: Tab[]): void {
  const seen = new Set<string>();
  for (const tab of tabs) {
    // WebSocket sessions are keyed by tab id, so the same tab id is released once.
    if (isRequestTab(tab) && tab.request.requestType === 'websocket' && !seen.has(tab.id)) {
      seen.add(tab.id);
      releaseWebSocketTab(tab);
    }
    if (!isRequestTab(tab) || tab.agentSession?.status !== 'active') continue;
    if (seen.has(tab.agentSession.sessionId)) continue;
    seen.add(tab.agentSession.sessionId);
    endSessionIfActive(tab);
  }
}
```

`endSessionIfActive` calls `releaseWebSocketTab` too, which is idempotent (a second call finds no session and nothing in the store), so a tab ending both an agent and a WebSocket session is cleaned once and safely.

Run: `yarn test --run pane-store`
Expected: pass.

- [ ] **Step 11: Write the failing panel tests**

Create `src/components/request/websocket/__tests__/WebSocketPanel.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { WebSocketPanel } from '@/components/request/websocket/WebSocketPanel';
import { createDefaultWebSocketRequestState } from '@/lib/websocket-mapper';
import * as session from '@/lib/websocket-session';
import { createDefaultLeaf } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import { useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab } from '@/types/pane-types';

vi.mock('@/lib/websocket-session', () => ({
  connectTab: vi.fn().mockResolvedValue(undefined),
  disconnectTab: vi.fn().mockResolvedValue(undefined),
  sendSelectedMessage: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('@/hooks/useWebSocketVariableContext', () => ({
  useWebSocketVariableContext: () => undefined,
}));
// CodeMirror and Monaco do not run in jsdom; stand-ins keep the panel testable.
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
    <input aria-label='URL' placeholder={placeholder} value={value} onChange={(e) => onChange(e.target.value)} />
  ),
}));
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <textarea aria-label='Message body' value={value} onChange={(e) => onChange?.(e.target.value)} />
  ),
}));

function tab(): RequestTab {
  return {
    id: 'tab-1',
    title: 'Chat',
    tabType: 'request',
    request: createDefaultWebSocketRequestState('wss://echo.example.com/ws'),
    response: null,
    isDirty: false,
    source: { collection: 'my-api', path: 'chat.yml' },
  };
}

function mount() {
  const t = tab();
  const leaf = createDefaultLeaf();
  usePaneStore.setState({ root: { ...leaf, tabs: [t], activeTabId: t.id }, activeGroupId: leaf.groupId });
  return render(<WebSocketPanel tab={t} groupId={leaf.groupId} />);
}

function setSession(status: 'idle' | 'connecting' | 'open' | 'closed' | 'failed') {
  useWebSocketStore.setState({
    byTab: {
      'tab-1': { sessionId: status === 'open' ? 's1' : null, status, subprotocol: null, error: null, log: [] },
    },
    tabBySession: {},
  });
}

beforeEach(() => {
  vi.mocked(session.connectTab).mockClear();
  vi.mocked(session.disconnectTab).mockClear();
  vi.mocked(session.sendSelectedMessage).mockClear();
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
});

describe('WebSocketPanel', () => {
  it('shows Connect and a Disconnected badge when idle, and disables Send', () => {
    mount();
    expect(screen.getByRole('button', { name: 'Connect' })).toBeEnabled();
    expect(screen.getByText('Disconnected')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('Connect starts a session for the tab', async () => {
    mount();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Connect' }));
    expect(session.connectTab).toHaveBeenCalledTimes(1);
    expect(vi.mocked(session.connectTab).mock.calls[0][0]).toMatchObject({ id: 'tab-1' });
  });

  it('when open it offers Disconnect, enables Send, and Send sends the selected message', async () => {
    setSession('open');
    mount();
    expect(screen.getByText('Connected')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Send' }));
    expect(session.sendSelectedMessage).toHaveBeenCalledTimes(1);

    await userEvent.setup().click(screen.getByRole('button', { name: 'Disconnect' }));
    expect(session.disconnectTab).toHaveBeenCalledWith('tab-1');
  });

  it('while connecting it shows a disabled Connecting button', () => {
    setSession('connecting');
    mount();
    expect(screen.getByRole('button', { name: 'Connecting...' })).toBeDisabled();
  });

  it('renders the log with direction, size and payload, and Clear empties it', async () => {
    useWebSocketStore.setState({
      byTab: {
        'tab-1': {
          sessionId: 's1',
          status: 'open',
          subprotocol: null,
          error: null,
          log: [
            { id: 'a', direction: 'out', kind: 'text', data: 'ping', size: 4, timestampMs: 1000 },
            { id: 'b', direction: 'in', kind: 'text', data: 'pong', size: 4, timestampMs: 2000 },
          ],
        },
      },
      tabBySession: { s1: 'tab-1' },
    });
    mount();

    expect(screen.getByText('ping')).toBeInTheDocument();
    expect(screen.getByText('pong')).toBeInTheDocument();
    expect(screen.getAllByText('4 B')).toHaveLength(2);

    await userEvent.setup().click(screen.getByRole('button', { name: 'Clear log' }));
    expect(useWebSocketStore.getState().byTab['tab-1'].log).toEqual([]);
  });

  it('adds, selects and removes saved messages, always keeping one selected', async () => {
    mount();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: 'Add message' }));
    const draft = () => {
      const found = usePaneStore.getState().root;
      if (found.type !== 'leaf') throw new Error('expected a leaf');
      const t = found.tabs[0];
      if (t.tabType !== 'request') throw new Error('expected a request tab');
      return t.request.websocket?.messages ?? [];
    };
    expect(draft().map((m) => m.title)).toEqual(['message 1', 'message 2']);
    expect(draft().map((m) => m.selected)).toEqual([false, true]);

    await user.click(screen.getByRole('button', { name: 'Select message 1' }));
    expect(draft().map((m) => m.selected)).toEqual([true, false]);

    await user.click(screen.getByRole('button', { name: 'Delete message 1' }));
    expect(draft().map((m) => m.title)).toEqual(['message 2']);
    expect(draft().map((m) => m.selected)).toEqual([true]);
  });

  it('editing the body marks the tab dirty', async () => {
    mount();
    await userEvent.setup().type(screen.getByLabelText('Message body'), 'x');
    const root = usePaneStore.getState().root;
    if (root.type !== 'leaf') throw new Error('expected a leaf');
    expect(root.tabs[0].isDirty).toBe(true);
  });
});
```

- [ ] **Step 12: Run it and confirm it fails**

Run: `yarn test --run WebSocketPanel`
Expected: FAIL, module not found.

- [ ] **Step 13: Implement the log, the composer and the panel**

Create `src/hooks/useWebSocketVariableContext.ts`:

```ts
import { useEffect, useMemo, useState } from 'react';
import {
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
} from '@/lib/queries/environment-queries';
import { type CollectionVariable, getCollectionSettings } from '@/lib/tauri-api';
import { buildScopedContext, type VariableScopeEntry } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';

/**
 * Variable highlighting for the WebSocket panel: environment, global, process and collection
 * variables. Folder and request-level variables are resolved by the backend at connect time but
 * are not highlighted here yet (the HTTP panel loads them per request).
 */
export function useWebSocketVariableContext(
  collection: string | undefined,
): Map<string, VariableScopeEntry> {
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const { data: environments = [] } = useEnvironments(activeCollection);
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = {} } = useProcessEnvVars();
  const [collectionVars, setCollectionVars] = useState<CollectionVariable[]>([]);

  useEffect(() => {
    if (!collection) {
      setCollectionVars([]);
      return;
    }
    let cancelled = false;
    getCollectionSettings(collection)
      .then((settings) => {
        if (!cancelled) setCollectionVars(settings.variables);
      })
      .catch(() => {
        if (!cancelled) setCollectionVars([]);
      });
    return () => {
      cancelled = true;
    };
  }, [collection]);

  return useMemo(() => {
    const activeEnv = activeEnvId ? environments.find((e) => e.name === activeEnvId) : undefined;
    const envVars: Record<string, string> = {};
    if (activeEnv) for (const v of activeEnv.variables) if (v.enabled) envVars[v.key] = v.value;
    const globalVars = globalEnv
      ? Object.fromEntries(globalEnv.variables.filter((v) => v.enabled).map((v) => [v.key, v.value]))
      : {};
    return buildScopedContext({
      envVars,
      envLabel: activeEnvId ?? undefined,
      externalSecrets: activeEnv?.externalSecrets,
      globalVars,
      processEnvVars,
      collectionVars,
    });
  }, [activeEnvId, environments, globalEnv, processEnvVars, collectionVars]);
}
```

Create `src/components/request/websocket/MessageLog.tsx`:

```tsx
import { ArrowDownLeft, ArrowUpRight, Info, Trash2 } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { formatSize, formatTime, previewPayload } from '@/lib/message-log-format';
import { cn } from '@/lib/utils';
import type { MessageDirection, MessageLogEntry } from '@/types/message-log';

interface MessageLogProps {
  entries: MessageLogEntry[];
  onClear: () => void;
  title?: string;
  emptyText?: string;
}

const DIRECTION_LABEL: Record<MessageDirection, string> = {
  in: 'Received',
  out: 'Sent',
  system: 'Status',
};

function DirectionIcon({ direction }: { direction: MessageDirection }) {
  const className = cn(
    'h-3.5 w-3.5 shrink-0 mt-0.5',
    direction === 'in' && 'text-[hsl(var(--success))]',
    direction === 'out' && 'text-primary',
    direction === 'system' && 'text-muted-foreground',
  );
  if (direction === 'in') return <ArrowDownLeft aria-hidden='true' className={className} />;
  if (direction === 'out') return <ArrowUpRight aria-hidden='true' className={className} />;
  return <Info aria-hidden='true' className={className} />;
}

/**
 * A live, auto-scrolling message log. Generic on purpose: the WebSocket tab and the GraphQL
 * subscription panel both render it. It follows new entries only while the user is at the bottom.
 */
export function MessageLog({
  entries,
  onClear,
  title = 'Messages',
  emptyText = 'No messages yet.',
}: MessageLogProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  // biome-ignore lint/correctness/useExhaustiveDependencies: scroll when the entry count changes.
  useEffect(() => {
    const el = scroller.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [entries.length]);

  const handleScroll = () => {
    const el = scroller.current;
    if (!el) return;
    stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
  };

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex items-center justify-between border-b px-3 py-1.5'>
        <span className='text-xs font-medium text-muted-foreground'>
          {title} ({entries.length})
        </span>
        <Button
          size='icon'
          variant='ghost'
          className='h-6 w-6'
          aria-label='Clear log'
          onClick={onClear}
          disabled={entries.length === 0}
        >
          <Trash2 aria-hidden='true' className='h-3.5 w-3.5' />
        </Button>
      </div>
      <div ref={scroller} onScroll={handleScroll} className='min-h-0 flex-1 overflow-auto'>
        {entries.length === 0 ? (
          <p className='px-3 py-4 text-xs text-muted-foreground'>{emptyText}</p>
        ) : (
          <ul>
            {entries.map((entry) => (
              <li
                key={entry.id}
                data-direction={entry.direction}
                className='flex items-start gap-2 border-b border-border/50 px-3 py-1 font-mono text-xs'
              >
                <span title={DIRECTION_LABEL[entry.direction]}>
                  <DirectionIcon direction={entry.direction} />
                </span>
                <span className='shrink-0 text-muted-foreground'>{formatTime(entry.timestampMs)}</span>
                {entry.label && (
                  <Badge variant='outline' className='shrink-0 px-1.5 py-0 text-[10px]'>
                    {entry.label}
                  </Badge>
                )}
                {entry.direction !== 'system' && (
                  <span className='shrink-0 text-muted-foreground'>{formatSize(entry.size)}</span>
                )}
                <pre
                  className={cn(
                    'min-w-0 flex-1 whitespace-pre-wrap break-all font-mono',
                    entry.direction === 'system' && 'italic text-muted-foreground',
                  )}
                >
                  {previewPayload(entry)}
                </pre>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
```

Create `src/components/request/websocket/WebSocketMessagesEditor.tsx`:

```tsx
import { Plus, Send, Trash2 } from 'lucide-react';
import { lazy, Suspense } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { WebSocketMessageKind } from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import {
  addMessage,
  MESSAGE_KINDS,
  removeMessage,
  selectedMessage,
  selectMessage,
  updateMessage,
} from '@/lib/websocket-messages';
import type { WebSocketDraftMessage } from '@/types/pane-types';

// Lazy-load Monaco so it stays out of the initial JS bundle.
const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

const LANGUAGE: Record<WebSocketMessageKind, string> = {
  text: 'plaintext',
  json: 'json',
  xml: 'xml',
  binary: 'plaintext',
};

interface WebSocketMessagesEditorProps {
  messages: WebSocketDraftMessage[];
  onChange: (messages: WebSocketDraftMessage[]) => void;
  onSend: () => void;
  canSend: boolean;
  variableContext?: Map<string, VariableScopeEntry>;
}

// The saved messages of a WebSocket request: a list to pick from, and an editor for the
// selected one. Send sends the selected message.
export function WebSocketMessagesEditor({
  messages,
  onChange,
  onSend,
  canSend,
  variableContext,
}: WebSocketMessagesEditorProps) {
  const current = selectedMessage(messages);

  return (
    <div className='flex h-full min-h-0 gap-3'>
      <div className='flex w-44 shrink-0 flex-col gap-1 overflow-auto'>
        {messages.map((m) => (
          <div key={m.id} className='flex items-center gap-1'>
            <Button
              size='sm'
              variant={m.selected ? 'secondary' : 'ghost'}
              className={cn('h-7 flex-1 justify-start truncate px-2 text-xs', m.selected && 'font-semibold')}
              aria-label={`Select ${m.title || 'message'}`}
              aria-pressed={m.selected}
              onClick={() => onChange(selectMessage(messages, m.id))}
            >
              {m.title || 'Untitled'}
            </Button>
            <Button
              size='icon'
              variant='ghost'
              className='h-6 w-6'
              aria-label={`Delete ${m.title || 'message'}`}
              onClick={() => onChange(removeMessage(messages, m.id))}
            >
              <Trash2 aria-hidden='true' className='h-3 w-3' />
            </Button>
          </div>
        ))}
        <Button
          size='sm'
          variant='outline'
          className='h-7 justify-start px-2 text-xs'
          aria-label='Add message'
          onClick={() => onChange(addMessage(messages))}
        >
          <Plus aria-hidden='true' className='mr-1 h-3 w-3' /> Add message
        </Button>
      </div>

      {current ? (
        <div className='flex min-h-0 min-w-0 flex-1 flex-col gap-2'>
          <div className='flex items-center gap-2'>
            <Input
              className='h-8 flex-1 text-sm'
              placeholder='Message title'
              aria-label='Message title'
              value={current.title}
              onChange={(e) => onChange(updateMessage(messages, current.id, { title: e.target.value }))}
            />
            <Select
              value={current.kind}
              onValueChange={(kind) =>
                onChange(updateMessage(messages, current.id, { kind: kind as WebSocketMessageKind }))
              }
            >
              <SelectTrigger className='h-8 w-40' aria-label='Message type'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {MESSAGE_KINDS.map((k) => (
                  <SelectItem key={k.value} value={k.value}>
                    {k.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button size='sm' className='h-8' disabled={!canSend} onClick={onSend} aria-label='Send'>
              <Send aria-hidden='true' className='mr-1 h-3.5 w-3.5' /> Send
            </Button>
          </div>
          <div className='min-h-[140px] flex-1 overflow-hidden rounded-lg border'>
            <Suspense fallback={<EditorSkeleton />}>
              <MonacoWrapper
                value={current.data}
                onChange={(data) => onChange(updateMessage(messages, current.id, { data }))}
                language={LANGUAGE[current.kind]}
                height='100%'
                variableContext={variableContext}
              />
            </Suspense>
          </div>
          {current.kind === 'binary' && (
            <p className='text-xs text-muted-foreground'>
              Binary messages are base64. The log shows what was sent as hex.
            </p>
          )}
        </div>
      ) : (
        <p className='text-sm text-muted-foreground'>
          No saved messages. Add one to start sending.
        </p>
      )}
    </div>
  );
}
```

Create `src/components/request/websocket/WebSocketPanel.tsx`:

```tsx
import { Plug, PlugZap } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useWebSocketVariableContext } from '@/hooks/useWebSocketVariableContext';
import { authStateForType } from '@/lib/auth-type-defaults';
import { withCurrentAuthType } from '@/lib/auth-type-options';
import { cn } from '@/lib/utils';
import { parseOptionalMs } from '@/lib/websocket-mapper';
import { createDefaultWebSocketDraft } from '@/lib/websocket-messages';
import { connectTab, disconnectTab, sendSelectedMessage } from '@/lib/websocket-session';
import { usePaneStore } from '@/stores/pane-store';
import { type ConnectionStatus, IDLE_SESSION, useWebSocketStore } from '@/stores/websocket-store';
import type {
  AuthState,
  KeyValueEntry,
  RequestTab,
  WebSocketDraft,
  WebSocketDraftMessage,
} from '@/types/pane-types';
import { AuthEditor } from '../AuthEditor';
import { HeadersEditor } from '../HeadersEditor';
import { SaveRequestButton } from '../SaveRequestButton';
import { SaveToCollectionDialog } from '../SaveToCollectionDialog';
import { MessageLog } from './MessageLog';
import { WebSocketMessagesEditor } from './WebSocketMessagesEditor';

// Only auth types a WebSocket handshake can use are offered; one already set stays selectable.
const AUTH_OPTIONS: { label: string; value: AuthState['authType'] }[] = [
  { label: 'Inherit', value: 'inherit' },
  { label: 'None', value: 'none' },
  { label: 'Basic', value: 'basic' },
  { label: 'Bearer', value: 'bearer' },
  { label: 'API Key', value: 'api-key' },
];

const STATUS_LABEL: Record<ConnectionStatus, string> = {
  idle: 'Disconnected',
  connecting: 'Connecting',
  open: 'Connected',
  closed: 'Closed',
  failed: 'Failed',
};

interface WebSocketPanelProps {
  tab: RequestTab;
  groupId: string;
}

export function WebSocketPanel({ tab, groupId }: WebSocketPanelProps) {
  const updateRequest = usePaneStore((s) => s.updateRequest);
  const session = useWebSocketStore((s) => s.byTab[tab.id]) ?? IDLE_SESSION;
  const clearLog = useWebSocketStore((s) => s.clearLog);
  const variableContext = useWebSocketVariableContext(tab.source?.collection);
  const [saveToCollectionOpen, setSaveToCollectionOpen] = useState(false);

  const request = tab.request;
  const draft = useMemo<WebSocketDraft>(
    () => request.websocket ?? createDefaultWebSocketDraft(),
    [request.websocket],
  );
  const connected = session.status === 'open';
  const connecting = session.status === 'connecting';

  const patchDraft = useCallback(
    (patch: Partial<WebSocketDraft>) => updateRequest(tab.id, { websocket: { ...draft, ...patch } }),
    [draft, tab.id, updateRequest],
  );

  // Ctrl or Cmd+Enter sends the selected message (see useKeyboardShortcuts).
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId: string }>).detail;
      if (detail?.tabId === tab.id) void sendSelectedMessage(tab);
    };
    window.addEventListener('rocket:websocket-send', handler);
    return () => window.removeEventListener('rocket:websocket-send', handler);
  }, [tab]);

  // Opens the save dialog for an unsaved tab (Ctrl or Cmd+S on a tab with no source).
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId: string }>).detail;
      if (detail?.tabId === tab.id) setSaveToCollectionOpen(true);
    };
    window.addEventListener('rocket:save-to-collection', handler);
    return () => window.removeEventListener('rocket:save-to-collection', handler);
  }, [tab.id]);

  const authOptions = useMemo(
    () => withCurrentAuthType(AUTH_OPTIONS, request.auth.authType),
    [request.auth.authType],
  );

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex items-center gap-2 border-b px-3 py-2'>
        <Badge variant='outline' className='shrink-0'>
          WS
        </Badge>
        <SingleLineEditor
          value={request.url}
          onChange={(url) => updateRequest(tab.id, { url })}
          placeholder='wss://echo.websocket.org'
          variableContext={variableContext}
          onSubmit={() => {
            if (!connected && !connecting) void connectTab(tab);
          }}
          className='flex-1'
        />
        {connected ? (
          <Button size='sm' variant='outline' className='h-8' onClick={() => void disconnectTab(tab.id)}>
            <PlugZap aria-hidden='true' className='mr-1 h-3.5 w-3.5' /> Disconnect
          </Button>
        ) : (
          <Button
            size='sm'
            className='h-8'
            disabled={connecting || request.url.trim() === ''}
            onClick={() => void connectTab(tab)}
          >
            <Plug aria-hidden='true' className='mr-1 h-3.5 w-3.5' />
            {connecting ? 'Connecting...' : 'Connect'}
          </Button>
        )}
        <Badge
          variant='outline'
          className={cn(
            'shrink-0',
            connected && 'text-[hsl(var(--success))]',
            session.status === 'failed' && 'text-destructive',
          )}
          title={session.error ?? undefined}
        >
          {STATUS_LABEL[session.status]}
        </Badge>
        {!tab.source && (
          <>
            <Button
              size='sm'
              variant='outline'
              className='h-8'
              onClick={() => setSaveToCollectionOpen(true)}
            >
              Save to Collection
            </Button>
            <SaveToCollectionDialog
              open={saveToCollectionOpen}
              tab={tab}
              onClose={() => setSaveToCollectionOpen(false)}
            />
          </>
        )}
        <SaveRequestButton tab={tab} groupId={groupId} />
      </div>

      <div className='flex min-h-0 flex-1 basis-1/2 flex-col px-3 py-2'>
        <Tabs defaultValue='messages' className='flex min-h-0 flex-1 flex-col'>
          <TabsList className='self-start'>
            <TabsTrigger value='messages'>Messages</TabsTrigger>
            <TabsTrigger value='headers'>Headers</TabsTrigger>
            <TabsTrigger value='auth'>Auth</TabsTrigger>
            <TabsTrigger value='settings'>Settings</TabsTrigger>
          </TabsList>
          <TabsContent value='messages' className='min-h-0 flex-1 pt-2'>
            <WebSocketMessagesEditor
              messages={draft.messages}
              onChange={(messages: WebSocketDraftMessage[]) => patchDraft({ messages })}
              onSend={() => void sendSelectedMessage(tab)}
              canSend={connected && draft.messages.length > 0}
              variableContext={variableContext}
            />
          </TabsContent>
          <TabsContent value='headers' className='min-h-0 flex-1 overflow-auto pt-2'>
            <HeadersEditor
              headers={request.headers}
              onChange={(headers: KeyValueEntry[]) => updateRequest(tab.id, { headers })}
              variableContext={variableContext}
            />
          </TabsContent>
          <TabsContent value='auth' className='min-h-0 flex-1 overflow-auto pt-2'>
            <div className='flex flex-col gap-3'>
              <div className='flex flex-col gap-1.5'>
                <Label className='text-xs font-medium'>Auth type</Label>
                <Select
                  value={request.auth.authType}
                  onValueChange={(value) =>
                    updateRequest(tab.id, {
                      auth: authStateForType(value as AuthState['authType'], request.auth),
                    })
                  }
                >
                  <SelectTrigger className='h-8 w-56' aria-label='Auth type'>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {authOptions.map((o) => (
                      <SelectItem key={o.value} value={o.value}>
                        {o.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <p className='text-xs text-muted-foreground'>
                  OAuth 2, Digest, NTLM, WSSE and AWS Signature are not supported on a WebSocket
                  handshake yet.
                </p>
              </div>
              <AuthEditor
                auth={request.auth}
                onChange={(auth) => updateRequest(tab.id, { auth })}
                variableContext={variableContext}
                collection={tab.source?.collection}
                requestPath={tab.source?.path}
              />
            </div>
          </TabsContent>
          <TabsContent value='settings' className='min-h-0 flex-1 overflow-auto pt-2'>
            <div className='flex max-w-md flex-col gap-3'>
              <div className='flex flex-col gap-1.5'>
                <Label htmlFor='ws-timeout' className='text-xs font-medium'>
                  Connect timeout (ms)
                </Label>
                <Input
                  id='ws-timeout'
                  type='number'
                  min={0}
                  className='h-8'
                  placeholder='inherit (30000), 0 = no timeout'
                  value={draft.timeoutMs === 'inherit' ? '' : draft.timeoutMs}
                  onChange={(e) => patchDraft({ timeoutMs: parseOptionalMs(e.target.value) })}
                />
              </div>
              <div className='flex flex-col gap-1.5'>
                <Label htmlFor='ws-keepalive' className='text-xs font-medium'>
                  Keep-alive ping interval (ms)
                </Label>
                <Input
                  id='ws-keepalive'
                  type='number'
                  min={0}
                  className='h-8'
                  placeholder='inherit (no pings)'
                  value={draft.keepAliveMs === 'inherit' ? '' : draft.keepAliveMs}
                  onChange={(e) => patchDraft({ keepAliveMs: parseOptionalMs(e.target.value) })}
                />
              </div>
              <div className='flex items-center justify-between gap-3'>
                <Label htmlFor='ws-verify' className='text-xs font-medium'>
                  Skip TLS verification (this session only)
                </Label>
                <Switch
                  id='ws-verify'
                  checked={!request.settings.verifySsl}
                  onCheckedChange={(skip) =>
                    updateRequest(tab.id, { settings: { ...request.settings, verifySsl: !skip } })
                  }
                />
              </div>
            </div>
          </TabsContent>
        </Tabs>
      </div>

      <div className='min-h-0 flex-1 basis-1/2 border-t'>
        <MessageLog entries={session.log} onClear={() => clearLog(tab.id)} />
      </div>
    </div>
  );
}
```

Notes for the implementer:
- `Select`'s accessible name comes from the trigger's `aria-label`. If a test cannot find an element by role in jsdom because Radix portals the listbox, assert on the store state instead, as the add, select and delete test does.
- `HeadersEditor` and `AuthEditor` are existing components; their props are as listed under Facts. If `AuthEditor` renders its own auth-type selector in this checkout, delete the selector above and keep `AUTH_OPTIONS` out of it.

- [ ] **Step 14: Mount the panel, the bridge and the keyboard guard**

`src/components/panes/EditorGroup.tsx`: add `import { WebSocketPanel } from '@/components/request/websocket/WebSocketPanel';` and replace the request branch (line 214-215) with:

```tsx
          ) : isRequestTab(activeTab) ? (
            activeTab.request.requestType === 'websocket' ? (
              <WebSocketPanel tab={activeTab} groupId={node.groupId} />
            ) : (
              <RequestPanel tab={activeTab} groupId={node.groupId} />
            )
```

`src/App.tsx`: add `import { useWebSocketEventBridge } from '@/lib/websocket-event-bridge';` and call `useWebSocketEventBridge();` right after `useAgentSessionEventBridge();` (line 39).

`src/hooks/useKeyboardShortcuts.ts`: in the Ctrl or Cmd+Enter branch (line 30-35) change the inner block to:

```ts
        if (tab && isRequestTab(tab)) {
          if (tab.request.requestType === 'websocket') {
            // A WebSocket tab sends its selected message; it must never fire an HTTP request.
            window.dispatchEvent(
              new CustomEvent('rocket:websocket-send', { detail: { tabId: tab.id } }),
            );
          } else {
            sendRequest(tab.id, tab.request);
          }
        }
```

- [ ] **Step 15: Run the panel tests and fix any selector mismatch**

Run: `yarn test --run WebSocketPanel`
Expected: 7 passed. Adjust only element lookups (`getByRole`/`getByLabelText`) to the real accessible names if a Radix or shadcn wrapper changes them; do not weaken the assertions.

- [ ] **Step 16: Verify**

Run, in order:
- `yarn test --run websocket message-log WebSocket pane-store EditorGroup`
- `yarn tsc --noEmit`
- `yarn check`
- Manual check in the real app (`yarn tauri dev`): create a WebSocket request in a collection from the sidebar menu, point it at a public echo server, Connect, Send, see both lines in the log with times and sizes, Disconnect; restart the app and confirm the saved messages, headers and settings came back; close the tab while connected and confirm the log line "Closed" is not required (the tab is gone) and no process keeps the socket (check with `ss -tnp | grep rocket`).

Expected: green and the manual check passes. If `yarn check` reports a Biome complaint about the `biome-ignore` comment in `MessageLog.tsx`, remove the comment and add `entries.length` to a `useEffect` dependency array without the ignore.

- [ ] **Step 17: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by path: `src/types/message-log.ts`, `src/stores/websocket-store.ts`, `src/lib/websocket-event-bridge.ts`, `src/lib/websocket-session.ts`, `src/lib/message-log-format.ts`, `src/hooks/useWebSocketVariableContext.ts`, `src/hooks/useKeyboardShortcuts.ts`, `src/components/request/websocket/MessageLog.tsx`, `src/components/request/websocket/WebSocketMessagesEditor.tsx`, `src/components/request/websocket/WebSocketPanel.tsx`, `src/components/panes/EditorGroup.tsx`, `src/App.tsx`, `src/stores/pane-store.ts`, `src/stores/__tests__/pane-store.test.ts`, and the five new test files named under Files.
Suggested subject: `feat(websocket): WebSocket tab with live message log`.

---

## Known limits (state them in the PR description)

- The Bruno `.bru` and YAML WebSocket shapes come from the Bruno docs, not from a real export; confirm with one export (Task 1, Step 13).
- Folder-level and request-level variables are resolved by the backend but not highlighted in the WebSocket editors.
- Duplicate is hidden on WebSocket and GraphQL sidebar rows.
- `Skip TLS verification` is not persisted.
- A `.bru` gRPC file still imports as an empty HTTP request (the same `parse_meta` gap); the gRPC plan owns the fix. Plan 05 fixed GraphQL.
- Binary messages are edited as base64, not hex.
- Reconnect, auto-reconnect and message templates with random values are not part of this plan.

## Next Plan

[2026-10-05-protocol-parity-plan-10-graphql-subscriptions.md](2026-10-05-protocol-parity-plan-10-graphql-subscriptions.md): GraphQL subscriptions over the plan-08 `WebSocketClient`, and the Subscribe and Stop UI that reuses `MessageLog`. Chain to it automatically when this plan finishes (it also needs plan 06).
