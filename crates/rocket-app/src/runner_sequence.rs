//! Turns a collection tree into the ordered list of executable steps for one
//! Collection Runner run, and turns each step into an `ExecuteRequestInput`.
//!
//! Pure functions — no I/O and no service state. The order is whatever
//! `CollectionRepository::get` returned, which is already `_order.yml` order
//! (see `rocket-infra` `build_folder_tree`), so the runner needs no ordering
//! concept of its own (spec §4).

use rocket_collection::{Collection, CollectionItem, Folder, Request};
use rocket_http::RequestOptions;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{RequestSettingValue, RequestSettings};

use crate::execution_service::ExecuteRequestInput;

/// One executable step in a run set.
#[derive(Debug, Clone)]
pub struct RunItem {
    /// Display name. This is what `rok.runner.setNextRequest(name)` matches on.
    pub name: String,
    /// Path relative to the collection root, e.g. `"auth/login.yml"`.
    pub request_path: String,
    /// The saved request definition.
    pub request: Request,
}

/// Flattens a collection, or one folder inside it, into the ordered list of
/// executable HTTP requests.
///
/// `folder_path` is relative to the collection root and uses on-disk directory
/// names; `None` or `""` runs the whole collection. Sub-folders are traversed
/// depth-first in item order. Folders, opaque protocol items (GraphQL/gRPC/
/// WebSocket) and sidebar summaries are not executable and never become steps.
pub fn flatten_run_set(
    collection: &Collection,
    folder_path: Option<&str>,
) -> DomainResult<Vec<RunItem>> {
    let trimmed = folder_path.unwrap_or("").trim_matches('/');

    let mut folder = &collection.root;
    let mut prefix = String::new();
    if !trimmed.is_empty() {
        for segment in trimmed.split('/') {
            folder = folder
                .items
                .iter()
                .find_map(|item| match item {
                    CollectionItem::Folder(f) if folder_dir_name(f) == segment => Some(f),
                    _ => None,
                })
                .ok_or_else(|| {
                    DomainError::NotFound(format!(
                        "folder '{trimmed}' in collection '{}'",
                        collection.name
                    ))
                })?;
            prefix.push_str(segment);
            prefix.push('/');
        }
    }

    let mut out = Vec::new();
    collect_items(folder, &prefix, &mut out);
    Ok(out)
}

/// Depth-first walk that preserves the on-disk item order.
fn collect_items(folder: &Folder, prefix: &str, out: &mut Vec<RunItem>) {
    for item in &folder.items {
        match item {
            CollectionItem::Request(request) => {
                let Some(file_name) = request.file_name.as_ref() else {
                    tracing::warn!(
                        request = %request.name,
                        "run set: request has no on-disk file name, skipping"
                    );
                    continue;
                };
                out.push(RunItem {
                    name: request.name.clone(),
                    request_path: format!("{prefix}{file_name}"),
                    request: request.clone(),
                });
            }
            CollectionItem::Folder(sub) => {
                let sub_prefix = format!("{prefix}{}/", folder_dir_name(sub));
                collect_items(sub, &sub_prefix, out);
            }
            // Non-HTTP protocols and sidebar summaries are not executable.
            CollectionItem::OpaqueItem(_) | CollectionItem::Summary(_) => {}
        }
    }
}

/// On-disk directory name for a folder, falling back to its display name.
fn folder_dir_name(folder: &Folder) -> &str {
    folder.dir_name.as_deref().unwrap_or(&folder.name)
}

/// Builds the execution input for one run step.
///
/// Mirrors what the Request tab sends for a single send: request-level auth
/// (collection auth is merged later inside `resolve_request`), the saved
/// settings mapped onto `RequestOptions`, and all three script phases.
/// `request.runtime_auth` is deliberately ignored — the single-send path does
/// not consume it either, and the runner must not diverge from it.
///
/// `request_guard_policy` is the workspace's SSRF guard policy (see
/// `request_guard.rs` / Item 6's request-mutation host guard spec) — the
/// runner must apply the same policy to every step's BeforeRequest script as
/// a single send would, not silently default to permissive.
pub fn build_step_input(
    item: &RunItem,
    collection: &str,
    environment_name: Option<&str>,
    global_env_name: Option<&str>,
    request_guard_policy: rocket_workspace::RequestGuardPolicy,
) -> ExecuteRequestInput {
    let request = &item.request;
    ExecuteRequestInput {
        method: request.method.clone(),
        url: request.url.clone(),
        headers: request.headers.clone(),
        query_params: request.query_params.clone(),
        body: request.body.clone(),
        auth: request.auth.clone(),
        options: request_options_from(request.settings.as_ref()),
        environment_name: environment_name.map(str::to_string),
        collection: Some(collection.to_string()),
        request_name: Some(request.name.clone()),
        request_path: Some(item.request_path.clone()),
        tags: request.tags.clone(),
        path_params: request.path_params.clone(),
        pre_request_script: request.pre_request_script.clone(),
        post_response_script: request.post_response_script.clone(),
        tests_script: request.tests.clone(),
        global_env_name: global_env_name.map(str::to_string),
        assertions: request.assertions.clone(),
        actions: request.actions.clone(),
        request_guard_policy,
    }
}

/// Maps a saved request's `settings` block onto executor `RequestOptions`.
/// A missing setting and the literal `"inherit"` both fall back to the
/// `RequestOptions` default, which is what a settings-less request sends today.
fn request_options_from(settings: Option<&RequestSettings>) -> RequestOptions {
    let mut options = RequestOptions::default();
    let Some(settings) = settings else {
        return options;
    };
    if let Some(RequestSettingValue::Value(v)) = settings.follow_redirects.as_ref() {
        options.follow_redirects = *v;
    }
    if let Some(RequestSettingValue::Value(v)) = settings.timeout.as_ref() {
        // `settings.timeout` is milliseconds in the OpenCollection format.
        if *v > 0.0 {
            options.timeout_ms = *v as u64;
        }
    }
    if let Some(RequestSettingValue::Value(v)) = settings.verify_ssl.as_ref() {
        options.verify_ssl = *v;
    }
    if let Some(RequestSettingValue::Value(v)) = settings.max_redirects.as_ref() {
        if *v >= 0.0 {
            options.max_redirects = Some(*v as u32);
        }
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::{Collection, Folder, OpaqueProtocolItem, Request};
    use rocket_shared::types::HttpMethod;

    fn req(name: &str, file: &str) -> Request {
        let mut r = Request::new(name, HttpMethod::Get, format!("https://api.test/{name}"));
        r.file_name = Some(file.to_string());
        r
    }

    fn folder(name: &str, dir: &str) -> Folder {
        let mut f = Folder::new(name);
        f.dir_name = Some(dir.to_string());
        f
    }

    /// root: [Login, auth/{Refresh, admin/{Purge}}, Logout]
    fn sample_collection() -> Collection {
        let mut admin = folder("admin", "admin");
        admin.add_request(req("Purge", "purge.yml"));

        let mut auth = folder("auth", "auth");
        auth.add_request(req("Refresh", "refresh.yml"));
        auth.add_subfolder(admin);

        let mut collection = Collection::new("my-api");
        collection.root.dir_name = Some("my-api".into());
        collection.root.add_request(req("Login", "login.yml"));
        collection.root.add_subfolder(auth);
        collection.root.add_request(req("Logout", "logout.yml"));
        collection
    }

    #[test]
    fn flattens_whole_collection_depth_first_in_item_order() {
        let items = flatten_run_set(&sample_collection(), None).expect("flatten");
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["Login", "Refresh", "Purge", "Logout"]);
    }

    #[test]
    fn builds_request_paths_relative_to_the_collection_root() {
        let items = flatten_run_set(&sample_collection(), None).expect("flatten");
        let paths: Vec<&str> = items.iter().map(|i| i.request_path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "login.yml",
                "auth/refresh.yml",
                "auth/admin/purge.yml",
                "logout.yml"
            ]
        );
    }

    #[test]
    fn folder_scoped_run_only_contains_that_subtree() {
        let items = flatten_run_set(&sample_collection(), Some("auth")).expect("flatten");
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["Refresh", "Purge"]);
        assert_eq!(items[1].request_path, "auth/admin/purge.yml");
    }

    #[test]
    fn unknown_folder_path_is_not_found() {
        let err = flatten_run_set(&sample_collection(), Some("nope")).expect_err("must fail");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::NotFound(_)
        ));
    }

    #[test]
    fn opaque_protocol_items_are_never_steps() {
        let mut collection = Collection::new("my-api");
        collection.root.add_request(req("Login", "login.yml"));
        collection
            .root
            .items
            .push(rocket_collection::CollectionItem::OpaqueItem(
                OpaqueProtocolItem {
                    protocol: "graphql".into(),
                    name: "Search".into(),
                    raw: serde_yaml::Value::Null,
                },
            ));

        let items = flatten_run_set(&collection, None).expect("flatten");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Login");
    }

    #[test]
    fn step_input_carries_scripts_path_and_scope_names() {
        let mut request = req("Login", "login.yml");
        request.pre_request_script = Some("// pre".into());
        request.tests = Some("// tests".into());
        request.tags = vec!["smoke".into()];
        let item = RunItem {
            name: request.name.clone(),
            request_path: "auth/login.yml".into(),
            request,
        };

        let input = build_step_input(
            &item,
            "my-api",
            Some("dev"),
            Some("shared-global"),
            rocket_workspace::RequestGuardPolicy::default(),
        );

        assert_eq!(input.collection.as_deref(), Some("my-api"));
        assert_eq!(input.request_path.as_deref(), Some("auth/login.yml"));
        assert_eq!(input.request_name.as_deref(), Some("Login"));
        assert_eq!(input.environment_name.as_deref(), Some("dev"));
        assert_eq!(input.global_env_name.as_deref(), Some("shared-global"));
        assert_eq!(input.pre_request_script.as_deref(), Some("// pre"));
        assert_eq!(input.tests_script.as_deref(), Some("// tests"));
        assert_eq!(input.tags, vec!["smoke".to_string()]);
    }

    #[test]
    fn step_input_maps_request_settings_onto_request_options() {
        use rocket_shared::types::{RequestSettingValue, RequestSettings};

        let mut request = req("Login", "login.yml");
        request.settings = Some(RequestSettings {
            encode_url: None,
            timeout: Some(RequestSettingValue::Value(5000.0)),
            follow_redirects: Some(RequestSettingValue::Value(false)),
            max_redirects: Some(RequestSettingValue::Value(3.0)),
            verify_ssl: Some(RequestSettingValue::Inherit("inherit".into())),
        });
        let item = RunItem {
            name: request.name.clone(),
            request_path: "login.yml".into(),
            request,
        };

        let input = build_step_input(
            &item,
            "my-api",
            None,
            None,
            rocket_workspace::RequestGuardPolicy::default(),
        );
        assert_eq!(input.options.timeout_ms, 5000);
        assert!(!input.options.follow_redirects);
        assert_eq!(input.options.max_redirects, Some(3));
        assert!(
            input.options.verify_ssl,
            "\"inherit\" falls back to the default"
        );
    }

    #[test]
    fn step_input_carries_the_request_guard_policy() {
        // The Collection Runner sends every step through the same BeforeRequest
        // SSRF guard as a single send (spec follows Item 6's request_mutation
        // host guard) -- the policy must not silently default to permissive.
        let item = RunItem {
            name: "Login".into(),
            request_path: "login.yml".into(),
            request: req("Login", "login.yml"),
        };
        let policy = rocket_workspace::RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };

        let input = build_step_input(&item, "my-api", None, None, policy.clone());
        assert_eq!(input.request_guard_policy, policy);
    }
}
