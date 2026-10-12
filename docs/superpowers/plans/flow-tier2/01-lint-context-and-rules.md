# Flow Tier 2 Lint Context and Rules Implementation Plan (Plan 1 of 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the lint tier real facts about the collection and the environments (F-20 context), then use them for two rules: a Request node whose saved request no longer exists (F-21, `dangling_saved_request`), and an inline Request or Input node that sends `{{name}}` as literal text because no variable has that name (F-22, `unknown_variable`).

**Architecture:** Task 1 adds `crates/rocket-app/src/flow_lint_context.rs` with `LintSources` (the repos), `LintEnvironment` (the two environment names) and `CollectionLintContext` (implements `rocket_flow::lint::LintContext`). `FlowService` gets the sources through a builder and a new `lint_with_environment`, and the `lint_flow` IPC command passes the active environment names. Task 2 adds the F-21 rule to `rocket-flow` `lint.rs`. Task 3 adds the F-22 rule there, then the client side: `lintFlow` sends the environment names, `useBackendFlowLints` lints again when they change and after a save, and `FlowPane` feeds both.

**Tech Stack:** Rust (`rocket-flow`, `rocket-app`, `src-tauri`), React, TypeScript, Vitest, Biome. Run with `cargo -j4` and `yarn`.

**Spec:** `.claude/flow-roadmap.md` rows F-21 and F-22 (Tier 2). F-20 (the lint tier and the `LintContext` trait) is done. Series index: `docs/superpowers/plans/flow-tier2/`.

## Global Constraints

- Rust gates: `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`. Never `--workspace` or `--all`. Never run `cargo fmt` over a crate. Check your own files with `rustfmt --check --edition 2021 <file>`.
- No unwrap or expect calls in production code. Test code may use `expect("message")`.
- Serde `camelCase` renames only on IPC DTOs. This plan adds no DTO and no persisted field.
- Frontend gates: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern> --run --maxWorkers=2`.
- shadcn/ui primitives and `lucide-react` icons only. This plan adds no UI element.
- Comments are short full sentences ending in a punctuation mark. No emojis.
- Commit through the `dev-workflow-skills:1-git-commit` skill, with explicit paths (`git add <paths> && git commit --only -m "..." -- <paths>`). Never `git add -A`. Conventional commit messages.
- Task 1 touches `rocket-app` environment and collection reads, variable scopes and the `lint_flow` IPC command. Its first step is the spec read below.
- Lints never block save or run. `FlowService::save` and `run_flow` do not call the lint path. Task 2 pins this with a test.
- The lint layer receives yes or no answers, never a resolved value. No message quotes a request path, a URL, a header, a body or a variable value. Variable NAMES are allowed.

## Review Focus

- A name a saved request's script may set at run time is never reported as unknown (Task 1 `a_flow_with_a_saved_request_never_answers_no`, Task 3 `a_saved_request_in_the_flow_silences_the_variable_rule`).
- With no active environment the context answers `None` for every name it cannot find, so a missing environment never produces a false lint (Task 1 `no_active_environment_leaves_unknown_names_unanswered`).
- No message holds a request path or any text around a placeholder (Task 2 `the_message_never_quotes_the_path`, Task 3 `the_message_lists_names_and_nothing_else`).
- A field that a wire replaces is not linted, because the literal text is never sent (Task 3 `a_wired_field_is_not_linted`).
- The rules ask the context once per saved node and once per distinct name per node, and stay linear in graph size (Task 2 `saved_request_rule_is_linear`, Task 3 `unknown_variable_rule_is_linear`).
- A flow with a dangling request still saves unchanged (Task 2 `a_flow_with_a_dangling_request_still_saves`).

## Findings that shape this plan

- `FlowService` only holds `flow_repo` today. It needs a collection repo and two environment sources, so `FlowService::with_lint_sources` is added. Without sources `lint` behaves exactly as before (`NoLintContext`), which keeps every existing `FlowService` test valid.
- A saved request is read at run time with `collection_repo.get_request(collection, request_path)` (`flow_execution_service.rs:~503`). The context uses the same call, so "exists" means exactly "the run can read it". `DomainError::NotFound` is the only error that means missing. Any other error (parse failure, a GraphQL file, a bad path) answers `None`.
- Inline Request nodes carry only `method`, `url`, `headers` and `body` (`InlineRequestData`). There is no inline `auth` field. A credential reaches an inline Request through an `auth` wire or an Auth node, which is F-24. So F-22 lints `url`, header names and values, `body`, and also the Input node value, which is resolved with `rocket_environment::resolve` and leaves unknown names as literal text too (`flow_execution_service.rs:~1572`).
- Inline requests resolve against global env < collection variables < active environment (< vault `alias.secret`) plus `callback.<name>` flow vars. They get no folder or request scope (`build_inline_request` doc comment), and the collection settings have no scripts. The only way a variable appears at run time that the context cannot see is a script or an extraction of an earlier saved request (`apply_env_writes`, `rok.setEnvVar`, `rok.setCollectionVar`). The conservative rule: a flow that contains any Saved Request node never gets a "no" for a name it cannot find.
- Environment reads: `FsEnvironmentRepo::get` hydrates secret values from the OS keychain. The lint only needs keys, so the wiring uses a repo built with `NullSecretStore` (no keychain access at lint time).
- Client: `useBackendFlowLints` already lints once at open with no delay and after a 500 ms pause on each edit of the graph content (`lintedFlowRef`, `openKey`). Node drags do not re-lint. It does NOT re-lint after a save, and it does not know the active environment. Task 3 adds both. `BACKEND_LINT_CODES` in `src/lib/flow-lint.ts` already lists `dangling_saved_request` and `unknown_variable`, `toFlowIssue` and `FlowIssuesButton` are generic over the code, so no new mapping is needed, only tests.
- `lint_flow` is a sync Tauri command today, which runs on the main thread. With file reads behind it, it becomes `async` so the UI does not wait on disk.

---

### Task 1: App-layer lint context and wiring

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/flow_lint_context.rs` (~230 lines with tests)
- Modify: `crates/rocket-app/src/lib.rs` (module list ~18-28, re-exports ~94)
- Modify: `crates/rocket-app/src/flow_service.rs` (struct and `new` ~1-12, `lint` ~46-62, tests at the end)
- Modify: `src-tauri/src/commands/flow.rs` (`lint_flow` ~503-516, imports ~1-4)
- Modify: `src-tauri/src/lib.rs` (`flow_svc` construction ~667-669)

**Interfaces:**
- Consumes: `rocket_flow::lint::LintContext`, `rocket_collection::CollectionRepository::{get_request, get_settings}`, `rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory, Environment}`, `rocket_flow::{Flow, FlowNodeKind, RequestSource}`.
- Produces in `crates/rocket-app/src/flow_lint_context.rs`:
  - `pub struct LintSources { pub collection_repo: Box<dyn CollectionRepository>, pub env_factory: Box<dyn EnvironmentRepositoryFactory>, pub global_env_repo: Box<dyn EnvironmentRepository> }`
  - `#[derive(Debug, Clone, Copy, Default)] pub struct LintEnvironment<'a> { pub environment_name: Option<&'a str>, pub global_env_name: Option<&'a str> }`
  - `pub struct CollectionLintContext<'a>` with `pub fn new(sources: &'a LintSources, collection: &'a str, env: LintEnvironment<'_>, flow: &Flow) -> Self`, implementing `LintContext`.
- Produces in `FlowService`: `pub fn with_lint_sources(self, sources: LintSources) -> Self` and `pub fn lint_with_environment(&self, collection: &str, flow: &Flow, env: LintEnvironment<'_>) -> Vec<FlowLint>`. `lint(collection, flow)` stays and calls the new method with `LintEnvironment::default()`.
- Produces in `rocket_app`: `pub use flow_lint_context::{LintEnvironment, LintSources};`.
- IPC: `lint_flow(collection, flow, environmentName?, globalEnvName?)`, async.

The answer rules of `variable_is_known(name)`, in this order:

1. The name is empty, starts with `$` (dynamic), starts with `process.env.` or starts with `callback.`: `None` (the run resolves these by other means, so the context never says "no").
2. The name is in the collected set (enabled collection variables, enabled variables of the active environment, enabled variables of the active global environment, and `alias.secretName` for every secret of the active environment's vault bindings): `Some(true)`.
3. The collected set is incomplete: `None`. It is incomplete when the flow contains a Saved Request node (its scripts may write variables), when no active environment name was given, when an environment or the collection settings could not be read, or when the active environment has `extends` or `dot_env_file_path` set (stored but not modelled here).
4. Otherwise `Some(false)`.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-app/src/lib.rs`, add `pub mod flow_lint_context;` directly after `pub mod flow_execution_service;` (keeps the list alphabetical) and add `pub use flow_lint_context::{LintEnvironment, LintSources};` next to `pub use flow_service::FlowService;`.

Create `crates/rocket-app/src/flow_lint_context.rs` containing only this test module (the production code comes in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::{ConfigurableCollectionRepo, NullEnvRepo, SharedCollectionRepo, StaticEnvRepo};
    use rocket_collection::{CollectionSettings, CollectionVariable, Request};
    use rocket_environment::{
        Environment, EnvironmentRepository, EnvironmentRepositoryFactory, ExternalSecretBinding,
        ExternalSecretRef, Variable,
    };
    use rocket_flow::{Flow, FlowNode, FlowNodeKind, InlineRequestData, NodePosition, RequestSource};
    use rocket_shared::types::HttpMethod;
    use std::sync::Arc;

    /// Answers with one fixed environment, or with `NotFound` when there is none.
    struct FakeFactory(Option<Environment>);

    impl EnvironmentRepositoryFactory for FakeFactory {
        fn for_collection(&self, _collection: &str) -> Box<dyn EnvironmentRepository> {
            match &self.0 {
                Some(env) => Box::new(StaticEnvRepo(env.clone())),
                None => Box::new(NullEnvRepo),
            }
        }
    }

    fn repo_for(env: Option<Environment>) -> Box<dyn EnvironmentRepository> {
        match env {
            Some(env) => Box::new(StaticEnvRepo(env)),
            None => Box::new(NullEnvRepo),
        }
    }

    fn collection_var(key: &str, enabled: bool) -> CollectionVariable {
        CollectionVariable {
            key: key.to_string(),
            value: "collection-secret-value".to_string(),
            initial_value: String::new(),
            enabled,
            secret: true,
        }
    }

    fn environment(keys: &[&str]) -> Environment {
        let mut env = Environment::new("dev");
        env.variables = keys.iter().map(|k| Variable::new(*k, "env-secret-value")).collect();
        env
    }

    fn node_of(id: &str, kind: FlowNodeKind) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind,
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn inline_node(id: &str) -> FlowNode {
        node_of(
            id,
            FlowNodeKind::Request {
                label: id.to_string(),
                source: RequestSource::Inline {
                    request: InlineRequestData {
                        method: "GET".to_string(),
                        url: "https://example.test".to_string(),
                        headers: vec![],
                        body: None,
                    },
                },
                debug: false,
                repeat_until: None,
            },
        )
    }

    fn saved_node(id: &str, path: &str) -> FlowNode {
        node_of(
            id,
            FlowNodeKind::Request {
                label: id.to_string(),
                source: RequestSource::Saved {
                    request_path: path.to_string(),
                },
                debug: false,
                repeat_until: None,
            },
        )
    }

    fn flow_of(nodes: Vec<FlowNode>) -> Flow {
        Flow {
            name: "f".to_string(),
            nodes,
            edges: vec![],
            callback_host: None,
        }
    }

    fn sources(
        repo: &Arc<ConfigurableCollectionRepo>,
        env: Option<Environment>,
        global: Option<Environment>,
    ) -> LintSources {
        LintSources {
            collection_repo: Box::new(SharedCollectionRepo(Arc::clone(repo))),
            env_factory: Box::new(FakeFactory(env)),
            global_env_repo: repo_for(global),
        }
    }

    fn settings_with(vars: Vec<CollectionVariable>) -> CollectionSettings {
        CollectionSettings {
            variables: vars,
            ..Default::default()
        }
    }

    const ENV: LintEnvironment<'static> = LintEnvironment {
        environment_name: Some("dev"),
        global_env_name: Some("shared"),
    };

    #[test]
    fn saved_request_exists_answers_true_false_and_none() {
        let repo = ConfigurableCollectionRepo::new();
        repo.with_request(
            "demo",
            "auth/login.yml",
            Request::new("Login", HttpMethod::Get, "https://example.test"),
        );
        let src = sources(&repo, None, None);
        let flow = flow_of(vec![inline_node("a")]);
        let ctx = CollectionLintContext::new(&src, "demo", ENV, &flow);
        assert_eq!(ctx.saved_request_exists("auth/login.yml"), Some(true));
        assert_eq!(ctx.saved_request_exists("auth/gone.yml"), Some(false));
        // A read error that is not "missing" must not read as "missing".
        repo.fail_request_reads();
        assert_eq!(ctx.saved_request_exists("auth/other.yml"), None);
    }

    #[test]
    fn saved_request_exists_asks_the_repo_once_per_path() {
        let repo = ConfigurableCollectionRepo::new();
        repo.with_request(
            "demo",
            "a.yml",
            Request::new("A", HttpMethod::Get, "https://example.test"),
        );
        let src = sources(&repo, None, None);
        let flow = flow_of(vec![inline_node("a")]);
        let ctx = CollectionLintContext::new(&src, "demo", ENV, &flow);
        assert_eq!(ctx.saved_request_exists("a.yml"), Some(true));
        // The repo now fails every read, so a second true answer came from the cache.
        repo.fail_request_reads();
        assert_eq!(ctx.saved_request_exists("a.yml"), Some(true));
    }

    #[test]
    fn variables_of_every_scope_are_known() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings("demo", settings_with(vec![collection_var("baseUrl", true)]));
        let mut env = environment(&["token"]);
        env.external_secrets = vec![ExternalSecretBinding {
            alias: "vault".to_string(),
            connection_id: "c1".to_string(),
            vault_name: "prod".to_string(),
            secret_names: vec![ExternalSecretRef {
                name: "apiKey".to_string(),
                secret_id: "s1".to_string(),
            }],
        }];
        let src = sources(&repo, Some(env), Some(environment(&["region"])));
        let flow = flow_of(vec![inline_node("a")]);
        let ctx = CollectionLintContext::new(&src, "demo", ENV, &flow);
        for name in ["baseUrl", "token", "vault.apiKey", "region"] {
            assert_eq!(ctx.variable_is_known(name), Some(true), "{name}");
        }
    }

    #[test]
    fn a_name_in_no_scope_is_unknown_and_a_disabled_variable_does_not_count() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings("demo", settings_with(vec![collection_var("off", false)]));
        let src = sources(&repo, Some(environment(&["token"])), None);
        let flow = flow_of(vec![inline_node("a")]);
        let ctx = CollectionLintContext::new(&src, "demo", ENV, &flow);
        assert_eq!(ctx.variable_is_known("missing"), Some(false));
        assert_eq!(ctx.variable_is_known("off"), Some(false));
        assert_eq!(ctx.variable_is_known("token"), Some(true));
    }

    #[test]
    fn no_active_environment_leaves_unknown_names_unanswered() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings("demo", settings_with(vec![collection_var("baseUrl", true)]));
        let src = sources(&repo, Some(environment(&["token"])), None);
        let flow = flow_of(vec![inline_node("a")]);
        let none = LintEnvironment::default();
        let ctx = CollectionLintContext::new(&src, "demo", none, &flow);
        // Defined in the collection: still a yes. Maybe defined in an environment: no answer.
        assert_eq!(ctx.variable_is_known("baseUrl"), Some(true));
        assert_eq!(ctx.variable_is_known("token"), None);
    }

    #[test]
    fn an_unreadable_scope_leaves_unknown_names_unanswered() {
        let repo = ConfigurableCollectionRepo::new();
        // The named environment does not exist.
        let src = sources(&repo, None, None);
        let flow = flow_of(vec![inline_node("a")]);
        let ctx = CollectionLintContext::new(&src, "demo", ENV, &flow);
        assert_eq!(ctx.variable_is_known("anything"), None);
        // The collection settings cannot be read.
        let repo = ConfigurableCollectionRepo::new();
        repo.fail_settings_for("demo");
        let src = sources(&repo, Some(environment(&["token"])), Some(environment(&[])));
        let ctx = CollectionLintContext::new(&src, "demo", ENV, &flow);
        assert_eq!(ctx.variable_is_known("anything"), None);
        assert_eq!(ctx.variable_is_known("token"), Some(true));
    }

    #[test]
    fn an_environment_with_inheritance_leaves_unknown_names_unanswered() {
        let repo = ConfigurableCollectionRepo::new();
        let mut env = environment(&["token"]);
        env.extends = Some("base".to_string());
        let src = sources(&repo, Some(env), Some(environment(&[])));
        let flow = flow_of(vec![inline_node("a")]);
        let ctx = CollectionLintContext::new(&src, "demo", ENV, &flow);
        assert_eq!(ctx.variable_is_known("missing"), None);
    }

    #[test]
    fn a_flow_with_a_saved_request_never_answers_no() {
        let repo = ConfigurableCollectionRepo::new();
        let src = sources(&repo, Some(environment(&["token"])), Some(environment(&[])));
        let flow = flow_of(vec![inline_node("a"), saved_node("b", "login.yml")]);
        let ctx = CollectionLintContext::new(&src, "demo", ENV, &flow);
        // A script of the saved request may set `missing` before the inline one runs.
        assert_eq!(ctx.variable_is_known("missing"), None);
        assert_eq!(ctx.variable_is_known("token"), Some(true));
    }

    #[test]
    fn run_time_names_are_never_answered_no() {
        let repo = ConfigurableCollectionRepo::new();
        let src = sources(&repo, Some(environment(&[])), Some(environment(&[])));
        let flow = flow_of(vec![inline_node("a")]);
        let ctx = CollectionLintContext::new(&src, "demo", ENV, &flow);
        for name in ["$guid", "$notARealOne", "process.env.HOME", "callback.payment", ""] {
            assert_eq!(ctx.variable_is_known(name), None, "{name:?}");
        }
        assert_eq!(ctx.variable_is_known("plain"), Some(false));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_lint_context`
Expected: FAILS to compile, `cannot find type LintSources / LintEnvironment / CollectionLintContext in this scope`.

- [ ] **Step 3: Write the implementation**

Put this at the top of `crates/rocket-app/src/flow_lint_context.rs`, above the test module:

```rust
//! The facts the flow lint tier needs from outside the flow file (F-20).
//!
//! `rocket-flow` does no I/O, so the lint rules ask a `LintContext`. This
//! context answers from the collection repo and the environment repos.
//! It answers yes, no or "cannot tell", and it never hands out a value:
//! only variable KEYS are read, and nothing here returns a string.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use rocket_collection::CollectionRepository;
use rocket_environment::{Environment, EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_flow::lint::LintContext;
use rocket_flow::{Flow, FlowNodeKind, RequestSource};
use rocket_shared::error::DomainError;

/// The repositories a lint reads. They follow the active workspace, and the
/// environment repos must not touch the OS keychain, because only keys are read.
pub struct LintSources {
    pub collection_repo: Box<dyn CollectionRepository>,
    /// Gives the repo of one collection's own environments.
    pub env_factory: Box<dyn EnvironmentRepositoryFactory>,
    /// The workspace-level (global) environments.
    pub global_env_repo: Box<dyn EnvironmentRepository>,
}

/// The names of the environments the next run would use. `None` means none is selected.
#[derive(Debug, Clone, Copy, Default)]
pub struct LintEnvironment<'a> {
    pub environment_name: Option<&'a str>,
    pub global_env_name: Option<&'a str>,
}

/// Answers `LintContext` questions for one collection and one flow.
pub struct CollectionLintContext<'a> {
    repo: &'a dyn CollectionRepository,
    collection: &'a str,
    /// Keys of every variable a run of this flow can see in the readable scopes.
    names: HashSet<String>,
    /// True when `names` holds every name a run could resolve.
    names_complete: bool,
    /// One read per distinct path, whatever the number of nodes that use it.
    requests: RefCell<HashMap<String, Option<bool>>>,
}

impl<'a> CollectionLintContext<'a> {
    pub fn new(
        sources: &'a LintSources,
        collection: &'a str,
        env: LintEnvironment<'_>,
        flow: &Flow,
    ) -> Self {
        let mut names = HashSet::new();
        // A saved request may carry scripts that set variables for later nodes.
        let mut complete = !flow_has_saved_request(flow);

        match sources.collection_repo.get_settings(collection) {
            Ok(settings) => names.extend(
                settings
                    .variables
                    .iter()
                    .filter(|v| v.enabled)
                    .map(|v| v.key.clone()),
            ),
            Err(_) => complete = false,
        }

        // With no active environment the names of any environment may still apply later.
        match env.environment_name {
            Some(name) => {
                let repo = sources.env_factory.for_collection(collection);
                complete &= add_environment(&mut names, repo.get(name));
            }
            None => complete = false,
        }

        // No global environment selected is a real answer: nothing comes from there.
        if let Some(name) = env.global_env_name {
            complete &= add_environment(&mut names, sources.global_env_repo.get(name));
        }

        Self {
            repo: sources.collection_repo.as_ref(),
            collection,
            names,
            names_complete: complete,
            requests: RefCell::new(HashMap::new()),
        }
    }
}

/// Adds the keys of one environment. Returns false when the scope may hold
/// names that were not added: it could not be read, or it inherits.
fn add_environment(names: &mut HashSet<String>, read: Result<Environment, DomainError>) -> bool {
    let Ok(environment) = read else {
        return false;
    };
    names.extend(
        environment
            .variables
            .iter()
            .filter(|v| v.enabled)
            .map(|v| v.key.clone()),
    );
    for binding in &environment.external_secrets {
        for secret in &binding.secret_names {
            names.insert(format!("{}.{}", binding.alias, secret.name));
        }
    }
    environment.extends.is_none() && environment.dot_env_file_path.is_none()
}

fn flow_has_saved_request(flow: &Flow) -> bool {
    flow.nodes.iter().any(|n| {
        matches!(
            &n.kind,
            FlowNodeKind::Request {
                source: RequestSource::Saved { .. },
                ..
            }
        )
    })
}

/// Names a run resolves by other means than the scopes read here.
fn resolved_at_run_time(name: &str) -> bool {
    name.is_empty()
        || name.starts_with('$')
        || name.starts_with("process.env.")
        || name.starts_with("callback.")
}

impl LintContext for CollectionLintContext<'_> {
    fn saved_request_exists(&self, request_path: &str) -> Option<bool> {
        let cached = self.requests.borrow().get(request_path).copied();
        if let Some(answer) = cached {
            return answer;
        }
        // The same read the run does, so "exists" means "the run can open it".
        let answer = match self.repo.get_request(self.collection, request_path) {
            Ok(_) => Some(true),
            Err(DomainError::NotFound(_)) => Some(false),
            Err(_) => None,
        };
        self.requests
            .borrow_mut()
            .insert(request_path.to_string(), answer);
        answer
    }

    fn variable_is_known(&self, name: &str) -> Option<bool> {
        if resolved_at_run_time(name) {
            return None;
        }
        if self.names.contains(name) {
            return Some(true);
        }
        self.names_complete.then_some(false)
    }
}
```

`flow_service.rs`: replace the struct, `new` and `lint`, and the import line.

```rust
use crate::flow_lint_context::{CollectionLintContext, LintEnvironment, LintSources};
use rocket_flow::lint::{graph_error_lints, validate_with_warnings, FlowLint, NoLintContext};
use rocket_flow::{validate, Flow, FlowGraphError, FlowRepository};
use rocket_shared::error::{DomainError, DomainResult};

pub struct FlowService {
    flow_repo: Box<dyn FlowRepository>,
    /// Where lints read the collection and the environments. Without it no
    /// lint that needs them can run.
    lint_sources: Option<LintSources>,
}

impl FlowService {
    pub fn new(flow_repo: Box<dyn FlowRepository>) -> Self {
        Self {
            flow_repo,
            lint_sources: None,
        }
    }

    /// Lets lints ask about saved requests and variables.
    pub fn with_lint_sources(mut self, sources: LintSources) -> Self {
        self.lint_sources = Some(sources);
        self
    }
```

and replace the `lint` method with:

```rust
    /// Lints `flow` with no environment selected. See `lint_with_environment`.
    pub fn lint(&self, collection: &str, flow: &Flow) -> Vec<FlowLint> {
        self.lint_with_environment(collection, flow, LintEnvironment::default())
    }

    /// Lints `flow` as given, not the saved file, and never fails. A
    /// structural `validate` failure comes first, as `invalid_graph` error
    /// lints that carry its node or edge ids. Then come the warnings of
    /// `validate_with_warnings`, which ask a context built from `env`. Without
    /// lint sources the context knows nothing and those rules stay silent.
    /// Lints never block save or run.
    pub fn lint_with_environment(
        &self,
        collection: &str,
        flow: &Flow,
        env: LintEnvironment<'_>,
    ) -> Vec<FlowLint> {
        let mut lints = match validate(flow) {
            Ok(_) => Vec::new(),
            Err(error) => graph_error_lints(flow, &error),
        };
        match &self.lint_sources {
            Some(sources) => {
                let ctx = CollectionLintContext::new(sources, collection, env, flow);
                lints.extend(validate_with_warnings(flow, &ctx));
            }
            None => lints.extend(validate_with_warnings(flow, &NoLintContext)),
        }
        lints
    }
```

Add this test at the end of `flow_service.rs` `mod tests` (it proves the plain `lint` is unchanged without sources):

```rust
    #[test]
    fn lint_with_environment_without_sources_matches_lint() {
        let svc = FlowService::new(Box::new(UntouchedRepo));
        let flow = routing_flow();
        let env = crate::flow_lint_context::LintEnvironment {
            environment_name: Some("dev"),
            global_env_name: None,
        };
        assert_eq!(svc.lint_with_environment("demo", &flow, env), svc.lint("demo", &flow));
    }
```

`src-tauri/src/commands/flow.rs`: change the import to `use rocket_app::{FlowExecutionService, FlowRunOptions, FlowRunSummary, FlowService, LintEnvironment, PartialRun, RequestExecutionService, RunFlowInput};` (keep the existing items, add `LintEnvironment`) and replace `lint_flow`:

```rust
/// Lints the graph the canvas holds now, saved or not. Never fails because
/// of what the graph contains: problems come back as lints. The environment
/// names are the ones a run would use. Without them the variable rule stays
/// silent for names it cannot find. Async, so the file reads do not run on
/// the main thread.
#[tauri::command]
pub async fn lint_flow(
    collection: String,
    flow: FlowDto,
    environment_name: Option<String>,
    global_env_name: Option<String>,
    svc: State<'_, FlowService>,
) -> Result<Vec<FlowLintDto>, DomainError> {
    let env = LintEnvironment {
        environment_name: environment_name.as_deref(),
        global_env_name: global_env_name.as_deref(),
    };
    Ok(svc
        .lint_with_environment(&collection, &flow.into(), env)
        .into_iter()
        .map(FlowLintDto::from)
        .collect())
}
```

`src-tauri/src/lib.rs`: replace the `flow_svc` construction (~667-669):

```rust
            // Lints read only variable keys, so these repos use a store that never
            // touches the OS keychain. They follow workspace switches like the rest.
            let lint_sources = rocket_app::LintSources {
                collection_repo: Box::new(SharedPathCollectionRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                env_factory: Box::new(SharedCollectionEnvironmentRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                global_env_repo: Box::new(SharedPathEnvironmentRepo::with_secret_store(
                    Arc::clone(&active_workspace_path),
                    Arc::new(rocket_environment::NullSecretStore),
                )),
            };
            let flow_svc = rocket_app::FlowService::new(Box::new(
                rocket_infra::SharedPathFlowRepo::new(Arc::clone(&active_workspace_path)),
            ))
            .with_lint_sources(lint_sources);
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_lint_context`, then `cargo test -j4 -p rocket-app flow_service`, then `cargo check -j4 -p rocket-app` and `cargo check -j4 -p rocket`. Then `rustfmt --check --edition 2021 crates/rocket-app/src/flow_lint_context.rs` and the same for the other files you edited (fix only your own lines by hand if it reports a diff in them; the repo is not fully rustfmt clean, so ignore pre-existing diffs).
Expected: all PASS, no warnings from your files. Manual check in the app (`yarn tauri dev`) is not needed yet because no rule asks the context until Task 2.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:
`git add crates/rocket-app/src/flow_lint_context.rs crates/rocket-app/src/lib.rs crates/rocket-app/src/flow_service.rs src-tauri/src/commands/flow.rs src-tauri/src/lib.rs && git commit --only -m "feat(flow): add the collection lint context and environment names to lint_flow" -- <the same paths>`

---

### Task 2: Lint dangling saved request (F-21)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-flow/src/lint.rs` (consts ~13-21, `validate_with_warnings` ~66-84, helpers near `warning` ~134-143, tests module ~440 onward)
- Modify: `crates/rocket-app/src/flow_service.rs` (tests at the end)

**Interfaces:**
- Consumes: `LintContext::saved_request_exists(&self, &str) -> Option<bool>`, `FlowNodeKind::Request { source: RequestSource::Saved { request_path }, .. }`, `display_label`.
- Produces in `lint.rs`:
  - `pub const DANGLING_SAVED_REQUEST: &str = "dangling_saved_request";`
  - `fn node_lint(severity: LintSeverity, code: &str, node: &FlowNode, message: String, hint: &str) -> FlowLint` (private, `warning` now calls it).
  - `fn dangling_request_lint(node: &FlowNode, ctx: &dyn LintContext) -> Option<FlowLint>` (private).
- Severity: `Error`. A run that reaches the node fails there for certain, because it reads the same file. It is still only a lint: it never blocks save or run (the client maps it to the same issue list, and nothing disables Run on severity). Open decision 1 covers switching it to `Warning`.

- [ ] **Step 1: Write the failing tests**

In `lint.rs`, change the tests' node import line `use crate::node::{NodePosition, RequestSource, SwitchCase};` to `use crate::node::{InlineHeader, InlineRequestData, NodePosition, RequestSource, SwitchCase};` (Task 3 uses the two new names too). Add these helpers and tests at the end of the tests module (after the existing tests, before the closing brace):

```rust
    /// Answers from fixed lists. Anything not listed gets `None`.
    struct FakeContext {
        requests: Vec<(&'static str, bool)>,
        variables: Vec<(&'static str, bool)>,
    }

    impl LintContext for FakeContext {
        fn saved_request_exists(&self, request_path: &str) -> Option<bool> {
            self.requests
                .iter()
                .find(|(path, _)| *path == request_path)
                .map(|(_, exists)| *exists)
        }

        fn variable_is_known(&self, name: &str) -> Option<bool> {
            self.variables
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, answer)| *answer)
        }
    }

    /// Says every saved request is missing and every variable is unknown.
    struct EverythingMissing;

    impl LintContext for EverythingMissing {
        fn saved_request_exists(&self, _request_path: &str) -> Option<bool> {
            Some(false)
        }

        fn variable_is_known(&self, _name: &str) -> Option<bool> {
            Some(false)
        }
    }

    /// Counts the questions, to prove how often a rule asks.
    #[derive(Default)]
    struct Counting {
        requests: std::cell::Cell<usize>,
        variables: std::cell::Cell<usize>,
    }

    impl LintContext for Counting {
        fn saved_request_exists(&self, _request_path: &str) -> Option<bool> {
            self.requests.set(self.requests.get() + 1);
            Some(true)
        }

        fn variable_is_known(&self, _name: &str) -> Option<bool> {
            self.variables.set(self.variables.get() + 1);
            Some(true)
        }
    }

    fn saved_request(id: &str, label: &str, path: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                label: label.to_string(),
                source: RequestSource::Saved {
                    request_path: path.to_string(),
                },
                debug: false,
                repeat_until: None,
            },
        )
    }

    fn missing(path: &'static str) -> FakeContext {
        FakeContext {
            requests: vec![(path, false)],
            variables: vec![],
        }
    }

    #[test]
    fn a_missing_saved_request_is_an_error_naming_the_node() {
        let f = flow(vec![saved_request("n1", "Log in", "auth/login.yml")], vec![]);
        let lints = validate_with_warnings(&f, &missing("auth/login.yml"));
        let found = only(&lints, DANGLING_SAVED_REQUEST);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, LintSeverity::Error);
        assert_eq!(found[0].node_id.as_deref(), Some("n1"));
        assert_eq!(found[0].edge_id, None);
        assert_eq!(
            found[0].message,
            "'Log in' uses a saved request that no longer exists in this collection."
        );
        let hint = found[0].hint.as_deref().expect("a hint");
        assert!(hint.starts_with("Pick another saved request"), "got: {hint}");
    }

    #[test]
    fn the_message_never_quotes_the_path() {
        let f = flow(vec![saved_request("n1", "Log in", "private/s3cr3t-dir/login.yml")], vec![]);
        let lints = validate_with_warnings(&f, &missing("private/s3cr3t-dir/login.yml"));
        for l in &lints {
            let text = format!("{} {}", l.message, l.hint.as_deref().unwrap_or(""));
            assert!(!text.contains("s3cr3t"), "leaked in: {text}");
            assert!(!text.contains("login.yml"), "leaked in: {text}");
        }
    }

    #[test]
    fn an_existing_or_unanswered_request_has_no_lint() {
        let f = flow(
            vec![
                saved_request("a", "A", "a.yml"),
                saved_request("b", "B", "b.yml"),
                saved_request("c", "C", "c.yml"),
            ],
            vec![],
        );
        let ctx = FakeContext {
            requests: vec![("a.yml", true), ("b.yml", true)],
            variables: vec![],
        };
        // `c.yml` is not listed, so the context cannot tell and the rule is skipped.
        assert!(only(&validate_with_warnings(&f, &ctx), DANGLING_SAVED_REQUEST).is_empty());
        assert!(only(&lint(&f), DANGLING_SAVED_REQUEST).is_empty());
    }

    #[test]
    fn an_inline_request_is_never_checked() {
        let f = flow(vec![inline_request("a", "A", "https://x.test", &[], None)], vec![]);
        let counting = Counting::default();
        let lints = validate_with_warnings(&f, &counting);
        assert!(only(&lints, DANGLING_SAVED_REQUEST).is_empty());
        assert_eq!(counting.requests.get(), 0);
    }

    #[test]
    fn lints_follow_node_order_and_each_node_asks_once() {
        let f = flow(
            vec![
                saved_request("z", "Zed", "z.yml"),
                saved_request("a", "Aye", "a.yml"),
            ],
            vec![],
        );
        let lints = validate_with_warnings(&f, &EverythingMissing);
        let ids: Vec<_> = only(&lints, DANGLING_SAVED_REQUEST)
            .iter()
            .map(|l| l.node_id.as_deref())
            .collect();
        assert_eq!(ids, vec![Some("z"), Some("a")]);
        let counting = Counting::default();
        let _ = validate_with_warnings(&f, &counting);
        assert_eq!(counting.requests.get(), 2);
    }

    /// n saved Request nodes and no wires.
    fn saved_requests(n: usize) -> Flow {
        flow(
            (0..n)
                .map(|i| saved_request(&format!("n{i}"), &format!("Req {i}"), &format!("r{i}.yml")))
                .collect(),
            vec![],
        )
    }

    #[test]
    fn saved_request_rule_is_linear() {
        let lints = validate_with_warnings(&saved_requests(20_000), &EverythingMissing);
        assert_eq!(only(&lints, DANGLING_SAVED_REQUEST).len(), 20_000);
        assert_linear(saved_requests, |f| {
            let _ = validate_with_warnings(f, &EverythingMissing);
        });
    }

    /// An inline Request. Header pairs are `(name, value)`.
    fn inline_request(
        id: &str,
        label: &str,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                label: label.to_string(),
                source: RequestSource::Inline {
                    request: InlineRequestData {
                        method: "GET".to_string(),
                        url: url.to_string(),
                        headers: headers
                            .iter()
                            .map(|(name, value)| InlineHeader {
                                name: name.to_string(),
                                value: value.to_string(),
                            })
                            .collect(),
                        body: body.map(str::to_string),
                    },
                },
                debug: false,
                repeat_until: None,
            },
        )
    }
```

(The `inline_request` helper is defined here because the test above uses it, and Task 3 reuses it.) `LintContext` is already in scope in the tests through `use super::*`.

In `crates/rocket-app/src/flow_service.rs` tests add (it needs Task 1's `LintSources`; `ConfigurableCollectionRepo` has no request, so every path is missing):

```rust
    fn lint_sources_with_nothing_saved() -> crate::flow_lint_context::LintSources {
        use crate::test_doubles::{ConfigurableCollectionRepo, NullEnvRepo, SharedCollectionRepo};
        struct NoEnvs;
        impl rocket_environment::EnvironmentRepositoryFactory for NoEnvs {
            fn for_collection(
                &self,
                _collection: &str,
            ) -> Box<dyn rocket_environment::EnvironmentRepository> {
                Box::new(NullEnvRepo)
            }
        }
        crate::flow_lint_context::LintSources {
            collection_repo: Box::new(SharedCollectionRepo(ConfigurableCollectionRepo::new())),
            env_factory: Box::new(NoEnvs),
            global_env_repo: Box::new(NullEnvRepo),
        }
    }

    fn flow_with_saved_request() -> Flow {
        Flow {
            name: "Dangling".to_string(),
            nodes: vec![node_of(
                "req",
                FlowNodeKind::Request {
                    label: "Log in".to_string(),
                    source: rocket_flow::RequestSource::Saved {
                        request_path: "auth/login.yml".to_string(),
                    },
                    debug: false,
                    repeat_until: None,
                },
            )],
            edges: vec![],
            callback_host: None,
        }
    }

    #[test]
    fn lint_reports_a_saved_request_that_is_gone() {
        let svc = FlowService::new(Box::new(UntouchedRepo))
            .with_lint_sources(lint_sources_with_nothing_saved());
        let lints = svc.lint("demo", &flow_with_saved_request());
        assert_eq!(
            lint_keys(&lints),
            vec![(Some("req"), None, rocket_flow::lint::DANGLING_SAVED_REQUEST)]
        );
        assert_eq!(lints[0].severity, LintSeverity::Error);
    }

    #[test]
    fn a_flow_with_a_dangling_request_still_saves() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::new()))
            .with_lint_sources(lint_sources_with_nothing_saved());
        let flow = flow_with_saved_request();
        assert!(!svc.lint("demo", &flow).is_empty());
        svc.save("demo", flow.clone())
            .expect("a flow with a dangling request must still save");
        assert_eq!(svc.get("demo", "Dangling").expect("get"), flow);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow lint::` and `cargo test -j4 -p rocket-app flow_service`
Expected: compile errors `cannot find value DANGLING_SAVED_REQUEST` in both crates.

- [ ] **Step 3: Write the implementation**

In `lint.rs`:

1. Change `use crate::node::FlowNodeKind;` to `use crate::node::{FlowNodeKind, RequestSource};`.
2. After `NO_PATH_TO_OUTPUT` add:

```rust
/// A Request node uses a saved request that is no longer in the collection.
pub const DANGLING_SAVED_REQUEST: &str = "dangling_saved_request";
```

3. Replace the `warning` helper with a general helper plus `warning`:

```rust
fn node_lint(
    severity: LintSeverity,
    code: &str,
    node: &FlowNode,
    message: String,
    hint: &str,
) -> FlowLint {
    FlowLint {
        code: code.to_string(),
        severity,
        node_id: Some(node.id.clone()),
        edge_id: None,
        message,
        hint: Some(hint.to_string()),
    }
}

fn warning(code: &str, node: &FlowNode, message: String, hint: &str) -> FlowLint {
    node_lint(LintSeverity::Warning, code, node, message, hint)
}
```

4. In `validate_with_warnings` delete the two lines `// No rule asks the context yet...` and `let _ = ctx;`, and after `lints.extend(no_path_lint(node, &index));` add:

```rust
        lints.extend(dangling_request_lint(node, ctx));
```

5. Add the rule after `no_path_lint`:

```rust
/// A Request node whose saved request the context says is gone. A run that
/// reaches the node reads the same file and fails there, so this is an error
/// lint. A node the context cannot answer for is skipped. The path is not
/// quoted.
fn dangling_request_lint(node: &FlowNode, ctx: &dyn LintContext) -> Option<FlowLint> {
    let FlowNodeKind::Request {
        source: RequestSource::Saved { request_path },
        ..
    } = &node.kind
    else {
        return None;
    };
    if ctx.saved_request_exists(request_path) != Some(false) {
        return None;
    }
    Some(node_lint(
        LintSeverity::Error,
        DANGLING_SAVED_REQUEST,
        node,
        format!(
            "'{}' uses a saved request that no longer exists in this collection.",
            display_label(node)
        ),
        "Pick another saved request in the node's properties, or restore the request file. A run that reaches this node fails there.",
    ))
}
```

Update the module doc of `validate_with_warnings` nothing else. The existing `exit_lints` and `no_path_lint` keep calling `warning`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-flow lint`, `cargo test -j4 -p rocket-app flow_service`, `cargo test -j4 -p rocket-app flow_lint_context`, `cargo check -j4 -p rocket-flow -p rocket-app`, and `rustfmt --check --edition 2021 crates/rocket-flow/src/lint.rs` (ignore pre-existing diffs outside your lines).
Expected: all PASS, including the older lint tests (they use `NoLintContext`, so the new rule is silent there).

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:
`git add crates/rocket-flow/src/lint.rs crates/rocket-app/src/flow_service.rs && git commit --only -m "feat(flow): lint a Request node whose saved request is gone" -- <the same paths>`

---

### Task 3: Lint unknown variable, and the client feed (F-22)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-flow/src/lint.rs` (const ~21, `GraphIndex` ~92-125, `validate_with_warnings`, new rule near `dangling_request_lint`, tests)
- Modify: `crates/rocket-app/src/flow_service.rs` (tests)
- Modify: `src/lib/tauri-api.ts` (`lintFlow` ~2280-2282)
- Modify: `src/hooks/useBackendFlowLints.ts` (whole file, ~65 lines)
- Modify: `src/components/flow/FlowPane.tsx` (`backendIssues` ~175)
- Test: `src/lib/__tests__/tauri-api.flow-lint.test.ts`, `src/hooks/__tests__/useBackendFlowLints.test.tsx`, `src/components/flow/__tests__/FlowPane.lintFeed.test.tsx`, `src/lib/__tests__/flow-lint.test.ts`

**Interfaces:**
- Consumes: `LintContext::variable_is_known(&self, &str) -> Option<bool>`, `InlineRequestData { url, headers, body }`, `FlowNodeKind::Input { value }` (`VariableValue::data() -> &str`), `FlowEdge::target_field`.
- Produces in `lint.rs`:
  - `pub const UNKNOWN_VARIABLE: &str = "unknown_variable";`
  - `fn unknown_variable_lint(node: &FlowNode, index: &GraphIndex<'_>, ctx: &dyn LintContext) -> Option<FlowLint>` (private, Warning).
  - `GraphIndex::wired_fields(&self, node_id: &str) -> WiredFields` where `#[derive(Clone, Copy, Default)] struct WiredFields { url: bool, body: bool, headers: bool }` (private).
- Produces on the client: `export interface FlowLintEnvironment { environmentName?: string | null; globalEnvName?: string | null }` in `tauri-api.ts`; `lintFlow(collection, flow, environment?)`; `useBackendFlowLints(collection, flow, delayMs?, options?: { environment?: FlowLintEnvironment; refreshKey?: number })`.
- Severity: `Warning`. The request still goes out with the literal text, a server may even accept it, and a name could be defined by something the context cannot see. A warning fits.
- Scope of the rule, verified in `flow_execution_service.rs`: an inline Request resolves `url`, header names and values, and `body` against the variable context. Input node values go through `rocket_environment::resolve` too. Both leave an unknown name as literal text. `If`, `Switch` and `Transform` fields are scripts, not templates, so they are not linted. A saved request's own fields are not linted either (open decision 4).
- Which names are linted: the text between `{{` and the first `}}`, trimmed, when it is 1 to 64 characters of letters, digits, `_`, `-` or `.`. This mirrors `rocket_environment::resolve`, and the character filter keeps JSON, GraphQL and script text that happens to contain `{{` from producing noise. `$dynamic`, `process.env.*` and `callback.*` names are never linted by the rule (the character filter drops `$`, and the two prefixes are excluded explicitly).

- [ ] **Step 1: Write the failing tests**

`lint.rs` tests (append; `inline_request`, `FakeContext`, `EverythingMissing`, `Counting`, `only`, `lint`, `assert_linear` already exist from Task 2 and earlier):

```rust
    fn unknown_in(f: &Flow) -> Vec<FlowLint> {
        let all = validate_with_warnings(f, &EverythingMissing);
        all.into_iter().filter(|l| l.code == UNKNOWN_VARIABLE).collect()
    }

    #[test]
    fn unknown_names_in_url_headers_and_body_are_one_warning_listing_names() {
        let f = flow(
            vec![inline_request(
                "n1",
                "Create user",
                "https://x.test/{{userId}}?k={{ apiKey }}",
                &[("X-{{tenant}}", "{{userId}}")],
                Some("{\"a\": \"{{payload}}\"}"),
            )],
            vec![],
        );
        let found = unknown_in(&f);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, LintSeverity::Warning);
        assert_eq!(found[0].node_id.as_deref(), Some("n1"));
        assert_eq!(
            found[0].message,
            "'Create user' leaves {{userId}}, {{apiKey}}, {{tenant}}, {{payload}} as literal text, because no variable has those names."
        );
        let hint = found[0].hint.as_deref().expect("a hint");
        assert!(hint.starts_with("Define the variable"), "got: {hint}");
    }

    #[test]
    fn one_unknown_name_reads_in_the_singular() {
        let f = flow(
            vec![inline_request("n1", "Get", "https://x.test/{{id}}/{{id}}", &[], None)],
            vec![],
        );
        assert_eq!(
            unknown_in(&f)[0].message,
            "'Get' leaves {{id}} as literal text, because no variable has that name."
        );
    }

    #[test]
    fn more_than_five_names_are_cut_with_a_count() {
        let f = flow(
            vec![inline_request(
                "n1",
                "Big",
                "{{a}}{{b}}{{c}}{{d}}{{e}}{{f}}{{g}}",
                &[],
                None,
            )],
            vec![],
        );
        assert_eq!(
            unknown_in(&f)[0].message,
            "'Big' leaves {{a}}, {{b}}, {{c}}, {{d}}, {{e}} and 2 more as literal text, because no variable has those names."
        );
    }

    #[test]
    fn known_and_unanswered_names_are_skipped() {
        let f = flow(
            vec![inline_request("n1", "Get", "{{known}}/{{maybe}}/{{gone}}", &[], None)],
            vec![],
        );
        let ctx = FakeContext {
            requests: vec![],
            variables: vec![("known", true), ("gone", false)],
        };
        let lints = validate_with_warnings(&f, &ctx);
        let found = only(&lints, UNKNOWN_VARIABLE);
        assert_eq!(found.len(), 1);
        // `maybe` got None and `known` got true, so only `gone` is named.
        assert!(found[0].message.contains("{{gone}}"));
        assert!(!found[0].message.contains("maybe"));
        assert!(!found[0].message.contains("known"));
        assert!(only(&lint(&f), UNKNOWN_VARIABLE).is_empty());
    }

    #[test]
    fn run_time_and_odd_names_are_never_linted() {
        let f = flow(
            vec![inline_request(
                "n1",
                "Get",
                "{{$guid}} {{ $randomInt }} {{process.env.HOME}} {{callback.pay}} {{}} {{\"x\":1}} {{a b}} {{open",
                &[],
                Some("query {{ user { id } }}"),
            )],
            vec![],
        );
        // EverythingMissing says "no" to anything it is asked, so silence proves the rule's own filter.
        assert!(unknown_in(&f).is_empty());
    }

    #[test]
    fn the_message_lists_names_and_nothing_else() {
        let f = flow(
            vec![inline_request(
                "n1",
                "Get",
                "https://secret-host.test/{{token}}?key=s3cr3t",
                &[("Authorization", "Bearer s3cr3t {{token}}")],
                Some("{\"password\": \"s3cr3t\"}"),
            )],
            vec![],
        );
        let found = unknown_in(&f);
        let text = format!("{} {}", found[0].message, found[0].hint.as_deref().unwrap_or(""));
        for leaked in ["s3cr3t", "secret-host", "https", "Bearer", "password"] {
            assert!(!text.contains(leaked), "{leaked} leaked in: {text}");
        }
    }

    #[test]
    fn a_wired_field_is_not_linted() {
        let f = flow(
            vec![
                input("in"),
                inline_request(
                    "n1",
                    "Get",
                    "{{urlVar}}",
                    &[("X-A", "{{headerVar}}")],
                    Some("{{bodyVar}}"),
                ),
            ],
            vec![
                edge("e1", "in", handle::RESULT, "n1", "url"),
                edge("e2", "in", handle::RESULT, "n1", "headers[X-A].value"),
                edge("e3", "in", handle::RESULT, "n1", "body"),
            ],
        );
        assert!(unknown_in(&f).is_empty());
        // A wire into one field leaves the other fields linted.
        let f = flow(
            vec![input("in"), inline_request("n1", "Get", "{{urlVar}}", &[], Some("{{bodyVar}}"))],
            vec![edge("e1", "in", handle::RESULT, "n1", "url")],
        );
        let found = unknown_in(&f);
        assert!(found[0].message.contains("{{bodyVar}}"));
        assert!(!found[0].message.contains("urlVar"));
    }

    #[test]
    fn an_input_node_value_is_linted() {
        let f = flow(
            vec![node(
                "in",
                FlowNodeKind::Input {
                    label: "User".to_string(),
                    value: rocket_shared::VariableValue::simple("{{who}}"),
                },
            )],
            vec![],
        );
        assert_eq!(
            unknown_in(&f)[0].message,
            "'User' leaves {{who}} as literal text, because no variable has that name."
        );
    }

    #[test]
    fn a_saved_request_has_no_variable_lint() {
        let f = flow(vec![saved_request("n1", "Log in", "{{x}}.yml")], vec![]);
        assert!(unknown_in(&f).is_empty());
    }

    #[test]
    fn the_rule_asks_once_per_distinct_name_per_node() {
        let f = flow(
            vec![inline_request("n1", "Get", "{{a}}{{a}}{{b}}", &[("H", "{{a}}")], Some("{{b}}"))],
            vec![],
        );
        let counting = Counting::default();
        let _ = validate_with_warnings(&f, &counting);
        assert_eq!(counting.variables.get(), 2);
    }

    /// n inline Request nodes that each use one unknown name.
    fn inline_requests(n: usize) -> Flow {
        flow(
            (0..n)
                .map(|i| inline_request(&format!("n{i}"), &format!("Req {i}"), &format!("{{{{v{i}}}}}"), &[], None))
                .collect(),
            vec![],
        )
    }

    #[test]
    fn unknown_variable_rule_is_linear() {
        assert_eq!(unknown_in(&inline_requests(20_000)).len(), 20_000);
        assert_linear(inline_requests, |f| {
            let _ = validate_with_warnings(f, &EverythingMissing);
        });
    }
```

`flow_service.rs` tests (append):

```rust
    fn inline_flow(url: &str) -> Flow {
        Flow {
            name: "Inline".to_string(),
            nodes: vec![node_of(
                "req",
                FlowNodeKind::Request {
                    label: "Get".to_string(),
                    source: rocket_flow::RequestSource::Inline {
                        request: rocket_flow::InlineRequestData {
                            method: "GET".to_string(),
                            url: url.to_string(),
                            headers: vec![],
                            body: None,
                        },
                    },
                    debug: false,
                    repeat_until: None,
                },
            )],
            edges: vec![],
            callback_host: None,
        }
    }

    #[test]
    fn lint_reports_an_unknown_variable_for_the_active_environment() {
        use crate::test_doubles::{ConfigurableCollectionRepo, NullEnvRepo, SharedCollectionRepo, StaticEnvRepo};
        use rocket_environment::{Environment, Variable};
        struct OneEnv(Environment);
        impl rocket_environment::EnvironmentRepositoryFactory for OneEnv {
            fn for_collection(
                &self,
                _collection: &str,
            ) -> Box<dyn rocket_environment::EnvironmentRepository> {
                Box::new(StaticEnvRepo(self.0.clone()))
            }
        }
        let mut env = Environment::new("dev");
        env.variables = vec![Variable::new("host", "h")];
        let svc = FlowService::new(Box::new(UntouchedRepo)).with_lint_sources(
            crate::flow_lint_context::LintSources {
                collection_repo: Box::new(SharedCollectionRepo(ConfigurableCollectionRepo::new())),
                env_factory: Box::new(OneEnv(env)),
                global_env_repo: Box::new(NullEnvRepo),
            },
        );
        let flow = inline_flow("{{host}}/{{typo}}");
        let dev = crate::flow_lint_context::LintEnvironment {
            environment_name: Some("dev"),
            global_env_name: None,
        };
        let lints = svc.lint_with_environment("demo", &flow, dev);
        assert_eq!(
            lint_keys(&lints),
            vec![(Some("req"), None, rocket_flow::lint::UNKNOWN_VARIABLE)]
        );
        assert!(lints[0].message.contains("{{typo}}"));
        assert!(!lints[0].message.contains("host"));
        // No environment selected: the rule cannot tell, so it stays silent.
        assert!(svc.lint("demo", &flow).is_empty());
    }

    #[test]
    fn a_saved_request_in_the_flow_silences_the_variable_rule() {
        let svc = FlowService::new(Box::new(UntouchedRepo))
            .with_lint_sources(lint_sources_with_nothing_saved());
        let mut flow = inline_flow("{{typo}}");
        flow.nodes.push(node_of(
            "saved",
            FlowNodeKind::Request {
                label: "Saved".to_string(),
                source: rocket_flow::RequestSource::Saved {
                    request_path: "a.yml".to_string(),
                },
                debug: false,
                repeat_until: None,
            },
        ));
        let dev = crate::flow_lint_context::LintEnvironment {
            environment_name: Some("dev"),
            global_env_name: None,
        };
        let lints = svc.lint_with_environment("demo", &flow, dev);
        assert!(
            lints.iter().all(|l| l.code != rocket_flow::lint::UNKNOWN_VARIABLE),
            "a saved request may set variables at run time"
        );
    }
```

Client tests.

`src/lib/__tests__/tauri-api.flow-lint.test.ts`: change the first test's assertion and add one:

```ts
  it('sends the unsaved graph and returns the lints', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await expect(lintFlow('demo', flow)).resolves.toEqual([]);
    expect(invoke).toHaveBeenCalledWith('lint_flow', {
      collection: 'demo',
      flow,
      environmentName: null,
      globalEnvName: null,
    });
  });

  it('sends the environment names the run would use', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await lintFlow('demo', flow, { environmentName: 'dev', globalEnvName: 'shared' });
    expect(invoke).toHaveBeenCalledWith('lint_flow', {
      collection: 'demo',
      flow,
      environmentName: 'dev',
      globalEnvName: 'shared',
    });
  });
```

`src/hooks/__tests__/useBackendFlowLints.test.tsx`: update the assertion at line 61 to `expect(lintFlow).toHaveBeenCalledWith('demo', outputs(2), { environmentName: null, globalEnvName: null });` and add inside the `describe`:

```tsx
  it('lints again when the active environment changes', async () => {
    vi.mocked(lintFlow).mockResolvedValue([]);
    const { rerender } = renderHook(
      ({ env }) =>
        useBackendFlowLints('demo', outputs(0), undefined, {
          environment: { environmentName: env, globalEnvName: null },
        }),
      { initialProps: { env: 'dev' } },
    );
    await flush();
    expect(lintFlow).toHaveBeenLastCalledWith('demo', outputs(0), {
      environmentName: 'dev',
      globalEnvName: null,
    });
    rerender({ env: 'prod' });
    await flush();
    expect(lintFlow).toHaveBeenCalledTimes(2);
    expect(lintFlow).toHaveBeenLastCalledWith('demo', outputs(0), {
      environmentName: 'prod',
      globalEnvName: null,
    });
  });

  it('lints again when the refresh key changes, such as after a save', async () => {
    vi.mocked(lintFlow).mockResolvedValue([]);
    const { rerender } = renderHook(
      ({ refreshKey }) =>
        useBackendFlowLints('demo', outputs(0), undefined, { refreshKey }),
      { initialProps: { refreshKey: 0 } },
    );
    await flush();
    rerender({ refreshKey: 0 });
    await flush();
    expect(lintFlow).toHaveBeenCalledTimes(1);
    rerender({ refreshKey: 1 });
    await flush();
    expect(lintFlow).toHaveBeenCalledTimes(2);
  });
```

`src/components/flow/__tests__/FlowPane.lintFeed.test.tsx`: change the call assertion in the first test to
`expect(lintFlow).toHaveBeenCalledWith('demo', expect.objectContaining({ name: 'lint' }), expect.any(Object))` and add (inside the describe; add `act` to the `@testing-library/react` import and `import userEvent from '@testing-library/user-event';`):

```tsx
  it('shows the dangling request and unknown variable lints in the issue list', async () => {
    vi.mocked(lintFlow).mockResolvedValue([
      {
        code: 'dangling_saved_request',
        severity: 'error',
        nodeId: 'req1',
        message: "'Login' uses a saved request that no longer exists in this collection.",
        hint: "Pick another saved request in the node's properties, or restore the request file.",
      },
      {
        code: 'unknown_variable',
        severity: 'warning',
        nodeId: 'in1',
        message: "'User' leaves {{who}} as literal text, because no variable has that name.",
      },
    ]);
    render(<Harness />);
    const button = await screen.findByRole('button', { name: /1 error, 1 warning/ });
    await userEvent.click(button);
    const list = await screen.findByRole('list', { name: 'Flow issues' });
    expect(list).toHaveTextContent('no longer exists in this collection');
    expect(list).toHaveTextContent('{{who}} as literal text');
  });

  it('lints again after a save clears the dirty flag', async () => {
    vi.mocked(lintFlow).mockResolvedValue([]);
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab({ ...baseTab, isDirty: true });
    render(<Harness />);
    await waitFor(() => expect(lintFlow).toHaveBeenCalledTimes(1));
    act(() => usePaneStore.getState().markClean(baseTab.id));
    await waitFor(() => expect(lintFlow).toHaveBeenCalledTimes(2), { timeout: 3000 });
  });
```

`src/lib/__tests__/flow-lint.test.ts` (inside the existing describe that imports `toFlowIssue`, or a new one):

```ts
describe('the F-21 and F-22 codes', () => {
  it('are owned by the backend', () => {
    expect(BACKEND_LINT_CODES).toEqual(
      expect.arrayContaining(['dangling_saved_request', 'unknown_variable']),
    );
  });

  it('map to client issues with their node, severity and hint', () => {
    expect(
      toFlowIssue({
        code: 'dangling_saved_request',
        severity: 'error',
        nodeId: 'req1',
        message: "'Login' uses a saved request that no longer exists in this collection.",
        hint: 'Pick another saved request.',
      }),
    ).toEqual({
      code: 'dangling_saved_request',
      severity: 'error',
      nodeId: 'req1',
      message: "'Login' uses a saved request that no longer exists in this collection.",
      hint: 'Pick another saved request.',
    });
    expect(
      toFlowIssue({ code: 'unknown_variable', severity: 'warning', nodeId: 'n', message: 'm' }),
    ).toEqual({ code: 'unknown_variable', severity: 'warning', nodeId: 'n', message: 'm' });
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow lint` (compile error: `UNKNOWN_VARIABLE` not found), `yarn test tauri-api.flow-lint useBackendFlowLints FlowPane.lintFeed flow-lint --run --maxWorkers=2`.
Expected: Rust fails to compile. Client: the `lintFlow` payload test fails (no `environmentName` key), the two hook tests fail (no 4th parameter, `lintFlow` called with two arguments), the FlowPane assertion `expect.any(Object)` fails (two arguments), the "lints again after a save" test fails (one call). The `flow-lint.test.ts` additions already pass, since the codes and the mapping exist. That is expected: they pin the existing behavior for the new codes.

- [ ] **Step 3: Write the implementation**

`lint.rs`:

1. After `DANGLING_SAVED_REQUEST` add:

```rust
/// An inline Request or an Input node sends `{{name}}` as literal text,
/// because no variable has that name.
pub const UNKNOWN_VARIABLE: &str = "unknown_variable";
```

2. Add near the top of the file (after the codes) the limits:

```rust
/// Longest variable name the variable rule lints.
const MAX_NAME_LEN: usize = 64;
/// How many names a message lists before it counts the rest.
const MAX_LISTED_NAMES: usize = 5;
```

3. `GraphIndex`: add the field, the type and the method; replace `GraphIndex::new`:

```rust
/// Which fields of a node a wire replaces.
#[derive(Debug, Clone, Copy, Default)]
struct WiredFields {
    url: bool,
    body: bool,
    /// Any `headers[...]` wire. One flag, so a selector the rule cannot
    /// match to a header never leads to a false lint.
    headers: bool,
}
```

```rust
struct GraphIndex<'a> {
    /// The exits of each node that have at least one wire.
    wired_exits: HashMap<&'a str, HashSet<&'a str>>,
    /// The request fields each node receives by wire.
    wired_fields: HashMap<&'a str, WiredFields>,
    /// Ids with a path to an Output, or `None` when the flow has no Output.
    reaching_output: Option<HashSet<&'a str>>,
}

impl<'a> GraphIndex<'a> {
    fn new(flow: &'a Flow) -> Self {
        let mut wired_exits: HashMap<&'a str, HashSet<&'a str>> = HashMap::new();
        let mut wired_fields: HashMap<&'a str, WiredFields> = HashMap::new();
        for edge in &flow.edges {
            wired_exits
                .entry(edge.source_node_id.as_str())
                .or_default()
                .insert(edge.source_handle.as_str());
            let fields = wired_fields.entry(edge.target_node_id.as_str()).or_default();
            match edge.target_field.as_str() {
                "url" => fields.url = true,
                "body" => fields.body = true,
                field if field.starts_with("headers[") => fields.headers = true,
                _ => {}
            }
        }
        Self {
            wired_exits,
            wired_fields,
            reaching_output: nodes_reaching_output(flow),
        }
    }

    fn is_wired(&self, node_id: &str, exit: &str) -> bool {
        self.wired_exits
            .get(node_id)
            .is_some_and(|exits| exits.contains(exit))
    }

    fn wired_fields(&self, node_id: &str) -> WiredFields {
        self.wired_fields.get(node_id).copied().unwrap_or_default()
    }
}
```

4. In `validate_with_warnings`, after the `dangling_request_lint` line add:

```rust
        lints.extend(unknown_variable_lint(node, &index, ctx));
```

5. Add after `dangling_request_lint`:

```rust
/// The text of `{{name}}` placeholders in `text` that the variable rule
/// may lint, first seen first, each name once. This reads the text like
/// `rocket_environment::resolve`: the name runs to the first `}}` and is
/// trimmed. An unclosed `{{` is ignored. Only plain variable-like names
/// are kept, so JSON, GraphQL or script text that holds `{{` adds nothing.
fn placeholder_names<'a>(text: &'a str, seen: &mut HashSet<&'a str>, out: &mut Vec<&'a str>) {
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return;
        };
        let name = after[..end].trim();
        if lintable_name(name) && seen.insert(name) {
            out.push(name);
        }
        rest = &after[end + 2..];
    }
}

/// A plain variable name that the run resolves from the scopes the
/// context reads. Dynamic (`$`), `process.env.` and `callback.` names are
/// resolved by other means, so they are left out.
fn lintable_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !name.starts_with("process.env.")
        && !name.starts_with("callback.")
}

fn braced(name: &str) -> String {
    let mut text = String::from("{{");
    text.push_str(name);
    text.push_str("}}");
    text
}

/// One warning per node that sends `{{name}}` as literal text. It covers an
/// inline Request's url, header names and values and body, and an Input
/// node's value, which are the fields the run resolves from variables and
/// leaves as written when a name is missing. A field that a wire replaces
/// is skipped. The message lists variable names only, never any other text.
fn unknown_variable_lint(
    node: &FlowNode,
    index: &GraphIndex<'_>,
    ctx: &dyn LintContext,
) -> Option<FlowLint> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut names: Vec<&str> = Vec::new();
    match &node.kind {
        FlowNodeKind::Request {
            source: RequestSource::Inline { request },
            ..
        } => {
            let wired = index.wired_fields(&node.id);
            if !wired.url {
                placeholder_names(&request.url, &mut seen, &mut names);
            }
            for header in &request.headers {
                placeholder_names(&header.name, &mut seen, &mut names);
                if !wired.headers {
                    placeholder_names(&header.value, &mut seen, &mut names);
                }
            }
            if let (Some(body), false) = (&request.body, wired.body) {
                placeholder_names(body, &mut seen, &mut names);
            }
        }
        FlowNodeKind::Input { value, .. } => placeholder_names(value.data(), &mut seen, &mut names),
        _ => return None,
    }
    let unknown: Vec<&str> = names
        .into_iter()
        .filter(|name| ctx.variable_is_known(name) == Some(false))
        .collect();
    if unknown.is_empty() {
        return None;
    }
    let mut list = unknown
        .iter()
        .take(MAX_LISTED_NAMES)
        .map(|name| braced(name))
        .collect::<Vec<_>>()
        .join(", ");
    if unknown.len() > MAX_LISTED_NAMES {
        list.push_str(&format!(" and {} more", unknown.len() - MAX_LISTED_NAMES));
    }
    let which = if unknown.len() == 1 {
        "that name"
    } else {
        "those names"
    };
    Some(warning(
        UNKNOWN_VARIABLE,
        node,
        format!(
            "'{}' leaves {list} as literal text, because no variable has {which}.",
            display_label(node)
        ),
        "Define the variable in the collection variables or in the active environment, or correct the name in the node.",
    ))
}
```

`src/lib/tauri-api.ts` (replace `lintFlow`):

```ts
/** The environments a lint reads, by name. Null or absent means none is selected. */
export interface FlowLintEnvironment {
  environmentName?: string | null;
  globalEnvName?: string | null;
}

/**
 * Lints the graph as the canvas holds it now, saved or not. The environment
 * names are the ones a run would use, so the variable rule can tell a typo
 * from a defined name. Without them that rule stays silent.
 */
export const lintFlow = (collection: string, flow: Flow, environment: FlowLintEnvironment = {}) =>
  invoke<FlowLint[]>('lint_flow', {
    collection,
    flow,
    environmentName: environment.environmentName ?? null,
    globalEnvName: environment.globalEnvName ?? null,
  });
```

`src/hooks/useBackendFlowLints.ts` (full replacement):

```ts
import { useEffect, useRef, useState } from 'react';
import type { FlowIssue } from '@/lib/flow-issues';
import { LINT_DEBOUNCE_MS, toFlowIssue } from '@/lib/flow-lint';
import { type Flow, type FlowLintEnvironment, lintFlow } from '@/lib/tauri-api';

export interface BackendLintOptions {
  /** The environments a run would use. A change lints again. */
  environment?: FlowLintEnvironment;
  /** Change this to lint again without an edit, for example after a save. */
  refreshKey?: number;
}

/**
 * Backend lint issues for the graph the canvas holds now. Lints at once
 * when a flow opens and after each pause in editing. It lints again when the
 * environment names or `refreshKey` change, because a saved request or a
 * variable may have changed outside the graph. A failed call shows no
 * backend issues; lints never block Run or Save.
 */
export function useBackendFlowLints(
  collection: string | null,
  flow: Flow | null,
  delayMs: number = LINT_DEBOUNCE_MS,
  options: BackendLintOptions = {},
): FlowIssue[] {
  const environmentName = options.environment?.environmentName ?? null;
  const globalEnvName = options.environment?.globalEnvName ?? null;
  const refreshKey = options.refreshKey ?? 0;
  const [issues, setIssues] = useState<FlowIssue[]>([]);
  // Bumped by every cleanup, so a response from an older effect is ignored.
  const generationRef = useRef(0);
  // Read when the timer fires, so the effect depends on the graph's content only.
  const flowRef = useRef(flow);
  flowRef.current = flow;
  // Node positions stay out of the key, so dragging a node does not lint.
  const key =
    collection && flow
      ? JSON.stringify({
          ...flow,
          nodes: flow.nodes.map((node) => ({ id: node.id, kind: node.kind })),
          refreshKey,
        })
      : null;
  // The flow whose lint ran last, so the first lint of a newly opened flow skips the delay.
  const lintedFlowRef = useRef<string | null>(null);
  const openKey = collection && flow ? `${collection}|${flow.name}` : null;

  useEffect(() => {
    const generation = generationRef.current;
    if (!collection || key === null) {
      setIssues([]);
      return;
    }
    const wait = lintedFlowRef.current === openKey ? delayMs : 0;
    lintedFlowRef.current = openKey;
    const timer = setTimeout(() => {
      const current = flowRef.current;
      if (!current) return;
      lintFlow(collection, current, { environmentName, globalEnvName })
        .then((lints) => {
          if (generationRef.current === generation) setIssues(lints.map(toFlowIssue));
        })
        .catch((err: unknown) => {
          if (generationRef.current !== generation) return;
          console.warn('[flow-lint] lint_flow failed', err);
          setIssues([]);
        });
    }, wait);
    return () => {
      clearTimeout(timer);
      generationRef.current += 1;
    };
  }, [collection, key, openKey, delayMs, environmentName, globalEnvName]);

  return issues;
}
```

`src/components/flow/FlowPane.tsx`: replace the `const backendIssues = useBackendFlowLints(...)` line with:

```tsx
  // A save lints again, because a saved request or a variable may have changed
  // since the last edit. Undoing back to the saved state also clears the flag,
  // which costs one extra lint and is harmless.
  const [lintRefresh, setLintRefresh] = useState(0);
  const wasDirtyRef = useRef(tab.isDirty);
  useEffect(() => {
    if (wasDirtyRef.current && !tab.isDirty) setLintRefresh((n) => n + 1);
    wasDirtyRef.current = tab.isDirty;
  }, [tab.isDirty]);
  const backendIssues = useBackendFlowLints(tab.collectionName ?? null, lintTarget, undefined, {
    environment: {
      environmentName: activeEnvironmentName,
      globalEnvName: getActiveGlobalEnvName() ?? null,
    },
    refreshKey: lintRefresh,
  });
```

(`useState`, `useRef`, `useEffect` and `getActiveGlobalEnvName` are already imported in this file. `activeEnvironmentName` is declared above, at ~94. The global environment name is read at render time, like the run toolbar does at click time, so it is picked up on the next render after a switch and not reactively. Open decision 6.)

Roadmap: mark F-21 and F-22 `done` in `.claude/flow-roadmap.md` with the commit ids and the evidence `lint.rs` `dangling_request_lint` and `unknown_variable_lint`, and move the "Run at save and when a flow opens" note to "done via useBackendFlowLints". That file is untracked in this checkout, so edit it but do not include it in the commit.

- [ ] **Step 4: Run the tests to verify they pass**

Run, in order:
- `cargo test -j4 -p rocket-flow lint`, `cargo test -j4 -p rocket-app flow_service`, `cargo test -j4 -p rocket-app flow_lint_context`
- `cargo check -j4 -p rocket-flow -p rocket-app -p rocket`
- `rustfmt --check --edition 2021 crates/rocket-flow/src/lint.rs crates/rocket-app/src/flow_service.rs crates/rocket-app/src/flow_lint_context.rs` (ignore diffs on lines you did not write)
- `yarn tsc --noEmit`, `yarn check`
- `yarn test tauri-api.flow-lint useBackendFlowLints FlowPane.lintFeed flow-lint flow-issues FlowIssuesButton FlowPane --run --maxWorkers=2`

Expected: all PASS. Then check by hand in `yarn tauri dev`: open a flow with a Request node set to a saved request, delete that request from the sidebar, press Save, and see the red issue appear in the issues popover; add an inline Request with `{{typo}}` in the URL and see the warning, then select the environment that defines `typo` and see it vanish without an edit.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:
`git add crates/rocket-flow/src/lint.rs crates/rocket-app/src/flow_service.rs src/lib/tauri-api.ts src/hooks/useBackendFlowLints.ts src/components/flow/FlowPane.tsx src/lib/__tests__/tauri-api.flow-lint.test.ts src/hooks/__tests__/useBackendFlowLints.test.tsx src/components/flow/__tests__/FlowPane.lintFeed.test.tsx src/lib/__tests__/flow-lint.test.ts && git commit --only -m "feat(flow): lint unknown variables and relint on save and environment change" -- <the same paths>`

---

## Open decisions

1. **Severity of `dangling_saved_request`.** Default: `Error`, because a run that reaches the node fails there. The alternative is `Warning`, which fits "may be created before the run". Nothing disables Run on either, so this only changes colour and sort order. One-line change in `dangling_request_lint`.
2. **No active environment.** Default: the variable rule stays silent for names it cannot find. The alternative is to also read `list()` of the collection's environments and treat a name that any environment defines as known, which would lint a bit more when nothing is selected.
3. **Saved requests silence the variable rule for the whole flow.** Default: yes, because their pre and post scripts and extractions can write variables (`rok.setEnvVar`, `rok.setCollectionVar`) that later nodes see. A later plan can read each saved request and check whether it has scripts, to restore the lint for flows that use only script-free saved requests. Not done here, to avoid one file read and parse per node.
4. **Lint the fields of saved requests too.** Default: no. It needs the request, the folder chain and request-level variables, and a merge of scopes the context does not model. Revisit with F-24 and F-25.
5. **What triggers a re-lint.** Default: open, graph edits (500 ms pause), environment name change and save. Not triggered: a sidebar rename or delete, an environment edited elsewhere, a file change on disk. Alternative: also re-lint on window focus, or on the collection file watcher event.
6. **Global environment name is not reactive in `FlowPane`.** It is read at render time with `getActiveGlobalEnvName()`. Switching only the global environment shows the new lint after the next render. Making it reactive means subscribing to the query key `environmentKeys.globalName`. Default: leave it.
7. **`lint_flow` becomes async.** Default: yes, so the disk reads leave the main thread. It keeps its name and arguments, plus two optional ones.
8. **Name filter.** Default: letters, digits, `_`, `-`, `.` and at most 64 characters. A placeholder with spaces or punctuation inside is never reported. Loosen only with a test that shows the noise stays out.
9. **Inline `auth`.** The roadmap row mentions auth, but an inline request has no auth field. Credentials come from an Auth node or an `auth` wire, which F-24 lints. Nothing to do here.

## Self-review

- Placeholders: none left. Every step has the code to type and the command to run.
- Names checked against the repo: `LintContext::{saved_request_exists, variable_is_known}`, `NoLintContext`, `GraphIndex`, `display_label`, `warning`, `only`, `lint`, `assert_linear`, `input`, `edge`, `flow`, `node` (lint.rs tests); `UntouchedRepo`, `FakeFlowRepo`, `node_of`, `lint_keys`, `routing_flow` (flow_service.rs tests); `ConfigurableCollectionRepo::{new, with_request, set_settings, fail_settings_for, fail_request_reads}`, `SharedCollectionRepo`, `StaticEnvRepo`, `NullEnvRepo` (test_doubles.rs); `Variable::new`, `Environment::new`, `ExternalSecretBinding`, `ExternalSecretRef`; `NullSecretStore`, `SharedPathEnvironmentRepo::with_secret_store`, `SharedCollectionEnvironmentRepo::new`, `SharedPathCollectionRepo::new`; `lintFlow`, `toFlowIssue`, `BACKEND_LINT_CODES`, `markClean`, `Harness`, `baseTab`.
- Consistent new names: `DANGLING_SAVED_REQUEST`, `UNKNOWN_VARIABLE`, `LintSources`, `LintEnvironment`, `CollectionLintContext`, `with_lint_sources`, `lint_with_environment`, `FlowLintEnvironment`, `BackendLintOptions`, `refreshKey`.
- Task 2 reuses `inline_request` and Task 3 reuses it, `FakeContext`, `EverythingMissing` and `Counting`, all defined in Task 2 Step 1. `lint_sources_with_nothing_saved` and `inline_flow` are defined once in `flow_service.rs` tests (Task 2 and Task 3), and Task 3 uses the first from Task 2.
- Linear in graph size: the index is built in one pass over nodes and edges. Each node scans its own text once. The context is asked once per distinct name per node and caches requests by path. The environment and settings reads are a constant number per lint. Pinned by the two `*_is_linear` tests and the counting tests.
- Never blocks save or run: `FlowService::save` calls only `validate`, `run_flow` never calls the lint path, the client treats lint errors as display only. Pinned by `a_flow_with_a_dangling_request_still_saves` and the unchanged `a_flow_with_warnings_still_saves_unchanged`.
- Not verified by running anything: this plan was written by reading source only. The first execution should confirm that `FlowPane.lintFeed` finds the button name `1 error, 1 warning`, that `act` and `userEvent` imports exist in that test file, and that rustfmt wants no changes on the new lines.

Next plan to execute: 02-lint-rules-auth-callback.md
