# DDD Phase 4, Plan 2: Proto Stamp, OAuth2 Token POST, Host Environment

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (the user chose native execution) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the last direct I/O from `rocket-app/src` outside tests: the proto file `fs::metadata` (D-43), the `reqwest` fallback client and token POST (D-44), and the process environment and `current_exe` reads (D-45).

**Architecture:** Each call moves behind a trait. `ProtoLoader` gets a `source_stamp` method. `TokenClientProvider` (in `rocket-http`) gets a provided async `post_form` method that owns the POST, and `OAuth2Service::new` requires a provider. A new `HostEnv` trait in `rocket-shared` covers env vars, the user name and the executable path. `rocket-infra` owns the real `ProcessHostEnv`, and `src-tauri` wires it.

**Tech Stack:** Rust, `rocket-grpc`, `rocket-http`, `rocket-shared`, `rocket-app`, `rocket-infra`, `src-tauri` wiring.

**Spec:** `docs/roadmaps/ddd-roadmap.md` (Phase 4, D-43 to D-45). This is the last Phase 4 plan. Plan 1 (D-40 to D-42) is on branch `refactor/ddd-phase4-plan-1`; work on a new branch `refactor/ddd-phase4-plan-2` cut from it.

## Global Constraints

- Tasks 2 and 3 start with: read `docs/superpowers/specs/opencollection-spec-reference.md` (they touch `rocket-http`, `rocket-infra`, environment and auth code). Task 1 touches only gRPC proto loading; it starts with the same read because it edits `rocket-infra`.
- Cargo commands always carry `-j4`. No `cargo test --workspace`. Test one crate at a time: `cargo test -j4 -p <crate> <name>`.
- No panicking unwrap calls in production paths. Serde attributes are not touched.
- Code comments are short full sentences ending in a period.
- Do not commit inside a task. After the final review, the branch is committed by group with the `dev-workflow-skills:1-git-commit` skill.
- Do not add a new `std::fs`, `std::env`, `tokio::fs` or `reqwest` use to `rocket-app/src` outside `#[cfg(test)]`. Test code may use `rocket_infra` (it is a dev-dependency).
- The roadmap line numbers are stale. Sites are found by name: `registry_from_file` in `grpc_service.rs`, `token_client` and `post_token_request` in `oauth2_service.rs`, `process_env_for` in `execution_service.rs`, `on_request_saved` author in `contract_service.rs`, `mcp_server_specs` in `acp_session_service.rs`.
- `reqwest::Client` stays in the `TokenClientProvider::client_for` signature. Removing it is D-50 (Phase 5).

## Review Focus

- A proto file that is missing or whose stamp is unreadable: no cache key, the load still runs and reports `NotFound` (Task 1).
- A proto file rewritten with an older modification time still gets a new stamp when the time differs; an equal stamp is a cache hit (Task 1).
- A token endpoint answering non-2xx: the error text stays `Token endpoint returned {status}: {body}` (Task 2).
- A token endpoint answering 200 with a body that is not a token: the error text stays `Failed to parse token response: ...` (Task 2).
- A provider whose `client_for` fails (bad certificate) still stops the request before anything is sent (Task 2).
- A service built without `with_host_env` sees an empty environment, no user and no executable: `process.env` resolves nothing, the contract author is `None`, and `mcp_server_specs` returns an error instead of panicking (Task 3).
- `src-tauri` wires `with_host_env` on every production service that needs it; a forgotten site silently empties `process.env` (Task 3).

---

### Task 1: Proto source stamp behind `ProtoLoader` (D-43)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-grpc/src/registry.rs` (trait `ProtoLoader`)
- Modify: `crates/rocket-infra/src/grpc/proto_reader.rs` (`FsProtoLoader`, tests)
- Modify: `crates/rocket-app/src/grpc_service.rs` (`registry_from_file`, `FakeLoader`, cache test)
- Modify: `crates/rocket-app/CLAUDE.md` (exceptions paragraph: drop D-43)

**Interfaces:**
- Produces: `fn source_stamp(&self, proto_file: &Path) -> Option<u128>;` on `ProtoLoader`, with no default. `Some(nanos since the Unix epoch of the file's last modification)`, or `None` when the file or its time cannot be read.
- Consumes: nothing from other tasks.

- [ ] **Step 1: Write the failing infra tests** in `crates/rocket-infra/src/grpc/proto_reader.rs`:
  - `source_stamp_is_none_for_a_missing_file`: `FsProtoLoader.source_stamp(&tmp.path().join("nope.proto"))` is `None`.
  - `source_stamp_changes_when_the_file_is_touched`: write a `.proto` file in a `TempDir`, read the stamp, set the modification time 60 seconds ahead with `OpenOptions::new().write(true).open(..)?.set_modified(SystemTime::now() + Duration::from_secs(60))`, assert the second stamp is `Some` and differs from the first.
  - `source_stamp_is_equal_for_an_untouched_file`: two reads return the same `Some` value.

- [ ] **Step 2: Run them to see them fail.**
  Run: `cargo test -j4 -p rocket-infra source_stamp`
  Expected: compile error, `no method named source_stamp`.

- [ ] **Step 3: Write the failing app test** in `crates/rocket-app/src/grpc_service.rs`. Change the existing test that touches the real file (it contains the assertion text `a new modification time forces a new compile`) so it no longer touches the file system. Give `FakeLoader` a field `stamps: Mutex<HashMap<PathBuf, u128>>` and a helper `set_stamp(&self, path, nanos)`. The test sets a stamp, calls twice (one compile, `an unchanged file is compiled once`), sets a different stamp, calls again (two compiles). Add `an_unreadable_stamp_is_never_cached`: with no stamp set, two calls compile twice.

- [ ] **Step 4: Run it to see it fail.**
  Run: `cargo test -j4 -p rocket-app grpc_service`
  Expected: compile error, `FakeLoader` does not implement `source_stamp` (or the trait has no such method).

- [ ] **Step 5: Add `source_stamp` to `ProtoLoader`** in `crates/rocket-grpc/src/registry.rs`, with a doc comment saying what the stamp is for (the cache key). Implement it in `FsProtoLoader` with `std::fs::metadata(..).and_then(|m| m.modified()).ok()`, converted to nanoseconds since `UNIX_EPOCH` (the same arithmetic `registry_from_file` uses today). Implement it on `FakeLoader` from the hashmap.

- [ ] **Step 6: Use it in `registry_from_file`.** Replace the `std::fs::metadata` chain with `self.proto_loader.source_stamp(&path)`. The key format stays `format!("proto:{}:{nanos}", path.display())`. Remove the `SystemTime` import from the non-test imports if nothing else uses it.

- [ ] **Step 7: Run the tests.**
  Run: `cargo test -j4 -p rocket-infra source_stamp && cargo test -j4 -p rocket-grpc && cargo test -j4 -p rocket-app grpc_service`
  Expected: all PASS.

- [ ] **Step 8: Docs.** In `crates/rocket-app/CLAUDE.md`, remove D-43 from the exceptions paragraph. Mark D-43 `done (2026-11-04)` in `docs/roadmaps/ddd-roadmap.md`.

---

### Task 2: Token POST behind `TokenClientProvider`, provider required (D-44)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-http/src/token_client.rs` (add `post_form`), `crates/rocket-http/src/lib.rs` (re-export if a new type is added)
- Modify: `crates/rocket-app/src/oauth2_service.rs` (constructor, `token_client`, `post_token_request`, `plain_token_client`, `with_token_client_provider`, tests)
- Modify: `crates/rocket-app/src/vault_certificates.rs` (test constructor calls)
- Modify: `crates/rocket-app/src/test_doubles.rs` (a refusing provider)
- Modify: `src-tauri/src/lib.rs` (the `make_oauth2_service` factory)
- Modify: `crates/rocket-app/Cargo.toml` (move `reqwest` to `[dev-dependencies]` only if no non-test use is left)
- Modify: `crates/rocket-app/CLAUDE.md`, `docs/roadmaps/ddd-roadmap.md`

**Interfaces:**
- Produces, in `rocket-http`:
  ```rust
  #[async_trait]
  pub trait TokenClientProvider: Send + Sync {
      fn client_for(&self, token_url: &str, verify_ssl: bool, certificates: &[ResolvedClientCertificate]) -> DomainResult<reqwest::Client>;
      async fn post_form(&self, token_url: &str, verify_ssl: bool, certificates: &[ResolvedClientCertificate],
                         form: &[(String, String)], extra_headers: &[(String, String)]) -> DomainResult<OAuthToken> { /* provided */ }
  }
  ```
  The provided `post_form` calls `client_for`, posts the form with the headers, and parses `OAuthToken`. It is the body of today's `OAuth2Service::post_token_request`, with the same error texts.
- Produces: `OAuth2Service::new(env_repo, collection_repo, token_clients: Arc<dyn TokenClientProvider>)`. `with_token_client_provider` is removed. `plain_token_client` is removed if `grep -rn plain_token_client src-tauri crates` shows no production caller; otherwise it returns `DomainResult<reqwest::Client>` through `client_for` and stays the only `reqwest` name in `rocket-app` (rule it in the ledger and list it in the CLAUDE.md exceptions).
- Consumes: nothing from other tasks.

- [ ] **Step 1: Write the failing `rocket-http` tests** in `crates/rocket-http/src/token_client.rs` (module `tests`, with `wiremock` if it is a dev-dependency of `rocket-http`, otherwise a local `tokio::net::TcpListener` answering a fixed HTTP response; check `Cargo.toml` first). Use a small test provider whose `client_for` returns `reqwest::Client::new()`:
  - `post_form_returns_the_parsed_token_on_200`: server answers `{"access_token":"abc","token_type":"Bearer"}`; the result's `access_token` is `abc`.
  - `post_form_reports_a_non_2xx_status_and_body`: server answers 400 with body `bad`; the error is `DomainError::Internal` and its text is `Token endpoint returned 400 Bad Request: bad`.
  - `post_form_reports_an_unparseable_body`: server answers 200 with `not json`; the error text starts with `Failed to parse token response:`.
  - `post_form_sends_the_form_and_headers`: the server matches `Authorization: Basic xyz` and a body containing `grant_type=client_credentials`.
  - `post_form_stops_when_client_for_fails`: the provider's `client_for` returns `Err`; no request reaches the server (`expect(0)`) and the same error comes back.

- [ ] **Step 2: Run them to see them fail.**
  Run: `cargo test -j4 -p rocket-http token_client`
  Expected: compile error, no method `post_form`.

- [ ] **Step 3: Implement `post_form`** as a provided method (add `#[async_trait]` to the trait; `async-trait` is already a dependency). Move the body of `OAuth2Service::post_token_request` into it unchanged.

- [ ] **Step 4: Run them.**
  Run: `cargo test -j4 -p rocket-http token_client`
  Expected: PASS.

- [ ] **Step 5: Write the failing app tests** in `crates/rocket-app/src/oauth2_service.rs`:
  - Update `make_service`, `make_service_with_env` and the other `OAuth2Service::new(...)` calls in this file and in `vault_certificates.rs` to the three-argument form. This is a compile error until Step 6, which is the red state. Services that never send use a new `test_doubles::RefusingTokenClients` (its `client_for` returns `Err(DomainError::Internal("no token client in this test".into()))`). Tests that hit a `wiremock` token server use `Arc::new(rocket_infra::ReqwestTokenClientProvider::new())`.
  - New test `a_service_gets_its_token_over_the_provider_it_was_built_with`: a recording provider overriding `post_form` returns a fixed `OAuthToken` and records `(url, verify_ssl, form, headers)`; `get_token_direct` returns that token and the recorded form holds `grant_type=client_credentials`.
  - Existing tests that used the fallback client (no provider, wiremock server) keep their assertions and now pass the infra provider.

- [ ] **Step 6: Implement.** Make the provider a required field set by `new`. `token_client`/`post` calls go through the provider: replace `Self::post_token_request(&client, ...)` at the three call sites (`get_token_direct`, the refresh path, the code-exchange path) with `self.token_clients.post_form(&url, verify_ssl, &certificates, &form, &extra_headers).await`, and delete `post_token_request`, the fallback `reqwest::ClientBuilder` branch and `with_token_client_provider`. Update the test doubles `CapturingProvider` (oauth2) and `RecordingTokenClientProvider` (vault_certificates) only if their calls changed; they keep `client_for` and inherit `post_form`.

- [ ] **Step 7: Wire `src-tauri`.** In `make_oauth2_service` (`src-tauri/src/lib.rs`), pass the `ReqwestTokenClientProvider::new().with_proxy(...)` as the third argument to `new` and delete the `.with_token_client_provider(...)` call. Check any other `OAuth2Service::new` in `src-tauri` with `grep -rn "OAuth2Service::new" src-tauri/src`.

- [ ] **Step 8: Run the tests and a check.**
  Run: `cargo test -j4 -p rocket-app oauth2 && cargo test -j4 -p rocket-app vault_certificates && cargo test -j4 -p rocket-http token_client && cargo check -j4 --all-targets`
  Expected: PASS and a clean check.

- [ ] **Step 9: Dependency and docs.** Run `grep -rn "reqwest" crates/rocket-app/src | grep -v "#\[cfg(test)\]"` and confirm no production use remains (test-module uses are fine). If none, move `reqwest` from `[dependencies]` to `[dev-dependencies]` in `crates/rocket-app/Cargo.toml` and run `cargo check -j4 -p rocket-app --all-targets`. Update the `rocket-app/CLAUDE.md` exceptions paragraph and mark D-44 `done (2026-11-04)`.

---

### Task 3: `HostEnv` port for env vars, user name and executable path (D-45)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-shared/src/host_env.rs`; modify `crates/rocket-shared/src/lib.rs` (`pub mod host_env;`)
- Create: `crates/rocket-infra/src/process_host_env.rs`; modify `crates/rocket-infra/src/lib.rs` (declare and re-export `ProcessHostEnv`)
- Modify: `crates/rocket-app/src/execution_service.rs` (`process_env_for`, builder), `crates/rocket-app/src/contract_service.rs` (author), `crates/rocket-app/src/acp_session_service.rs` (`mcp_server_specs`, builder), `crates/rocket-app/src/agent_config_service.rs` (`process_env_reader`)
- Modify: `crates/rocket-app/src/lib.rs` (re-exports), tests that set real env vars (`execution_service/websocket_resolution.rs`, `flow_execution_service.rs`, `execution_service.rs`)
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/mcp/tool_server.rs` (wiring)
- Modify: `crates/rocket-app/CLAUDE.md`, `crates/rocket-infra/CLAUDE.md`, `docs/roadmaps/ddd-roadmap.md`

**Interfaces:**
- Produces, in `rocket-shared`:
  ```rust
  pub trait HostEnv: Send + Sync {
      fn vars(&self) -> HashMap<String, String>;
      fn var(&self, name: &str) -> Option<String>;
      fn user_name(&self) -> Option<String>;
      fn current_exe(&self) -> Option<PathBuf>;
  }
  pub struct EmptyHostEnv; // all four return empty / None
  ```
- Produces, in `rocket-infra`: `pub struct ProcessHostEnv;` implementing `HostEnv` with `std::env::vars()`, `std::env::var(name).ok()`, `USER` then `USERNAME`, and `std::env::current_exe().ok()`.
- Produces, in `rocket-app`: builder `with_host_env(mut self, env: Arc<dyn HostEnv>) -> Self` on `RequestExecutionService`, `ContractService` and `AcpSessionService`; the default is `Arc::new(EmptyHostEnv)`. `pub fn env_reader_from(env: Arc<dyn HostEnv>) -> EnvReader` replaces `process_env_reader()` (removed, with its re-export).
- Consumes: nothing from other tasks.

- [ ] **Step 1: Write the failing `rocket-shared` and `rocket-infra` tests.**
  - `crates/rocket-shared/src/host_env.rs`: `empty_host_env_has_no_vars_user_or_exe` asserts `vars()` is empty and `var("PATH")`, `user_name()`, `current_exe()` are `None`. Add `HostEnv` object safety (`let _: Arc<dyn HostEnv> = Arc::new(EmptyHostEnv);`).
  - `crates/rocket-infra/src/process_host_env.rs`: `var_reads_the_real_environment` (use a unique name set with `std::env::set_var` in the test; mark the module's tests as the only user of that name), `vars_contains_a_set_variable`, `user_name_is_none_or_non_empty`, `current_exe_is_some_and_absolute`.

- [ ] **Step 2: Run them to see them fail.**
  Run: `cargo test -j4 -p rocket-shared host_env && cargo test -j4 -p rocket-infra process_host_env`
  Expected: compile errors, the types do not exist.

- [ ] **Step 3: Implement the trait, `EmptyHostEnv` and `ProcessHostEnv`.** Run the two commands again. Expected: PASS.

- [ ] **Step 4: Write the failing app tests.** Add a test double `FixedHostEnv { vars, user, exe }` in `crates/rocket-app/src/test_doubles.rs` implementing `HostEnv`.
  - `execution_service.rs`: `process_env_for_reads_the_injected_host_env` (a service built with `with_host_env(FixedHostEnv)` and an allowed gate returns the injected vars; `ProcessEnv.allowed` is true) and `process_env_for_without_a_host_env_has_no_os_values` (default service returns only `.env` values, not the real `PATH`).
  - `contract_service.rs`: `the_changelog_author_comes_from_the_host_env` (`FixedHostEnv` with user `alice`; a breaking change recorded through `on_request_saved` carries author `alice`) and `the_changelog_author_is_none_without_a_user`. Reuse the helper the existing audit tests use to produce a change.
  - `acp_session_service.rs`: `mcp_server_specs_use_the_host_env_executable` (a fixed exe path `/opt/rocket/rocket` shows up as the stdio command) and `mcp_server_specs_fail_without_an_executable` (error text `could not resolve current executable`). `mcp_server_specs` becomes `mcp_server_specs(env: &dyn HostEnv, creds)`; test it as a free function.
  - `agent_config_service.rs`: `env_reader_from_forwards_to_var`.

- [ ] **Step 5: Run them to see them fail.**
  Run: `cargo test -j4 -p rocket-app host_env`
  Expected: compile errors, no `with_host_env`, no `env_reader_from`.

- [ ] **Step 6: Implement.** Add the `host_env` field and `with_host_env` to the three services. `process_env_for` takes `self.host_env.vars()` where it called `std::env::vars().collect()`. The contract author is `self.host_env.user_name()`. The ACP service passes `self.host_env.as_ref()` to `mcp_server_specs`, which maps `current_exe() == None` to `DomainError::Internal("could not resolve current executable")`. Replace `process_env_reader()` with `env_reader_from` and update its callers (`src-tauri/src/lib.rs` near `agent_env_reader`, and the four `crate::process_env_reader()` uses in `acp_session_service.rs` tests, which now pass `env_reader_from(Arc::new(EmptyHostEnv))`).

- [ ] **Step 7: Fix the tests that set real env vars.** `websocket_resolution.rs`, `flow_execution_service.rs` and the `execution_service.rs` tests that call `std::env::set_var` and expect `process.env` to see it now build their service with `.with_host_env(Arc::new(rocket_infra::ProcessHostEnv))` (dev-dependency) and keep their assertions. ACP tests that start a session use `.with_host_env(Arc::new(FixedHostEnv::with_exe("/opt/rocket/rocket")))`.
  Run: `cargo test -j4 -p rocket-app execution_service && cargo test -j4 -p rocket-app flow_execution_service && cargo test -j4 -p rocket-app acp_session && cargo test -j4 -p rocket-app contract_service`
  Expected: PASS.

- [ ] **Step 8: Wire production.** In `src-tauri/src/lib.rs` add `.with_host_env(Arc::new(rocket_infra::ProcessHostEnv))` to `RequestExecutionService::new_with_audit` (both constructions, around the lines of `exec_svc` and the Flow runner's second instance), `ContractService::new_with_audit` and `AcpSessionService::new`. Do the same for the construction in `src-tauri/src/mcp/tool_server.rs`. Build `agent_env_reader` with `rocket_app::env_reader_from(Arc::new(rocket_infra::ProcessHostEnv))`.
  Verify the wiring is complete: `grep -rn "with_host_env" src-tauri/src` must list five sites (two `RequestExecutionService` in `lib.rs`, one in `tool_server.rs`, one `ContractService`, one `AcpSessionService`); `grep -rn "RequestExecutionService::new\|ContractService::new\|AcpSessionService::new" src-tauri/src` must show each construction followed by `with_host_env` (rule on any construction that legitimately needs no environment, and ledger it).

- [ ] **Step 9: Final checks for the whole plan.**
  Run: `cargo check -j4 --all-targets`, then `cargo test -j4 -p rocket-shared`, `-p rocket-grpc`, `-p rocket-http`, `-p rocket-infra`, `-p rocket-app`, and `cargo test -j4 -p rocket --lib` one crate at a time. Run `cargo clippy -j4 --all-targets` and confirm only the known pre-existing lint at `execution_service.rs` (type complexity) remains.
  Then run: `grep -rnE "std::(fs|env)|tokio::fs|reqwest::|current_exe" crates/rocket-app/src --include='*.rs'` and confirm every remaining hit is inside a `#[cfg(test)]` module or a `_tests.rs`/`test_doubles.rs` file. Hits that are not (for example `rocket_shared::paths::home_dir` in `grpc_service.rs`, which is a `rocket-shared` call) are listed in the ledger.

- [ ] **Step 10: Docs.** In `crates/rocket-app/CLAUDE.md`, delete the exceptions paragraph (all of D-40 to D-45 are done) and state that the crate has no direct I/O. Add `ProcessHostEnv` to the table in `crates/rocket-infra/CLAUDE.md`. Mark D-45 `done (2026-11-04)` in `docs/roadmaps/ddd-roadmap.md`, and update D-23's note that the exceptions are gone.

---

## After the plan

This is the last Phase 4 plan. The user's standing instruction: after the final whole-branch review and its fix pass, commit by group (the commit steps use the `dev-workflow-skills:1-git-commit` skill), rebase the branch onto `main`, then merge it into local `main`. Never push.
