//! Flattening of an environment with its `extends` parent chain.

use crate::invalid::invalid;
use crate::{Environment, EnvironmentRepository, ExternalSecretBinding, Variable};
use rocket_shared::certificate::ClientCertificate;
use rocket_shared::error::DomainResult;
use std::collections::HashSet;

/// A variable together with the name of the environment that defines it.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatVariable {
    pub variable: Variable,
    pub source: String,
}

/// An external secret binding together with the environment that defines it.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatBinding {
    pub binding: ExternalSecretBinding,
    pub source: String,
}

/// A client certificate together with the environment that defines it.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatCertificate {
    pub certificate: ClientCertificate,
    pub source: String,
}

/// The result of flattening: merged variables, bindings and certificates plus
/// non-fatal warnings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Flattened {
    pub variables: Vec<FlatVariable>,
    pub external_secrets: Vec<FlatBinding>,
    pub client_certificates: Vec<FlatCertificate>,
    pub warnings: Vec<String>,
}

/// Domain key for the certificate merge: trimmed and ASCII lowercased.
fn domain_key(cert: &ClientCertificate) -> String {
    cert.domain().trim().to_ascii_lowercase()
}

impl Flattened {
    /// The merged bindings without their source.
    pub fn bindings(&self) -> Vec<ExternalSecretBinding> {
        self.external_secrets
            .iter()
            .map(|f| f.binding.clone())
            .collect()
    }

    /// The merged certificates without their source.
    pub fn certificates(&self) -> Vec<ClientCertificate> {
        self.client_certificates
            .iter()
            .map(|f| f.certificate.clone())
            .collect()
    }

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
            return Chain {
                layers,
                cycle: None,
                missing: None,
            };
        };
        if let Some(pos) = names.iter().position(|n| *n == parent) {
            let cycle = names[pos..].to_vec();
            return Chain {
                layers,
                cycle: Some(cycle),
                missing: None,
            };
        }
        match lookup(&parent) {
            Some(found) => {
                names.push(parent);
                layers.push(found);
            }
            None => {
                return Chain {
                    layers,
                    cycle: None,
                    missing: Some(parent),
                };
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
        for binding in &layer.external_secrets {
            // A nearer layer's binding replaces the inherited one of the same alias.
            out.external_secrets
                .retain(|f| f.binding.alias != binding.alias);
            out.external_secrets.push(FlatBinding {
                binding: binding.clone(),
                source: layer.name.clone(),
            });
        }
    }
    // Certificates list the child's own entries first, then inherited ones.
    let mut taken = HashSet::new();
    for layer in &chain.layers {
        // Only a nearer layer hides an entry. Twins inside one environment all stay.
        for cert in &layer.client_certificates {
            if !taken.contains(&domain_key(cert)) {
                out.client_certificates.push(FlatCertificate {
                    certificate: cert.clone(),
                    source: layer.name.clone(),
                });
            }
        }
        taken.extend(layer.client_certificates.iter().map(domain_key));
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

    fn flat(child: &Environment, s: &HashMap<String, Environment>) -> DomainResult<Flattened> {
        flatten_environment(child, &|n| s.get(n).cloned())
    }

    fn binding(alias: &str, vault: &str) -> ExternalSecretBinding {
        ExternalSecretBinding {
            alias: alias.to_string(),
            connection_id: "conn".to_string(),
            vault_name: vault.to_string(),
            secret_names: Vec::new(),
        }
    }

    fn pkcs12(domain: &str) -> ClientCertificate {
        pkcs12_at(domain, "/c.p12")
    }

    fn pkcs12_at(domain: &str, path: &str) -> ClientCertificate {
        ClientCertificate::Pkcs12 {
            domain: domain.to_string(),
            pkcs12_file_path: path.to_string(),
            pkcs12_secret: None,
            passphrase: None,
        }
    }

    fn aliases(out: &Flattened) -> Vec<&str> {
        out.external_secrets
            .iter()
            .map(|f| f.binding.alias.as_str())
            .collect()
    }

    #[test]
    fn child_binding_replaces_parent_binding_with_same_alias() {
        let mut p = env("p", None, &[]);
        p.external_secrets = vec![binding("prod", "parent-vault"), binding("stage", "s")];
        let mut c = env("c", Some("p"), &[]);
        c.external_secrets = vec![binding("prod", "child-vault")];
        let out = flat(&c, &store(vec![p, c.clone()])).expect("flatten");
        let got: Vec<(&str, &str, &str)> = out
            .external_secrets
            .iter()
            .map(|f| {
                (
                    f.binding.alias.as_str(),
                    f.binding.vault_name.as_str(),
                    f.source.as_str(),
                )
            })
            .collect();
        assert_eq!(got, vec![("stage", "s", "p"), ("prod", "child-vault", "c")]);
    }

    #[test]
    fn parent_only_bindings_are_inherited() {
        let mut p = env("p", None, &[]);
        p.external_secrets = vec![binding("a", "v")];
        let c = env("c", Some("p"), &[]);
        let out = flat(&c, &store(vec![p])).expect("flatten");
        assert_eq!(out.external_secrets.len(), 1);
        assert_eq!(out.external_secrets[0].binding.alias, "a");
        assert_eq!(out.external_secrets[0].source, "p");
    }

    #[test]
    fn three_level_chain_merges_bindings_furthest_first() {
        let mut g = env("g", None, &[]);
        g.external_secrets = vec![binding("g1", "v")];
        let mut p = env("p", Some("g"), &[]);
        p.external_secrets = vec![binding("p1", "v")];
        let mut c = env("c", Some("p"), &[]);
        c.external_secrets = vec![binding("c1", "v")];
        let out = flat(&c, &store(vec![g, p])).expect("flatten");
        assert_eq!(aliases(&out), vec!["g1", "p1", "c1"]);
    }

    #[test]
    fn aliases_differing_in_case_are_distinct() {
        let mut p = env("p", None, &[]);
        p.external_secrets = vec![binding("Prod", "v")];
        let mut c = env("c", Some("p"), &[]);
        c.external_secrets = vec![binding("prod", "v")];
        let out = flat(&c, &store(vec![p])).expect("flatten");
        assert_eq!(out.external_secrets.len(), 2);
    }

    #[test]
    fn child_certificate_replaces_parent_certificate_with_same_domain() {
        let mut p = env("p", None, &[]);
        p.client_certificates = vec![pkcs12("a.com")];
        let mut c = env("c", Some("p"), &[]);
        c.client_certificates = vec![pkcs12_at("a.com", "/child.p12")];
        let out = flat(&c, &store(vec![p])).expect("flatten");
        assert_eq!(out.client_certificates.len(), 1);
        assert_eq!(out.client_certificates[0].source, "c");
        assert_eq!(
            out.client_certificates[0].certificate,
            pkcs12_at("a.com", "/child.p12")
        );
    }

    #[test]
    fn domain_match_ignores_case_and_spaces() {
        let mut p = env("p", None, &[]);
        p.client_certificates = vec![pkcs12("a.com")];
        let mut c = env("c", Some("p"), &[]);
        c.client_certificates = vec![pkcs12(" A.COM ")];
        let out = flat(&c, &store(vec![p])).expect("flatten");
        assert_eq!(out.client_certificates.len(), 1);
        assert_eq!(out.client_certificates[0].source, "c");
    }

    #[test]
    fn certificates_list_child_first_then_inherited() {
        let mut p = env("p", None, &[]);
        p.client_certificates = vec![pkcs12("p.com")];
        let mut c = env("c", Some("p"), &[]);
        c.client_certificates = vec![pkcs12("c.com")];
        let out = flat(&c, &store(vec![p])).expect("flatten");
        let domains: Vec<&str> = out
            .client_certificates
            .iter()
            .map(|f| f.certificate.domain())
            .collect();
        assert_eq!(domains, vec!["c.com", "p.com"]);
    }

    #[test]
    fn same_domain_twins_in_one_environment_both_stay() {
        let mut c = env("c", None, &[]);
        c.client_certificates = vec![
            pkcs12_at("a.com", "/one.p12"),
            pkcs12_at("a.com", "/two.p12"),
        ];
        let out = flat(&c, &store(vec![])).expect("flatten");
        assert_eq!(out.certificates(), c.client_certificates);
    }

    #[test]
    fn no_extends_returns_own_lists_unchanged() {
        let mut c = env("c", None, &[]);
        c.external_secrets = vec![binding("a", "v")];
        c.client_certificates = vec![pkcs12("a.com")];
        let out = flat(&c, &store(vec![])).expect("flatten");
        assert_eq!(out.bindings(), c.external_secrets);
        assert_eq!(out.certificates(), c.client_certificates);
        assert_eq!(out.external_secrets[0].source, "c");
        assert_eq!(out.client_certificates[0].source, "c");
    }

    #[test]
    fn missing_parent_warns_and_keeps_own_lists() {
        let mut c = env("c", Some("gone"), &[]);
        c.external_secrets = vec![binding("a", "v")];
        let out = flat(&c, &store(vec![])).expect("flatten");
        assert_eq!(aliases(&out), vec!["a"]);
        assert_eq!(out.warnings.len(), 1);
    }

    #[test]
    fn cycle_still_errors_with_bindings_present() {
        let mut a = env("a", Some("b"), &[]);
        a.external_secrets = vec![binding("x", "v")];
        let mut b = env("b", Some("a"), &[]);
        b.external_secrets = vec![binding("y", "v")];
        let err = flat(&a, &store(vec![b])).expect_err("cycle").to_string();
        assert!(err.contains("a -> b -> a"), "{err}");
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
            let parent = if i == 29 {
                None
            } else {
                Some(format!("e{}", i + 1))
            };
            envs.push(env(
                &format!("e{i}"),
                parent.as_deref(),
                &[(&format!("k{i}"), "v")],
            ));
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
