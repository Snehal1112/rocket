//! Secret variable values of a whole workspace, for masking stored
//! conversations. Vault values are not fetched here (spec section 8).

use std::collections::HashSet;

use rocket_collection::{CollectionItem, CollectionRepository, CollectionVariable, Folder};
use rocket_environment::{
    flatten_with_repo, Environment, EnvironmentRepository, EnvironmentRepositoryFactory,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::runner_sequence::folder_dir_name;

/// Every secret variable value of the workspace: global environments, and
/// for each collection its variables (value and initial value), its
/// environments, and the variables of every folder and request. A read
/// error is returned, because a skipped scope would leave its secrets
/// unmasked. A folder without `folder.yml` (`NotFound`) has no variables.
pub(crate) fn workspace_secret_values(
    collections: &dyn CollectionRepository,
    collection_envs: &dyn EnvironmentRepositoryFactory,
    global_envs: &dyn EnvironmentRepository,
) -> DomainResult<HashSet<String>> {
    let mut out = HashSet::new();
    let globals = global_envs
        .list_strict()
        .map_err(|e| unreadable("global environments", e))?;
    add_env_secrets(&mut out, &globals, global_envs)?;
    let listed = collections
        .list_strict()
        .map_err(|e| unreadable("collection list", e))?;
    for summary in listed {
        let name = summary.name;
        let settings = collections
            .get_settings(&name)
            .map_err(|e| unreadable(&format!("settings of collection '{name}'"), e))?;
        add_variable_secrets(&mut out, &settings.variables);
        let env_repo = collection_envs.for_collection(&name);
        let envs = env_repo
            .list_strict()
            .map_err(|e| unreadable(&format!("environments of collection '{name}'"), e))?;
        add_env_secrets(&mut out, &envs, env_repo.as_ref())?;
        let tree = collections
            .get_strict(&name)
            .map_err(|e| unreadable(&format!("requests of collection '{name}'"), e))?;
        walk_folder(collections, &name, &tree.root, "", &mut out)?;
    }
    Ok(out)
}

/// Logs which scope could not be read and the kind of error, never its text, which may name
/// a value. The error is returned unchanged for the caller to refuse the save.
fn unreadable(scope: &str, e: DomainError) -> DomainError {
    let kind = match &e {
        DomainError::NotFound(_) => "not_found",
        DomainError::InvalidInput(_) => "invalid_input",
        DomainError::Io(_) => "io",
        DomainError::Serialization(_) => "serialization",
        DomainError::Internal(_) => "internal",
        _ => "other",
    };
    tracing::warn!(
        scope,
        kind,
        "cannot read a variable scope, so the conversation is not saved"
    );
    e
}

fn add_variable_secrets(out: &mut HashSet<String>, vars: &[CollectionVariable]) {
    for var in vars.iter().filter(|v| v.secret) {
        for value in [&var.value, &var.initial_value] {
            if !value.is_empty() {
                out.insert(value.clone());
            }
        }
    }
}

/// Adds the secret values of each environment flattened with its parents. A cycle is an error,
/// so a missed secret never leaks.
fn add_env_secrets(
    out: &mut HashSet<String>,
    envs: &[Environment],
    repo: &dyn EnvironmentRepository,
) -> DomainResult<()> {
    for env in envs {
        let flat = flatten_with_repo(env, repo).map_err(|e| unreadable("environment chain", e))?;
        for f in flat
            .variables
            .iter()
            .filter(|f| f.variable.secret && !f.variable.value.is_empty())
        {
            out.insert(f.variable.value.clone());
        }
    }
    Ok(())
}

fn walk_folder(
    collections: &dyn CollectionRepository,
    collection: &str,
    folder: &Folder,
    path: &str,
    out: &mut HashSet<String>,
) -> DomainResult<()> {
    for item in &folder.items {
        match item {
            CollectionItem::Request(r) => add_variable_secrets(out, &r.variables),
            CollectionItem::GraphQl(r) => add_variable_secrets(out, &r.variables),
            CollectionItem::WebSocket(r) => add_variable_secrets(out, &r.variables),
            CollectionItem::Grpc(r) => add_variable_secrets(out, &r.variables),
            CollectionItem::Folder(child) => {
                let child_path = if path.is_empty() {
                    folder_dir_name(child).to_string()
                } else {
                    format!("{path}/{}", folder_dir_name(child))
                };
                match collections.get_folder_settings(collection, &child_path) {
                    Ok(settings) => add_variable_secrets(out, &settings.variables),
                    Err(DomainError::NotFound(_)) => {}
                    Err(e) => {
                        return Err(unreadable(
                            &format!("folder settings '{child_path}' of collection '{collection}'"),
                            e,
                        ))
                    }
                }
                walk_folder(collections, collection, child, &child_path, out)?;
            }
            CollectionItem::OpaqueItem(_)
            | CollectionItem::Summary(_)
            | CollectionItem::ScriptFile(_) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_read_views::mask_secret_text;
    use crate::test_doubles::ConfigurableCollectionRepo;
    use rocket_environment::Variable;

    struct MapEnvRepo(Vec<Environment>);

    impl EnvironmentRepository for MapEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.0.clone())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.0
                .iter()
                .find(|env| env.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, _: &Environment) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    struct NoCollectionEnvs;

    impl EnvironmentRepositoryFactory for NoCollectionEnvs {
        fn for_collection(&self, _: &str) -> Box<dyn EnvironmentRepository> {
            Box::new(MapEnvRepo(vec![]))
        }
    }

    fn env(name: &str, extends: Option<&str>) -> Environment {
        let mut e = Environment::new(name);
        e.extends = extends.map(str::to_string);
        e
    }

    #[test]
    fn conversation_secrets_include_an_inherited_secret() {
        let mut parent = env("base", None);
        parent
            .variables
            .push(Variable::secret("token", "parent-secret-value-1"));
        let child = env("child", Some("base"));
        let globals = MapEnvRepo(vec![child, parent]);
        let secrets = workspace_secret_values(
            ConfigurableCollectionRepo::new().as_ref(),
            &NoCollectionEnvs,
            &globals,
        )
        .expect("secrets");
        let text = mask_secret_text("sent parent-secret-value-1 here", &secrets);
        assert!(!text.contains("parent-secret-value-1"), "{text}");
    }

    #[test]
    fn conversation_secrets_fail_closed_on_a_cycle() {
        let globals = MapEnvRepo(vec![env("a", Some("b")), env("b", Some("a"))]);
        let result = workspace_secret_values(
            ConfigurableCollectionRepo::new().as_ref(),
            &NoCollectionEnvs,
            &globals,
        );
        assert!(result.is_err());
    }
}
