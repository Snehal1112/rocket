use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    pub runtime: HashMap<String, String>,
    pub request: HashMap<String, String>,
    pub folder: HashMap<String, String>,
    pub env: HashMap<String, String>,
    pub collection: HashMap<String, String>,
    /// Values fetched from RocketVault, keyed by "{alias}.{secretName}" per
    /// the active environment's `external_secrets` bindings. Populated once
    /// per resolve/run by `RequestExecutionService::resolve_external_secrets`
    /// (Plan 06) — this crate has no I/O and never fetches these values
    /// itself. Kept as its own field rather than folded into `env`, so
    /// `rok.getEnvVar` never accidentally returns a vault-sourced value
    /// through an unrelated code path.
    pub external_secrets: HashMap<String, String>,
    pub global_env: HashMap<String, String>,
    pub process_env: HashMap<String, String>,
    /// Keys (from any scope) whose *value* must be redacted if it appears in
    /// script-emitted console/test-error text. Not a per-scope map — a value is
    /// either sensitive or not, regardless of which scope surfaced it.
    pub secret_values: HashSet<String>,
    /// Names of the secret variables of any scope, whatever their value, plus the RocketVault
    /// keys. A value a script sets under one of these names is treated as secret too, even when
    /// the original value is empty or too short to be masked.
    pub secret_keys: HashSet<String>,
}

impl VariableContext {
    /// The scope maps in precedence order, lowest first. This is the only place the order is written.
    /// Later layers overwrite earlier ones on key collision. `process_env` is not a layer here:
    /// `flatten_with_process_env` adds it below all of these.
    fn layers(&self) -> [&HashMap<String, String>; 7] {
        [
            &self.global_env,
            &self.collection,
            &self.external_secrets,
            &self.env,
            &self.folder,
            &self.request,
            &self.runtime,
        ]
    }

    /// Folds every layer into `out`, lowest precedence first.
    fn merge_layers_into(&self, out: &mut HashMap<String, String>) {
        for layer in self.layers() {
            out.extend(layer.clone());
        }
    }

    /// Merge all scopes except process_env.
    /// Insertion order: global_env → collection → external_secrets → env → folder → request → runtime.
    /// Later layers overwrite earlier on key collision.
    pub fn flatten(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        self.merge_layers_into(&mut out);
        out
    }

    /// Same as flatten() but also inserts process env vars with "process.env." prefix (lowest priority).
    pub fn flatten_with_process_env(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for (k, v) in &self.process_env {
            out.insert(format!("process.env.{}", k), v.clone());
        }
        self.merge_layers_into(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn env_beats_collection() {
        let ctx = VariableContext {
            env: m(&[("k", "env")]),
            collection: m(&[("k", "col")]),
            ..Default::default()
        };
        assert_eq!(ctx.flatten().get("k").unwrap(), "env");
    }

    #[test]
    fn folder_beats_env() {
        let ctx = VariableContext {
            folder: m(&[("k", "folder")]),
            env: m(&[("k", "env")]),
            ..Default::default()
        };
        assert_eq!(ctx.flatten().get("k").unwrap(), "folder");
    }

    #[test]
    fn request_beats_folder() {
        let ctx = VariableContext {
            request: m(&[("k", "req")]),
            folder: m(&[("k", "folder")]),
            ..Default::default()
        };
        assert_eq!(ctx.flatten().get("k").unwrap(), "req");
    }

    #[test]
    fn runtime_beats_all() {
        let ctx = VariableContext {
            runtime: m(&[("k", "rt")]),
            request: m(&[("k", "req")]),
            ..Default::default()
        };
        assert_eq!(ctx.flatten().get("k").unwrap(), "rt");
    }

    #[test]
    fn global_beats_nothing_but_process() {
        let ctx = VariableContext {
            global_env: m(&[("k", "global")]),
            collection: m(&[("k", "col")]),
            ..Default::default()
        };
        assert_eq!(ctx.flatten().get("k").unwrap(), "col"); // collection beats global
    }

    #[test]
    fn process_env_uses_dotted_key() {
        let ctx = VariableContext {
            process_env: m(&[("API_KEY", "secret")]),
            ..Default::default()
        };
        let flat = ctx.flatten_with_process_env();
        assert!(!flat.contains_key("API_KEY"));
        assert_eq!(flat.get("process.env.API_KEY").unwrap(), "secret");
    }

    #[test]
    fn env_beats_global() {
        let ctx = VariableContext {
            env: m(&[("t", "env")]),
            global_env: m(&[("t", "global")]),
            ..Default::default()
        };
        assert_eq!(ctx.flatten().get("t").unwrap(), "env");
    }

    #[test]
    fn empty_is_empty() {
        assert!(VariableContext::default().flatten().is_empty());
    }

    #[test]
    fn folder_chain_innermost_wins() {
        let ctx = VariableContext {
            folder: m(&[("k", "inner")]),
            env: m(&[("k", "env")]),
            ..Default::default()
        };
        assert_eq!(ctx.flatten().get("k").unwrap(), "inner");
    }

    #[test]
    fn full_hierarchy_runtime_wins() {
        // All 9 scopes present — runtime must win.
        let ctx = VariableContext {
            runtime: m(&[("k", "runtime")]),
            request: m(&[("k", "request")]),
            folder: m(&[("k", "folder")]),
            env: m(&[("k", "env")]),
            collection: m(&[("k", "collection")]),
            external_secrets: m(&[("k", "vault")]),
            global_env: m(&[("k", "global")]),
            process_env: m(&[("k", "process")]),
            secret_values: std::collections::HashSet::new(),
            secret_keys: std::collections::HashSet::new(),
        };
        assert_eq!(ctx.flatten().get("k").expect("k present"), "runtime");
    }

    #[test]
    fn env_beats_external_secrets() {
        let ctx = VariableContext {
            env: m(&[("k", "env")]),
            external_secrets: m(&[("k", "vault")]),
            ..Default::default()
        };
        assert_eq!(ctx.flatten().get("k").expect("k present"), "env");
    }

    #[test]
    fn external_secrets_beats_collection() {
        let ctx = VariableContext {
            external_secrets: m(&[("k", "vault")]),
            collection: m(&[("k", "col")]),
            ..Default::default()
        };
        assert_eq!(ctx.flatten().get("k").expect("k present"), "vault");
    }

    #[test]
    fn external_secrets_value_passes_through_flatten_unchanged() {
        // A value present only in external_secrets, with no key collision
        // anywhere else, shows up in flatten()'s output unchanged — plain
        // pass-through, not filtered.
        let ctx = VariableContext {
            external_secrets: m(&[("payments.stripeKey", "sk-live-abcdef123")]),
            ..Default::default()
        };
        let flat = ctx.flatten();
        assert_eq!(
            flat.get("payments.stripeKey")
                .expect("payments.stripeKey present"),
            "sk-live-abcdef123"
        );
    }

    #[test]
    fn secret_values_defaults_to_empty() {
        assert!(VariableContext::default().secret_values.is_empty());
    }

    #[test]
    fn secret_values_does_not_affect_flatten() {
        let mut ctx = VariableContext {
            env: m(&[("API_KEY", "sk-live-abcdef123")]),
            ..Default::default()
        };
        ctx.secret_values.insert("sk-live-abcdef123".to_string());
        // flatten() still returns the real value — secret_values is a
        // separate redaction list, not a filter on the scope maps.
        let flat = ctx.flatten();
        assert_eq!(
            flat.get("API_KEY").expect("API_KEY present"),
            "sk-live-abcdef123"
        );
    }

    #[test]
    fn secret_values_does_not_affect_flatten_with_process_env() {
        let mut ctx = VariableContext {
            env: m(&[("API_KEY", "sk-live-abcdef123")]),
            ..Default::default()
        };
        ctx.secret_values.insert("sk-live-abcdef123".to_string());
        let flat = ctx.flatten_with_process_env();
        assert_eq!(
            flat.get("API_KEY").expect("API_KEY present"),
            "sk-live-abcdef123"
        );
    }

    #[test]
    fn secret_values_is_content_addressed_not_tied_to_a_scope_key() {
        // A value can be marked sensitive without needing to also appear
        // in any scope map — redaction matches on the value alone.
        let mut ctx = VariableContext::default();
        ctx.secret_values.insert("standalone-secret".to_string());
        assert!(ctx.secret_values.contains("standalone-secret"));
        assert!(ctx.flatten().is_empty());
    }

    #[test]
    fn process_env_is_the_lowest_layer_and_the_rest_matches_flatten() {
        let ctx = VariableContext {
            process_env: m(&[("PATH", "/bin"), ("shared", "from-process")]),
            global_env: m(&[("process.env.shared", "from-global"), ("g", "1")]),
            collection: m(&[("c", "1")]),
            runtime: m(&[("r", "1")]),
            ..Default::default()
        };
        let mut expected = ctx.flatten();
        expected
            .entry("process.env.PATH".to_string())
            .or_insert_with(|| "/bin".to_string());
        expected
            .entry("process.env.shared".to_string())
            .or_insert_with(|| "from-process".to_string());
        let flat = ctx.flatten_with_process_env();
        assert_eq!(flat, expected);
        assert_eq!(
            flat.get("process.env.shared").expect("present"),
            "from-global"
        );
    }

    #[test]
    fn process_env_shadowed_by_same_prefixed_user_var() {
        // If a user explicitly names a collection var "process.env.X",
        // it should override the prefixed process env entry.
        let ctx = VariableContext {
            process_env: m(&[("X", "from_os")]),
            collection: m(&[("process.env.X", "user_override")]),
            ..Default::default()
        };
        let flat = ctx.flatten_with_process_env();
        assert_eq!(flat.get("process.env.X").unwrap(), "user_override");
    }

    #[test]
    fn precedence_order_is_pinned_for_both_flatten_variants() {
        let names = [
            "global_env",
            "collection",
            "external_secrets",
            "env",
            "folder",
            "request",
            "runtime",
        ];
        for i in 0..names.len() {
            let mut ctx = VariableContext::default();
            for name in &names[..=i] {
                let layer = m(&[("k", name)]);
                match *name {
                    "global_env" => ctx.global_env = layer,
                    "collection" => ctx.collection = layer,
                    "external_secrets" => ctx.external_secrets = layer,
                    "env" => ctx.env = layer,
                    "folder" => ctx.folder = layer,
                    "request" => ctx.request = layer,
                    _ => ctx.runtime = layer,
                }
            }
            assert_eq!(ctx.flatten().get("k").unwrap(), names[i]);
            assert_eq!(ctx.flatten_with_process_env().get("k").unwrap(), names[i]);
        }
        let ctx = VariableContext {
            process_env: m(&[("k", "process")]),
            ..Default::default()
        };
        assert_eq!(
            ctx.flatten_with_process_env().get("process.env.k").unwrap(),
            "process"
        );
    }
}
