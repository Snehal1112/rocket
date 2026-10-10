//! Secret variable values of a whole workspace, for masking stored
//! conversations. Vault values are not fetched here (spec section 8).

use std::collections::HashSet;

use rocket_collection::{CollectionItem, CollectionRepository, CollectionVariable, Folder};
use rocket_environment::{Environment, EnvironmentRepository, EnvironmentRepositoryFactory};
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
    add_env_secrets(&mut out, &global_envs.list()?);
    for summary in collections.list()? {
        let name = summary.name;
        add_variable_secrets(&mut out, &collections.get_settings(&name)?.variables);
        add_env_secrets(&mut out, &collection_envs.for_collection(&name).list()?);
        let tree = collections.get(&name)?;
        walk_folder(collections, &name, &tree.root, "", &mut out)?;
    }
    Ok(out)
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

fn add_env_secrets(out: &mut HashSet<String>, envs: &[Environment]) {
    for var in envs
        .iter()
        .flat_map(|env| env.variables.iter())
        .filter(|v| v.secret && !v.value.is_empty())
    {
        out.insert(var.value.clone());
    }
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
                    Err(e) => return Err(e),
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
