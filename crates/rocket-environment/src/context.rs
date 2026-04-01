use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    pub runtime:     HashMap<String, String>,
    pub request:     HashMap<String, String>,
    pub folder:      HashMap<String, String>,
    pub env:         HashMap<String, String>,
    pub collection:  HashMap<String, String>,
    pub global_env:  HashMap<String, String>,
    pub process_env: HashMap<String, String>,
}

impl VariableContext {
    /// Merge all scopes except process_env.
    /// Insertion order: global_env → collection → env → folder → request → runtime.
    /// Later layers overwrite earlier on key collision.
    pub fn flatten(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        out.extend(self.global_env.clone());
        out.extend(self.collection.clone());
        out.extend(self.env.clone());
        out.extend(self.folder.clone());
        out.extend(self.request.clone());
        out.extend(self.runtime.clone());
        out
    }

    /// Same as flatten() but also inserts process env vars with "process.env." prefix (lowest priority).
    pub fn flatten_with_process_env(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for (k, v) in &self.process_env {
            out.insert(format!("process.env.{}", k), v.clone());
        }
        out.extend(self.global_env.clone());
        out.extend(self.collection.clone());
        out.extend(self.env.clone());
        out.extend(self.folder.clone());
        out.extend(self.request.clone());
        out.extend(self.runtime.clone());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
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
        assert!(flat.get("API_KEY").is_none());
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
        // All 7 scopes present — runtime must win.
        let ctx = VariableContext {
            runtime:     m(&[("k", "runtime")]),
            request:     m(&[("k", "request")]),
            folder:      m(&[("k", "folder")]),
            env:         m(&[("k", "env")]),
            collection:  m(&[("k", "collection")]),
            global_env:  m(&[("k", "global")]),
            process_env: m(&[("k", "process")]),
        };
        assert_eq!(ctx.flatten().get("k").unwrap(), "runtime");
    }

    #[test]
    fn process_env_shadowed_by_same_prefixed_user_var() {
        // If a user explicitly names a collection var "process.env.X",
        // it should override the prefixed process env entry.
        let ctx = VariableContext {
            process_env: m(&[("X", "from_os")]),
            collection:  m(&[("process.env.X", "user_override")]),
            ..Default::default()
        };
        let flat = ctx.flatten_with_process_env();
        assert_eq!(flat.get("process.env.X").unwrap(), "user_override");
    }
}
