//! The host environment (`{{process.env.*}}` and `.env` files) a send may see: the
//! scope a tab asks for, the values it got, and how the scope picks its target.

use serde::Deserialize;

/// Which host environment (`{{process.env.*}}` and `.env` files) a send may see. It is
/// decided by the tab, on top of the collection trust gate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProcessEnvScopeInput {
    /// A tab outside any collection: full host access.
    Scratch,
    /// A tab of a collection, with the environment the tab uses.
    Collection {
        name: String,
        #[serde(default)]
        environment_name: Option<String>,
    },
    /// No host environment at all.
    #[default]
    Closed,
}

/// The `process.env` values of one send. `dot_env_keys` names the keys whose value came
/// from a `.env` file, so they can be masked.
#[derive(Debug, Clone, Default)]
pub struct ProcessEnv {
    pub vars: std::collections::HashMap<String, String>,
    pub dot_env_keys: Vec<String>,
    /// The trust gate's one decision for this send. False when it denied access, so
    /// `vars` is empty.
    pub allowed: bool,
}

/// Whether `text` holds a `{{process.env.NAME}}` placeholder, spaces inside the braces
/// allowed, found the way a request send finds its names.
pub(crate) fn names_process_env(text: &str) -> bool {
    rocket_environment::resolve(text, &std::collections::HashMap::new())
        .unresolved
        .iter()
        .any(|name| name.starts_with("process.env."))
}

/// The `(collection, environment)` a scope reads host values for, or `None` when the
/// scope is closed. A `collection` given keeps its own gate and the scope can only close
/// it. Without a `collection`, a `Collection` scope names the gate and the environment,
/// and `Scratch` keeps full host access.
pub(crate) fn scope_target<'a>(
    collection: Option<&'a str>,
    environment_name: Option<&'a str>,
    scope: &'a ProcessEnvScopeInput,
) -> Option<(Option<&'a str>, Option<&'a str>)> {
    match (collection, scope) {
        (_, ProcessEnvScopeInput::Closed) => None,
        (Some(c), _) => Some((Some(c), environment_name)),
        (
            None,
            ProcessEnvScopeInput::Collection {
                name,
                environment_name: scope_env,
            },
        ) => Some((Some(name.as_str()), scope_env.as_deref())),
        (None, ProcessEnvScopeInput::Scratch) => Some((None, environment_name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_process_env_needs_a_placeholder() {
        assert!(names_process_env("a {{process.env.X}} b"));
        assert!(names_process_env("{{ process.env.X }}"));
        assert!(!names_process_env("see process.env.X docs"));
        assert!(!names_process_env("{{other}}"));
    }

    #[test]
    fn scope_target_closed_is_none() {
        assert_eq!(
            scope_target(Some("api"), Some("dev"), &ProcessEnvScopeInput::Closed),
            None
        );
        assert_eq!(
            scope_target(None, None, &ProcessEnvScopeInput::Closed),
            None
        );
    }

    #[test]
    fn scope_target_collection_input_keeps_its_own_gate() {
        let scope = ProcessEnvScopeInput::Collection {
            name: "other".into(),
            environment_name: Some("prod".into()),
        };
        assert_eq!(
            scope_target(Some("api"), Some("dev"), &scope),
            Some((Some("api"), Some("dev")))
        );
        assert_eq!(
            scope_target(Some("api"), Some("dev"), &ProcessEnvScopeInput::Scratch),
            Some((Some("api"), Some("dev")))
        );
    }

    #[test]
    fn scope_target_scratch_without_collection_reads_no_collection() {
        assert_eq!(
            scope_target(None, Some("dev"), &ProcessEnvScopeInput::Scratch),
            Some((None, Some("dev")))
        );
        let scope = ProcessEnvScopeInput::Collection {
            name: "api".into(),
            environment_name: None,
        };
        assert_eq!(
            scope_target(None, Some("dev"), &scope),
            Some((Some("api"), None))
        );
    }

    #[test]
    fn scope_deserializes_scratch_collection_and_closed() {
        let s: ProcessEnvScopeInput =
            serde_json::from_value(serde_json::json!({ "kind": "scratch" })).expect("scratch");
        assert_eq!(s, ProcessEnvScopeInput::Scratch);
        let c: ProcessEnvScopeInput = serde_json::from_value(
            serde_json::json!({ "kind": "collection", "name": "api", "environmentName": "dev" }),
        )
        .expect("collection");
        assert_eq!(
            c,
            ProcessEnvScopeInput::Collection {
                name: "api".into(),
                environment_name: Some("dev".into())
            }
        );
        let n: ProcessEnvScopeInput =
            serde_json::from_value(serde_json::json!({ "kind": "collection", "name": "api" }))
                .expect("no env");
        assert_eq!(
            n,
            ProcessEnvScopeInput::Collection {
                name: "api".into(),
                environment_name: None
            }
        );
        let x: ProcessEnvScopeInput =
            serde_json::from_value(serde_json::json!({ "kind": "closed" })).expect("closed");
        assert_eq!(x, ProcessEnvScopeInput::Closed);
        assert_eq!(
            ProcessEnvScopeInput::default(),
            ProcessEnvScopeInput::Closed
        );
        serde_json::from_value::<ProcessEnvScopeInput>(serde_json::json!({ "kind": "open" }))
            .expect_err("unknown kind");
    }
}
