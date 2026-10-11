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
        // A run adds `flow-auth.<id>`, `flow-callback.<id>.url` and similar secrets.
        || name.starts_with("flow-")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::{
        ConfigurableCollectionRepo, NullEnvRepo, SharedCollectionRepo, StaticEnvRepo,
    };
    use rocket_collection::{CollectionSettings, CollectionVariable, Request};
    use rocket_environment::{
        Environment, EnvironmentRepository, EnvironmentRepositoryFactory, ExternalSecretBinding,
        ExternalSecretRef, Variable,
    };
    use rocket_flow::{
        Flow, FlowNode, FlowNodeKind, InlineRequestData, NodePosition, RequestSource,
    };
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
        env.variables = keys
            .iter()
            .map(|k| Variable::new(*k, "env-secret-value"))
            .collect();
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
        let src = sources(&repo, Some(environment(&["token"])), Some(environment(&[])));
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
        for name in [
            "$guid",
            "$notARealOne",
            "process.env.HOME",
            "callback.payment",
            "flow-callback.n1.url",
            "",
        ] {
            assert_eq!(ctx.variable_is_known(name), None, "{name:?}");
        }
        assert_eq!(ctx.variable_is_known("plain"), Some(false));
    }
}
