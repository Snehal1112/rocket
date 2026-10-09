//! Finds the target of `rok.runRequest` and guards against runaway nesting.
//!
//! Pure functions over the collection tree. A path is the request's file path
//! relative to the collection root, with folder directory names and no extension,
//! the same paths the Collection Runner uses.

use std::collections::HashMap;

use rocket_collection::{Collection, CollectionItem, Folder};

use crate::runner_sequence::{flatten_run_set, RunItem};

/// Deepest chain of nested `rok.runRequest` runs.
pub(crate) const MAX_RUN_DEPTH: usize = 5;

/// What a `rok.runRequest` path points at.
#[derive(Debug)]
pub(crate) enum RunTarget {
    /// An HTTP or GraphQL request that can run.
    Http(Box<RunItem>),
    /// A request of another protocol, which `rok.runRequest` skips.
    Skipped,
    /// Nothing at that path.
    NotFound,
}

/// Normalizes a request path: forward slashes, no outer slashes, no extension.
pub(crate) fn normalize_run_path(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let path = path.trim_matches('/');
    for ext in [".yml", ".yaml", ".json", ".bru"] {
        if let Some(stem) = path.strip_suffix(ext) {
            return stem.to_string();
        }
    }
    path.to_string()
}

/// Rejects a call that would revisit a request of the chain or nest too deep.
///
/// `chain` holds the requests already running, outermost first, so its length
/// is the nesting level the new run would have.
pub(crate) fn check_run_chain(chain: &[String], target: &str) -> Result<(), String> {
    if chain.iter().any(|path| path == target) {
        return Err(format!("rok.runRequest: recursive call to {target}"));
    }
    if chain.len() > MAX_RUN_DEPTH {
        return Err(format!(
            "rok.runRequest: nesting deeper than {MAX_RUN_DEPTH} requests"
        ));
    }
    Ok(())
}

/// Finds the item at `path`, which must already be normalized.
pub(crate) fn find_run_target(collection: &Collection, path: &str) -> RunTarget {
    if let Ok(items) = flatten_run_set(collection, None) {
        if let Some(item) = items
            .into_iter()
            .find(|item| normalize_run_path(&item.request_path) == path)
        {
            return RunTarget::Http(Box::new(item));
        }
    }
    if has_other_protocol_item(&collection.root, "", path) {
        RunTarget::Skipped
    } else {
        RunTarget::NotFound
    }
}

/// True when a WebSocket or gRPC request file sits at `path`.
fn has_other_protocol_item(folder: &Folder, prefix: &str, path: &str) -> bool {
    folder.items.iter().any(|item| {
        let file_name = match item {
            CollectionItem::WebSocket(ws) => ws.file_name.as_deref(),
            CollectionItem::Grpc(grpc) => grpc.file_name.as_deref(),
            CollectionItem::Folder(sub) => {
                let dir = sub.dir_name.as_deref().unwrap_or(&sub.name);
                return has_other_protocol_item(sub, &format!("{prefix}{dir}/"), path);
            }
            _ => None,
        };
        file_name.is_some_and(|name| normalize_run_path(&format!("{prefix}{name}")) == path)
    })
}

/// What a nested run changed in the runtime scope it was seeded with: the keys
/// it set or changed, and the keys it removed (sorted).
pub(crate) fn runtime_changes(
    seed: &HashMap<String, String>,
    after: &HashMap<String, String>,
) -> (HashMap<String, String>, Vec<String>) {
    let set = after
        .iter()
        .filter(|(key, value)| seed.get(*key) != Some(*value))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut removed: Vec<String> = seed
        .keys()
        .filter(|key| !after.contains_key(*key))
        .cloned()
        .collect();
    removed.sort();
    (set, removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::{Request, WebSocketRequest};
    use rocket_shared::types::HttpMethod;

    fn saved(name: &str, file: &str) -> Request {
        let mut request = Request::new(name, HttpMethod::Get, format!("https://api.test/{file}"));
        request.file_name = Some(file.to_string());
        request
    }

    /// root: [main.yml, socket.yml (WebSocket), auth/ [login.yml]]
    fn collection() -> Collection {
        let mut collection = Collection::new("api");
        collection.root.add_request(saved("Main", "main.yml"));
        let mut socket = WebSocketRequest::new("Socket", "wss://api.test/ws");
        socket.file_name = Some("socket.yml".into());
        collection
            .root
            .items
            .push(CollectionItem::WebSocket(Box::new(socket)));
        let mut auth = Folder::new("auth");
        auth.add_request(saved("Login", "login.yml"));
        collection.root.add_subfolder(auth);
        collection
    }

    #[test]
    fn normalize_run_path_accepts_the_usual_spellings() {
        assert_eq!(normalize_run_path("auth/login"), "auth/login");
        assert_eq!(normalize_run_path(" /auth/login.yml/ "), "auth/login");
        assert_eq!(normalize_run_path("auth\\login"), "auth/login");
        assert_eq!(normalize_run_path("main.bru"), "main");
    }

    #[test]
    fn check_run_chain_rejects_a_revisit() {
        let chain = vec!["main".to_string(), "auth/login".to_string()];
        assert_eq!(
            check_run_chain(&chain, "main"),
            Err("rok.runRequest: recursive call to main".to_string())
        );
        assert_eq!(check_run_chain(&chain, "other"), Ok(()));
    }

    #[test]
    fn check_run_chain_allows_five_nested_levels() {
        let five: Vec<String> = (1..=5).map(|i| format!("r{i}")).collect();
        assert_eq!(check_run_chain(&five, "r6"), Ok(()));
        let six: Vec<String> = (1..=6).map(|i| format!("r{i}")).collect();
        assert_eq!(
            check_run_chain(&six, "r7"),
            Err("rok.runRequest: nesting deeper than 5 requests".to_string())
        );
        assert_eq!(check_run_chain(&[], "r1"), Ok(()));
    }

    #[test]
    fn find_run_target_finds_http_requests_in_folders() {
        match find_run_target(&collection(), "auth/login") {
            RunTarget::Http(item) => {
                assert_eq!(item.name, "Login");
                assert_eq!(item.request_path, "auth/login.yml");
            }
            other => panic!("expected an HTTP target, got {other:?}"),
        }
        assert!(matches!(find_run_target(&collection(), "main"), RunTarget::Http(_)));
    }

    #[test]
    fn find_run_target_skips_other_protocols_and_misses_unknown_paths() {
        assert!(matches!(find_run_target(&collection(), "socket"), RunTarget::Skipped));
        assert!(matches!(find_run_target(&collection(), "auth/nope"), RunTarget::NotFound));
        assert!(matches!(find_run_target(&collection(), "login"), RunTarget::NotFound));
    }

    #[test]
    fn runtime_changes_reports_sets_and_removals() {
        let seed: HashMap<String, String> = [("keep", "1"), ("change", "a"), ("drop", "x")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let after: HashMap<String, String> = [("keep", "1"), ("change", "b"), ("new", "n")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let (set, removed) = runtime_changes(&seed, &after);
        assert_eq!(set.len(), 2);
        assert_eq!(set.get("change").map(String::as_str), Some("b"));
        assert_eq!(set.get("new").map(String::as_str), Some("n"));
        assert_eq!(removed, vec!["drop".to_string()]);
    }
}
