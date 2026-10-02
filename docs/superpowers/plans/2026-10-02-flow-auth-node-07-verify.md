# Flow Auth Node — Plan 7: End-to-end tests, security review, docs, final verification

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 7 of 7 (last).** Previous plan: `docs/superpowers/plans/2026-10-02-flow-auth-node-06-preflight.md` (must be merged and green).
**Next plan to run: none.** After this plan, the feature is done. Open follow-ups are listed at the end.
**Recommended model: Opus** (security review and final gate).

**Goal:** Prove the feature end to end (no token ever leaks, the credential is fetched once and reused, an unauthenticated interactive node sends nothing), review it against the spec's security requirements, document it, and run the full verification.

**Architecture:** No new production code except fixes the review finds. Task 1 adds backend end-to-end tests with an echoing HTTP double. Task 2 is a checklist-driven security review. Task 3 updates crate docs and runs the project's verification, then a manual smoke test in the real app.

**Tech Stack:** Rust (tokio tests), Vitest, `verify-rocket` skill, `yarn tauri dev`.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`.
- Tokens are never written to disk, never cached beyond the run, and never appear in step output, debug/exchange records, events, logs or `Debug` output.
- A missing interactive token fails the run before the first event and sends no request.
- Any Critical or High security finding is fixed in this plan with a failing test first; Medium and Low findings are recorded in the spec.
- No `unwrap()` in production Rust paths; never shell out to `git`.
- Conventional commits.
- Before each Rust commit run `cargo fmt`; before each frontend commit run `yarn lint` then `yarn check`.

## File Structure

| File | Change |
|---|---|
| `crates/rocket-app/src/flow_execution_service.rs` | End-to-end tests only |
| `crates/rocket-flow/CLAUDE.md`, `crates/rocket-app/CLAUDE.md` | Document the Auth node |
| `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md` | Status, security review notes |
| Any file a review finding touches | Fix with a test |

---

### Task 1: End-to-end backend tests

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (tests only)

**Interfaces:**
- Consumes (all from earlier plans): `FakeFetcher`, `SharedFetcher`, `client_credentials()`, `authorization_code()` (`crate::flow_auth::test_support`); `recording_http_exec`, `auth_and_request_flow`, `service_with_saved_request`, `run_input`, `saved_flow_node` (test helpers added in Plans 2–3 and earlier); `RecordingExecutor::sent_auths()`; `FlowExecutionService::{run, run_with_auth, with_token_fetcher}`.
- Produces: the regression tests below. No production API.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Add the tests**

Append inside `mod tests` of `flow_execution_service.rs`, after the Plan 3 tests:

```rust
    /// Echoes a Bearer token into the response body: the worst case for redaction.
    struct EchoTokenExecutor;

    #[async_trait]
    impl HttpExecutor for EchoTokenExecutor {
        async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
            let body = match &req.auth {
                rocket_shared::types::Auth::Bearer { token } => {
                    format!("{{\"echo\":\"{token}\"}}")
                }
                _ => "{}".to_string(),
            };
            Ok(HttpResponse {
                size_bytes: body.len(),
                body,
                status: 200,
                status_text: "OK".to_string(),
                headers: Vec::new(),
                duration_ms: 1,
                ttfb_ms: 1,
            })
        }
    }

    fn client_credentials_auth_node(id: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Auth {
                label: "Sign in".to_string(),
                auth: rocket_shared::types::Auth::OAuth2(Box::new(
                    crate::flow_auth::test_support::client_credentials(),
                )),
                apply_to_inherit: true,
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn inherit_saved_request() -> Request {
        let mut saved = Request::new("Get", HttpMethod::Get, "https://api.example.com/x");
        saved.runtime_auth = Some(rocket_shared::types::Auth::Inherit);
        saved
    }

    #[tokio::test]
    async fn a_fetched_token_never_appears_in_any_run_output() {
        use crate::flow_auth::test_support::{FakeFetcher, SharedFetcher};

        let token = "fetched-token-999999";
        let fetcher = FakeFetcher::ok(token);
        let publisher = RecordingPublisher::new();
        let mut request_node = saved_flow_node("r", "req.yml");
        if let FlowNodeKind::Request { debug, .. } = &mut request_node.kind {
            *debug = true;
        }
        let flow = Flow {
            name: "auth-req".to_string(),
            nodes: vec![client_credentials_auth_node("a"), request_node],
            edges: Vec::new(),
            callback_host: None,
        };
        let service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", flow)),
            Box::new(FakeCollectionRepo::new().with_request(
                "my-api",
                "req.yml",
                inherit_saved_request(),
            )),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
        )
        .with_token_fetcher(Box::new(SharedFetcher(Arc::clone(&fetcher))));
        let exec = RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(EchoTokenExecutor),
            Box::new(NullHistoryRepo),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let summary = service
            .run(&exec, run_input("auth-req"))
            .await
            .expect("run must succeed");

        let step = summary
            .steps
            .iter()
            .find(|s| s.node_id == "r")
            .expect("request step recorded");
        assert_eq!(step.status, FlowNodeStatus::Success);
        // The token reached the HTTP layer, which echoed it into the response.
        // The record of that response must show it masked.
        let debug = step.debug_request.as_ref().expect("debug record");
        let response = debug.response.as_ref().expect("response in record");
        assert!(
            response.body.contains(crate::redaction::REDACTED),
            "got: {}",
            response.body
        );
        let everything = format!(
            "{}\n{:?}",
            serde_json::to_string(&summary).expect("serialize summary"),
            publisher.events()
        );
        assert!(
            !everything.contains(token),
            "the token leaked into run output: {everything}"
        );
        assert_eq!(fetcher.calls(), 1);
    }

    #[tokio::test]
    async fn the_credential_is_fetched_once_and_reused_by_every_request() {
        use crate::flow_auth::test_support::{FakeFetcher, SharedFetcher};
        use rocket_shared::types::Auth;

        let fetcher = FakeFetcher::ok("fetched-token-999999");
        let executor = crate::test_doubles::RecordingExecutor::new();
        let exec = recording_http_exec(&executor);
        let flow = Flow {
            name: "auth-req".to_string(),
            nodes: vec![
                client_credentials_auth_node("a"),
                saved_flow_node("r1", "req.yml"),
                saved_flow_node("r2", "req.yml"),
            ],
            edges: Vec::new(),
            callback_host: None,
        };
        let service = service_with_saved_request(flow, Auth::Inherit)
            .with_token_fetcher(Box::new(SharedFetcher(Arc::clone(&fetcher))));

        service
            .run(&exec, run_input("auth-req"))
            .await
            .expect("run must succeed");

        let bearer = Auth::Bearer {
            token: "fetched-token-999999".to_string(),
        };
        assert_eq!(executor.sent_auths(), vec![bearer.clone(), bearer]);
        assert_eq!(fetcher.calls(), 1, "one fetch per run, not per request");
    }

    #[tokio::test]
    async fn an_unauthenticated_interactive_node_sends_no_request() {
        use crate::flow_auth::test_support::authorization_code;
        use rocket_shared::types::Auth;

        let executor = crate::test_doubles::RecordingExecutor::new();
        let exec = recording_http_exec(&executor);
        let flow = Flow {
            name: "auth-req".to_string(),
            nodes: vec![
                FlowNode {
                    id: "a".to_string(),
                    kind: FlowNodeKind::Auth {
                        label: "Corporate SSO".to_string(),
                        auth: Auth::OAuth2(Box::new(authorization_code())),
                        apply_to_inherit: true,
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                saved_flow_node("r", "req.yml"),
            ],
            edges: Vec::new(),
            callback_host: None,
        };
        let service = service_with_saved_request(flow, Auth::Inherit);

        let err = service
            .run(&exec, run_input("auth-req"))
            .await
            .expect_err("the run must not start");

        assert!(
            err.to_string().contains("Corporate SSO")
                && err.to_string().contains("needs you to authenticate first"),
            "got: {err}"
        );
        assert!(executor.sent_urls().is_empty(), "no request may be sent");
    }

    #[tokio::test]
    async fn a_supplied_token_for_an_interactive_node_is_what_requests_send() {
        use crate::flow_auth::test_support::authorization_code;
        use crate::flow_auth::{FlowAuthTokens, SuppliedToken};
        use rocket_shared::types::Auth;

        let executor = crate::test_doubles::RecordingExecutor::new();
        let exec = recording_http_exec(&executor);
        let flow = Flow {
            name: "auth-req".to_string(),
            nodes: vec![
                FlowNode {
                    id: "a".to_string(),
                    kind: FlowNodeKind::Auth {
                        label: "Corporate SSO".to_string(),
                        auth: Auth::OAuth2(Box::new(authorization_code())),
                        apply_to_inherit: true,
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                saved_flow_node("r", "req.yml"),
            ],
            edges: Vec::new(),
            callback_host: None,
        };
        let service = service_with_saved_request(flow, Auth::Inherit);
        let tokens: FlowAuthTokens = HashMap::from([(
            "a".to_string(),
            SuppliedToken {
                access_token: "supplied-token-123456".to_string(),
            },
        )]);

        service
            .run_with_auth(&exec, run_input("auth-req"), tokens)
            .await
            .expect("run must succeed");

        assert_eq!(
            executor.sent_auths(),
            vec![Auth::Bearer {
                token: "supplied-token-123456".to_string()
            }]
        );
    }
```

If `HttpExecutor`, `HttpRequest`, `HttpResponse`, `async_trait`, `DomainResult`, `Request`, `HttpMethod` are not already in scope in `mod tests`, they are the same names the neighboring tests import (`use super::*;` plus the `rocket_http` and `rocket_collection` imports at the top of `mod tests`).

- [ ] **Step 3: Run the tests**

Run: `cargo test -p rocket-app -j4 flow_execution_service`
Expected: the four new tests PASS and all existing tests still PASS.

If `a_fetched_token_never_appears_in_any_run_output` FAILS because the token shows up in the output, that is a real leak: do not weaken the assertion. Find which record shows it (the failure message prints everything), fix the production code with the smallest change (add the missing secret to the masking path, not a special case), and note the finding for Task 2.

- [ ] **Step 4: Commit**

```bash
cargo fmt
git add crates/rocket-app/src/flow_execution_service.rs
git commit -m "test(flow): end-to-end coverage for Auth node tokens"
```

---

### Task 2: Security review

**Files:**
- Modify: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md` (append a "Security review" section)
- Modify: any file a Critical/High finding touches (with a failing test first)

**Interfaces:**
- Consumes: the whole feature (Plans 1–6).
- Produces: a findings table in the spec; fixes for Critical/High findings.

- [ ] **Step 1: Run the mechanical checks**

Run each command from the repo root and record the result. Every one must come back empty (or as stated).

```bash
# S1: no console output in the Auth feature's frontend code
grep -rn "console\." src/lib/flow-auth.ts src/lib/flow-auth-preflight.ts src/lib/oauth2-requests.ts src/stores/flow-auth-store.ts src/components/flow/properties/AuthNodeEditor.tsx src/components/flow/nodes/AuthNode.tsx

# S2: the in-memory store never persists
grep -rn "persist\|localStorage\|sessionStorage" src/stores/flow-auth-store.ts src/lib/flow-auth.ts src/lib/flow-auth-preflight.ts

# S3: no logging in the backend module
grep -n "tracing::\|println!\|eprintln!\|log::\|dbg!" crates/rocket-app/src/flow_auth.rs

# S4: the secret-holding types must not derive Debug (they have manual impls)
grep -n "derive(.*Debug" crates/rocket-app/src/flow_auth.rs
```

For S4 the only acceptable matches are types that hold no secret. `SuppliedToken`, `FetchContext` and `FlowCredentials` must show a manual `impl std::fmt::Debug`.

- [ ] **Step 2: Check the IPC surface**

Run: `grep -n "tauri::command" src-tauri/src/commands/flow.rs src-tauri/src/commands/oauth2.rs`
Read each hit. Expected: `run_flow` only receives tokens (input), and no command added by this feature returns a token. The `oauth2_*` commands are pre-existing and unchanged.

- [ ] **Step 3: Dispatch the security review**

Use the Agent tool with `subagent_type: "security-engineer"` and this prompt:

```
Review the Flow Auth node feature in this repository for security problems.
Read docs/superpowers/specs/2026-10-02-flow-auth-node-design.md (Security section)
first, then these files:
  crates/rocket-app/src/flow_auth.rs
  crates/rocket-app/src/flow_execution_service.rs (run_with_auth, the Auth and Request arms of execute_node)
  src-tauri/src/commands/flow.rs (RunFlowInputDto, run_flow)
  src-tauri/src/lib.rs (the OAuth2 fetcher wiring)
  src/lib/flow-auth.ts, src/lib/flow-auth-preflight.ts, src/lib/oauth2-requests.ts
  src/stores/flow-auth-store.ts
  src/components/flow/properties/AuthNodeEditor.tsx, src/components/flow/FlowToolbar.tsx

Answer each question with evidence (file:line), not opinion:
1. Can an OAuth2 token or refresh token reach disk (flow yml, history, logs, localStorage)?
2. Can a token appear in a step value, debug_request, exchange, event payload,
   console entry or Debug output? Think about Output, Transform and Input nodes
   wired from an Auth node, and about responses that echo the token.
3. Can the backend open a browser or otherwise start an interactive sign-in?
4. Can a token be sent to a different environment, collection or flow than the
   one it was obtained for? Check the store key and the config-change reset.
5. Is the credential applied before or after the request guard / redirect rules?
   Does it bypass either?
6. The run injects each credential into the run's external-secrets map under
   `flow-auth.<node id>`. Can a flow author (or an imported flow file) use that
   to send a token somewhere the user would not expect, beyond what wiring an
   Auth node into a URL already allows?
7. Is there any new IPC command or event that exposes a token?
8. Any race or lifetime problem: tokens kept after the run ends, or shared
   between concurrent runs?

Report findings as a table: ID, severity (Critical/High/Medium/Low/Info),
file:line, description, suggested fix. Do not edit files.
```

- [ ] **Step 4: Triage**

For each finding:
- **Critical/High:** write a failing test that demonstrates it, fix it with the smallest change, run the test, commit as `fix(flow): <what>`.
- **Medium/Low/Info:** do not fix here; record it.

Expected known note (Info): question 6 — a flow author can already send a token anywhere by wiring an Auth node into a URL, so `{{flow-auth.<node id>}}` adds no new capability. Record it as accepted unless the review shows otherwise.

- [ ] **Step 5: Record the review in the spec**

Append to `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`:

```markdown
## Security review

Reviewed on <date of the review> against the implementation.

| ID | Severity | Finding | Resolution |
|---|---|---|---|
| (one row per finding from Step 3, or a single row "No findings above Info" ) | | | |

Mechanical checks S1–S4 and the IPC surface check were clean.
```

Fill the table with the real findings and resolutions. Do not leave a placeholder row.

- [ ] **Step 6: Commit**

```bash
git add docs/superpowers/specs/2026-10-02-flow-auth-node-design.md
git commit -m "docs(flow): record the Auth node security review"
```

---

### Task 3: Docs and final verification

**Files:**
- Modify: `crates/rocket-flow/CLAUDE.md`
- Modify: `crates/rocket-app/CLAUDE.md`
- Modify: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md` (status line)

**Interfaces:**
- Consumes: the finished feature.
- Produces: updated crate guidance and a verified tree.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Document the node in the crate guides**

Read `crates/rocket-flow/CLAUDE.md`, then append this section at the end of the file:

```markdown
## Auth node

`FlowNodeKind::Auth { label, auth, apply_to_inherit }` authenticates once per
run. It has no inputs and one `result` exit. `auth` is any `rocket_shared`
`Auth` except `none` and `inherit`; only configuration is stored, never a
token. Validation rules V11–V13: no wires into an Auth node; an `auth` wire
(`handle::AUTH`) must go from an Auth node into a Request node; at most one
Auth node with `apply_to_inherit = true`, and every Auth node needs a concrete
auth type.
```

Read `crates/rocket-app/CLAUDE.md`, then append:

```markdown
## Flow Auth nodes (`flow_auth.rs`)

`resolve_flow_credentials` turns every Auth node into a credential before a
run starts: a UI-supplied token wins, a non-interactive OAuth2 grant is fetched
through the `FlowTokenFetcher` port (`OAuth2ServiceFetcher` in production), an
interactive grant without a token fails the run before any event. Static auth
types pass through. `FlowExecutionService::run_with_auth` injects each
credential secret into the run's external-secrets map as `flow-auth.<node id>`
so the existing redaction masks it. Request nodes whose auth is `inherit` use
the auto-apply credential; an `auth` wire overrides it. Types holding secrets
(`SuppliedToken`, `FetchContext`, `FlowCredentials`) have redacting `Debug`
impls; keep it that way.
```

In the spec, change the line `Status: Draft for review` to `Status: Implemented`.

- [ ] **Step 3: Run the project verification**

Invoke the `verify-rocket` skill and follow it. At minimum, all of these must pass:

```bash
cargo check -j4
cargo test -p rocket-flow -p rocket-app -p rocket-infra -p rocket -j4
yarn tsc --noEmit
yarn check
yarn test
```

Expected: everything PASS. Report any failure with its output; do not claim success without it.

- [ ] **Step 4: Manual smoke test in the real app**

Run `yarn tauri dev` and check each of these. Use a collection whose auth is OAuth2 and a test identity provider you control (a local mock token endpoint is enough for client credentials).

1. **Client credentials, headless path.** Flow: one Auth node (OAuth2 client credentials, "Apply to inherited auth" on) and one saved request whose auth is "Inherit from parent". Click Run without opening the Auth node. Expected: the request is sent with `Authorization: Bearer <token>` (check the mock endpoint's log), the Auth step shows success with no value, and the console/last-run panels never show the token.
2. **Authorization code prompt.** Change the Auth node to authorization code. Click Run. Expected: the sign-in window or system browser opens before any node runs. Complete it: the run proceeds and the request carries the token. Run again within the token lifetime: no second prompt.
3. **Cancelled sign-in.** Run again after "Clear" (or restart the app), and close the sign-in window. Expected: a toast `Sign-in for Auth node "…" failed: …`, and no node status changes (the run never started).
4. **Config edit drops the token.** Fetch a token in the Auth node's editor ("Get New Access Token"), then change the client id. Expected: the token field empties. Run: it prompts again.
5. **Restart.** Quit and reopen the app. Open the flow file in a text editor: it holds the Auth configuration but no token. Run: it prompts (interactive) or fetches (client credentials).
6. **Explicit wire.** Add a second saved request with its own Basic auth and wire the Auth node's `result` to its `Auth` handle. Expected: that request is sent with the Auth node's credential, not Basic.
7. **Output masking.** Wire the Auth node into an Output node. Expected: the Output step shows `••••••`, not the token.

Record the outcome of each item in your final report. Anything that fails is a bug: fix it with a test, not a note.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-flow/CLAUDE.md crates/rocket-app/CLAUDE.md docs/superpowers/specs/2026-10-02-flow-auth-node-design.md
git commit -m "docs(flow): document the Auth node and mark the spec implemented"
```

---

**End of Plan 7 — the Flow Auth node feature is complete.**

**Next plan to run: none.**

Open follow-ups (each needs its own spec):
- Collection Runner support: a run-level auth parameter reusing the same token pass-through and redaction.
- "Use collection auth" as an Auth node source, so the node can start from the collection's OAuth2 configuration.
- Mid-run token refresh for flows that outlast a token.
- Consolidating the three duplicated OAuth2 request mappings (`OAuth2AuthEditor.tsx` still has its own copy of what `oauth2-requests.ts` now provides).
