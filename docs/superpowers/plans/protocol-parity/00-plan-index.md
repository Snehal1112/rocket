# Protocol parity with Bruno, Plan Index

**Goal:** bring Rocket to Bruno parity for REST, SOAP, GraphQL, WebSocket and gRPC. The audit found REST working with gaps, SOAP working only as XML over HTTP, and GraphQL, gRPC and WebSocket persisted as opaque YAML with no execution, editor or import.

**Scope:** 13 plans, 38 tasks (max 3 per plan). Each plan ends with a **Next Plan** section. Run one plan at a time, chaining to the next when the current one finishes.

## Plan breakdown

| # | Plan | Tasks | Area | Depends on |
|---|---|---|---|---|
| 01 | [REST methods, SPARQL and OAuth 1.0 UI, param resolution](2026-10-05-protocol-parity-plan-01-rest-methods-auth-params.md) | 3 | shared, http, app, frontend | none |
| 02 | [Cookie jar, binary responses, SigV4 body signing](2026-10-05-protocol-parity-plan-02-rest-cookies-binary-sigv4.md) | 3 | infra, app, frontend | 01 |
| 03 | [Multipart files, settings and proxy, NTLM](2026-10-05-protocol-parity-plan-03-rest-multipart-settings-ntlm.md) | 3 | infra, frontend | 01, 02 |
| 04 | [SOAP via WSDL import](2026-10-05-protocol-parity-plan-04-soap-wsdl-import.md) | 3 | rocket-import, frontend | none |
| 05 | [GraphQL model, persistence, Bruno import, sidebar](2026-10-05-protocol-parity-plan-05-graphql-model-and-import.md) | 3 | collection, infra, import, frontend | none (foundation for 08, 11) |
| 06 | [GraphQL execution, editor, responses, runner](2026-10-05-protocol-parity-plan-06-graphql-editor-and-execution.md) | 3 | app, frontend | 05 |
| 07 | [GraphQL schema, docs explorer, autocomplete, query builder](2026-10-05-protocol-parity-plan-07-graphql-schema-and-builder.md) | 3 | app, frontend | 06 |
| 08 | [WebSocket backend](2026-10-05-protocol-parity-plan-08-websocket-backend.md) | 3 | collection, http, infra, app, tauri | 05 task 1 |
| 09 | [WebSocket UI and Bruno import](2026-10-05-protocol-parity-plan-09-websocket-ui-and-import.md) | 3 | import, frontend | 08 |
| 10 | [GraphQL subscriptions](2026-10-05-protocol-parity-plan-10-graphql-subscriptions.md) | 2 | app, frontend | 06, 08, 09 |
| 11 | [gRPC model, persistence, proto engine](2026-10-05-protocol-parity-plan-11-grpc-model-and-proto.md) | 3 | new `rocket-grpc` crate, collection, infra | 05 task 1 |
| 12 | [gRPC execution and streaming](2026-10-05-protocol-parity-plan-12-grpc-execution-and-streaming.md) | 3 | rocket-grpc, infra, app, tauri | 11 |
| 13 | [gRPC UI and Bruno import](2026-10-05-protocol-parity-plan-13-grpc-ui-and-import.md) | 3 | import, frontend | 12 |

Recommended order: 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13. Plans 04 and 05 are independent of the REST plans and can move earlier if wanted.

## Locked contract

Every plan is written against these decisions. If an implementer must deviate, update this index and every plan that mentions the name.

- **Typed items:** each protocol replaces `OpaqueItem` with a boxed typed variant: `CollectionItem::GraphQl(Box<GraphQlRequest>)`, `CollectionItem::WebSocket(Box<WebSocketRequest>)`, `CollectionItem::Grpc(Box<GrpcRequest>)`. The opaque fallback stays for unknown kinds.
- **Discriminator:** `RequestKind { Http, GraphQl, Grpc, WebSocket }` (serde lowercase) and `RequestSummary.kind`, defined in plan 05 in `rocket-collection`. The frontend uses `requestType: 'http'|'graphql'|'grpc'|'websocket'`. Plans 08 and 11 reuse these and add no second discriminator.
- **Repository:** `CollectionRepository` gains defaulted `get_*_request` / `save_*_request` methods per protocol plus `request_kind`, so existing test doubles keep compiling. `rename_request`, `update_request_docs` and the request-variable helpers become kind-aware.
- **Execution:** GraphQL reuses the HTTP executor. WebSocket and gRPC use a session model: a backend session registry keyed by a frontend-chosen session id, with Tauri commands to start, send and end, and Tauri events for inbound messages and status.
- **Persistence:** OpenCollection YAML in `crates/rocket-infra/src/oc/*.rs`, backward compatible. `uid` is added to the GraphQL, WebSocket and gRPC OC structs and listed in `KNOWN_DEFERRED`.
- **Create dialog:** plan 05 disables the gRPC and WebSocket options (they save an HTTP file under the wrong label today). Plans 09 and 13 re-enable them.

## Merge points between plans

Plans 05, 08 and 11 each edit these files, so rebase carefully when running them in sequence: `crates/rocket-collection/src/folder.rs`, `repository.rs`, `crates/rocket-infra/src/fs_collection/tree.rs`, `schema_shape_tests.rs`, `fs_collection/tests.rs`, `conversions/tests.rs`, `src-tauri/src/events.rs`, `tauri_event_bus.rs`, `src/components/request/CreateRequestDialog.tsx`, `RequestNode.tsx`, `EditorGroup.tsx`, `crates/rocket-import/src/importer.rs`.

Plans 02 task 3 and 03 task 3 edit `toApiAuth` and the shared auth type lists. Do not run them while the Flow auth work on the other PC is in flight.

## Rules baked into every plan

- `cargo` commands always use `-j4`; never `cargo test --workspace`.
- Commits go through the `dev-workflow-skills:1-git-commit` skill.
- Tasks touching collection, environment, http, import, workspace, infra, auth or variable resolution start with the step to read `docs/superpowers/specs/opencollection-spec-reference.md`.
- None of the code in these plans was compiled against the real workspace unless a plan says so. The gRPC plans were compiled in scratch workspaces. The Tauri wiring, the importer integration tests and the `CollectionService` changes were not.

## Decisions already made (2026-10-05)

- New `rocket-grpc` crate for the pure gRPC engine: approved.
- `HttpMethod` loses `Copy` (custom methods via `Custom(String)`): approved.
- New dependencies approved: `roxmltree` (WSDL), `tokio-tungstenite` (WebSocket), `md4` (NTLM), `graphql` and `graphql-language-service` (GraphQL editor), `tonic` and its proto crates (gRPC).
- Auth timing for plan 02 task 3 and plan 03 task 3: answer was "yes", which does not name a time. Still open.

## Open decisions for the user

**REST (plans 01-03)**
1. `HttpMethod` stops being `Copy` (custom methods need `Custom(String)`), so expect `.clone()` edits across the workspace.
2. CONNECT is sent like any request, not as a tunnel.
3. Cookie jar is on by default via `RequestOptions.use_cookie_jar` (load test turns it off); no cookie manager UI.
4. Binary responses over 32 MiB are flagged but not carried; scripts see an empty `res.body` for binary. "Save to file" needs `fs:allow-write-file` in `capabilities/default.json`.
5. Proxy is app-level (`~/.rocket-api/proxy.yml`), password in the OS keychain, no SOCKS, and OAuth2 token requests do not use it yet.
6. New dependencies: `md4 0.10`, and `mime_guess` (already in the lock file).
7. NTLM is NTLMv2 only, with no signing or sealing.
8. The AWS get-vanilla and NTLMv2 test vectors were typed from memory; recheck the vector first if a test fails.

**SOAP (plan 04)**
9. Approve `roxmltree 0.20` in `rocket-import` (versus `quick-xml`).
10. Local WSDL files only; remote URLs are reported as not fetched (no HTTP client in `rocket-import`, SSRF questions).
11. Folder layout `<service>/<port>/<operation>`, and the known limits: rpc/encoded gets a literal sample, `choice` uses its first alternative, XSD attributes, `xsd:any` and `xsd:group` are ignored.

**GraphQL (plans 05-07, 10)**
12. Top-level `uid` and `settings.verifySsl` break the OpenCollection `additionalProperties: false` schema (added to `KNOWN_DEFERRED`); `operationName` is not persisted.
13. A multi-operation document errors on send until the user picks one; the runner runs the first.
14. HTTP 200 with non-empty `errors[]` counts as a failed step.
15. Only OpenCollection-shaped GraphQL YAML is imported from Bruno YAML; GraphQL `auth` in YAML is reported unsupported.
16. Plan 07 adds `graphql` 16 and `graphql-language-service` 5 via Yarn and rejects `monaco-graphql`.
17. Subscriptions: a server with no subprotocol is assumed to speak `graphql-transport-ws`; no `graphql-sse`; the subscription URL override has no editor.

**WebSocket (plans 08-09)**
18. `tokio-tungstenite 0.30` with native-tls; a `wss://` handshake needs one manual check.
19. WebSocket connects fail on a failing vault binding even if unused (HTTP tolerates it).
20. OAuth 2, Digest, NTLM, WSSE and SigV4 return an explicit error on the handshake.
21. Bruno WebSocket shapes come from the docs page, not a real export; import one real file and fix fixtures if they differ.
22. Defaults: 30 s connect timeout, keep-alive off, base64 for binary, "skip TLS verification" session-only, Duplicate hidden for WebSocket and GraphQL rows.

**gRPC (plans 11-13)**
23. Confirm the new `rocket-grpc` crate (pure engine; only `rocket-infra` does I/O).
24. gRPC TLS uses rustls with `ring` and OS roots (tonic has no native-tls); no skip-verification and no mutual TLS.
25. Bruno gRPC YAML shape is undocumented; test against a real Bruno 2.10+ export.
26. Proto imports are searched in the proto's directory plus the collection root only; the Browse button stores an absolute path.
27. Unary deadline is a fixed 30 s, not saved. Auth supports bearer, basic, API key header, none and inherit.
28. gRPC has four `grpc-session-*` events while WebSocket has two `ws:*`; say if you want them harmonised.
29. Plan 11 corrects spec reference 2.5: gRPC `auth` lives inside the `grpc` block, and `runtime` allows only variables, scripts and assertions.

## Audit corrections found while planning

- `.bru` import does not skip GraphQL, WebSocket or gRPC: they import as empty HTTP requests and the query is silently lost. Plans 05, 09 and 13 fix this.
- The sidebar hid these items at the summary loader (`tree.rs` returning `Ok(None)`), not only in the opaque guards.
- The create dialog saved an HTTP file labelled gRPC or WebSocket.
- SigV4 never reached the backend from the UI, and OAuth1 had no editor.
- The UI could not send multipart file parts or a binary body at all.
- Any unknown HTTP method in a file silently became GET.
- PDF preview is not feasible in the app (no PDF viewer, WebKitGTK iframe limits, CSP); plan 02 previews images and saves everything else to a file.
