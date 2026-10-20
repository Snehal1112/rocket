//! Layering of `.env` files along an environment's `extends` chain.

use std::path::Path;

use crate::flatten::walk_chain;
use crate::Environment;

/// Reads one `.env` file. The `Err` text is a warning message.
pub trait DotEnvLoader: Send + Sync {
    fn load(&self, raw_path: &str, base: Option<&Path>) -> Result<Vec<(String, String)>, String>;
}

/// The layered `.env` values plus non-fatal warnings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DotEnvValues {
    pub values: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

/// Layers the `.env` files of `env` and its parents, furthest ancestor first. A later
/// layer replaces an earlier value by key, so the child wins.
pub fn collect_dot_env(
    env: &Environment,
    lookup: &dyn Fn(&str) -> Option<Environment>,
    loader: &dyn DotEnvLoader,
    base: Option<&Path>,
) -> DotEnvValues {
    let chain = walk_chain(env, lookup);
    let mut out = DotEnvValues::default();
    if let Some(cycle) = &chain.cycle {
        out.warnings.push(format!(
            "Environment extends cycle ({}); .env files beyond it are not read.",
            cycle.join(" -> ")
        ));
    }
    for layer in chain.layers.iter().rev() {
        let Some(path) = layer.dot_env_file_path.as_deref() else {
            continue;
        };
        match loader.load(path, base) {
            Ok(pairs) => {
                for (key, value) in pairs {
                    match out.values.iter_mut().find(|(k, _)| *k == key) {
                        Some(slot) => slot.1 = value,
                        None => out.values.push((key, value)),
                    }
                }
            }
            Err(message) => out.warnings.push(message),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        files: HashMap<String, Vec<(String, String)>>,
        bases: Mutex<Vec<Option<PathBuf>>>,
    }

    impl DotEnvLoader for Fake {
        fn load(&self, raw: &str, base: Option<&Path>) -> Result<Vec<(String, String)>, String> {
            if let Ok(mut b) = self.bases.lock() {
                b.push(base.map(Path::to_path_buf));
            }
            self.files
                .get(raw)
                .cloned()
                .ok_or_else(|| format!("cannot read {raw}"))
        }
    }

    fn env(name: &str, extends: Option<&str>, path: Option<&str>) -> Environment {
        let mut e = Environment::new(name);
        e.extends = extends.map(str::to_string);
        e.dot_env_file_path = path.map(str::to_string);
        e
    }

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(k, x)| (k.to_string(), x.to_string()))
            .collect()
    }

    fn run(envs: &[Environment], child: &str, fake: &Fake, base: Option<&Path>) -> DotEnvValues {
        let map: HashMap<String, Environment> =
            envs.iter().map(|e| (e.name.clone(), e.clone())).collect();
        collect_dot_env(&map[child], &|n| map.get(n).cloned(), fake, base)
    }

    #[test]
    fn child_file_wins_over_parent_file() {
        let mut fake = Fake::default();
        fake.files.insert("p.env".into(), pairs(&[("A", "1")]));
        fake.files.insert("c.env".into(), pairs(&[("A", "2")]));
        let envs = [
            env("p", None, Some("p.env")),
            env("c", Some("p"), Some("c.env")),
        ];
        let out = run(&envs, "c", &fake, None);
        assert_eq!(out.values, pairs(&[("A", "2")]));
        assert!(out.warnings.is_empty());
    }

    #[test]
    fn parent_file_is_read_through_the_chain() {
        let mut fake = Fake::default();
        fake.files.insert("p.env".into(), pairs(&[("B", "1")]));
        fake.files.insert("c.env".into(), pairs(&[("A", "2")]));
        let envs = [
            env("p", None, Some("p.env")),
            env("c", Some("p"), Some("c.env")),
        ];
        let out = run(&envs, "c", &fake, None);
        assert_eq!(out.values, pairs(&[("B", "1"), ("A", "2")]));
    }

    #[test]
    fn environment_without_a_path_adds_nothing_but_its_parents_still_count() {
        let mut fake = Fake::default();
        fake.files.insert("g.env".into(), pairs(&[("G", "1")]));
        let envs = [
            env("g", None, Some("g.env")),
            env("p", Some("g"), None),
            env("c", Some("p"), None),
        ];
        let out = run(&envs, "c", &fake, None);
        assert_eq!(out.values, pairs(&[("G", "1")]));
    }

    #[test]
    fn a_loader_error_becomes_a_warning_and_other_files_still_load() {
        let mut fake = Fake::default();
        fake.files.insert("c.env".into(), pairs(&[("A", "2")]));
        let envs = [
            env("p", None, Some("missing.env")),
            env("c", Some("p"), Some("c.env")),
        ];
        let out = run(&envs, "c", &fake, None);
        assert_eq!(out.values, pairs(&[("A", "2")]));
        assert_eq!(out.warnings, vec!["cannot read missing.env".to_string()]);
    }

    #[test]
    fn a_cycle_stops_the_walk_with_a_warning() {
        let mut fake = Fake::default();
        fake.files.insert("a.env".into(), pairs(&[("A", "1")]));
        fake.files.insert("b.env".into(), pairs(&[("B", "1")]));
        let envs = [
            env("a", Some("b"), Some("a.env")),
            env("b", Some("a"), Some("b.env")),
        ];
        let out = run(&envs, "a", &fake, None);
        assert_eq!(out.warnings.len(), 1);
        assert!(out.warnings[0].contains("cycle"));
        assert_eq!(out.values.len(), 2);
    }

    #[test]
    fn the_base_is_passed_to_the_loader_unchanged() {
        let mut fake = Fake::default();
        fake.files.insert("c.env".into(), pairs(&[("A", "2")]));
        let envs = [env("c", None, Some("c.env"))];
        let base = PathBuf::from("/tmp/coll");
        run(&envs, "c", &fake, Some(&base));
        let seen = fake.bases.lock().map(|b| b.clone()).unwrap_or_default();
        assert_eq!(seen, vec![Some(base)]);
    }
}
