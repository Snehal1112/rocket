# Protocol parity, Plan 13: gRPC UI and Bruno import

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** gRPC is usable end to end. Bruno gRPC requests import (`.bru` files, Bruno YAML and OpenCollection YAML, with their `.proto` files). A gRPC request shows in the sidebar with its own badge, opens in its own tab, and can be created from the New Request dialog as a real gRPC item. The tab has a method picker fed by the `.proto` file or by server reflection, a Monaco JSON message editor with saved messages, a metadata table, auth, a unary Send, a streaming Start with Send message, End requests and Cancel, a live message log, and the status, headers and trailers of the call.

**Architecture:** `rocket-import` parses the Bruno `grpc`, `metadata` and `body:grpc` blocks into new `BruDocument` fields and converts them to `GrpcRequest`, and copies the collection's `.proto` files. The frontend follows Plan 05's pattern for GraphQL: `requestType` is the persisted discriminator, a sidebar row is a `summary` with `kind: 'grpc'`, `RequestState` gains an optional `grpc` block next to the shared URL, metadata (as `headers`) and auth fields, and one `saveTabRequest` router writes a gRPC tab through `saveGrpcRequest`. Streaming output arrives as `grpc-session-*` events (Plan 12) into a Zustand store keyed by session id, so an event that beats the command's return value is not lost. The gRPC tab is a separate panel, chosen in `EditorGroup`, because its layout is not the HTTP one.

**Tech Stack:** Rust (`rocket-import`), React + TypeScript, Zustand, Monaco (lazy), Tauri 2 IPC, Vitest. Run Rust tests with `cargo test -j4 -p <crate> <name>`.

**Spec:** `docs/superpowers/specs/opencollection-spec-reference.md` section 2.5 (with Plan 11's correction). Behaviour reference: https://docs.usebruno.com/send-requests/grpc/overview.

**Depends on:** Plan 11 (typed item and its types), Plan 12 (commands and events) and Plan 05 (the `RequestKind`, `RequestSummary.kind`, `createDefaultRequestFor`, `saveTabRequest`, `auto-save` and `RequestNode` changes that Task 2 extends). Plan 09 (WebSocket UI) and Plan 06 (GraphQL editor) edit some of the same files (`CreateRequestDialog.tsx`, `RequestNode.tsx`, `EditorGroup.tsx`, `pane-store.ts`, `tauri-api.ts`, `request-save-mapper.ts`, `pane-utils.ts`, `importer.rs`, `converter/request.rs`, `yml_adapter.rs`). Whichever lands second resolves the textual merge by keeping both sides. Tasks 2 and 3 should land together: after Task 2 alone, a gRPC tab opens in the HTTP panel.

## Facts verified for this plan

- Bruno's `.bru` grammar (bruno-lang `bruToJson.js`): a gRPC file has `meta { name, type: grpc, seq }`, a `grpc { url, method, body: grpc, auth, methodType, protoPath }` block, a `metadata { name: value }` block parsed like headers, and one `body:grpc { name: <title> content: '''<json>''' }` block per message. `method` is written with a leading slash (`/package.Service/Method`).
- The OpenCollection YAML shape is the one in the spec reference (`info`, `grpc` with `protoFilePath`, `metadata`, `message`). The older Bruno YAML shape for gRPC is **not documented**. The adapter reads it leniently (both spellings of the proto path, a message as a string or a list, metadata flagged `disabled` or `enabled`, auth as `type:` or `mode:`) and the tests pin what it accepts. Check one real Bruno 2.10+ YAML export before relying on it (see "Decisions to confirm" at the end).
- A Bruno 3 (OpenCollection) collection is imported by copying its `.yml` files, so a gRPC item arrives as a file and loads as a typed item. Only `.yml` and `.yaml` were copied before, so `.proto` files never came along and every imported `protoFilePath` pointed at nothing. Both import paths copy `.proto` files now.
- The `.bru` parser never records an `unsupported_type` block (only the YAML adapter does), so a `.bru` file of an unsupported type, including gRPC today, was imported as an **empty GET request**. Task 1 fixes that for every non-HTTP type.
- `closeTab` already calls auto-save for a dirty tab, and the pane-store close paths end an agent session through `endSessionIfActive` or `endActiveSessions`. A gRPC tab must go through the same two places: auto-save must write a gRPC file, and closing the tab must cancel a running stream.

## Global Constraints

- Frontend: shadcn/ui primitives and `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>` or `<form>`. Single-line variable-aware fields use `SingleLineEditor` (CodeMirror 6). The multi-line JSON editor is Monaco. Zustand: narrow selectors only, never destructure the whole store at the top of a component.
- No `unwrap()` in production Rust paths. Always pass `-j4` to cargo. Never run `cargo test --workspace`.
- Commit with the `dev-workflow-skills:1-git-commit` skill, conventional subjects, and stage by explicit path only.
- Tauri event payload fields are snake_case (`session_id`, `code_name`, `duration_ms`). Command results are camelCase.
- Verification for frontend work: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`. For Rust: `cargo check -j4` and the targeted test.

## Review Focus

1. A gRPC tab must never be written through `saveRequest`. Auto-save runs on tab close and tab switch, and an HTTP write would replace the gRPC file (Task 2 tests `saves a grpc tab through saveGrpcRequest, never saveRequest` and `routes a grpc tab to saveGrpcRequest and never to saveRequest`).
2. What the editor does not show must round-trip: message titles and the selected flag, `seq`, `description` and `scripts` (Task 2 test `round-trips a saved request, including the fields the editor never shows`).
3. Bruno files: a multi-line `body:grpc` content block, the leading slash on `method`, a `protoPath` relative to the request file, and the `.proto` copy (Task 1 tests `parses_a_multi_line_grpc_message_and_keeps_its_indentation_shape`, `a_path_relative_to_the_request_file_is_rebased_to_the_collection_root` and `bru_grpc_file_imports_as_a_grpc_item_and_copies_its_proto`). A `.bru` file of an unsupported type must be skipped, never become an empty GET (`a_bru_file_of_an_unsupported_type_is_skipped_instead_of_becoming_an_empty_get`).
4. A stream event can arrive before the start command returns, a stream can run for hours, and closing the tab must stop the call (Task 3 tests `shows an event that arrives before the start command returns`, `keeps only the newest entries when a stream runs long` and `cancels its running stream and forgets its results`).
5. The picker must not show a stale list after the proto path or URL changes, and a Send without a method must say so instead of calling the backend (Task 3 tests `drops a loaded list when the source changes` and `asks for a method instead of calling the backend`).

---

## Task 1: Bruno gRPC import

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-import/src/bru/ast.rs`, `bru/parser.rs`, `bru/yml_adapter.rs`
- Create: `crates/rocket-import/src/converter/grpc.rs`
- Modify: `crates/rocket-import/src/converter/mod.rs`, `converter/request.rs`, `importer.rs`
- Modify: `crates/rocket-import/tests/integration_test.rs`
- Modify: `crates/rocket-import/CLAUDE.md`

**Interfaces:**
- Consumes: `rocket_collection::{GrpcRequest, GrpcMetadataEntry, GrpcMessage, GrpcMethodType}`, `CollectionRepository::save_grpc_request` (Plan 11), Plan 05's `Converted` and `convert_item`.
- Produces:
  - `BruDocument.grpc: Option<BruGrpc>`, `BruDocument.grpc_metadata: Vec<BruKeyValue>`, `BruDocument.grpc_messages: Vec<BruGrpcMessage>`.
  - `converter::grpc::{is_grpc, convert}`; `convert(&BruDocument) -> (Option<GrpcRequest>, Vec<SkipReason>)`.
  - `converter::request::Converted::Grpc(GrpcRequest)`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** Section 2.5 and `GrpcMetadata`.

- [ ] **Step 2: Write the failing tests**

Parser tests, appended to the `tests` module of `crates/rocket-import/src/bru/parser.rs`:

```diff
@@ -340,5 +439,70 @@
         assert_eq!(doc.secret_vars.len(), 2);
         assert!(doc.secret_vars.contains(&"DB_PASSWORD".to_string()));
     }
+
+    const GRPC_BRU: &str = "meta {\n  name: Say Hello\n  type: grpc\n  seq: 2\n}\n\ngrpc {\n  url: localhost:50051\n  method: /demo.greeter.v1.Greeter/SayHello\n  body: grpc\n  auth: none\n  methodType: unary\n  protoPath: protos/greeter.proto\n}\n\nmetadata {\n  x-trace: abc\n  ~x-off: 1\n}\n\nbody:grpc {\n  name: message 1\n  content: '''\n    {\n      \"name\": \"ada\"\n    }\n  '''\n}\n";
+
+    #[test]
+    fn parses_a_grpc_request() {
+        let doc = parse(GRPC_BRU);
+        let meta = doc.meta.expect("meta");
+        assert_eq!(
+            (meta.name.as_str(), meta.request_type.as_str()),
+            ("Say Hello", "grpc")
+        );
+        assert_eq!(meta.seq, Some(2));
+        let grpc = doc.grpc.expect("grpc block");
+        assert_eq!(grpc.url.as_deref(), Some("localhost:50051"));
+        assert_eq!(
+            grpc.method.as_deref(),
+            Some("/demo.greeter.v1.Greeter/SayHello")
+        );
+        assert_eq!(grpc.method_type.as_deref(), Some("unary"));
+        assert_eq!(grpc.proto_path.as_deref(), Some("protos/greeter.proto"));
+        assert_eq!(grpc.auth_mode.as_deref(), Some("none"));
+        assert!(
+            doc.unknown_blocks.is_empty(),
+            "a gRPC file has no unknown blocks: {:?}",
+            doc.unknown_blocks
+        );
+        assert!(doc.body.is_none(), "a gRPC message is not an HTTP body");
+    }
+
+    #[test]
+    fn parses_grpc_metadata_with_disabled_entries() {
+        let doc = parse(GRPC_BRU);
+        assert_eq!(doc.grpc_metadata.len(), 2);
+        assert_eq!(doc.grpc_metadata[0].key, "x-trace");
+        assert!(!doc.grpc_metadata[0].disabled);
+        assert_eq!(doc.grpc_metadata[1].key, "x-off");
+        assert!(doc.grpc_metadata[1].disabled);
+    }
+
+    #[test]
+    fn parses_a_multi_line_grpc_message_and_keeps_its_indentation_shape() {
+        let doc = parse(GRPC_BRU);
+        assert_eq!(doc.grpc_messages.len(), 1);
+        assert_eq!(doc.grpc_messages[0].title, "message 1");
+        assert_eq!(doc.grpc_messages[0].content, "{\n  \"name\": \"ada\"\n}");
+    }
+
+    #[test]
+    fn parses_several_and_inline_grpc_messages_in_file_order() {
+        let doc = parse(
+            "grpc {\n  url: h:1\n}\n\nbody:grpc {\n  name: first\n  content: '''{\"a\": 1}'''\n}\n\nbody:grpc {\n  name: second\n  content: '''\n    {\"b\": 2}\n  '''\n}\n",
+        );
+        let titles: Vec<&str> = doc.grpc_messages.iter().map(|m| m.title.as_str()).collect();
+        assert_eq!(titles, vec!["first", "second"]);
+        assert_eq!(doc.grpc_messages[0].content, "{\"a\": 1}");
+        assert_eq!(doc.grpc_messages[1].content, "{\"b\": 2}");
+    }
+
+    #[test]
+    fn grpc_auth_blocks_still_reach_the_auth_parser() {
+        let doc =
+            parse("grpc {\n  url: h:1\n  auth: bearer\n}\n\nauth:bearer {\n  token: {{tok}}\n}\n");
+        assert!(matches!(doc.auth, Some(BruAuth::Bearer { ref token }) if token == "{{tok}}"));
+        assert_eq!(doc.grpc.expect("grpc").auth_mode.as_deref(), Some("bearer"));
+    }
 }
 
```

Adapter tests, appended to the `tests` module of `crates/rocket-import/src/bru/yml_adapter.rs`:

```diff
@@ -445,5 +560,102 @@
         assert_eq!(doc.unknown_blocks.len(), 1);
         assert_eq!(doc.unknown_blocks[0].name, "unsupported_type");
     }
+
+    #[test]
+    fn opencollection_grpc_request_is_adapted() {
+        let yml = r#"
+info:
+  name: Say Hello
+  type: grpc
+  seq: 3
+grpc:
+  url: grpcs://api.example.com:443
+  method: demo.greeter.v1.Greeter/SayHello
+  methodType: server-streaming
+  protoFilePath: protos/greeter.proto
+  metadata:
+    - name: x-trace
+      value: abc
+    - name: x-off
+      value: "1"
+      disabled: true
+  message: '{"name": "ada"}'
+  auth:
+    type: bearer
+    token: "{{tok}}"
+"#;
+        let doc = bru_document_from_yml_str(yml).unwrap();
+        assert!(doc.unknown_blocks.is_empty(), "{:?}", doc.unknown_blocks);
+        let meta = doc.meta.as_ref().expect("meta");
+        assert_eq!(
+            (meta.name.as_str(), meta.request_type.as_str(), meta.seq),
+            ("Say Hello", "grpc", Some(3))
+        );
+        let grpc = doc.grpc.as_ref().expect("grpc");
+        assert_eq!(grpc.url.as_deref(), Some("grpcs://api.example.com:443"));
+        assert_eq!(grpc.method_type.as_deref(), Some("server-streaming"));
+        assert_eq!(grpc.proto_path.as_deref(), Some("protos/greeter.proto"));
+        assert_eq!(doc.grpc_metadata.len(), 2);
+        assert!(doc.grpc_metadata[1].disabled);
+        assert_eq!(doc.grpc_messages.len(), 1);
+        assert_eq!(doc.grpc_messages[0].content, "{\"name\": \"ada\"}");
+        assert!(matches!(doc.auth, Some(BruAuth::Bearer { ref token }) if token == "{{tok}}"));
+    }
+
+    #[test]
+    fn bruno_spellings_of_the_grpc_block_are_accepted() {
+        let yml = r#"
+meta:
+  name: Old Style
+  type: grpc
+grpc:
+  url: localhost:50051
+  method: /demo.Svc/Do
+  protoPath: ../protos/svc.proto
+  metadata:
+    - name: k
+      value: v
+      enabled: false
+  message:
+    - title: one
+      content: '{"a": 1}'
+    - title: two
+      message: '{"a": 2}'
+  auth:
+    mode: bearer
+    bearer:
+      token: t
+"#;
+        let doc = bru_document_from_yml_str(yml).unwrap();
+        assert_eq!(
+            doc.grpc.as_ref().and_then(|g| g.proto_path.as_deref()),
+            Some("../protos/svc.proto")
+        );
+        assert!(
+            doc.grpc_metadata[0].disabled,
+            "enabled: false disables the entry"
+        );
+        let titles: Vec<&str> = doc.grpc_messages.iter().map(|m| m.title.as_str()).collect();
+        assert_eq!(titles, vec!["one", "two"]);
+        assert_eq!(doc.grpc_messages[1].content, "{\"a\": 2}");
+        assert!(matches!(doc.auth, Some(BruAuth::Bearer { ref token }) if token == "t"));
+    }
+
+    #[test]
+    fn grpc_auth_that_cannot_be_converted_is_reported_not_dropped() {
+        let yml = "info:\n  name: A\n  type: grpc\ngrpc:\n  url: h:1\n  auth:\n    type: oauth2\n";
+        let doc = bru_document_from_yml_str(yml).unwrap();
+        assert_eq!(doc.unknown_blocks.len(), 1);
+        assert_eq!(doc.unknown_blocks[0].name, "auth");
+        assert_eq!(doc.unknown_blocks[0].subtype.as_deref(), Some("oauth2"));
+    }
+
+    #[test]
+    fn a_grpc_type_without_a_grpc_block_is_still_unsupported() {
+        let yml = "meta:\n  name: G\n  type: grpc\nhttp:\n  method: POST\n  url: grpc://x\n";
+        let doc = bru_document_from_yml_str(yml).unwrap();
+        assert_eq!(doc.unknown_blocks.len(), 1);
+        assert_eq!(doc.unknown_blocks[0].name, "unsupported_type");
+    }
 }
 
```

Converter tests. Create `crates/rocket-import/src/converter/grpc.rs` with only this test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn grpc_doc() -> BruDocument {
        BruDocument {
            meta: Some(BruMeta {
                name: "Say Hello".into(),
                request_type: "grpc".into(),
                seq: Some(2),
            }),
            grpc: Some(BruGrpc {
                url: Some("localhost:50051".into()),
                method: Some("/demo.greeter.v1.Greeter/SayHello".into()),
                method_type: Some("server-streaming".into()),
                proto_path: Some("protos/greeter.proto".into()),
                auth_mode: Some("none".into()),
            }),
            grpc_metadata: vec![
                BruKeyValue {
                    key: "x-trace".into(),
                    value: "abc".into(),
                    disabled: false,
                },
                BruKeyValue {
                    key: "x-off".into(),
                    value: "1".into(),
                    disabled: true,
                },
            ],
            grpc_messages: vec![
                BruGrpcMessage {
                    title: "first".into(),
                    content: "{\"name\": \"a\"}".into(),
                },
                BruGrpcMessage {
                    title: "second".into(),
                    content: "{\"name\": \"b\"}".into(),
                },
            ],
            ..BruDocument::default()
        }
    }

    #[test]
    fn converts_the_call_description() {
        let (g, skipped) = convert(&grpc_doc());
        assert!(skipped.is_empty());
        let g = g.expect("grpc request");
        assert_eq!(g.name, "Say Hello");
        assert_eq!(g.seq, Some(2));
        assert_eq!(g.url, "localhost:50051");
        assert_eq!(
            g.method.as_deref(),
            Some("demo.greeter.v1.Greeter/SayHello"),
            "the leading slash is dropped"
        );
        assert_eq!(g.method_type, GrpcMethodType::ServerStreaming);
        assert_eq!(g.proto_file_path.as_deref(), Some("protos/greeter.proto"));
        assert_eq!(g.auth, Auth::None);
    }

    #[test]
    fn keeps_disabled_metadata_and_marks_the_first_message_selected() {
        let (g, _) = convert(&grpc_doc());
        let g = g.expect("grpc request");
        assert_eq!(g.metadata.len(), 2);
        assert!(g.metadata[0].enabled);
        assert!(!g.metadata[1].enabled);
        assert_eq!(g.messages.len(), 2);
        assert!(g.messages[0].selected);
        assert!(!g.messages[1].selected);
        assert_eq!(g.messages[1].title, "second");
    }

    #[test]
    fn metadata_written_as_headers_is_kept() {
        let mut doc = grpc_doc();
        doc.grpc_metadata.clear();
        doc.headers = vec![BruKeyValue {
            key: "x-h".into(),
            value: "v".into(),
            disabled: false,
        }];
        let g = convert(&doc).0.expect("grpc request");
        assert_eq!(g.metadata.len(), 1);
        assert_eq!(g.metadata[0].key, "x-h");
    }

    #[test]
    fn an_unknown_method_type_falls_back_to_unary() {
        let mut doc = grpc_doc();
        doc.grpc.as_mut().expect("grpc").method_type = Some("duplex".into());
        assert_eq!(
            convert(&doc).0.expect("grpc request").method_type,
            GrpcMethodType::Unary
        );
    }

    #[test]
    fn inherit_and_bearer_auth_are_converted() {
        let mut doc = grpc_doc();
        doc.grpc.as_mut().expect("grpc").auth_mode = Some("inherit".into());
        assert_eq!(convert(&doc).0.expect("grpc request").auth, Auth::Inherit);

        let mut doc = grpc_doc();
        doc.auth = Some(BruAuth::Bearer {
            token: "{{tok}}".into(),
        });
        assert_eq!(
            convert(&doc).0.expect("grpc request").auth,
            Auth::Bearer {
                token: "{{tok}}".into()
            }
        );
    }

    #[test]
    fn unsupported_auth_is_reported_and_the_request_still_imports_without_auth() {
        let mut doc = grpc_doc();
        doc.auth = Some(BruAuth::Bearer { token: "t".into() });
        doc.unknown_blocks.push(BruRawBlock {
            name: "auth".into(),
            subtype: Some("oauth2".into()),
            content: String::new(),
        });
        let (g, skipped) = convert(&doc);
        assert!(
            matches!(skipped.as_slice(), [SkipReason::UnsupportedAuthType(t)] if t == "oauth2")
        );
        assert_eq!(g.expect("still imported").auth, Auth::None);
    }

    #[test]
    fn a_meta_type_of_grpc_is_enough_to_be_a_grpc_document() {
        let doc = BruDocument {
            meta: Some(BruMeta {
                name: "G".into(),
                request_type: "grpc".into(),
                seq: None,
            }),
            ..BruDocument::default()
        };
        assert!(is_grpc(&doc));
        assert!(!is_grpc(&BruDocument::default()));
    }
}
```

and append to the `tests` module of `crates/rocket-import/src/converter/request.rs`:

```rust
    #[test]
    fn a_bru_file_of_an_unsupported_type_is_skipped_instead_of_becoming_an_empty_get() {
        // The `.bru` parser never records an `unsupported_type` block, only `meta`.
        let doc = BruDocument {
            meta: Some(BruMeta {
                name: "Chat".into(),
                request_type: "websocket".into(),
                seq: None,
            }),
            ..BruDocument::default()
        };
        let (req, skipped) = convert(&doc);
        assert!(req.is_none());
        assert!(matches!(
            skipped.as_slice(),
            [SkipReason::UnsupportedRequestType(t)] if t == "websocket"
        ));
    }

    #[test]
    fn convert_item_routes_grpc_documents_to_the_grpc_converter() {
        let doc = BruDocument {
            meta: Some(BruMeta {
                name: "G".into(),
                request_type: "grpc".into(),
                seq: None,
            }),
            grpc: Some(BruGrpc::default()),
            ..BruDocument::default()
        };
        let (item, skipped) = convert_item(&doc);
        assert!(skipped.is_empty());
        assert!(matches!(item, Some(Converted::Grpc(_))));

        let http = doc_with_method(BruMethod::Get, "https://example.com");
        assert!(matches!(convert_item(&http).0, Some(Converted::Http(_))));
    }
}
```

Importer helper tests. Append to `crates/rocket-import/src/importer.rs`:

```rust
#[cfg(test)]
mod proto_path_tests {
    use super::*;
    use tempfile::TempDir;

    fn layout() -> TempDir {
        let dir = TempDir::new().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("protos")).expect("mkdir");
        std::fs::create_dir_all(dir.path().join("calls")).expect("mkdir");
        std::fs::write(
            dir.path().join("protos/greeter.proto"),
            "syntax = \"proto3\";\n",
        )
        .expect("write");
        dir
    }

    #[test]
    fn a_path_relative_to_the_request_file_is_rebased_to_the_collection_root() {
        let dir = layout();
        let rebased = rebase_proto_path(
            dir.path(),
            &dir.path().join("calls"),
            "../protos/greeter.proto",
        );
        assert_eq!(rebased.as_deref(), Some("protos/greeter.proto"));
    }

    #[test]
    fn a_path_relative_to_the_collection_root_is_found_too() {
        let dir = layout();
        let rebased = rebase_proto_path(
            dir.path(),
            &dir.path().join("calls"),
            "protos/greeter.proto",
        );
        assert_eq!(rebased.as_deref(), Some("protos/greeter.proto"));
    }

    #[test]
    fn missing_absolute_and_outside_paths_stay_as_written() {
        let dir = layout();
        let outside = TempDir::new().expect("tempdir");
        std::fs::write(outside.path().join("other.proto"), "syntax = \"proto3\";\n")
            .expect("write");
        let calls = dir.path().join("calls");
        assert_eq!(
            rebase_proto_path(dir.path(), &calls, "protos/missing.proto"),
            None
        );
        assert_eq!(
            rebase_proto_path(
                dir.path(),
                &calls,
                &outside.path().join("other.proto").to_string_lossy()
            ),
            None
        );
        let escaping = format!(
            "../../{}/other.proto",
            outside.path().file_name().expect("name").to_string_lossy()
        );
        assert_eq!(rebase_proto_path(dir.path(), &calls, &escaping), None);
    }
}
```

Integration tests. Append to `crates/rocket-import/tests/integration_test.rs` (it already defines `make_service`):

```rust
#[test]
fn bru_grpc_file_imports_as_a_grpc_item_and_copies_its_proto() {
    use rocket_collection::{CollectionRepository, GrpcMethodType};

    let src = TempDir::new().unwrap();
    let root = src.path().join("grpc-api");
    std::fs::create_dir_all(root.join("protos")).unwrap();
    std::fs::create_dir_all(root.join("calls")).unwrap();
    std::fs::write(
        root.join("bruno.json"),
        r#"{ "name": "grpc-api", "version": "1", "type": "collection" }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("protos/greeter.proto"),
        "syntax = \"proto3\";\npackage demo.v1;\nservice Greeter { rpc SayHello (Req) returns (Rep); }\nmessage Req { string name = 1; }\nmessage Rep { string message = 1; }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("calls/say-hello.bru"),
        "meta {\n  name: Say Hello\n  type: grpc\n  seq: 1\n}\n\ngrpc {\n  url: localhost:50051\n  method: /demo.v1.Greeter/SayHello\n  body: grpc\n  auth: none\n  methodType: unary\n  protoPath: ../protos/greeter.proto\n}\n\nmetadata {\n  x-trace: abc\n  ~x-off: 1\n}\n\nbody:grpc {\n  name: message 1\n  content: '''\n    {\n      \"name\": \"ada\"\n    }\n  '''\n}\n",
    )
    .unwrap();

    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());
    let report = service.import_collection(&root, "default").unwrap();

    assert_eq!(report.imported, 1, "skipped: {:?}", report.skipped);
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);

    let name = &report.created_collections[0];
    let repo = FsCollectionRepo::new_standalone(workspace_dir.path().join("collections"));
    let g = repo.get_grpc_request(name, "calls/say-hello.yml").unwrap();
    assert_eq!(g.url, "localhost:50051");
    assert_eq!(g.method.as_deref(), Some("demo.v1.Greeter/SayHello"));
    assert_eq!(g.method_type, GrpcMethodType::Unary);
    assert_eq!(g.proto_file_path.as_deref(), Some("protos/greeter.proto"));
    assert_eq!(g.metadata.len(), 2);
    assert!(!g.metadata[1].enabled);
    assert!(g.messages[0].content.contains("\"name\": \"ada\""));
    assert!(
        workspace_dir
            .path()
            .join("collections")
            .join(name)
            .join("protos/greeter.proto")
            .exists(),
        "the proto file travels with the requests"
    );
}

#[test]
fn opencollection_grpc_collection_imports_with_its_proto() {
    use rocket_collection::CollectionRepository;

    let src = TempDir::new().unwrap();
    let root = src.path().join("oc-grpc");
    std::fs::create_dir_all(root.join("protos")).unwrap();
    std::fs::write(root.join("opencollection.yml"), "opencollection: 1.0.0\ninfo:\n  name: oc-grpc\n").unwrap();
    std::fs::write(root.join("protos/greeter.proto"), "syntax = \"proto3\";\n").unwrap();
    std::fs::write(
        root.join("say-hello.yml"),
        "info:\n  name: Say Hello\n  type: grpc\ngrpc:\n  url: localhost:50051\n  method: demo.v1.Greeter/SayHello\n  methodType: unary\n  protoFilePath: protos/greeter.proto\n  message: '{}'\n",
    )
    .unwrap();

    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());
    let report = service.import_collection(&root, "default").unwrap();
    assert_eq!(report.imported, 1, "the proto file is not counted as a request");

    let name = &report.created_collections[0];
    let repo = FsCollectionRepo::new_standalone(workspace_dir.path().join("collections"));
    let g = repo.get_grpc_request(name, "say-hello.yml").unwrap();
    assert_eq!(g.proto_file_path.as_deref(), Some("protos/greeter.proto"));
    assert!(workspace_dir
        .path()
        .join("collections")
        .join(name)
        .join("protos/greeter.proto")
        .exists());
}

#[test]
fn bru_file_of_an_unsupported_type_is_skipped_not_imported_as_an_empty_get() {
    use rocket_import::SkipReason;

    let src = TempDir::new().unwrap();
    let root = src.path().join("ws-api");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("bruno.json"),
        r#"{ "name": "ws-api", "version": "1", "type": "collection" }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("chat.bru"),
        "meta {\n  name: Chat\n  type: websocket\n}\n\nws {\n  url: wss://example.com/ws\n}\n",
    )
    .unwrap();

    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());
    let report = service.import_collection(&root, "default").unwrap();

    assert_eq!(report.imported, 0);
    assert!(matches!(
        report.skipped.as_slice(),
        [item] if matches!(&item.reason, SkipReason::UnsupportedRequestType(t) if t == "websocket")
    ), "{:?}", report.skipped);
    assert!(!workspace_dir
        .path()
        .join("collections")
        .join(&report.created_collections[0])
        .join("chat.yml")
        .exists());
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-import grpc`
Expected: FAIL to compile (`BruGrpc`, `BruGrpcMessage`, `doc.grpc` and `converter::grpc` do not exist).

- [ ] **Step 4: Implement the AST, parser and adapter changes**

`crates/rocket-import/src/bru/ast.rs`:

```diff
@@ -14,8 +14,35 @@
     pub secret_vars: Vec<String>,
     pub pre_request_script: Option<String>,
     pub post_response_script: Option<String>,
+    /// The `grpc {}` block of a gRPC request.
+    pub grpc: Option<BruGrpc>,
+    /// Entries of the `metadata {}` block of a gRPC request.
+    pub grpc_metadata: Vec<BruKeyValue>,
+    /// One entry per `body:grpc {}` block, in file order.
+    pub grpc_messages: Vec<BruGrpcMessage>,
     /// Unrecognised or unsupported blocks — fed into ImportReport.
     pub unknown_blocks: Vec<BruRawBlock>,
+}
+
+/// The `grpc {}` block: where to call and which method.
+#[derive(Debug, Clone, PartialEq, Default)]
+pub struct BruGrpc {
+    pub url: Option<String>,
+    /// `/package.Service/Method` in Bruno files.
+    pub method: Option<String>,
+    /// `unary`, `client-streaming`, `server-streaming` or `bidi-streaming`.
+    pub method_type: Option<String>,
+    /// `protoPath` in `.bru` files, `protoFilePath` in OpenCollection YAML.
+    pub proto_path: Option<String>,
+    /// The `auth:` mode named in the block, such as `none` or `inherit`.
+    pub auth_mode: Option<String>,
+}
+
+/// One saved gRPC message (`body:grpc {}` block).
+#[derive(Debug, Clone, PartialEq, Default)]
+pub struct BruGrpcMessage {
+    pub title: String,
+    pub content: String,
 }
 
 #[derive(Debug, Clone, PartialEq)]
```

`crates/rocket-import/src/bru/parser.rs` (the dispatch arms go before the generic `("body", Some(st))` arm, because that arm would swallow `body:grpc` as an unknown body):

```diff
@@ -45,6 +45,9 @@
         ("vars", None) => parse_vars(doc, tokens),
         ("vars", Some("secret")) => parse_secret_vars(doc, tokens),
         ("auth", Some(st)) => parse_auth(doc, st, tokens),
+        ("grpc", None) => parse_grpc(doc, tokens),
+        ("metadata", None) => parse_grpc_metadata(doc, tokens),
+        ("body", Some("grpc")) => parse_grpc_message(doc, tokens),
         ("body", Some(st)) => parse_body(doc, st, tokens),
         ("script", Some("pre-request")) => {
             doc.pre_request_script = extract_raw_text(tokens);
```

```diff
@@ -107,6 +110,102 @@
     if let Some((_, url)) = map.iter().find(|(k, _)| k == "url") {
         doc.url = Some(url.clone());
     }
+}
+
+/// The `grpc {}` block, for example `url`, `method`, `methodType` and `protoPath`.
+fn parse_grpc(doc: &mut BruDocument, tokens: &[Token]) {
+    let map = kv_map(tokens);
+    let get = |keys: &[&str]| {
+        keys.iter()
+            .find_map(|k| map.iter().find(|(key, _)| key == k))
+            .map(|(_, v)| v.clone())
+            .filter(|v| !v.is_empty())
+    };
+    doc.grpc = Some(BruGrpc {
+        url: get(&["url"]),
+        method: get(&["method"]),
+        method_type: get(&["methodType"]),
+        proto_path: get(&["protoPath", "protoFilePath"]),
+        auth_mode: get(&["auth"]),
+    });
+}
+
+/// The `metadata {}` block: one `name: value` per line, `~` marks a disabled entry.
+fn parse_grpc_metadata(doc: &mut BruDocument, tokens: &[Token]) {
+    for (key, value) in kv_map(tokens) {
+        let disabled = key.starts_with('~');
+        doc.grpc_metadata.push(BruKeyValue {
+            key: key.trim_start_matches('~').to_string(),
+            value,
+            disabled,
+        });
+    }
+}
+
+/// A `body:grpc {}` block holds `name: <title>` and `content: '''<json>'''`.
+fn parse_grpc_message(doc: &mut BruDocument, tokens: &[Token]) {
+    let raw = extract_raw_text(tokens).unwrap_or_default();
+    doc.grpc_messages.push(grpc_message_from(&raw));
+}
+
+fn grpc_message_from(raw: &str) -> BruGrpcMessage {
+    const QUOTES: &str = "'''";
+    let mut message = BruGrpcMessage::default();
+    let lines: Vec<&str> = raw.lines().collect();
+    let mut i = 0;
+    while i < lines.len() {
+        let line = lines[i].trim();
+        if let Some(title) = line.strip_prefix("name:") {
+            message.title = title.trim().to_string();
+        } else if let Some(rest) = line.strip_prefix("content:") {
+            let rest = rest.trim();
+            match rest.strip_prefix(QUOTES) {
+                // The whole value is on this line: content: '''{"a":1}'''
+                Some(inline) if inline.ends_with(QUOTES) && inline.len() >= QUOTES.len() => {
+                    message.content = inline[..inline.len() - QUOTES.len()].trim().to_string();
+                }
+                // A block that runs to the closing quotes.
+                Some(first) => {
+                    let mut body: Vec<&str> = Vec::new();
+                    if !first.trim().is_empty() {
+                        body.push(first);
+                    }
+                    i += 1;
+                    while i < lines.len() && lines[i].trim() != QUOTES {
+                        body.push(lines[i]);
+                        i += 1;
+                    }
+                    message.content = dedent(&body);
+                }
+                None => message.content = rest.to_string(),
+            }
+        }
+        i += 1;
+    }
+    message
+}
+
+/// Removes the common leading whitespace of the non-empty lines and trims the ends.
+fn dedent(lines: &[&str]) -> String {
+    let indent = lines
+        .iter()
+        .filter(|l| !l.trim().is_empty())
+        .map(|l| l.len() - l.trim_start().len())
+        .min()
+        .unwrap_or(0);
+    lines
+        .iter()
+        .map(|l| {
+            if l.len() >= indent {
+                &l[indent..]
+            } else {
+                l.trim_start()
+            }
+        })
+        .collect::<Vec<_>>()
+        .join("\n")
+        .trim()
+        .to_string()
 }
 
 fn parse_headers(doc: &mut BruDocument, tokens: &[Token]) {
```

`crates/rocket-import/src/bru/yml_adapter.rs`. `BruYmlRequest` gains `grpc` (and `info`, which Plan 05 already added for GraphQL; add it only if Plan 05 has not landed). In `adapt_request`, put the gRPC branch next to Plan 05's GraphQL early return. Both consume `info.or(meta)` but each returns, so there is no double move:

```diff
@@ -8,6 +8,10 @@
 pub struct BruYmlRequest {
     pub meta: Option<BruYmlMeta>,
     pub http: Option<BruYmlHttp>,
+    /// OpenCollection-shaped `info:` block, used by gRPC (and GraphQL) files.
+    pub info: Option<BruYmlMeta>,
+    /// The `grpc:` block of a gRPC request, read leniently as raw YAML.
+    pub grpc: Option<serde_yaml::Value>,
 }
 
 #[derive(Debug, Deserialize)]
```

```diff
@@ -148,6 +152,9 @@
 }
 
 fn adapt_request(yml: BruYmlRequest) -> BruDocument {
+    if let Some(grpc) = &yml.grpc {
+        return adapt_grpc(yml.info.or(yml.meta), grpc);
+    }
     let mut doc = BruDocument::default();
 
     // Meta
```

```diff
@@ -208,6 +215,114 @@
         }
     }
 
+    doc
+}
+
+/// Reads a gRPC request. The block is read as loose YAML because the same keys
+/// appear in two spellings: `protoFilePath` (OpenCollection) and `protoPath` (Bruno),
+/// a message that is one string or a list, and metadata flagged `disabled` or `enabled`.
+fn adapt_grpc(info: Option<BruYmlMeta>, grpc: &serde_yaml::Value) -> BruDocument {
+    use serde_yaml::Value;
+
+    let text = |v: &Value, keys: &[&str]| -> Option<String> {
+        keys.iter()
+            .find_map(|k| v.get(*k).and_then(Value::as_str))
+            .map(str::to_string)
+            .filter(|s| !s.is_empty())
+    };
+
+    let mut doc = BruDocument::default();
+    if let Some(m) = info {
+        doc.meta = Some(BruMeta {
+            name: m.name.unwrap_or_default(),
+            request_type: "grpc".into(),
+            seq: m.seq,
+        });
+    }
+
+    let mut section = BruGrpc {
+        url: text(grpc, &["url"]),
+        method: text(grpc, &["method"]),
+        method_type: text(grpc, &["methodType", "method_type"]),
+        proto_path: text(grpc, &["protoFilePath", "protoPath"]),
+        auth_mode: None,
+    };
+
+    for entry in grpc
+        .get("metadata")
+        .and_then(Value::as_sequence)
+        .into_iter()
+        .flatten()
+    {
+        let (Some(key), Some(value)) = (
+            text(entry, &["name"]),
+            entry.get("value").and_then(Value::as_str),
+        ) else {
+            continue;
+        };
+        let disabled = entry
+            .get("disabled")
+            .and_then(Value::as_bool)
+            .unwrap_or(false)
+            || entry.get("enabled").and_then(Value::as_bool) == Some(false);
+        doc.grpc_metadata.push(BruKeyValue {
+            key,
+            value: value.to_string(),
+            disabled,
+        });
+    }
+
+    match grpc.get("message") {
+        Some(Value::String(content)) => doc.grpc_messages.push(BruGrpcMessage {
+            title: String::new(),
+            content: content.clone(),
+        }),
+        Some(Value::Sequence(items)) => {
+            for item in items {
+                let content = match item {
+                    Value::String(s) => Some(s.clone()),
+                    _ => text(item, &["message", "content"]),
+                };
+                if let Some(content) = content {
+                    doc.grpc_messages.push(BruGrpcMessage {
+                        title: text(item, &["title", "name"]).unwrap_or_default(),
+                        content,
+                    });
+                }
+            }
+        }
+        _ => {}
+    }
+
+    if let Some(auth) = grpc.get("auth") {
+        match auth.get("type").and_then(Value::as_str).or(auth.as_str()) {
+            Some("none") => section.auth_mode = Some("none".into()),
+            Some("inherit") => section.auth_mode = Some("inherit".into()),
+            Some("bearer") => {
+                doc.auth = Some(BruAuth::Bearer {
+                    token: text(auth, &["token"]).unwrap_or_default(),
+                });
+            }
+            Some("basic") => {
+                doc.auth = Some(BruAuth::Basic {
+                    username: text(auth, &["username"]).unwrap_or_default(),
+                    password: text(auth, &["password"]).unwrap_or_default(),
+                });
+            }
+            Some(other) => doc.unknown_blocks.push(BruRawBlock {
+                name: "auth".into(),
+                subtype: Some(other.to_string()),
+                content: String::new(),
+            }),
+            None => {
+                // The `mode:` spelling that the HTTP block uses.
+                if let Ok(parsed) = serde_yaml::from_value::<BruYmlAuth>(auth.clone()) {
+                    doc.auth = adapt_auth(parsed, &mut doc.unknown_blocks);
+                }
+            }
+        }
+    }
+    doc.grpc = Some(section);
     doc
 }
 
```

- [ ] **Step 5: Implement the converter**

Put this above the test module in `crates/rocket-import/src/converter/grpc.rs`:

```rust
use rocket_collection::{GrpcMessage, GrpcMetadataEntry, GrpcMethodType, GrpcRequest};
use rocket_shared::types::Auth;

use crate::bru::ast::*;
use crate::report::SkipReason;

use super::request::bru_auth_to_domain;

/// True when the document is a gRPC request: it has a `grpc` block or its `meta` says so.
pub fn is_grpc(doc: &BruDocument) -> bool {
    doc.grpc.is_some() || doc.meta.as_ref().is_some_and(|m| m.request_type == "grpc")
}

/// Converts a gRPC Bruno document to a domain `GrpcRequest`.
///
/// The proto path is copied as written. The importer rewrites it when it can find
/// the file. Unsupported auth is reported and the request still imports with `auth: None`.
pub fn convert(doc: &BruDocument) -> (Option<GrpcRequest>, Vec<SkipReason>) {
    let skipped: Vec<SkipReason> = doc
        .unknown_blocks
        .iter()
        .filter(|b| b.name == "auth")
        .map(|b| SkipReason::UnsupportedAuthType(b.subtype.clone().unwrap_or_default()))
        .collect();

    let section = doc.grpc.clone().unwrap_or_default();
    let name = doc
        .meta
        .as_ref()
        .map(|m| m.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Untitled".into());
    let mut g = GrpcRequest::new(name, section.url.clone().unwrap_or_default());
    g.seq = doc.meta.as_ref().and_then(|m| m.seq);
    // Bruno writes `/package.Service/Method`. Rocket stores it without the slash.
    g.method = section
        .method
        .as_deref()
        .map(|m| m.trim_start_matches('/').to_string())
        .filter(|m| !m.is_empty());
    g.method_type = section
        .method_type
        .as_deref()
        .and_then(GrpcMethodType::parse)
        .unwrap_or_default();
    g.proto_file_path = section.proto_path.clone();

    // Metadata may sit in `metadata {}` or, in some files, in `headers {}`.
    for kv in doc.grpc_metadata.iter().chain(doc.headers.iter()) {
        let mut entry = GrpcMetadataEntry::new(kv.key.clone(), kv.value.clone());
        entry.enabled = !kv.disabled;
        g.metadata.push(entry);
    }

    g.messages = doc
        .grpc_messages
        .iter()
        .enumerate()
        .map(|(i, m)| GrpcMessage {
            title: m.title.clone(),
            selected: i == 0,
            content: m.content.clone(),
        })
        .collect();

    if section.auth_mode.as_deref() == Some("inherit") {
        g.auth = Auth::Inherit;
    } else if skipped.is_empty() {
        if let Some(auth) = &doc.auth {
            g.auth = bru_auth_to_domain(auth);
        }
    }
    (Some(g), skipped)
}
```

In `crates/rocket-import/src/converter/mod.rs` add `pub(crate) mod grpc;` after `pub(crate) mod environment;`. In `converter/request.rs`, make `bru_auth_to_domain` `pub(crate)` and add `GrpcRequest` to the `rocket_collection` import. Plan 05 already created `Converted` and `convert_item`. Add the `Grpc` variant and the first branch to them, so they read:

```rust
/// What a Bruno file turns into.
// A short-lived value made once per imported file, so the size gap is harmless.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Converted {
    Http(Request),
    Grpc(GrpcRequest),
}

/// Converts a Bruno document to whichever domain item it describes.
/// Unsupported request types (WebSocket) still produce `(None, [skip])`.
pub fn convert_item(doc: &BruDocument) -> (Option<Converted>, Vec<SkipReason>) {
    if super::grpc::is_grpc(doc) {
        let (grpc, skipped) = super::grpc::convert(doc);
        return (grpc.map(Converted::Grpc), skipped);
    }
    let (req, skipped) = convert(doc);
    (req.map(Converted::Http), skipped)
}
```

(If Plan 05 has not landed, this is the whole of both definitions. Plan 05 adds a `GraphQl(GraphQlRequest)` variant and its branch next to these.)

Then add this check inside `convert`, right before the comment `// Unsupported type: bail entirely, no Request produced.`. It is the fix for the empty-GET import:

```rust
    // A `.bru` file names its type in `meta`, and the parser never records an
    // `unsupported_type` block for it, so check the meta too. Without this, a
    // WebSocket file became an empty GET request.
    if let Some(meta) = &doc.meta {
        let t = meta.request_type.as_str();
        let already = skipped
            .iter()
            .any(|s| matches!(s, SkipReason::UnsupportedRequestType(_)));
        if !already && !matches!(t, "http" | "") {
            skipped.push(SkipReason::UnsupportedRequestType(t.to_string()));
        }
    }
```

- [ ] **Step 6: Implement the importer changes**

In `crates/rocket-import/src/importer.rs`, in the `match item_opt` Plan 05 wrote in `walk_requests`, add this arm next to the `Http` and `GraphQl` arms. If Plan 05 has not landed, replace the whole `Ok(doc) => { ... }` arm of the `match bru::parse_file(&p)` with this one (it calls `convert_item`, which Step 5 defines):

```rust
                        Some(req_converter::Converted::Grpc(mut grpc)) => {
                            // Point the proto path at the copy inside the new collection.
                            if let Some(raw) = grpc.proto_file_path.clone() {
                                let bru_dir = p.parent().unwrap_or(root);
                                if let Some(rebased) = rebase_proto_path(root, bru_dir, &raw) {
                                    grpc.proto_file_path = Some(rebased);
                                }
                            }
                            match repo.save_grpc_request(collection_name, &out_path, &grpc) {
                                Ok(_) => report.imported += 1,
                                Err(e) => report.skipped.push(SkippedItem {
                                    path: rel_str.clone(),
                                    reason: SkipReason::ParseError(e.to_string()),
                                }),
                            }
                        }
```

```rust
                Ok(doc) => {
                    let (item_opt, skipped_reasons) = req_converter::convert_item(&doc);

                    for reason in skipped_reasons {
                        report.skipped.push(SkippedItem {
                            path: rel_str.clone(),
                            reason,
                        });
                    }

                    let out_path = rel_path.with_extension("yml").to_string_lossy().to_string();
                    match item_opt {
                        Some(req_converter::Converted::Http(req)) => {
                            let _ = repo.save_request(collection_name, &out_path, &req);
                            report.imported += 1;
                        }
                        Some(req_converter::Converted::Grpc(mut grpc)) => {
                            // Point the proto path at the copy inside the new collection.
                            if let Some(raw) = grpc.proto_file_path.clone() {
                                let bru_dir = p.parent().unwrap_or(root);
                                if let Some(rebased) = rebase_proto_path(root, bru_dir, &raw) {
                                    grpc.proto_file_path = Some(rebased);
                                }
                            }
                            match repo.save_grpc_request(collection_name, &out_path, &grpc) {
                                Ok(_) => report.imported += 1,
                                Err(e) => report.skipped.push(SkippedItem {
                                    path: rel_str.clone(),
                                    reason: SkipReason::ParseError(e.to_string()),
                                }),
                            }
                        }
                        None => {}
                    }
                }
```

The remaining importer changes copy `.proto` files in both import paths and add the path helper:

```diff
@@ -301,6 +301,21 @@
             }
 
             let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
+            // A gRPC request points at a `.proto` file by path, so the proto files
+            // travel with the requests. They are not requests and are not counted.
+            if ext == "proto" {
+                let rel = p.strip_prefix(root).unwrap_or(&p);
+                let dest = self
+                    .workspace_path
+                    .join("collections")
+                    .join(collection_name)
+                    .join(rel);
+                if let Some(parent) = dest.parent() {
+                    std::fs::create_dir_all(parent)?;
+                }
+                std::fs::copy(&p, &dest)?;
+                continue;
+            }
             if !matches!(ext, "bru" | "yml" | "yaml") {
                 continue;
             }
```

```diff
@@ -607,6 +641,15 @@
             }
 
             let ext = src_path.extension().and_then(|e| e.to_str()).unwrap_or("");
+            // gRPC requests refer to `.proto` files by path, so copy them as well.
+            // They are not requests and are not counted.
+            if ext == "proto" {
+                if let Some(parent) = dest_path.parent() {
+                    std::fs::create_dir_all(parent)?;
+                }
+                std::fs::copy(&src_path, &dest_path)?;
+                continue;
+            }
             if !matches!(ext, "yml" | "yaml") {
                 continue;
             }
```

```diff
@@ -636,6 +679,31 @@
     }
 }
 
+/// Finds the `.proto` file a Bruno request names and returns its path relative to
+/// the collection root, with forward slashes. Bruno paths are relative to the
+/// request file or to the collection root, so both are tried. Returns `None` for an
+/// absolute path, a missing file, or a file outside the collection, and the caller
+/// keeps the path as written.
+fn rebase_proto_path(root: &Path, request_dir: &Path, raw: &str) -> Option<String> {
+    let raw_path = Path::new(raw);
+    if raw_path.is_absolute() {
+        return None;
+    }
+    let canonical_root = root.canonicalize().ok()?;
+    [request_dir.join(raw_path), root.join(raw_path)]
+        .iter()
+        .filter_map(|candidate| candidate.canonicalize().ok())
+        .find(|real| real.is_file() && real.starts_with(&canonical_root))
+        .and_then(|real| {
+            real.strip_prefix(&canonical_root).ok().map(|rel| {
+                rel.components()
+                    .map(|c| c.as_os_str().to_string_lossy().to_string())
+                    .collect::<Vec<_>>()
+                    .join("/")
+            })
+        })
+}
+
 /// Sanitize a Postman item name for use as a folder/file path component.
 fn sanitize_postman_filename(name: &str) -> String {
     name.chars()
```

The legacy path copies every `.proto` under the collection, not only the ones a request names, because a proto usually imports its neighbours. A proto path that cannot be found, is absolute, or points outside the collection is kept as written, and the call then fails with a message that says the file is missing.

- [ ] **Step 7: Run the tests to verify they pass**

Run:
- `cargo test -j4 -p rocket-import grpc`
- `cargo test -j4 -p rocket-import`

Expected: PASS. The existing `import_report_counts_correctly` is unchanged: its fixture has no `.proto` and no non-HTTP file.

- [ ] **Step 8: Docs**

In `crates/rocket-import/CLAUDE.md`: in "Non-fatal skips" say that unsupported request types are WebSocket (GraphQL and gRPC import); in the module map add `converter/grpc.rs` (`convert(doc) → (Option<GrpcRequest>, Vec<SkipReason>)`); add `grpc`, `grpc_metadata` and `grpc_messages` to the BruDocument fields table (the `grpc {}`, `metadata {}` and `body:grpc {}` blocks, or the YAML `grpc:` block); and add a bullet "`.proto` files are copied next to the requests in both import paths, and a gRPC proto path is rebased to the collection root when the file can be found."

- [ ] **Step 9: Run the checks**

Run:
- `cargo check -j4 -p rocket-import`
- `cargo test -j4 -p rocket-import`

Expected: PASS.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add crates/rocket-import
```

Suggested subject: `feat(import): import Bruno gRPC requests and their proto files`.

---

## Task 2: Typed item in the frontend, sidebar and create dialog

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

This task is a delta on Plan 05 Task 3. Where it says "Plan 05's code" it means the code that task writes.

**Files:**
- Modify: `src-tauri/src/commands/collections.rs`, `src-tauri/src/lib.rs`
- Modify: `src/lib/tauri-api.ts`, `src/lib/colors.ts`, `src/lib/pane-utils.ts`, `src/lib/request-save-mapper.ts`, `src/lib/save-tab-request.ts`, `src/lib/auto-save.ts`
- Modify: `src/types/pane-types.ts`
- Modify: `src/components/collections/RequestNode.tsx`, `src/components/request/CreateRequestDialog.tsx`
- Create tests: `src/lib/__tests__/grpc-state.test.ts`, `src/lib/__tests__/save-tab-request-grpc.test.ts`, `src/components/request/__tests__/CreateRequestDialog.grpc.test.tsx`
- Modify tests: `src/lib/__tests__/auto-save.test.ts`, `src/components/collections/__tests__/RequestNode.test.tsx`

**Interfaces:**
- Consumes: Plan 11's `GrpcRequest` TS type and `CollectionService::{get_grpc_request, save_grpc_request}`, Plan 05's `createDefaultRequestFor`, `saveTabRequest`, `RequestKind`, `RequestSummary.kind`.
- Produces:
  - Tauri commands `get_grpc_request(collection, path) -> GrpcRequest` and `save_grpc_request(collection, path, request) -> GrpcRequest`.
  - TS `getGrpcRequest`, `saveGrpcRequest`.
  - `RequestState.grpc?: GrpcState` with `GrpcState { method, methodType, protoFilePath, messages: GrpcMessageState[], activeMessage, passthrough }`.
  - `mapGrpcToState(g: GrpcRequest): RequestState`, `createDefaultGrpcState()`, `DEFAULT_GRPC_MESSAGE`, `createDefaultRequestFor('grpc')`.
  - `toApiGrpcRequest(uid, name, request)` and `buildGrpcSavePayload(tab, overrides?)`.

Rename and docs edits already work for a gRPC row: Plan 11 made `CollectionService::rename_request` and `update_request_docs` kind-aware, so the existing `renameRequest` command is used as is. Delete and move are path based and need no change. Duplicate is hidden for non-HTTP rows below.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Add the Tauri commands**

In `src-tauri/src/commands/collections.rs`, add `GrpcRequest` to the `rocket_collection` import and add after Plan 05's GraphQL commands (or after `get_request` and `save_request` if Plan 05 has not landed):

```rust
#[tauri::command]
pub fn get_grpc_request(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<GrpcRequest, DomainError> {
    svc.get_grpc_request(&collection, &path)
}

/// Saves a gRPC request. The contract audit hook is HTTP-only, so it does not run here.
#[tauri::command]
pub fn save_grpc_request(
    collection: String,
    path: String,
    request: GrpcRequest,
    svc: State<'_, CollectionService>,
) -> Result<GrpcRequest, DomainError> {
    svc.save_grpc_request(&collection, &path, &request)
}
```

Register both in `generate_handler!` in `src-tauri/src/lib.rs`: `commands::collections::get_grpc_request,` after `commands::collections::get_request,` and `commands::collections::save_grpc_request,` after `commands::collections::save_request,`.

Run: `cargo check -j4 -p rocket`
Expected: PASS.

- [ ] **Step 3: Write the failing frontend tests**

Create `src/lib/__tests__/grpc-state.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { GrpcRequest } from '@/lib/tauri-api';
import {
  createDefaultGrpcState,
  createDefaultRequestFor,
  DEFAULT_GRPC_MESSAGE,
  mapGrpcToState,
} from '../pane-utils';
import { buildGrpcSavePayload, toApiGrpcRequest } from '../request-save-mapper';

const saved: GrpcRequest = {
  uid: 'g1',
  name: 'Say Hello',
  url: 'grpcs://api.example.com:443',
  method: 'demo.greeter.v1.Greeter/SayHello',
  methodType: 'server-streaming',
  protoFilePath: 'protos/greeter.proto',
  metadata: [
    { key: 'x-trace', value: 'abc', enabled: true },
    { key: 'x-off', value: '1', enabled: false },
  ],
  messages: [
    { title: 'first', selected: false, content: '{"name":"a"}' },
    { title: 'second', selected: true, content: '{"name":"b"}' },
  ],
  auth: { authType: 'bearer', token: 't' } as GrpcRequest['auth'],
  tags: ['smoke'],
  docs: 'Greets',
  seq: 4,
  description: 'a description',
  scripts: [{ type: 'before-request', code: '// pre' }],
};

describe('mapGrpcToState', () => {
  it('marks the tab grpc and carries the call description', () => {
    const state = mapGrpcToState(saved);
    expect(state.requestType).toBe('grpc');
    expect(state.url).toBe('grpcs://api.example.com:443');
    expect(state.grpc?.method).toBe('demo.greeter.v1.Greeter/SayHello');
    expect(state.grpc?.methodType).toBe('server-streaming');
    expect(state.grpc?.protoFilePath).toBe('protos/greeter.proto');
  });

  it('reuses the header rows for metadata and keeps disabled rows', () => {
    const state = mapGrpcToState(saved);
    expect(state.headers).toHaveLength(2);
    expect(state.headers[1]).toMatchObject({ key: 'x-off', value: '1', enabled: false });
  });

  it('shows the selected saved message first', () => {
    const state = mapGrpcToState(saved);
    expect(state.grpc?.messages.map((m) => m.title)).toEqual(['first', 'second']);
    expect(state.grpc?.activeMessage).toBe(1);
  });

  it('gives a request with no saved message one empty message', () => {
    const state = mapGrpcToState({ ...saved, messages: undefined, metadata: undefined });
    expect(state.grpc?.messages).toHaveLength(1);
    expect(state.grpc?.messages[0].content).toBe(DEFAULT_GRPC_MESSAGE);
    expect(state.grpc?.activeMessage).toBe(0);
    expect(state.headers).toEqual([]);
  });

  it('maps auth, tags and docs like an HTTP request', () => {
    const state = mapGrpcToState(saved);
    expect(state.auth.authType).toBe('bearer');
    expect(state.tags).toEqual(['smoke']);
    expect(state.docs).toBe('Greets');
  });
});

describe('createDefaultRequestFor', () => {
  it('seeds a grpc request with an empty unary call', () => {
    const state = createDefaultRequestFor('grpc');
    expect(state.requestType).toBe('grpc');
    expect(state.grpc?.methodType).toBe('unary');
    expect(state.grpc?.method).toBe('');
    expect(state.grpc?.messages).toHaveLength(1);
    expect(state.grpc?.messages[0].content).toBe(DEFAULT_GRPC_MESSAGE);
  });

  it('leaves the other kinds without grpc state', () => {
    expect(createDefaultRequestFor('http').grpc).toBeUndefined();
    expect(createDefaultRequestFor('websocket').grpc).toBeUndefined();
  });

  it('gives every default state its own message ids', () => {
    expect(createDefaultGrpcState().messages[0].id).not.toBe(
      createDefaultGrpcState().messages[0].id,
    );
  });
});

describe('toApiGrpcRequest', () => {
  it('round-trips a saved request, including the fields the editor never shows', () => {
    const back = toApiGrpcRequest('g1', 'Say Hello', mapGrpcToState(saved));
    expect(back).toMatchObject({
      uid: 'g1',
      name: 'Say Hello',
      url: saved.url,
      method: saved.method,
      methodType: 'server-streaming',
      protoFilePath: 'protos/greeter.proto',
      tags: ['smoke'],
      docs: 'Greets',
      seq: 4,
      description: 'a description',
      scripts: [{ type: 'before-request', code: '// pre' }],
    });
    expect(back.metadata).toEqual(saved.metadata);
    expect(back.messages).toEqual(saved.messages);
  });

  it('marks exactly the active message as selected', () => {
    const state = mapGrpcToState(saved);
    if (state.grpc) state.grpc.activeMessage = 0;
    const back = toApiGrpcRequest('g1', 'Say Hello', state);
    expect(back.messages?.map((m) => m.selected)).toEqual([true, false]);
  });

  it('omits a blank method and a blank proto path', () => {
    const state = createDefaultRequestFor('grpc');
    state.grpc = { ...(state.grpc ?? createDefaultGrpcState()), protoFilePath: '   ' };
    const back = toApiGrpcRequest('g2', 'New', state);
    expect(back.method).toBeUndefined();
    expect(back.protoFilePath).toBeUndefined();
  });

  it('drops a blank-key draft metadata row but keeps a disabled one', () => {
    const state = mapGrpcToState(saved);
    state.headers = [
      ...state.headers,
      { id: 'draft', key: '', value: 'unfinished', enabled: true },
    ];
    const back = toApiGrpcRequest('g1', 'Say Hello', state);
    expect(back.metadata).toHaveLength(2);
    expect(back.metadata?.[1]).toMatchObject({ key: 'x-off', enabled: false });
  });
});

describe('buildGrpcSavePayload', () => {
  it('uses the tab id as the uid and applies the save-to-collection overrides', () => {
    const payload = buildGrpcSavePayload(
      {
        id: 'tab-9',
        title: 'Untitled',
        tabType: 'request',
        request: createDefaultRequestFor('grpc'),
        response: null,
        isDirty: true,
      },
      { name: 'Chosen', fileName: 'chosen.yml' },
    );
    expect(payload.uid).toBe('tab-9');
    expect(payload.name).toBe('Chosen');
    expect(payload.fileName).toBe('chosen.yml');
    expect(payload.methodType).toBe('unary');
  });
});
```

Create `src/lib/__tests__/save-tab-request-grpc.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  saveRequest: vi.fn().mockResolvedValue({ fileName: 'x.yml' }),
  saveGrpcRequest: vi.fn().mockResolvedValue({ fileName: 'x.yml' }),
}));

import { saveGrpcRequest, saveRequest } from '@/lib/tauri-api';
import type { RequestTab } from '@/types/pane-types';
import { createDefaultRequestFor } from '../pane-utils';
import { saveTabRequest } from '../save-tab-request';

function tabOf(kind: 'grpc' | 'http'): RequestTab {
  return {
    id: 'tab-1',
    title: 'Call',
    tabType: 'request',
    request: createDefaultRequestFor(kind),
    response: null,
    isDirty: true,
  };
}

describe('saveTabRequest for grpc', () => {
  beforeEach(() => vi.clearAllMocks());

  it('routes a grpc tab to saveGrpcRequest and never to saveRequest', async () => {
    await saveTabRequest('api', 'call.yml', tabOf('grpc'));
    expect(saveGrpcRequest).toHaveBeenCalledTimes(1);
    expect(saveRequest).not.toHaveBeenCalled();
    const [collection, path, payload] = vi.mocked(saveGrpcRequest).mock.calls[0];
    expect([collection, path]).toEqual(['api', 'call.yml']);
    expect(payload.uid).toBe('tab-1');
    expect(payload.methodType).toBe('unary');
  });

  it('keeps routing an http tab to saveRequest', async () => {
    await saveTabRequest('api', 'call.yml', tabOf('http'));
    expect(saveRequest).toHaveBeenCalledTimes(1);
    expect(saveGrpcRequest).not.toHaveBeenCalled();
  });
});
```

Create `src/components/request/__tests__/CreateRequestDialog.grpc.test.tsx`. It polyfills the pointer-capture APIs that Radix Select needs in jsdom, the same way `RunnerPane.test.tsx` does:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { CreateRequestDialog } from '@/components/request/CreateRequestDialog';
import { createDefaultLeaf, findTabInTree } from '@/lib/pane-utils';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    saveRequest: vi.fn(),
    saveGrpcRequest: vi.fn(),
  };
});

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
beforeAll(() => {
  HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLElement.prototype.releasePointerCapture = vi.fn();
  HTMLElement.prototype.scrollIntoView = vi.fn();
});

async function chooseGrpc() {
  await userEvent.click(screen.getByLabelText('Request Type'));
  await userEvent.click(await screen.findByRole('option', { name: 'gRPC' }));
}

describe('CreateRequestDialog grpc', () => {
  beforeEach(() => {
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.saveRequest).mockReset();
    vi.mocked(tauriApi.saveGrpcRequest).mockReset();
  });

  it('offers gRPC as a selectable type', async () => {
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);
    await userEvent.click(screen.getByLabelText('Request Type'));
    const option = await screen.findByRole('option', { name: 'gRPC' });
    expect(option.getAttribute('aria-disabled')).not.toBe('true');
  });

  it('saves a real gRPC item, not an HTTP request tagged grpc', async () => {
    vi.mocked(tauriApi.saveGrpcRequest).mockImplementation(async (_c, _p, request) => ({
      ...request,
      fileName: 'say-hello.yml',
    }));
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);

    await chooseGrpc();
    await userEvent.type(screen.getByLabelText('Request Name'), 'say hello');
    await userEvent.type(screen.getByLabelText('URL'), 'localhost:50051');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => expect(tauriApi.saveGrpcRequest).toHaveBeenCalledTimes(1));
    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    const [collection, , payload] = vi.mocked(tauriApi.saveGrpcRequest).mock.calls[0];
    expect(collection).toBe('api');
    expect(payload.url).toBe('localhost:50051');
    expect(payload.methodType).toBe('unary');
    expect(payload.messages).toEqual([{ title: '', selected: true, content: '{}' }]);

    const tab = findTabInTree(usePaneStore.getState().root, payload.uid)?.tab;
    expect(tab && 'request' in tab && tab.request.requestType).toBe('grpc');
    expect(tab?.source?.path).toBe('say-hello.yml');
  });

  it('does not show the HTTP method select for gRPC', async () => {
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);
    await chooseGrpc();
    expect(screen.queryByText('HTTP Method')).toBeNull();
  });
});
```

In `src/lib/__tests__/auto-save.test.ts`: extend the `vi.mock('@/lib/tauri-api', ...)` factory with `saveGrpcRequest: vi.fn().mockResolvedValue(undefined),`, import it next to `saveRequest`, import `createDefaultRequestFor` next to `createDefaultRequest`, and add this test inside the `describe`, before the first existing `it`:

```ts
  it('saves a grpc tab through saveGrpcRequest, never saveRequest', () => {
    const request = createDefaultRequestFor('grpc');
    request.url = 'localhost:50051';

    scheduleAutoSave('tab1', 'my-collection', 'call.yml', 'Call', request);
    vi.advanceTimersByTime(500);

    expect(saveGrpcRequest).toHaveBeenCalledWith(
      'my-collection',
      'call.yml',
      expect.objectContaining({ uid: 'tab1', url: 'localhost:50051', methodType: 'unary' }),
    );
    expect(saveRequest).not.toHaveBeenCalled();
  });
```

In `src/components/collections/__tests__/RequestNode.test.tsx`: add `getGrpcRequest: vi.fn(),` to the `vi.mock('@/lib/tauri-api', ...)` factory next to `getRequest`, and append:

```tsx
describe('RequestNode grpc items', () => {
  const grpcSummary: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }> = {
    type: 'summary',
    uid: 'g-1',
    name: 'Say Hello',
    method: 'GRPC',
    url: 'localhost:50051',
    kind: 'grpc',
  };

  beforeEach(() => {
    vi.setConfig({ testTimeout: 10000 });
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.getRequest).mockReset();
    vi.mocked(tauriApi.getGrpcRequest).mockReset();
  });

  it('shows a gRPC badge instead of the method', () => {
    renderNode(grpcSummary, 'say-hello.yml');
    expect(screen.getByText('gRPC')).toBeTruthy();
    expect(screen.queryByText('GRPC')).toBeNull();
  });

  it('opens through getGrpcRequest and yields a grpc tab', async () => {
    vi.mocked(tauriApi.getGrpcRequest).mockResolvedValue({
      uid: 'g-1',
      name: 'Say Hello',
      url: 'localhost:50051',
      method: 'demo.Greeter/SayHello',
      methodType: 'unary',
      auth: { authType: 'none' },
      messages: [{ title: '', selected: true, content: '{"name":"ada"}' }],
    });
    renderNode(grpcSummary, 'say-hello.yml');
    await userEvent.click(screen.getByLabelText('Open gRPC Say Hello'));

    await waitFor(() => {
      expect(findTabInTree(usePaneStore.getState().root, 'g-1')).not.toBeNull();
    });
    const tab = findTabInTree(usePaneStore.getState().root, 'g-1')?.tab;
    expect(tab && 'request' in tab && tab.request.requestType).toBe('grpc');
    expect(tab && 'request' in tab && tab.request.grpc?.method).toBe('demo.Greeter/SayHello');
    expect(tauriApi.getGrpcRequest).toHaveBeenCalledWith('my-api', 'say-hello.yml');
    expect(tauriApi.getRequest).not.toHaveBeenCalled();
  });

  it('is not draggable into a Flow, because Flow requests are HTTP only', () => {
    renderNode(grpcSummary, 'say-hello.yml');
    expect(screen.getByTestId('request-item-gRPC-Say Hello').getAttribute('draggable')).toBe(
      'false',
    );
  });
});
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `yarn test grpc-state save-tab-request-grpc CreateRequestDialog.grpc auto-save RequestNode`
Expected: FAIL (missing exports and types: `mapGrpcToState`, `createDefaultRequestFor('grpc')`, `saveGrpcRequest`).

- [ ] **Step 5: Implement the types, mappers and IPC wrappers**

`src/lib/tauri-api.ts`, after `saveRequest`:

```ts
export const getGrpcRequest = (collection: string, path: string) =>
  invoke<GrpcRequest>('get_grpc_request', { collection, path });

export const saveGrpcRequest = (collection: string, path: string, request: GrpcRequest) =>
  invoke<GrpcRequest>('save_grpc_request', { collection, path, request });
```

`src/types/pane-types.ts`: add the `grpc` field to `RequestState` (after `actions`) and the types below it:

```ts
  /** Present when `requestType` is 'grpc'. The URL, metadata (as `headers`) and auth live in the shared fields. */
  grpc?: GrpcState;
}

/** One saved message of a gRPC request. */
export interface GrpcMessageState {
  id: string;
  title: string;
  /** Protobuf JSON text. */
  content: string;
}

/** Saved fields the gRPC editor never changes. They are sent back unchanged, so a save loses nothing. */
export interface GrpcPassthrough {
  seq?: number;
  description?: unknown;
  scripts?: import('@/lib/tauri-api').GrpcScript[];
}

export interface GrpcState {
  /** `package.Service/Method`. Empty until a method is picked. */
  method: string;
  methodType: import('@/lib/tauri-api').GrpcMethodType;
  /** Path of the `.proto` file. Empty means use server reflection. */
  protoFilePath: string;
  messages: GrpcMessageState[];
  /** Index of the message the editor shows. It is also the one a save marks as selected. */
  activeMessage: number;
  passthrough: GrpcPassthrough;
}
```

`src/lib/pane-utils.ts`: add `GrpcRequest` to the `@/lib/tauri-api` type import and `GrpcState` to the `@/types/pane-types` import, and replace Plan 05's `createDefaultRequestFor` with this block (it adds the helpers and the `grpc` branch):

```ts
// The editor state of a gRPC request that has nothing saved yet.
export function createDefaultGrpcState(): GrpcState {
  return {
    method: '',
    methodType: 'unary',
    protoFilePath: '',
    messages: [{ id: crypto.randomUUID(), title: '', content: DEFAULT_GRPC_MESSAGE }],
    activeMessage: 0,
    passthrough: {},
  };
}

// An empty message is valid protobuf JSON for every message type.
export const DEFAULT_GRPC_MESSAGE = '{}';

// Maps a saved gRPC request to the tab state. The URL, metadata, auth, tags and docs reuse the
// HTTP fields, so the metadata and auth editors work unchanged.
export function mapGrpcToState(g: GrpcRequest): RequestState {
  const saved = g.messages ?? [];
  const messages = saved.map((m) => ({
    id: crypto.randomUUID(),
    title: m.title,
    content: m.content,
  }));
  const selected = saved.findIndex((m) => m.selected);
  return {
    ...createDefaultRequest(),
    requestType: 'grpc',
    method: 'POST',
    url: g.url,
    headers: (g.metadata ?? []).map((h) => ({
      id: crypto.randomUUID(),
      key: h.key,
      value: h.value,
      enabled: h.enabled,
    })),
    auth: fromPersistedAuth(g.auth, 'inherit'),
    tags: g.tags ?? [],
    docs: g.docs ?? null,
    assertions: g.assertions ?? [],
    grpc: {
      method: g.method ?? '',
      methodType: g.methodType,
      protoFilePath: g.protoFilePath ?? '',
      messages:
        messages.length > 0
          ? messages
          : [{ id: crypto.randomUUID(), title: '', content: DEFAULT_GRPC_MESSAGE }],
      activeMessage: selected >= 0 ? selected : 0,
      passthrough: { seq: g.seq, description: g.description, scripts: g.scripts },
    },
  };
}

// Builds a blank request of the given kind. Only gRPC has its own editor state so far.
export function createDefaultRequestFor(kind: RequestKind): RequestState {
  const base = createDefaultRequest();
  if (kind === 'grpc') {
    return { ...base, requestType: 'grpc', method: 'POST', grpc: createDefaultGrpcState() };
  }
  return { ...base, requestType: kind };
}
```

`src/lib/request-save-mapper.ts`: add `GrpcRequest` to the `@/lib/tauri-api` type import and `RequestState` to the `@/types/pane-types` import (Plan 05 already imports `RequestState`), then append:

```ts
// Builds the persisted gRPC payload from tab state. Shared by the Save button,
// save-to-collection and auto-save, so all three write the same fields. Request
// variables are not sent: they are saved on their own path and an empty list keeps them.
export function toApiGrpcRequest(uid: string, name: string, request: RequestState): GrpcRequest {
  const g = request.grpc;
  return {
    uid,
    name,
    url: request.url,
    method: g?.method ? g.method : undefined,
    methodType: g?.methodType ?? 'unary',
    protoFilePath: g?.protoFilePath.trim() ? g.protoFilePath.trim() : undefined,
    metadata: toPersistedHeaders(request.headers),
    messages: (g?.messages ?? []).map((m, i) => ({
      title: m.title,
      selected: i === g?.activeMessage,
      content: m.content,
    })),
    auth: toPersistedAuth(request.auth),
    tags: request.tags && request.tags.length > 0 ? request.tags : undefined,
    docs: request.docs ?? null,
    assertions: request.assertions ?? [],
    seq: g?.passthrough.seq,
    description: g?.passthrough.description,
    scripts: g?.passthrough.scripts,
  };
}

export function buildGrpcSavePayload(
  tab: RequestTab,
  overrides?: RequestSavePayloadOverrides,
): GrpcRequest {
  const payload = toApiGrpcRequest(
    tab.id || crypto.randomUUID(),
    overrides?.name ?? tab.title,
    tab.request,
  );
  return overrides?.fileName !== undefined ? { ...payload, fileName: overrides.fileName } : payload;
}
```

`src/lib/save-tab-request.ts`: import `buildGrpcSavePayload` and `saveGrpcRequest`, and add this branch next to Plan 05's GraphQL branch, before the HTTP fallthrough:

```ts
  if (tab.request.requestType === 'grpc') {
    return saveGrpcRequest(collection, path, buildGrpcSavePayload(tab, overrides));
  }
```

`src/lib/auto-save.ts`: import `toApiGrpcRequest` from `@/lib/request-save-mapper` and `saveGrpcRequest` from `@/lib/tauri-api`. The timer callback must pick the save command by protocol (Plan 05 added the GraphQL case; add gRPC next to it):

```ts
      if (request.requestType === 'grpc') {
        await saveGrpcRequest(
          collection,
          path,
          toApiGrpcRequest(tabId || crypto.randomUUID(), title, request),
        );
      } else {
        await saveRequest(
          collection,
          path,
          toApiRequest(tabId || crypto.randomUUID(), title, request),
        );
      }
```

`src/lib/colors.ts`, in `METHOD_BADGE_COLOR` after `HEAD`:

```ts
  GRPC: 'text-teal-500 dark:text-teal-400 border-teal-500/30 bg-teal-500/10 dark:bg-teal-500/20',
```

- [ ] **Step 6: Implement the sidebar, open and create behaviour**

`src/components/collections/RequestNode.tsx`: import `getGrpcRequest` with `getRequest` and `renameRequest`, and `mapGrpcToState` with the other `@/lib/pane-utils` imports. Plan 05 derives `kind` and `badge` after the `RequestNodeProps` destructure. Make `badge` know about gRPC (the badge colour lookup already upper-cases it, so `gRPC` finds the `GRPC` colour):

```tsx
  const kind = itemData.type === 'summary' ? (itemData.kind ?? 'http') : 'http';
  // The badge shows the protocol for GraphQL and gRPC, the HTTP verb otherwise.
  const badge = kind === 'graphql' ? 'GQL' : kind === 'grpc' ? 'gRPC' : method;
```

and add the gRPC branch to `createTab`, next to Plan 05's GraphQL branch:

```tsx
  async function createTab(): Promise<RequestTab> {
    let request: RequestState;
    if (kind === 'grpc') {
      request = mapGrpcToState(await getGrpcRequest(collectionName, path));
    } else {
      const full = itemData.type === 'request' ? itemData : await getRequest(collectionName, path);
      request = mapApiRequestToState(full, true);
    }
    return {
      id: uid,
      title: name,
      tabType: 'request',
      request,
      response: null,
      isDirty: false,
      source: { collection: collectionName, path },
    };
  }
```

Plan 05 already sets `draggable={kind === 'http'}` on the row, so a gRPC row cannot be dragged into a Flow. Duplicate only knows HTTP requests (`CollectionsSidebar.handleDuplicate` returns without doing anything for another kind), so hide it for the other kinds. Wrap the two Duplicate items, in the dropdown menu and in the context menu, in `{kind === 'http' && (...)}`:

```tsx
              {kind === 'http' && (
                <DropdownMenuItem onClick={() => void onDuplicate(collectionName, path, name)}>
                  <Copy aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Duplicate
                </DropdownMenuItem>
              )}
```

and the same for the `ContextMenuItem` that does the same thing.

`src/components/request/CreateRequestDialog.tsx`. Plan 05 disabled the gRPC and WebSocket options because they saved an HTTP file under a protocol label. Re-enable gRPC by changing its entry in `REQUEST_TYPES` back to:

```tsx
  { label: 'gRPC', value: 'grpc' },
```

(leave WebSocket disabled until its plan lands). Import `DEFAULT_GRPC_MESSAGE` and `mapGrpcToState` from `@/lib/pane-utils` and `saveGrpcRequest` from `@/lib/tauri-api`. In `handleCreate`, after `const filePath = ...` and before the HTTP `payload`, add the gRPC branch. It saves a real gRPC item and opens its tab:

```tsx
      if (requestType === 'grpc') {
        const saved = await saveGrpcRequest(collectionName, filePath, {
          uid,
          name: trimmedName,
          url,
          methodType: 'unary',
          messages: [{ title: '', selected: true, content: DEFAULT_GRPC_MESSAGE }],
          auth: { authType: 'none' as const },
          fileName: filePath,
        });
        const grpcTab: RequestTab = {
          id: uid,
          title: trimmedName,
          tabType: 'request',
          request: mapGrpcToState(saved),
          response: null,
          isDirty: false,
          source: { collection: collectionName, path: saved.fileName ?? filePath },
        };
        usePaneStore.getState().openTab(grpcTab);
        reset();
        onClose();
        return;
      }
```

and make the URL placeholder depend on the type:

```tsx
              placeholder={
                requestType === 'grpc' ? 'localhost:50051' : 'https://api.example.com/users'
              }
```

- [ ] **Step 7: Run the checks**

Run:
- `yarn test grpc-state save-tab-request-grpc CreateRequestDialog.grpc auto-save RequestNode collectPaths CollectionNode`
- `yarn tsc --noEmit`
- `yarn check`
- `cargo check -j4 -p rocket`

Expected: PASS. `yarn tsc --noEmit` flags any other site that switches on `RequestState['requestType']`. A gRPC tab opens in the HTTP panel until Task 3, so do not ship Task 2 alone.

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add src-tauri/src/commands/collections.rs src-tauri/src/lib.rs \
  src/lib src/types/pane-types.ts src/components/collections src/components/request
```

Suggested subject: `feat(ui): open, create and save gRPC requests from the sidebar`.

---

## Task 3: The gRPC tab

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/lib/tauri-api.ts`, `src/components/panes/EditorGroup.tsx`, `src/stores/pane-store.ts`
- Create: `src/stores/grpc-store.ts`, `src/hooks/useGrpcVariableContext.ts`
- Create: `src/components/grpc/GrpcPanel.tsx`, `GrpcMethodPicker.tsx`, `GrpcMessageEditor.tsx`, `GrpcResponseView.tsx`
- Create tests: `src/stores/__tests__/grpc-store.test.ts`, `src/stores/__tests__/pane-store.grpc.test.ts`, `src/components/grpc/__tests__/GrpcMethodPicker.test.tsx`, `GrpcPanel.test.tsx`

**Interfaces:**
- Consumes: Plan 12's commands and events, Task 2's `RequestState.grpc`, `toApiGrpcRequest`, `SaveRequestButton`, `SaveToCollectionDialog`, `AuthEditor`, `KeyValueEditor`, `RequestVariablesPanel`, `MonacoWrapper`, `SingleLineEditor`.
- Produces:
  - TS: `GrpcPair`, `GrpcStatus`, `GrpcUnaryResponse`, `GrpcMethodInfo`, `GrpcServiceInfo`, `GrpcExecuteInput`, `grpcUnaryCall`, `grpcStartSession`, `grpcSendMessage`, `grpcEndRequests`, `grpcCancelSession`, `grpcListServices`, and the four `onGrpcSession*` listeners.
  - `useGrpcStore` with `sessions`, `sessionByTab`, `unaryByTab`, `attachSession`, `recordOutbound`, `setUnary`, `cancelTabSession`, `dropTab`; `ensureGrpcListeners()`; `MAX_GRPC_LOG_ENTRIES`.
  - `GrpcPanel` (`{ tab: RequestTab; groupId: string }`), `GrpcMethodPicker`, `GrpcMessageEditor`, `GrpcResponseView`.

How a call behaves in the tab: **unary** Send calls `grpcUnaryCall` with a 30 second deadline and shows the status, reply, headers and trailers. **Server-streaming** Start opens a session with the active message and shows the log live. **Client-streaming and bidirectional** Start opens a session with no message. Send message sends the active message, End requests half-closes, Cancel cancels. A running session is also cancelled when its tab closes.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Write the failing tests**

Create `src/stores/__tests__/grpc-store.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({
  handlers: {} as Record<string, (event: unknown) => void>,
  listen: vi.fn(),
}));

vi.mock('@/lib/tauri-api', () => {
  const on = (name: string) =>
    vi.fn(async (handler: (event: unknown) => void) => {
      hoisted.listen(name);
      hoisted.handlers[name] = handler;
      return () => undefined;
    });
  return {
    onGrpcSessionStarted: on('started'),
    onGrpcSessionHeaders: on('headers'),
    onGrpcSessionMessage: on('message'),
    onGrpcSessionFinished: on('finished'),
    grpcCancelSession: vi.fn().mockResolvedValue(undefined),
  };
});

import { grpcCancelSession } from '@/lib/tauri-api';
import { ensureGrpcListeners, MAX_GRPC_LOG_ENTRIES, useGrpcStore } from '../grpc-store';

function emit(name: string, payload: object) {
  hoisted.handlers[name]?.(payload);
}

beforeEach(async () => {
  useGrpcStore.setState({ sessions: {}, sessionByTab: {}, unaryByTab: {} });
  vi.mocked(grpcCancelSession).mockClear();
  await ensureGrpcListeners();
});

describe('ensureGrpcListeners', () => {
  it('subscribes to the four session events exactly once', async () => {
    await ensureGrpcListeners();
    await ensureGrpcListeners();
    expect(hoisted.listen.mock.calls.map(([name]) => name).sort()).toEqual([
      'finished',
      'headers',
      'message',
      'started',
    ]);
  });
});

describe('session events', () => {
  it('builds the view of a session from its events in order', () => {
    emit('started', {
      type: 'grpcSessionStarted',
      session_id: 's1',
      method_type: 'bidi-streaming',
    });
    emit('headers', {
      type: 'grpcSessionHeaders',
      session_id: 's1',
      headers: [{ name: 'content-type', value: 'application/grpc' }],
    });
    emit('message', { type: 'grpcSessionMessage', session_id: 's1', index: 0, json: '{"a":1}' });
    emit('message', { type: 'grpcSessionMessage', session_id: 's1', index: 1, json: '{"a":2}' });
    emit('finished', {
      type: 'grpcSessionFinished',
      session_id: 's1',
      code: 0,
      code_name: 'OK',
      message: '',
      trailers: [{ name: 'x-end', value: '1' }],
      duration_ms: 42,
    });

    const s = useGrpcStore.getState().sessions.s1;
    expect(s.methodType).toBe('bidi-streaming');
    expect(s.headers).toEqual([{ name: 'content-type', value: 'application/grpc' }]);
    expect(s.log.map((l) => [l.direction, l.json])).toEqual([
      ['in', '{"a":1}'],
      ['in', '{"a":2}'],
    ]);
    expect(s.status).toBe('finished');
    expect(s.finished).toEqual({
      status: { code: 0, codeName: 'OK', message: '' },
      trailers: [{ name: 'x-end', value: '1' }],
      durationMs: 42,
    });
  });

  it('creates the session from whichever event arrives first', () => {
    emit('message', { type: 'grpcSessionMessage', session_id: 'early', index: 0, json: '{}' });
    emit('started', {
      type: 'grpcSessionStarted',
      session_id: 'early',
      method_type: 'server-streaming',
    });
    const s = useGrpcStore.getState().sessions.early;
    expect(s.status).toBe('running');
    expect(s.methodType).toBe('server-streaming');
    expect(s.log).toHaveLength(1);
  });

  it('keeps only the newest entries when a stream runs long', () => {
    for (let i = 0; i < MAX_GRPC_LOG_ENTRIES + 5; i++) {
      emit('message', { type: 'grpcSessionMessage', session_id: 'long', index: i, json: `${i}` });
    }
    const log = useGrpcStore.getState().sessions.long.log;
    expect(log).toHaveLength(MAX_GRPC_LOG_ENTRIES);
    expect(log[log.length - 1].json).toBe(`${MAX_GRPC_LOG_ENTRIES + 4}`);
    expect(log[0].json).toBe('5');
  });
});

describe('store actions', () => {
  it('records a sent message as an outbound entry', () => {
    useGrpcStore.getState().recordOutbound('s2', '{"x":1}');
    const log = useGrpcStore.getState().sessions.s2.log;
    expect(log).toHaveLength(1);
    expect(log[0]).toMatchObject({ direction: 'out', json: '{"x":1}' });
  });

  it('drops the previous finished session of a tab when a new one is attached', () => {
    emit('finished', {
      type: 'grpcSessionFinished',
      session_id: 'old',
      code: 0,
      code_name: 'OK',
      message: '',
      trailers: [],
      duration_ms: 1,
    });
    useGrpcStore.getState().attachSession('tab', 'old');
    useGrpcStore.getState().attachSession('tab', 'new');
    expect(useGrpcStore.getState().sessions.old).toBeUndefined();
    expect(useGrpcStore.getState().sessionByTab.tab).toBe('new');
  });

  it('shows an attached session as running before any event reaches the store', () => {
    useGrpcStore.getState().attachSession('tab', 'fresh');
    expect(useGrpcStore.getState().sessions.fresh.status).toBe('running');
  });

  it('keeps what an early event already recorded when the session is attached', () => {
    emit('message', { type: 'grpcSessionMessage', session_id: 'early', index: 0, json: '{}' });
    useGrpcStore.getState().attachSession('tab', 'early');
    expect(useGrpcStore.getState().sessions.early.log).toHaveLength(1);
  });

  it('cancels only a running session and survives a failed cancel', async () => {
    emit('started', {
      type: 'grpcSessionStarted',
      session_id: 'run',
      method_type: 'bidi-streaming',
    });
    useGrpcStore.getState().attachSession('tab', 'run');
    await useGrpcStore.getState().cancelTabSession('tab');
    expect(grpcCancelSession).toHaveBeenCalledWith('run');

    vi.mocked(grpcCancelSession).mockRejectedValueOnce(new Error('gone'));
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    await expect(useGrpcStore.getState().cancelTabSession('tab')).resolves.toBeUndefined();

    vi.mocked(grpcCancelSession).mockClear();
    emit('finished', {
      type: 'grpcSessionFinished',
      session_id: 'run',
      code: 1,
      code_name: 'CANCELLED',
      message: '',
      trailers: [],
      duration_ms: 1,
    });
    await useGrpcStore.getState().cancelTabSession('tab');
    expect(grpcCancelSession).not.toHaveBeenCalled();
  });

  it('forgets a closed tab and cancels its running call first', async () => {
    emit('started', {
      type: 'grpcSessionStarted',
      session_id: 'run',
      method_type: 'bidi-streaming',
    });
    useGrpcStore.getState().attachSession('tab', 'run');
    useGrpcStore.getState().setUnary('tab', { status: 'sending' });
    await useGrpcStore.getState().dropTab('tab');
    expect(grpcCancelSession).toHaveBeenCalledWith('run');
    const s = useGrpcStore.getState();
    expect(s.sessions.run).toBeUndefined();
    expect(s.sessionByTab.tab).toBeUndefined();
    expect(s.unaryByTab.tab).toBeUndefined();
  });

  it('clears a unary result when given undefined', () => {
    useGrpcStore.getState().setUnary('tab', { status: 'error', error: 'boom' });
    useGrpcStore.getState().setUnary('tab', undefined);
    expect(useGrpcStore.getState().unaryByTab.tab).toBeUndefined();
  });
});
```

Create `src/stores/__tests__/pane-store.grpc.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultRequestFor } from '@/lib/pane-utils';
import type { RequestTab } from '@/types/pane-types';
import { useGrpcStore } from '../grpc-store';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, grpcCancelSession: vi.fn().mockResolvedValue(undefined) };
});

import { grpcCancelSession } from '@/lib/tauri-api';

function tab(kind: 'grpc' | 'http'): RequestTab {
  return {
    id: crypto.randomUUID(),
    title: 'T',
    tabType: 'request',
    request: createDefaultRequestFor(kind),
    response: null,
    isDirty: false,
  };
}

function leaf() {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('expected a leaf');
  return root;
}

describe('closing a gRPC tab', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    useGrpcStore.setState({ sessions: {}, sessionByTab: {}, unaryByTab: {} });
    vi.mocked(grpcCancelSession).mockClear();
  });

  it('cancels its running stream and forgets its results', async () => {
    const t = tab('grpc');
    usePaneStore.getState().openTab(t);
    useGrpcStore.getState().attachSession(t.id, 'run-1');
    useGrpcStore.getState().setUnary(t.id, { status: 'sending' });

    usePaneStore.getState().closeTab(t.id, leaf().groupId);

    await vi.waitFor(() => expect(grpcCancelSession).toHaveBeenCalledWith('run-1'));
    await vi.waitFor(() => expect(useGrpcStore.getState().sessionByTab[t.id]).toBeUndefined());
    expect(useGrpcStore.getState().unaryByTab[t.id]).toBeUndefined();
  });

  it('leaves the gRPC store alone when an http tab closes', () => {
    const t = tab('http');
    usePaneStore.getState().openTab(t);
    usePaneStore.getState().closeTab(t.id, leaf().groupId);
    expect(grpcCancelSession).not.toHaveBeenCalled();
  });
});
```

Create `src/components/grpc/__tests__/GrpcMethodPicker.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GrpcExecuteInput, GrpcServiceInfo } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { GrpcMethodPicker } from '../GrpcMethodPicker';

vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    'aria-label': label,
  }: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => <input aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} />,
}));

const dialog = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => dialog);

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, grpcListServices: vi.fn() };
});

const services: GrpcServiceInfo[] = [
  {
    name: 'demo.greeter.v1.Greeter',
    methods: [
      {
        name: 'SayHello',
        fullName: 'demo.greeter.v1.Greeter/SayHello',
        methodType: 'unary',
        inputType: 'demo.greeter.v1.HelloRequest',
        outputType: 'demo.greeter.v1.HelloReply',
      },
      {
        name: 'Chat',
        fullName: 'demo.greeter.v1.Greeter/Chat',
        methodType: 'bidi-streaming',
        inputType: 'demo.greeter.v1.HelloRequest',
        outputType: 'demo.greeter.v1.HelloReply',
      },
    ],
  },
];

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
beforeAll(() => {
  HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLElement.prototype.releasePointerCapture = vi.fn();
  HTMLElement.prototype.scrollIntoView = vi.fn();
});

const input: GrpcExecuteInput = {
  collection: 'api',
  request: {
    uid: 'g',
    name: 'G',
    url: 'localhost:50051',
    methodType: 'unary',
    auth: { authType: 'none' },
  },
};

function renderPicker(overrides: Partial<React.ComponentProps<typeof GrpcMethodPicker>> = {}) {
  const props = {
    method: '',
    protoFilePath: 'protos/greeter.proto',
    onProtoFilePathChange: vi.fn(),
    onPick: vi.fn(),
    buildInput: () => input,
    sourceKey: 'a',
    ...overrides,
  };
  return { props, ...render(<GrpcMethodPicker {...props} />) };
}

describe('GrpcMethodPicker', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.grpcListServices).mockReset();
    dialog.open.mockReset();
  });

  it('loads the methods the first time the list is opened and lists them by service', async () => {
    vi.mocked(tauriApi.grpcListServices).mockResolvedValue(services);
    renderPicker();
    await userEvent.click(screen.getByLabelText('Method'));

    expect(await screen.findByText('SayHello')).toBeInTheDocument();
    expect(screen.getByText('demo.greeter.v1.Greeter')).toBeInTheDocument();
    expect(screen.getByText('Bidi stream')).toBeInTheDocument();
    expect(tauriApi.grpcListServices).toHaveBeenCalledWith(input, false);
  });

  it('reports the picked method with its call shape', async () => {
    vi.mocked(tauriApi.grpcListServices).mockResolvedValue(services);
    const { props } = renderPicker();
    await userEvent.click(screen.getByLabelText('Method'));
    await userEvent.click(await screen.findByText('Chat'));

    expect(props.onPick).toHaveBeenCalledWith(
      expect.objectContaining({
        fullName: 'demo.greeter.v1.Greeter/Chat',
        methodType: 'bidi-streaming',
      }),
    );
  });

  it('shows the saved method before any list is loaded', () => {
    renderPicker({ method: 'demo.greeter.v1.Greeter/SayHello' });
    expect(screen.getByLabelText('Method')).toHaveTextContent('demo.greeter.v1.Greeter/SayHello');
    expect(tauriApi.grpcListServices).not.toHaveBeenCalled();
  });

  it('reloads with refresh when the reload button is used', async () => {
    vi.mocked(tauriApi.grpcListServices).mockResolvedValue(services);
    renderPicker();
    await userEvent.click(screen.getByLabelText('Reload methods'));
    await waitFor(() => expect(tauriApi.grpcListServices).toHaveBeenCalledWith(input, true));
  });

  it('shows the error when the methods cannot be loaded', async () => {
    vi.mocked(tauriApi.grpcListServices).mockRejectedValue(
      new Error('the server does not support gRPC server reflection'),
    );
    renderPicker({ protoFilePath: '' });
    await userEvent.click(screen.getByLabelText('Reload methods'));
    expect(await screen.findByRole('alert')).toHaveTextContent('does not support');
  });

  it('says methods come from reflection when no proto file is set', () => {
    renderPicker({ protoFilePath: '' });
    expect(screen.getByText(/server reflection on the URL/)).toBeInTheDocument();
  });

  it('drops a loaded list when the source changes', async () => {
    vi.mocked(tauriApi.grpcListServices).mockResolvedValue(services);
    const { rerender, props } = renderPicker();
    await userEvent.click(screen.getByLabelText('Reload methods'));
    await waitFor(() => expect(tauriApi.grpcListServices).toHaveBeenCalledTimes(1));

    rerender(<GrpcMethodPicker {...props} sourceKey='b' />);
    await userEvent.click(screen.getByLabelText('Method'));
    await waitFor(() => expect(tauriApi.grpcListServices).toHaveBeenCalledTimes(2));
  });

  it('puts the chosen file in the proto path', async () => {
    dialog.open.mockResolvedValue('/work/protos/greeter.proto');
    const { props } = renderPicker({ protoFilePath: '' });
    await userEvent.click(screen.getByLabelText('Browse for a proto file'));
    await waitFor(() =>
      expect(props.onProtoFilePathChange).toHaveBeenCalledWith('/work/protos/greeter.proto'),
    );
    expect(dialog.open).toHaveBeenCalledWith(
      expect.objectContaining({ filters: [{ name: 'Protocol Buffers', extensions: ['proto'] }] }),
    );
  });
});
```

Create `src/components/grpc/__tests__/GrpcPanel.test.tsx`. It stands in for Monaco and `SingleLineEditor` with plain inputs, mocks `listen` to capture the event handlers, and reads the tab from the pane store like the real pane does:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultLeaf, createDefaultRequestFor, findTabInTree } from '@/lib/pane-utils';
import * as tauriApi from '@/lib/tauri-api';
import { useGrpcStore } from '@/stores/grpc-store';
import { usePaneStore } from '@/stores/pane-store';
import { isRequestTab, type RequestTab } from '@/types/pane-types';
import { GrpcPanel } from '../GrpcPanel';

const events = vi.hoisted(() => ({
  handlers: {} as Record<string, (event: { payload: unknown }) => void>,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    events.handlers[name] = handler;
    return () => undefined;
  }),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    'aria-label': label,
  }: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => <input aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} />,
}));

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <textarea
      aria-label='Message JSON'
      value={value}
      onChange={(e) => onChange?.(e.target.value)}
    />
  ),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    grpcUnaryCall: vi.fn(),
    grpcStartSession: vi.fn(),
    grpcSendMessage: vi.fn(),
    grpcEndRequests: vi.fn(),
    grpcCancelSession: vi.fn(),
    grpcListServices: vi.fn().mockResolvedValue([]),
    listEnvironments: vi.fn().mockResolvedValue([]),
    getGlobalEnvironmentName: vi.fn().mockResolvedValue(null),
    getGlobalEnvironment: vi.fn().mockResolvedValue(null),
    listGlobalEnvironments: vi.fn().mockResolvedValue([]),
    getProcessEnvVars: vi.fn().mockResolvedValue({}),
    getCollectionSettings: vi.fn().mockResolvedValue({ variables: [], headers: [] }),
    getFolderVariables: vi.fn().mockResolvedValue([]),
    getRequestVariables: vi.fn().mockResolvedValue([]),
  };
});

const TAB_ID = 'tab-g';

function grpcTab(methodType: tauriApi.GrpcMethodType, method = 'demo.Greeter/Say'): RequestTab {
  const request = createDefaultRequestFor('grpc');
  request.url = 'localhost:50051';
  if (request.grpc) {
    request.grpc.method = method;
    request.grpc.methodType = methodType;
    request.grpc.messages = [{ id: 'm1', title: '', content: '{"name":"ada"}' }];
  }
  return {
    id: TAB_ID,
    title: 'Say',
    tabType: 'request',
    request,
    response: null,
    isDirty: false,
    source: { collection: 'api', path: 'say.yml' },
  };
}

// Reads the tab from the store like the real pane does, so edits show up.
function Harness() {
  const tab = usePaneStore((s) => findTabInTree(s.root, TAB_ID)?.tab);
  return tab && isRequestTab(tab) ? <GrpcPanel tab={tab} groupId='g' /> : null;
}

function mount(tab: RequestTab) {
  const leaf = { ...createDefaultLeaf(), tabs: [tab], activeTabId: tab.id };
  usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <Harness />
    </QueryClientProvider>,
  );
}

function emit(name: string, payload: object) {
  act(() => events.handlers[name]({ payload }));
}

const okResponse: tauriApi.GrpcUnaryResponse = {
  headers: [{ name: 'content-type', value: 'application/grpc' }],
  trailers: [{ name: 'x-end', value: '1' }],
  messageJson: '{\n  "message": "hello ada"\n}',
  status: { code: 0, codeName: 'OK', message: '' },
  durationMs: 12,
};

describe('GrpcPanel unary', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useGrpcStore.setState({ sessions: {}, sessionByTab: {}, unaryByTab: {} });
  });

  it('sends the editor state and shows the reply and status', async () => {
    vi.mocked(tauriApi.grpcUnaryCall).mockResolvedValue(okResponse);
    mount(grpcTab('unary'));

    await userEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => expect(tauriApi.grpcUnaryCall).toHaveBeenCalledTimes(1));
    const input = vi.mocked(tauriApi.grpcUnaryCall).mock.calls[0][0];
    expect(input.collection).toBe('api');
    expect(input.requestPath).toBe('say.yml');
    expect(input.message).toBe('{"name":"ada"}');
    expect(input.request.method).toBe('demo.Greeter/Say');
    expect(input.request.url).toBe('localhost:50051');
    expect(input.timeoutMs).toBe(30000);

    expect(await screen.findByText('0 OK')).toBeInTheDocument();
    expect(screen.getByText(/hello ada/)).toBeInTheDocument();
  });

  it('shows a failing status with its message', async () => {
    vi.mocked(tauriApi.grpcUnaryCall).mockResolvedValue({
      headers: [],
      trailers: [],
      messageJson: null,
      status: { code: 5, codeName: 'NOT_FOUND', message: 'no such user' },
      durationMs: 3,
    });
    mount(grpcTab('unary'));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByText('5 NOT_FOUND')).toBeInTheDocument();
    expect(screen.getByText('no such user')).toBeInTheDocument();
    expect(screen.getByText('The call returned no message.')).toBeInTheDocument();
  });

  it('shows a transport error from the backend', async () => {
    vi.mocked(tauriApi.grpcUnaryCall).mockRejectedValue(
      'Http error: could not connect to http://h:1',
    );
    mount(grpcTab('unary'));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('could not connect');
  });

  it('asks for a method instead of calling the backend', async () => {
    mount(grpcTab('unary', ''));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Choose a method first.')).toBeInTheDocument();
    expect(tauriApi.grpcUnaryCall).not.toHaveBeenCalled();
  });

  it('writes edits to the tab, so a save keeps them', async () => {
    mount(grpcTab('unary'));
    await userEvent.clear(screen.getByLabelText('gRPC URL'));
    await userEvent.type(screen.getByLabelText('gRPC URL'), 'api.test:9');
    await userEvent.click(screen.getByRole('button', { name: 'Add message' }));

    const tab = findTabInTree(usePaneStore.getState().root, TAB_ID)?.tab;
    const request = tab && isRequestTab(tab) ? tab.request : undefined;
    expect(request?.url).toBe('api.test:9');
    expect(request?.grpc?.messages).toHaveLength(2);
    expect(request?.grpc?.activeMessage).toBe(1);
    expect(tab?.isDirty).toBe(true);
  });

  it('removes the active message and keeps one selected', async () => {
    const tab = grpcTab('unary');
    tab.request.grpc?.messages.push({ id: 'm2', title: 'second', content: '{}' });
    mount(tab);
    await userEvent.click(screen.getByRole('button', { name: 'second' }));
    await userEvent.click(screen.getByRole('button', { name: 'Remove message' }));

    const saved = findTabInTree(usePaneStore.getState().root, TAB_ID)?.tab;
    const grpc = saved && isRequestTab(saved) ? saved.request.grpc : undefined;
    expect(grpc?.messages.map((m) => m.id)).toEqual(['m1']);
    expect(grpc?.activeMessage).toBe(0);
  });
});

describe('GrpcPanel streaming', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useGrpcStore.setState({ sessions: {}, sessionByTab: {}, unaryByTab: {} });
  });

  it('starts a server stream with the message and shows messages as they arrive', async () => {
    vi.mocked(tauriApi.grpcStartSession).mockResolvedValue('s1');
    mount(grpcTab('server-streaming'));

    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    await waitFor(() => expect(tauriApi.grpcStartSession).toHaveBeenCalledTimes(1));
    expect(vi.mocked(tauriApi.grpcStartSession).mock.calls[0][0].message).toBe('{"name":"ada"}');
    expect(await screen.findByRole('button', { name: /Cancel/ })).toBeInTheDocument();

    emit('grpc-session-started', { session_id: 's1', method_type: 'server-streaming' });
    emit('grpc-session-message', { session_id: 's1', index: 0, json: '{"message":"one"}' });
    emit('grpc-session-message', { session_id: 's1', index: 1, json: '{"message":"two"}' });

    const log = await screen.findByRole('list', { name: 'Message log' });
    expect(log).toHaveTextContent('one');
    expect(log).toHaveTextContent('two');
    expect(screen.getByText('Messages (2)')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Send message' })).toBeNull();

    emit('grpc-session-finished', {
      session_id: 's1',
      code: 0,
      code_name: 'OK',
      message: '',
      trailers: [],
      duration_ms: 9,
    });
    expect(await screen.findByText('0 OK')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start' })).toBeInTheDocument();
  });

  it('sends messages and ends the requests of a bidi stream', async () => {
    vi.mocked(tauriApi.grpcStartSession).mockResolvedValue('s2');
    vi.mocked(tauriApi.grpcSendMessage).mockResolvedValue(undefined);
    vi.mocked(tauriApi.grpcEndRequests).mockResolvedValue(undefined);
    mount(grpcTab('bidi-streaming'));

    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    await waitFor(() => expect(tauriApi.grpcStartSession).toHaveBeenCalledTimes(1));
    expect(vi.mocked(tauriApi.grpcStartSession).mock.calls[0][0].message).toBe('{"name":"ada"}');
    emit('grpc-session-started', { session_id: 's2', method_type: 'bidi-streaming' });

    await userEvent.click(await screen.findByRole('button', { name: 'Send message' }));
    await waitFor(() =>
      expect(tauriApi.grpcSendMessage).toHaveBeenCalledWith('s2', '{"name":"ada"}'),
    );
    expect(screen.getByText('Sent')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'End requests' }));
    await waitFor(() => expect(tauriApi.grpcEndRequests).toHaveBeenCalledWith('s2'));
    expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled();
  });

  it('cancels a running stream', async () => {
    vi.mocked(tauriApi.grpcStartSession).mockResolvedValue('s3');
    vi.mocked(tauriApi.grpcCancelSession).mockResolvedValue(undefined);
    mount(grpcTab('bidi-streaming'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    emit('grpc-session-started', { session_id: 's3', method_type: 'bidi-streaming' });

    await userEvent.click(await screen.findByRole('button', { name: /Cancel/ }));
    await waitFor(() => expect(tauriApi.grpcCancelSession).toHaveBeenCalledWith('s3'));

    emit('grpc-session-finished', {
      session_id: 's3',
      code: 1,
      code_name: 'CANCELLED',
      message: 'cancelled by the user',
      trailers: [],
      duration_ms: 5,
    });
    expect(await screen.findByText('1 CANCELLED')).toBeInTheDocument();
  });

  it('shows the error when a stream cannot start', async () => {
    vi.mocked(tauriApi.grpcStartSession).mockRejectedValue(
      'Invalid input: invalid HelloRequest message',
    );
    mount(grpcTab('server-streaming'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('invalid HelloRequest message');
    expect(screen.getByRole('button', { name: 'Start' })).toBeEnabled();
  });

  it('shows an event that arrives before the start command returns', async () => {
    let resolveStart: (id: string) => void = () => undefined;
    vi.mocked(tauriApi.grpcStartSession).mockReturnValue(
      new Promise<string>((resolve) => {
        resolveStart = resolve;
      }),
    );
    mount(grpcTab('server-streaming'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    await waitFor(() => expect(tauriApi.grpcStartSession).toHaveBeenCalled());

    emit('grpc-session-message', { session_id: 's4', index: 0, json: '{"message":"early"}' });
    await act(async () => resolveStart('s4'));

    expect(await screen.findByRole('list', { name: 'Message log' })).toHaveTextContent('early');
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn test grpc-store pane-store.grpc GrpcMethodPicker GrpcPanel`
Expected: FAIL (modules and exports do not exist).

- [ ] **Step 4: Implement the IPC wrappers and the store**

Append to `src/lib/tauri-api.ts`:

```ts
// ============================================================
// gRPC calls
// ============================================================

/** One metadata (header or trailer) line. Binary values are base64 text. */
export interface GrpcPair {
  name: string;
  value: string;
}

export interface GrpcStatus {
  /** Canonical gRPC code, 0 is OK. */
  code: number;
  codeName: string;
  message: string;
}

export interface GrpcUnaryResponse {
  headers: GrpcPair[];
  trailers: GrpcPair[];
  /** Protobuf JSON of the reply. Absent when the call failed. */
  messageJson?: string | null;
  status: GrpcStatus;
  durationMs: number;
}

export interface GrpcMethodInfo {
  name: string;
  /** `package.Service/Method`, the value stored in a request. */
  fullName: string;
  methodType: GrpcMethodType;
  inputType: string;
  outputType: string;
}

export interface GrpcServiceInfo {
  name: string;
  methods: GrpcMethodInfo[];
}

/** What the gRPC tab sends. `request` is the editor state, which may be unsaved. */
export interface GrpcExecuteInput {
  collection?: string;
  request: GrpcRequest;
  /** The message to send. Omitted means the selected saved message. */
  message?: string;
  environmentName?: string;
  globalEnvName?: string;
  requestPath?: string;
  /** Deadline in milliseconds. 0 or absent means none. */
  timeoutMs?: number;
}

export const grpcUnaryCall = (input: GrpcExecuteInput) =>
  invoke<GrpcUnaryResponse>('grpc_unary_call', { input });

/** Opens a streaming call and returns its session id. Messages arrive as events. */
export const grpcStartSession = (input: GrpcExecuteInput) =>
  invoke<string>('grpc_start_session', { input });

export const grpcSendMessage = (sessionId: string, message: string) =>
  invoke<void>('grpc_send_message', { sessionId, message });

/** Ends the request side of a streaming call (half-close). */
export const grpcEndRequests = (sessionId: string) =>
  invoke<void>('grpc_end_requests', { sessionId });

export const grpcCancelSession = (sessionId: string) =>
  invoke<void>('grpc_cancel_session', { sessionId });

/** Lists services from the request's .proto file, or from server reflection when it has none. */
export const grpcListServices = (input: GrpcExecuteInput, refresh: boolean) =>
  invoke<GrpcServiceInfo[]>('grpc_list_services', { input, refresh });

// Event fields stay snake_case on the wire, like the agent session events.
export interface GrpcSessionStartedEvent {
  type: 'grpcSessionStarted';
  session_id: string;
  method_type: string;
}

export interface GrpcSessionHeadersEvent {
  type: 'grpcSessionHeaders';
  session_id: string;
  headers: GrpcPair[];
}

export interface GrpcSessionMessageEvent {
  type: 'grpcSessionMessage';
  session_id: string;
  index: number;
  json: string;
}

export interface GrpcSessionFinishedEvent {
  type: 'grpcSessionFinished';
  session_id: string;
  code: number;
  code_name: string;
  message: string;
  trailers: GrpcPair[];
  duration_ms: number;
}

export const onGrpcSessionStarted = (
  handler: (event: GrpcSessionStartedEvent) => void,
): Promise<UnlistenFn> =>
  listen<GrpcSessionStartedEvent>('grpc-session-started', (e) => handler(e.payload));

export const onGrpcSessionHeaders = (
  handler: (event: GrpcSessionHeadersEvent) => void,
): Promise<UnlistenFn> =>
  listen<GrpcSessionHeadersEvent>('grpc-session-headers', (e) => handler(e.payload));

export const onGrpcSessionMessage = (
  handler: (event: GrpcSessionMessageEvent) => void,
): Promise<UnlistenFn> =>
  listen<GrpcSessionMessageEvent>('grpc-session-message', (e) => handler(e.payload));

export const onGrpcSessionFinished = (
  handler: (event: GrpcSessionFinishedEvent) => void,
): Promise<UnlistenFn> =>
  listen<GrpcSessionFinishedEvent>('grpc-session-finished', (e) => handler(e.payload));
```

Create `src/stores/grpc-store.ts`:

```ts
import { create } from 'zustand';
import {
  type GrpcPair,
  type GrpcStatus,
  type GrpcUnaryResponse,
  grpcCancelSession,
  onGrpcSessionFinished,
  onGrpcSessionHeaders,
  onGrpcSessionMessage,
  onGrpcSessionStarted,
} from '@/lib/tauri-api';

/** A stream that runs for hours must not grow the log without limit. */
export const MAX_GRPC_LOG_ENTRIES = 1000;

export type GrpcLogDirection = 'in' | 'out';

export interface GrpcLogEntry {
  id: number;
  direction: GrpcLogDirection;
  /** Protobuf JSON text. */
  json: string;
  at: number;
}

export interface GrpcSessionView {
  id: string;
  methodType: string;
  status: 'running' | 'finished';
  headers: GrpcPair[];
  log: GrpcLogEntry[];
  nextLogId: number;
  finished?: { status: GrpcStatus; trailers: GrpcPair[]; durationMs: number };
}

export interface GrpcUnaryView {
  status: 'sending' | 'done' | 'error';
  response?: GrpcUnaryResponse;
  error?: string;
}

interface GrpcStoreState {
  /** Every session the backend has told us about, by session id. */
  sessions: Record<string, GrpcSessionView>;
  /** The session a tab started most recently. */
  sessionByTab: Record<string, string>;
  unaryByTab: Record<string, GrpcUnaryView>;
  attachSession: (tabId: string, sessionId: string) => void;
  recordOutbound: (sessionId: string, json: string) => void;
  setUnary: (tabId: string, view: GrpcUnaryView | undefined) => void;
  cancelTabSession: (tabId: string) => Promise<void>;
  /** Forgets everything about a closed tab. Cancels its session first. */
  dropTab: (tabId: string) => Promise<void>;
}

function newSession(id: string): GrpcSessionView {
  return { id, methodType: '', status: 'running', headers: [], log: [], nextLogId: 0 };
}

function appended(
  session: GrpcSessionView,
  direction: GrpcLogDirection,
  json: string,
): GrpcSessionView {
  const entry: GrpcLogEntry = { id: session.nextLogId, direction, json, at: Date.now() };
  const log = [...session.log, entry];
  return {
    ...session,
    log: log.length > MAX_GRPC_LOG_ENTRIES ? log.slice(log.length - MAX_GRPC_LOG_ENTRIES) : log,
    nextLogId: session.nextLogId + 1,
  };
}

export const useGrpcStore = create<GrpcStoreState>()((set, get) => ({
  sessions: {},
  sessionByTab: {},
  unaryByTab: {},

  attachSession(tabId, sessionId) {
    set((state) => {
      const previous = state.sessionByTab[tabId];
      const sessions = { ...state.sessions };
      // The tab shows one session at a time, so the older one can go.
      if (previous && previous !== sessionId && sessions[previous]?.status === 'finished') {
        delete sessions[previous];
      }
      // The call is already running, whether or not an event has arrived for it yet.
      sessions[sessionId] = sessions[sessionId] ?? newSession(sessionId);
      return { sessions, sessionByTab: { ...state.sessionByTab, [tabId]: sessionId } };
    });
  },

  recordOutbound(sessionId, json) {
    set((state) => {
      const session = state.sessions[sessionId] ?? newSession(sessionId);
      return { sessions: { ...state.sessions, [sessionId]: appended(session, 'out', json) } };
    });
  },

  setUnary(tabId, view) {
    set((state) => {
      const unaryByTab = { ...state.unaryByTab };
      if (view) unaryByTab[tabId] = view;
      else delete unaryByTab[tabId];
      return { unaryByTab };
    });
  },

  async cancelTabSession(tabId) {
    const id = get().sessionByTab[tabId];
    if (!id || get().sessions[id]?.status === 'finished') return;
    try {
      await grpcCancelSession(id);
    } catch (err) {
      // The call may have finished on its own a moment ago. Nothing is left to cancel.
      console.warn('[grpc] cancel failed:', err);
    }
  },

  async dropTab(tabId) {
    await get().cancelTabSession(tabId);
    set((state) => {
      const id = state.sessionByTab[tabId];
      const sessions = { ...state.sessions };
      if (id) delete sessions[id];
      const sessionByTab = { ...state.sessionByTab };
      delete sessionByTab[tabId];
      const unaryByTab = { ...state.unaryByTab };
      delete unaryByTab[tabId];
      return { sessions, sessionByTab, unaryByTab };
    });
  },
}));

function update(sessionId: string, change: (session: GrpcSessionView) => GrpcSessionView) {
  useGrpcStore.setState((state) => ({
    sessions: {
      ...state.sessions,
      [sessionId]: change(state.sessions[sessionId] ?? newSession(sessionId)),
    },
  }));
}

let listening: Promise<void> | null = null;

/**
 * Subscribes to the session events once for the life of the app. Events can arrive
 * before the command that started the session has returned, so a session is created
 * by whichever event reaches the store first.
 */
export function ensureGrpcListeners(): Promise<void> {
  if (!listening) {
    listening = Promise.all([
      onGrpcSessionStarted((e) =>
        update(e.session_id, (s) => ({ ...s, methodType: e.method_type })),
      ),
      onGrpcSessionHeaders((e) => update(e.session_id, (s) => ({ ...s, headers: e.headers }))),
      onGrpcSessionMessage((e) => update(e.session_id, (s) => appended(s, 'in', e.json))),
      onGrpcSessionFinished((e) =>
        update(e.session_id, (s) => ({
          ...s,
          status: 'finished',
          finished: {
            status: { code: e.code, codeName: e.code_name, message: e.message },
            trailers: e.trailers,
            durationMs: e.duration_ms,
          },
        })),
      ),
    ])
      .then(() => undefined)
      .catch((err) => {
        listening = null;
        console.error('[grpc] could not listen for session events:', err);
      });
  }
  return listening;
}
```

`attachSession` creates the session as running at once. The start command returns only after the backend has opened the call, and the first event can arrive before or after that, so the Cancel button must not wait for an event.

In `src/stores/pane-store.ts`, import the store (`import { useGrpcStore } from '@/stores/grpc-store';` after the `flow-auth-store` import) and make the two session helpers cancel a gRPC tab's call. The close paths already call them:

```ts
function endSessionIfActive(tab: Tab): void {
  // A running gRPC stream would otherwise keep a connection open with no tab to show it.
  if (isRequestTab(tab) && tab.request.requestType === 'grpc') {
    void useGrpcStore.getState().dropTab(tab.id);
  }
  if (isRequestTab(tab) && tab.agentSession?.status === 'active') {
    Promise.resolve(endAgentSession(tab.agentSession.sessionId)).catch((err) => {
      console.error('[pane-store] failed to end agent session', err);
    });
  }
}

// Ends every active agent session among tabs that are about to be discarded.
// The same session can appear twice (a live tab plus a stale snapshot copy),
// so each session id is ended only once.
function endActiveSessions(tabs: Tab[]): void {
  const seen = new Set<string>();
  for (const tab of tabs) {
    if (isRequestTab(tab) && tab.request.requestType === 'grpc') {
      endSessionIfActive(tab);
      continue;
    }
    if (!isRequestTab(tab) || tab.agentSession?.status !== 'active') continue;
    if (seen.has(tab.agentSession.sessionId)) continue;
    seen.add(tab.agentSession.sessionId);
    endSessionIfActive(tab);
  }
}
```

- [ ] **Step 5: Implement the variable context hook and the components**

Create `src/hooks/useGrpcVariableContext.ts`. It gives the editors the same scope-aware variable map `RequestPanel` builds, and gives the call the environment names:

```ts
import { useEffect, useMemo, useState } from 'react';
import {
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
} from '@/lib/queries/environment-queries';
import {
  type CollectionVariable,
  getCollectionSettings,
  getFolderVariables,
  getRequestVariables,
} from '@/lib/tauri-api';
import { buildScopedContext, type VariableScopeEntry } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';

export interface GrpcVariableScope {
  /** Scope-aware variables for the editors, so `{{name}}` highlights like it does for HTTP. */
  variableContext: Map<string, VariableScopeEntry>;
  /** Sent with a call so the backend resolves the same environment. */
  environmentName?: string;
  globalEnvName?: string;
}

/**
 * The variable scopes a gRPC tab sees: process, global and active environment,
 * the collection, the folder chain and the request itself.
 */
export function useGrpcVariableContext(
  source: { collection: string; path: string } | undefined,
): GrpcVariableScope {
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const { data: environments = [] } = useEnvironments(activeCollection);
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = {} } = useProcessEnvVars();

  const [collectionVars, setCollectionVars] = useState<CollectionVariable[]>([]);
  const [folderVars, setFolderVars] = useState<CollectionVariable[]>([]);
  const [requestVars, setRequestVars] = useState<CollectionVariable[]>([]);

  const collection = source?.collection;
  const path = source?.path;

  useEffect(() => {
    if (!collection) {
      setCollectionVars([]);
      return;
    }
    getCollectionSettings(collection)
      .then((s) => setCollectionVars(s.variables))
      .catch(() => setCollectionVars([]));
  }, [collection]);

  useEffect(() => {
    if (!collection || !path) {
      setFolderVars([]);
      setRequestVars([]);
      return;
    }
    const folderPath = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
    getRequestVariables(collection, path)
      .then(setRequestVars)
      .catch(() => setRequestVars([]));
    if (folderPath) {
      getFolderVariables(collection, folderPath)
        .then(setFolderVars)
        .catch(() => setFolderVars([]));
    } else {
      setFolderVars([]);
    }
  }, [collection, path]);

  const variableContext = useMemo(() => {
    const activeEnv = activeEnvId ? environments.find((e) => e.name === activeEnvId) : undefined;
    const envVars: Record<string, string> = {};
    if (activeEnv) for (const v of activeEnv.variables) if (v.enabled) envVars[v.key] = v.value;
    const globalVars: Record<string, string> = globalEnv
      ? Object.fromEntries(
          globalEnv.variables.filter((v) => v.enabled).map((v) => [v.key, v.value]),
        )
      : {};
    return buildScopedContext({
      envVars,
      envLabel: activeEnvId ?? undefined,
      externalSecrets: activeEnv?.externalSecrets,
      globalVars,
      processEnvVars,
      collectionVars,
      folderVars,
      requestVars,
    });
  }, [
    activeEnvId,
    environments,
    globalEnv,
    processEnvVars,
    collectionVars,
    folderVars,
    requestVars,
  ]);

  return {
    variableContext,
    environmentName: activeEnvId ?? undefined,
    globalEnvName: globalEnvName ?? undefined,
  };
}
```

(`RequestPanel` builds the same map inline. A later change can make it use this hook. Not done here to keep `RequestPanel` out of this plan.)

Create `src/components/grpc/GrpcMethodPicker.tsx`:

```tsx
import { open } from '@tauri-apps/plugin-dialog';
import { FolderOpen, Loader2, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  type GrpcExecuteInput,
  type GrpcMethodInfo,
  type GrpcMethodType,
  type GrpcServiceInfo,
  grpcListServices,
} from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';

export const METHOD_TYPE_LABEL: Record<GrpcMethodType, string> = {
  unary: 'Unary',
  'client-streaming': 'Client stream',
  'server-streaming': 'Server stream',
  'bidi-streaming': 'Bidi stream',
};

interface GrpcMethodPickerProps {
  /** `package.Service/Method`, empty until one is picked. */
  method: string;
  protoFilePath: string;
  onProtoFilePathChange: (path: string) => void;
  onPick: (method: GrpcMethodInfo) => void;
  /** Builds the call input when the methods are listed, so it always reads the latest edits. */
  buildInput: () => GrpcExecuteInput;
  /** Changes whenever the proto path or the URL changes, so a stale list is dropped. */
  sourceKey: string;
  variableContext?: Map<string, VariableScopeEntry>;
}

/**
 * Picks the method to call. The list comes from the request's `.proto` file or, when no
 * file is set, from server reflection on the URL.
 */
export function GrpcMethodPicker({
  method,
  protoFilePath,
  onProtoFilePathChange,
  onPick,
  buildInput,
  sourceKey,
  variableContext,
}: GrpcMethodPickerProps) {
  const [services, setServices] = useState<GrpcServiceInfo[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  // A different file or URL means the loaded list may no longer apply.
  // biome-ignore lint/correctness/useExhaustiveDependencies: sourceKey is the trigger on purpose.
  useEffect(() => {
    setServices(null);
    setError('');
  }, [sourceKey]);

  const load = useCallback(
    async (refresh: boolean) => {
      setLoading(true);
      setError('');
      try {
        setServices(await grpcListServices(buildInput(), refresh));
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setLoading(false);
      }
    },
    [buildInput],
  );

  const handleBrowse = useCallback(async () => {
    const picked = await open({
      multiple: false,
      title: 'Select a .proto file',
      filters: [{ name: 'Protocol Buffers', extensions: ['proto'] }],
    });
    if (typeof picked === 'string') onProtoFilePathChange(picked);
  }, [onProtoFilePathChange]);

  const handleValueChange = (fullName: string) => {
    const found = services?.flatMap((s) => s.methods).find((m) => m.fullName === fullName);
    if (found) onPick(found);
  };

  return (
    <div className='flex flex-col gap-2'>
      <div className='flex items-center gap-2'>
        <div className='flex-1 min-w-0'>
          <SingleLineEditor
            aria-label='Proto file path'
            placeholder='Path to a .proto file, or leave empty to use server reflection'
            value={protoFilePath}
            onChange={onProtoFilePathChange}
            variableContext={variableContext}
          />
        </div>
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='h-8 px-2'
          aria-label='Browse for a proto file'
          onClick={() => void handleBrowse()}
        >
          <FolderOpen className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      </div>
      <div className='flex items-center gap-2'>
        <Select
          value={method}
          onValueChange={handleValueChange}
          onOpenChange={(isOpen) => {
            if (isOpen && services === null && !loading) void load(false);
          }}
        >
          <SelectTrigger className='h-8 flex-1 font-mono text-xs' aria-label='Method'>
            <SelectValue placeholder='Choose a method'>{method || undefined}</SelectValue>
          </SelectTrigger>
          <SelectContent>
            {loading && (
              <div className='flex items-center gap-2 px-2 py-1.5 text-xs text-muted-foreground'>
                <Loader2 className='h-3 w-3 animate-spin' aria-hidden='true' /> Loading methods
              </div>
            )}
            {(services ?? []).map((service) => (
              <SelectGroup key={service.name}>
                <SelectLabel className='font-mono text-xs'>{service.name}</SelectLabel>
                {service.methods.map((m) => (
                  <SelectItem key={m.fullName} value={m.fullName}>
                    <span className='flex items-center gap-2'>
                      <span className='font-mono text-xs'>{m.name}</span>
                      <Badge variant='outline' className='text-[10px] px-1.5 py-0'>
                        {METHOD_TYPE_LABEL[m.methodType]}
                      </Badge>
                    </span>
                  </SelectItem>
                ))}
              </SelectGroup>
            ))}
            {services !== null && services.length === 0 && !loading && (
              <div className='px-2 py-1.5 text-xs text-muted-foreground'>No services found</div>
            )}
          </SelectContent>
        </Select>
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='h-8 px-2'
          aria-label='Reload methods'
          disabled={loading}
          onClick={() => void load(true)}
        >
          <RefreshCw className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      </div>
      {error && (
        <p role='alert' className='text-xs text-destructive'>
          {error}
        </p>
      )}
      {!protoFilePath.trim() && !error && (
        <p className='text-xs text-muted-foreground'>
          No .proto file set. Methods come from server reflection on the URL.
        </p>
      )}
    </div>
  );
}
```

Create `src/components/grpc/GrpcMessageEditor.tsx`:

```tsx
import { Plus, X } from 'lucide-react';
import { lazy, Suspense } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { VariableScopeEntry } from '@/lib/url-variables';
import type { GrpcMessageState } from '@/types/pane-types';

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

interface GrpcMessageEditorProps {
  messages: GrpcMessageState[];
  active: number;
  onSelect: (index: number) => void;
  onChange: (index: number, patch: Partial<Pick<GrpcMessageState, 'title' | 'content'>>) => void;
  onAdd: () => void;
  onRemove: (index: number) => void;
  variableContext?: Map<string, VariableScopeEntry>;
}

/** Edits the saved messages of a request, one at a time, as protobuf JSON. */
export function GrpcMessageEditor({
  messages,
  active,
  onSelect,
  onChange,
  onAdd,
  onRemove,
  variableContext,
}: GrpcMessageEditorProps) {
  const current = messages[active];
  return (
    <div className='flex h-full min-h-0 flex-col gap-2'>
      <div className='flex flex-wrap items-center gap-1'>
        {messages.map((m, i) => (
          <Button
            key={m.id}
            type='button'
            size='sm'
            variant={i === active ? 'secondary' : 'ghost'}
            className='h-7 px-2 text-xs'
            aria-pressed={i === active}
            onClick={() => onSelect(i)}
          >
            {m.title || `Message ${i + 1}`}
          </Button>
        ))}
        <Button
          type='button'
          size='sm'
          variant='ghost'
          className='h-7 px-2'
          aria-label='Add message'
          onClick={onAdd}
        >
          <Plus className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
        {messages.length > 1 && (
          <Button
            type='button'
            size='sm'
            variant='ghost'
            className='h-7 px-2'
            aria-label='Remove message'
            onClick={() => onRemove(active)}
          >
            <X className='h-3.5 w-3.5' aria-hidden='true' />
          </Button>
        )}
        <Input
          aria-label='Message title'
          placeholder='Title'
          value={current?.title ?? ''}
          onChange={(e) => onChange(active, { title: e.target.value })}
          className='ml-auto h-7 w-40 text-xs'
        />
      </div>
      <div className='min-h-[140px] flex-1'>
        {current && (
          <Suspense fallback={<EditorSkeleton />}>
            <MonacoWrapper
              value={current.content}
              onChange={(value) => onChange(active, { content: value })}
              language='json'
              height='100%'
              variableContext={variableContext}
            />
          </Suspense>
        )}
      </div>
    </div>
  );
}
```

Create `src/components/grpc/GrpcResponseView.tsx`:

```tsx
import { ArrowDown, ArrowUp, Loader2 } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import type { GrpcPair, GrpcStatus } from '@/lib/tauri-api';
import type { GrpcSessionView, GrpcUnaryView } from '@/stores/grpc-store';

interface GrpcResponseViewProps {
  unary?: GrpcUnaryView;
  session?: GrpcSessionView;
}

function StatusBadge({ status, durationMs }: { status: GrpcStatus; durationMs: number }) {
  return (
    <div className='flex items-center gap-2 text-xs'>
      <Badge variant={status.code === 0 ? 'secondary' : 'destructive'}>
        {status.code} {status.codeName}
      </Badge>
      <span className='text-muted-foreground'>{durationMs} ms</span>
      {status.message && <span className='truncate text-muted-foreground'>{status.message}</span>}
    </div>
  );
}

function PairTable({ pairs, empty }: { pairs: GrpcPair[]; empty: string }) {
  if (pairs.length === 0) return <p className='p-3 text-xs text-muted-foreground'>{empty}</p>;
  return (
    <dl className='grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 p-3 font-mono text-xs'>
      {pairs.map((p, i) => (
        // Metadata may repeat a name, so the position is part of the key.
        // biome-ignore lint/suspicious/noArrayIndexKey: names are not unique.
        <div key={`${p.name}-${i}`} className='contents'>
          <dt className='text-muted-foreground'>{p.name}</dt>
          <dd className='break-all'>{p.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function Json({ text }: { text: string }) {
  return <pre className='whitespace-pre-wrap break-all font-mono text-xs'>{text}</pre>;
}

/** Shows the result of a unary call, or the live log of a streaming call. */
export function GrpcResponseView({ unary, session }: GrpcResponseViewProps) {
  if (session) {
    const done = session.finished;
    return (
      <div className='flex h-full min-h-0 flex-col gap-2'>
        <div className='flex items-center gap-2'>
          {done ? (
            <StatusBadge status={done.status} durationMs={done.durationMs} />
          ) : (
            <span className='flex items-center gap-1.5 text-xs text-muted-foreground'>
              <Loader2 className='h-3 w-3 animate-spin' aria-hidden='true' /> Streaming
            </span>
          )}
        </div>
        <Tabs defaultValue='messages' className='flex min-h-0 flex-1 flex-col'>
          <TabsList className='h-8 self-start'>
            <TabsTrigger value='messages' className='text-xs'>
              Messages ({session.log.length})
            </TabsTrigger>
            <TabsTrigger value='headers' className='text-xs'>
              Headers
            </TabsTrigger>
            <TabsTrigger value='trailers' className='text-xs'>
              Trailers
            </TabsTrigger>
          </TabsList>
          <TabsContent value='messages' className='min-h-0 flex-1'>
            <ScrollArea className='h-full'>
              <ol aria-label='Message log' className='flex flex-col gap-2 p-2'>
                {session.log.map((entry) => (
                  <li key={entry.id} className='rounded-md border border-border/60 p-2'>
                    <div className='mb-1 flex items-center gap-1.5 text-[10px] uppercase text-muted-foreground'>
                      {entry.direction === 'out' ? (
                        <ArrowUp className='h-3 w-3' aria-label='Sent' />
                      ) : (
                        <ArrowDown className='h-3 w-3' aria-label='Received' />
                      )}
                      {entry.direction === 'out' ? 'Sent' : 'Received'}
                      <span>{new Date(entry.at).toLocaleTimeString()}</span>
                    </div>
                    <Json text={entry.json} />
                  </li>
                ))}
                {session.log.length === 0 && (
                  <p className='text-xs text-muted-foreground'>No messages yet.</p>
                )}
              </ol>
            </ScrollArea>
          </TabsContent>
          <TabsContent value='headers'>
            <PairTable pairs={session.headers} empty='No headers yet.' />
          </TabsContent>
          <TabsContent value='trailers'>
            <PairTable pairs={done?.trailers ?? []} empty='Trailers arrive when the call ends.' />
          </TabsContent>
        </Tabs>
      </div>
    );
  }

  if (!unary) {
    return <p className='p-3 text-xs text-muted-foreground'>Send the request to see the reply.</p>;
  }
  if (unary.status === 'sending') {
    return (
      <p className='flex items-center gap-2 p-3 text-xs text-muted-foreground'>
        <Loader2 className='h-3 w-3 animate-spin' aria-hidden='true' /> Sending
      </p>
    );
  }
  if (unary.status === 'error' || !unary.response) {
    return (
      <p role='alert' className='p-3 text-xs text-destructive'>
        {unary.error ?? 'The call failed.'}
      </p>
    );
  }
  const r = unary.response;
  return (
    <div className='flex h-full min-h-0 flex-col gap-2'>
      <StatusBadge status={r.status} durationMs={r.durationMs} />
      <Tabs defaultValue='response' className='flex min-h-0 flex-1 flex-col'>
        <TabsList className='h-8 self-start'>
          <TabsTrigger value='response' className='text-xs'>
            Response
          </TabsTrigger>
          <TabsTrigger value='headers' className='text-xs'>
            Headers
          </TabsTrigger>
          <TabsTrigger value='trailers' className='text-xs'>
            Trailers
          </TabsTrigger>
        </TabsList>
        <TabsContent value='response' className='min-h-0 flex-1'>
          <ScrollArea className='h-full'>
            <div className='p-2'>
              {r.messageJson ? (
                <Json text={r.messageJson} />
              ) : (
                <p className='text-xs text-muted-foreground'>The call returned no message.</p>
              )}
            </div>
          </ScrollArea>
        </TabsContent>
        <TabsContent value='headers'>
          <PairTable pairs={r.headers} empty='No headers.' />
        </TabsContent>
        <TabsContent value='trailers'>
          <PairTable pairs={r.trailers} empty='No trailers.' />
        </TabsContent>
      </Tabs>
    </div>
  );
}
```

Create `src/components/grpc/GrpcPanel.tsx`:

```tsx
import { Check, Play, Send, Square } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { AuthEditor } from '@/components/request/AuthEditor';
import { KeyValueEditor } from '@/components/request/KeyValueEditor';
import { RequestVariablesPanel } from '@/components/request/RequestVariablesPanel';
import { SaveRequestButton } from '@/components/request/SaveRequestButton';
import { SaveToCollectionDialog } from '@/components/request/SaveToCollectionDialog';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useGrpcVariableContext } from '@/hooks/useGrpcVariableContext';
import { createDefaultGrpcState, DEFAULT_GRPC_MESSAGE } from '@/lib/pane-utils';
import { toApiGrpcRequest } from '@/lib/request-save-mapper';
import {
  type GrpcExecuteInput,
  type GrpcMethodInfo,
  grpcEndRequests,
  grpcSendMessage,
  grpcStartSession,
  grpcUnaryCall,
} from '@/lib/tauri-api';
import { ensureGrpcListeners, useGrpcStore } from '@/stores/grpc-store';
import { usePaneStore } from '@/stores/pane-store';
import type { GrpcState, RequestTab } from '@/types/pane-types';
import { GrpcMessageEditor } from './GrpcMessageEditor';
import { GrpcMethodPicker } from './GrpcMethodPicker';
import { GrpcResponseView } from './GrpcResponseView';

/** A unary call that never answers would hang the tab, since it has no Cancel. */
const UNARY_DEADLINE_MS = 30_000;

type Section = 'message' | 'metadata' | 'auth' | 'variables';

interface GrpcPanelProps {
  tab: RequestTab;
  groupId: string;
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** The request tab of a gRPC request: method, message, metadata, auth, and the reply or stream. */
export function GrpcPanel({ tab, groupId }: GrpcPanelProps) {
  const request = tab.request;
  const grpc = request.grpc ?? createDefaultGrpcState();
  const updateRequest = usePaneStore((s) => s.updateRequest);

  const sessionId = useGrpcStore((s) => s.sessionByTab[tab.id]);
  const session = useGrpcStore((s) => (sessionId ? s.sessions[sessionId] : undefined));
  const unary = useGrpcStore((s) => s.unaryByTab[tab.id]);
  const attachSession = useGrpcStore((s) => s.attachSession);
  const recordOutbound = useGrpcStore((s) => s.recordOutbound);
  const setUnary = useGrpcStore((s) => s.setUnary);
  const cancelTabSession = useGrpcStore((s) => s.cancelTabSession);

  const scope = useGrpcVariableContext(tab.source);
  const [section, setSection] = useState<Section>('message');
  const [saveOpen, setSaveOpen] = useState(false);
  const [error, setError] = useState('');
  const [endedFor, setEndedFor] = useState<string | null>(null);

  useEffect(() => {
    void ensureGrpcListeners();
  }, []);

  const patchGrpc = useCallback(
    (patch: Partial<GrpcState>) => updateRequest(tab.id, { grpc: { ...grpc, ...patch } }),
    [updateRequest, tab.id, grpc],
  );

  const buildInput = useCallback(
    (): GrpcExecuteInput => ({
      collection: tab.source?.collection,
      request: toApiGrpcRequest(tab.id, tab.title, request),
      message: grpc.messages[grpc.activeMessage]?.content,
      environmentName: scope.environmentName,
      globalEnvName: scope.globalEnvName,
      requestPath: tab.source?.path,
    }),
    [tab.id, tab.title, tab.source, request, grpc.messages, grpc.activeMessage, scope],
  );

  const running = session?.status === 'running';
  const sending = unary?.status === 'sending';
  const streamsRequests =
    grpc.methodType === 'client-streaming' || grpc.methodType === 'bidi-streaming';
  const requestsEnded = session !== undefined && endedFor === session.id;

  const handlePick = (method: GrpcMethodInfo) =>
    patchGrpc({ method: method.fullName, methodType: method.methodType });

  const handleStart = async () => {
    setError('');
    if (!grpc.method) {
      setError('Choose a method first.');
      return;
    }
    if (grpc.methodType === 'unary') {
      setUnary(tab.id, { status: 'sending' });
      try {
        const response = await grpcUnaryCall({ ...buildInput(), timeoutMs: UNARY_DEADLINE_MS });
        setUnary(tab.id, { status: 'done', response });
      } catch (err) {
        setUnary(tab.id, { status: 'error', error: errorText(err) });
      }
      return;
    }
    setUnary(tab.id, undefined);
    try {
      attachSession(tab.id, await grpcStartSession(buildInput()));
    } catch (err) {
      setError(errorText(err));
    }
  };

  const handleSendMessage = async () => {
    if (!session) return;
    setError('');
    const content = grpc.messages[grpc.activeMessage]?.content ?? DEFAULT_GRPC_MESSAGE;
    try {
      await grpcSendMessage(session.id, content);
      recordOutbound(session.id, content);
    } catch (err) {
      setError(errorText(err));
    }
  };

  const handleEndRequests = async () => {
    if (!session) return;
    setError('');
    try {
      await grpcEndRequests(session.id);
      setEndedFor(session.id);
    } catch (err) {
      setError(errorText(err));
    }
  };

  const changeMessage = (index: number, patch: { title?: string; content?: string }) =>
    patchGrpc({ messages: grpc.messages.map((m, i) => (i === index ? { ...m, ...patch } : m)) });

  const addMessage = () =>
    patchGrpc({
      messages: [
        ...grpc.messages,
        { id: crypto.randomUUID(), title: '', content: DEFAULT_GRPC_MESSAGE },
      ],
      activeMessage: grpc.messages.length,
    });

  const removeMessage = (index: number) => {
    if (grpc.messages.length <= 1) return;
    const remaining = grpc.messages.filter((_, i) => i !== index);
    const next = index < grpc.activeMessage ? grpc.activeMessage - 1 : grpc.activeMessage;
    patchGrpc({
      messages: remaining,
      activeMessage: Math.max(0, Math.min(next, remaining.length - 1)),
    });
  };

  const startLabel = grpc.methodType === 'unary' ? 'Send' : 'Start';

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex items-center gap-2 border-b border-border/60 px-3 py-2'>
        <Badge variant='outline' className='shrink-0 text-teal-500'>
          gRPC
        </Badge>
        <div className='min-w-0 flex-1'>
          <SingleLineEditor
            aria-label='gRPC URL'
            placeholder='localhost:50051 or grpcs://host:443'
            value={request.url}
            onChange={(url) => updateRequest(tab.id, { url })}
            variableContext={scope.variableContext}
          />
        </div>
        {running ? (
          <Button
            size='sm'
            variant='destructive'
            className='h-8'
            onClick={() => void cancelTabSession(tab.id)}
          >
            <Square className='mr-1 h-3.5 w-3.5' aria-hidden='true' /> Cancel
          </Button>
        ) : (
          <Button size='sm' className='h-8' disabled={sending} onClick={() => void handleStart()}>
            {grpc.methodType === 'unary' ? (
              <Send className='mr-1 h-3.5 w-3.5' aria-hidden='true' />
            ) : (
              <Play className='mr-1 h-3.5 w-3.5' aria-hidden='true' />
            )}
            {startLabel}
          </Button>
        )}
        {!tab.source && (
          <>
            <Button size='sm' variant='outline' className='h-8' onClick={() => setSaveOpen(true)}>
              Save to Collection
            </Button>
            <SaveToCollectionDialog open={saveOpen} tab={tab} onClose={() => setSaveOpen(false)} />
          </>
        )}
        <SaveRequestButton tab={tab} groupId={groupId} />
      </div>

      <div className='border-b border-border/60 px-3 py-2'>
        <GrpcMethodPicker
          method={grpc.method}
          protoFilePath={grpc.protoFilePath}
          onProtoFilePathChange={(protoFilePath) => patchGrpc({ protoFilePath })}
          onPick={handlePick}
          buildInput={buildInput}
          sourceKey={`${grpc.protoFilePath}\n${request.url}`}
          variableContext={scope.variableContext}
        />
        {error && (
          <p role='alert' className='mt-2 text-xs text-destructive'>
            {error}
          </p>
        )}
      </div>

      <div className='flex min-h-0 flex-1 flex-col gap-2 p-3'>
        <Tabs
          value={section}
          onValueChange={(v) => setSection(v as Section)}
          className='flex min-h-0 flex-1 flex-col'
        >
          <div className='flex items-center justify-between gap-2'>
            <TabsList className='h-8'>
              <TabsTrigger value='message' className='text-xs'>
                Message
              </TabsTrigger>
              <TabsTrigger value='metadata' className='text-xs'>
                Metadata
              </TabsTrigger>
              <TabsTrigger value='auth' className='text-xs'>
                Auth
              </TabsTrigger>
              <TabsTrigger value='variables' className='text-xs'>
                Variables
              </TabsTrigger>
            </TabsList>
            {running && streamsRequests && (
              <div className='flex items-center gap-2'>
                <Button
                  size='sm'
                  variant='outline'
                  className='h-8'
                  disabled={requestsEnded}
                  onClick={() => void handleSendMessage()}
                >
                  <Send className='mr-1 h-3.5 w-3.5' aria-hidden='true' /> Send message
                </Button>
                <Button
                  size='sm'
                  variant='outline'
                  className='h-8'
                  disabled={requestsEnded}
                  onClick={() => void handleEndRequests()}
                >
                  <Check className='mr-1 h-3.5 w-3.5' aria-hidden='true' /> End requests
                </Button>
              </div>
            )}
          </div>
          <TabsContent value='message' className='min-h-0 flex-1'>
            <GrpcMessageEditor
              messages={grpc.messages}
              active={grpc.activeMessage}
              onSelect={(activeMessage) => patchGrpc({ activeMessage })}
              onChange={changeMessage}
              onAdd={addMessage}
              onRemove={removeMessage}
              variableContext={scope.variableContext}
            />
          </TabsContent>
          <TabsContent value='metadata'>
            <KeyValueEditor
              entries={request.headers}
              onChange={(headers) => updateRequest(tab.id, { headers })}
              keyPlaceholder='Metadata name'
              valuePlaceholder='Value'
              addLabel='Add Metadata'
              variableContext={scope.variableContext}
            />
          </TabsContent>
          <TabsContent value='auth'>
            <AuthEditor
              auth={request.auth}
              onChange={(auth) => updateRequest(tab.id, { auth })}
              variableContext={scope.variableContext}
              collection={tab.source?.collection}
              environmentName={scope.environmentName}
              requestPath={tab.source?.path}
            />
          </TabsContent>
          <TabsContent value='variables'>
            {tab.source ? (
              <RequestVariablesPanel
                collection={tab.source.collection}
                requestPath={tab.source.path}
              />
            ) : (
              <p className='p-3 text-xs text-muted-foreground'>
                Save the request to a collection to add request variables.
              </p>
            )}
          </TabsContent>
        </Tabs>
        <div className='min-h-[160px] flex-1 overflow-hidden rounded-md border border-border/60'>
          <GrpcResponseView unary={unary} session={session} />
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 6: Route gRPC tabs to the panel**

In `src/components/panes/EditorGroup.tsx`:

```diff
@@ -39,6 +39,7 @@
 );
 
 import { MousePointer2 } from 'lucide-react';
+import { GrpcPanel } from '@/components/grpc/GrpcPanel';
 import { RocketLaunch } from '@/components/illustrations';
 import { RequestPanel } from '@/components/request/RequestPanel';
 import {
```

```diff
@@ -211,6 +212,8 @@
             <Suspense fallback={<EditorSkeleton />}>
               <DiffViewer diffState={activeTab.diffState} />
             </Suspense>
+          ) : isRequestTab(activeTab) && activeTab.request.requestType === 'grpc' ? (
+            <GrpcPanel tab={activeTab} groupId={node.groupId} />
           ) : isRequestTab(activeTab) ? (
             <RequestPanel tab={activeTab} groupId={node.groupId} />
           ) : isGitTab(activeTab) ? (
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `yarn test grpc-store pane-store.grpc GrpcMethodPicker GrpcPanel pane-store`
Expected: PASS. The existing `pane-store` tests must still pass, because the two helpers only add a gRPC branch.

- [ ] **Step 8: Run the checks**

Run:
- `yarn tsc --noEmit`
- `yarn check`
- `yarn test grpc`
- `cargo check -j4 -p rocket`

Expected: PASS. If `yarn check` reports formatting in the new files, run `yarn biome check --write <files>` on them only.

- [ ] **Step 9: Manual check in the real app**

Run `yarn tauri dev` with a gRPC server at hand (for example the `greeter.proto` fixture from `crates/rocket-infra/test-fixtures/grpc`, or any server with reflection). Confirm:
1. New Request, type gRPC, a name and `localhost:50051`: a `gRPC` row appears and its tab opens in the gRPC panel. The file on disk is `grpc:`-shaped YAML.
2. With no proto path, opening the method list loads methods from reflection. Set a `.proto` path (Browse) and reload: the methods come from the file. Pick a unary method, type a JSON message and Send: the status, reply, headers and trailers show.
3. Pick a server-streaming method and Start: messages append live, the status shows when the stream ends. Pick a bidirectional method: Start, Send message twice, End requests, and see the closing status. Start another and press Cancel: `1 CANCELLED`.
4. Close a tab with a running stream, then confirm in the server log that the call ended.
5. Edit the URL, switch tabs and back: the edit is saved (auto-save), the file is still gRPC, rename the row from the sidebar and confirm the file stays gRPC.
6. Import a Bruno collection that has a gRPC request and a `.proto`: it appears with the `gRPC` badge and its method list loads from the copied proto.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add src/lib/tauri-api.ts src/stores src/hooks src/components/grpc src/components/panes/EditorGroup.tsx
```

Suggested subject: `feat(ui): add the gRPC request tab with streaming and reflection`.

---

## Known limits (state them in the PR description)

- Auth types other than none, inherit, bearer, basic and API key in a header fail at send time with a message that names the type. The auth editor still lists every type.
- Variables in a `.proto` path resolve, but the Browse button stores an absolute path. Absolute paths do not move with the collection. A relative path inside the collection does.
- Closing the app ends running streams, but a crash does not.
- The unary deadline is fixed at 30 seconds and not saved. Streams have no deadline.
- Plan 11's `CollectionsSidebar.handleDuplicate` does not duplicate gRPC requests. The menu item is hidden.
- gRPC items are not Collection Runner steps, cannot be dragged into a Flow and have no contract support.

## Decisions to confirm before merging

1. **Bruno YAML shape.** The Bruno-flavoured YAML for gRPC (as opposed to OpenCollection YAML) is inferred, not documented. Import one real Bruno 2.10+ YAML gRPC export and adjust `adapt_grpc` if its keys differ.
2. **TLS.** gRPC trusts the OS roots only (Plan 12). Do you want a per-request "skip verification" switch, an extra CA path, or client certificates reused from the environment? Each needs a field in the model, and the OpenCollection schema has no place for them in a gRPC request.
3. **Import paths.** A `.proto` import is searched in the proto file's own directory and the collection root. Bruno lets the user add more import paths. Add a collection-level setting if users need it.
4. **Unary deadline.** The tab uses a fixed 30 seconds and does not save it. The schema has no timeout for gRPC.
5. **Reflection credentials.** Reflection sends the request's metadata and auth, so a protected server can be listed. Confirm that is intended, since the results are cached per credential only in memory.
6. **Auth types.** OAuth2, Digest, AWS and the rest fail for gRPC. Say whether OAuth2 token reuse should come next.

---

## Next Plan

Series complete; run the final cross-protocol review and update docs/superpowers/specs/opencollection-spec-reference.md compliance notes.
