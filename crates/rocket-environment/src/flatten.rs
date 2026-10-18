//! Flattening of an environment with its `extends` parent chain.

use crate::invalid::invalid;
use crate::{Environment, EnvironmentRepository, Variable};
use rocket_shared::error::DomainResult;

/// A variable together with the name of the environment that defines it.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatVariable {
    pub variable: Variable,
    pub source: String,
}

/// The result of flattening: merged variables plus non-fatal warnings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Flattened {
    pub variables: Vec<FlatVariable>,
    pub warnings: Vec<String>,
}

impl Flattened {
    /// Enabled key and value pairs, in variable order.
    pub fn enabled_pairs(&self) -> Vec<(&str, &str)> {
        self.variables
            .iter()
            .filter(|f| f.variable.enabled)
            .map(|f| (f.variable.key.as_str(), f.variable.value.as_str()))
            .collect()
    }
}

/// Result of walking the chain: layers from child to root, an optional cycle,
/// and an optional missing parent name.
pub(crate) struct Chain {
    pub(crate) layers: Vec<Environment>,
    pub(crate) cycle: Option<Vec<String>>,
    missing: Option<String>,
}

pub(crate) fn walk_chain(env: &Environment, lookup: &dyn Fn(&str) -> Option<Environment>) -> Chain {
    let mut layers = vec![env.clone()];
    let mut names = vec![env.name.clone()];
    loop {
        let Some(parent) = layers.last().and_then(|e| e.extends.clone()) else {
            return Chain { layers, cycle: None, missing: None };
        };
        if let Some(pos) = names.iter().position(|n| *n == parent) {
            let cycle = names[pos..].to_vec();
            return Chain { layers, cycle: Some(cycle), missing: None };
        }
        match lookup(&parent) {
            Some(found) => {
                names.push(parent);
                layers.push(found);
            }
            None => {
                return Chain { layers, cycle: None, missing: Some(parent) };
            }
        }
    }
}

/// The names in an `extends` cycle reachable from `env`, in chain order.
pub fn find_extends_cycle(
    env: &Environment,
    lookup: &dyn Fn(&str) -> Option<Environment>,
) -> Option<Vec<String>> {
    walk_chain(env, lookup).cycle
}

fn cycle_error(cycle: &[String]) -> rocket_shared::error::DomainError {
    let first = cycle.first().cloned().unwrap_or_default();
    let path = cycle
        .iter()
        .cloned()
        .chain(std::iter::once(first))
        .collect::<Vec<_>>()
        .join(" -> ");
    invalid(format!("Environment extends cycle: {path}"))
}

/// Merges `env` with its parent chain. Later layers replace earlier ones by
/// key, and a disabled entry in a child shadows the parent's entry.
pub fn flatten_environment(
    env: &Environment,
    lookup: &dyn Fn(&str) -> Option<Environment>,
) -> DomainResult<Flattened> {
    let chain = walk_chain(env, lookup);
    if let Some(cycle) = chain.cycle {
        return Err(cycle_error(&cycle));
    }
    let mut out = Flattened::default();
    if let Some(missing) = chain.missing {
        out.warnings.push(format!(
            "Environment '{}' extends '{}', which was not found; using its own variables.",
            chain.layers.last().map(|e| e.name.as_str()).unwrap_or(""),
            missing
        ));
    }
    for layer in chain.layers.iter().rev() {
        for var in &layer.variables {
            let flat = FlatVariable {
                variable: var.clone(),
                source: layer.name.clone(),
            };
            match out.variables.iter_mut().find(|f| f.variable.key == var.key) {
                Some(slot) => *slot = flat,
                None => out.variables.push(flat),
            }
        }
    }
    Ok(out)
}

/// Like `flatten_environment`, looking parents up in `repo` by name.
pub fn flatten_with_repo(
    env: &Environment,
    repo: &dyn EnvironmentRepository,
) -> DomainResult<Flattened> {
    flatten_environment(env, &|name| repo.get(name).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(name: &str, extends: Option<&str>, vars: &[(&str, &str)]) -> Environment {
        let mut e = Environment::new(name);
        e.extends = extends.map(str::to_string);
        for (k, v) in vars {
            e.set_variable(Variable::new(*k, *v));
        }
        e
    }

    fn store(envs: Vec<Environment>) -> HashMap<String, Environment> {
        envs.into_iter().map(|e| (e.name.clone(), e)).collect()
    }

    fn flat(
        child: &Environment,
        s: &HashMap<String, Environment>,
    ) -> DomainResult<Flattened> {
        flatten_environment(child, &|n| s.get(n).cloned())
    }

    #[test]
    fn child_value_wins_over_parent() {
        let p = env("p", None, &[("a", "1")]);
        let c = env("c", Some("p"), &[("a", "2")]);
        let f = flat(&c, &store(vec![p])).unwrap();
        assert_eq!(f.variables.len(), 1);
        assert_eq!(f.variables[0].variable.value, "2");
        assert_eq!(f.variables[0].source, "c");
    }

    #[test]
    fn parent_variable_is_inherited() {
        let p = env("p", None, &[("a", "1")]);
        let c = env("c", Some("p"), &[]);
        let f = flat(&c, &store(vec![p])).unwrap();
        assert_eq!(f.variables[0].variable.value, "1");
        assert_eq!(f.variables[0].source, "p");
    }

    #[test]
    fn three_level_chain_flattens_furthest_ancestor_first() {
        let root = env("root", None, &[("a", "1"), ("b", "1")]);
        let mid = env("mid", Some("root"), &[("b", "2")]);
        let leaf = env("leaf", Some("mid"), &[("c", "3")]);
        let f = flat(&leaf, &store(vec![root, mid])).unwrap();
        assert_eq!(f.enabled_pairs(), vec![("a", "1"), ("b", "2"), ("c", "3")]);
    }

    #[test]
    fn disabled_child_variable_shadows_parent() {
        let p = env("p", None, &[("a", "1")]);
        let mut c = env("c", Some("p"), &[]);
        let mut v = Variable::new("a", "x");
        v.enabled = false;
        c.set_variable(v);
        let f = flat(&c, &store(vec![p])).unwrap();
        assert!(f.enabled_pairs().iter().all(|(k, _)| *k != "a"));
    }

    #[test]
    fn secret_flag_survives_inheritance() {
        let mut p = Environment::new("p");
        let mut v = Variable::new("s", "x");
        v.secret = true;
        p.set_variable(v);
        let c = env("c", Some("p"), &[]);
        let f = flat(&c, &store(vec![p])).unwrap();
        assert!(f.variables[0].variable.secret);
    }

    #[test]
    fn no_extends_returns_own_variables_with_own_source() {
        let c = env("c", None, &[("a", "1")]);
        let f = flat(&c, &store(vec![])).unwrap();
        assert_eq!(f.variables[0].source, "c");
        assert!(f.warnings.is_empty());
    }

    #[test]
    fn missing_parent_warns_and_keeps_child_variables() {
        let c = env("c", Some("gone"), &[("a", "1")]);
        let f = flat(&c, &store(vec![])).unwrap();
        assert_eq!(f.warnings.len(), 1);
        assert!(f.warnings[0].contains("gone"));
        assert_eq!(f.enabled_pairs(), vec![("a", "1")]);
    }

    #[test]
    fn parent_in_another_store_is_a_missing_parent() {
        let c = env("c", Some("global-base"), &[("a", "1")]);
        let f = flatten_environment(&c, &|_| None).unwrap();
        assert_eq!(f.warnings.len(), 1);
        assert!(f.warnings[0].contains("global-base"));
        assert_eq!(f.variables.len(), 1);
    }

    #[test]
    fn self_extends_is_a_cycle_error() {
        let c = env("loop", Some("loop"), &[]);
        let err = flat(&c, &store(vec![])).unwrap_err();
        assert!(err.to_string().contains("loop"));
    }

    #[test]
    fn two_environment_cycle_names_both() {
        let a = env("alpha", Some("beta"), &[]);
        let b = env("beta", Some("alpha"), &[]);
        let err = flat(&a, &store(vec![b])).unwrap_err().to_string();
        assert!(err.contains("alpha") && err.contains("beta"));
    }

    #[test]
    fn a_chain_of_thirty_environments_flattens() {
        let mut envs = Vec::new();
        for i in 0..30 {
            let parent = if i == 29 { None } else { Some(format!("e{}", i + 1)) };
            envs.push(env(&format!("e{i}"), parent.as_deref(), &[(&format!("k{i}"), "v")]));
        }
        let s = store(envs.clone());
        let f = flat(&envs[0], &s).unwrap();
        assert_eq!(f.variables.len(), 30);
    }

    #[test]
    fn find_extends_cycle_returns_none_for_a_chain_and_names_for_a_cycle() {
        let root = env("root", None, &[]);
        let leaf = env("leaf", Some("root"), &[]);
        let s = store(vec![root]);
        assert!(find_extends_cycle(&leaf, &|n| s.get(n).cloned()).is_none());
        let a = env("a", Some("b"), &[]);
        let b = env("b", Some("a"), &[]);
        let s2 = store(vec![b]);
        let cycle = find_extends_cycle(&a, &|n| s2.get(n).cloned()).unwrap();
        assert_eq!(cycle, vec!["a".to_string(), "b".to_string()]);
    }
}
