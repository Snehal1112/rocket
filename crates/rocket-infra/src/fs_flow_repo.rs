use std::fs;
use std::path::{Path, PathBuf};

use rocket_collection::Collection;
use rocket_flow::{Flow, FlowRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

pub struct FsFlowRepo {
    base_dir: PathBuf,
}

impl FsFlowRepo {
    /// `base_dir` is the same collections base directory `FsCollectionRepo`
    /// uses — one `FsFlowRepo` instance serves every collection, dispatched
    /// by the `collection` argument on each trait method.
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Rejects collection names that could escape `base_dir`, like `FsCollectionRepo` does.
    fn flows_dir(&self, collection: &str) -> DomainResult<PathBuf> {
        Collection::validate_name(collection)?;
        Ok(self.base_dir.join(collection).join("flows"))
    }

    /// Rejects flow names whose slug is empty, since they would all map to a hidden `.yml` file.
    fn file_path(&self, collection: &str, name: &str) -> DomainResult<PathBuf> {
        let slug = slugify(name);
        if slug.is_empty() {
            return Err(DomainError::InvalidInput(format!(
                "Flow name '{name}' must contain at least one ASCII letter or digit"
            )));
        }
        Ok(self.flows_dir(collection)?.join(format!("{slug}.yml")))
    }

    fn read_flow(path: &Path) -> DomainResult<Flow> {
        let content = fs::read_to_string(path)
            .map_err(|e| DomainError::Io(format!("Failed to read flow file: {e}")))?;
        serde_yaml::from_str(&content)
            .map_err(|e| DomainError::InvalidInput(format!("Failed to parse flow YAML: {e}")))
    }

    /// The rename algorithm. `remove` deletes the old file and is a parameter
    /// only so a test can make it fail. Order matters: the new file is fully
    /// written before the old one goes, so a crash leaves at least one copy,
    /// and a failed removal deletes the new copy again so there is never a
    /// duplicate.
    fn rename_with(
        &self,
        collection: &str,
        old_name: &str,
        new_name: &str,
        remove: &dyn Fn(&Path) -> std::io::Result<()>,
    ) -> DomainResult<()> {
        let old_path = self.file_path(collection, old_name)?;
        // Rejects a target whose slug is empty before anything is touched.
        let new_path = self.file_path(collection, new_name)?;
        if !old_path.exists() {
            return Err(not_found(collection, old_name));
        }
        let mut flow = Self::read_flow(&old_path)?;
        // A different name that shares the slug is a different flow.
        if flow.name != old_name {
            return Err(not_found(collection, old_name));
        }
        flow.name = new_name.to_string();
        let yaml = serde_yaml::to_string(&flow)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize flow: {e}")))?;

        // Case or punctuation only: both names use one file, so rewrite it in place.
        if old_path == new_path {
            return atomic_write(&new_path, yaml.as_bytes())
                .map_err(|e| DomainError::Io(format!("Failed to write flow file: {e}")));
        }
        if new_path.exists() {
            return Err(DomainError::Conflict(format!(
                "Flow name '{new_name}' collides with an existing flow in collection '{collection}'"
            )));
        }
        atomic_write(&new_path, yaml.as_bytes())
            .map_err(|e| DomainError::Io(format!("Failed to write flow file: {e}")))?;
        if let Err(e) = remove(&old_path) {
            // Best effort: if this also fails there is nothing more to do, and the
            // original error is the one worth reporting.
            let _ = fs::remove_file(&new_path);
            return Err(DomainError::Io(format!(
                "Failed to remove the old flow file: {e}"
            )));
        }
        Ok(())
    }
}

fn not_found(collection: &str, name: &str) -> DomainError {
    DomainError::NotFound(format!("Flow '{name}' in collection '{collection}'"))
}

/// Lowercase, hyphen-separated slug for a Flow's filename. This repo has no
/// existing shared slugify helper to reuse (`Collection::validate_name`
/// rejects invalid names rather than transforming them) — this is new,
/// self-contained logic.
fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_hyphen = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_hyphen = false;
        } else if !last_was_hyphen && !slug.is_empty() {
            slug.push('-');
            last_was_hyphen = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

impl FlowRepository for FsFlowRepo {
    fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
        let dir = self.flows_dir(collection)?;
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(&dir).map_err(|e| DomainError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| DomainError::Io(e.to_string()))?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "yml") {
                continue;
            }
            // Skip files that can't be read or parsed, continue with the rest.
            let Ok(flow) = Self::read_flow(&path) else {
                continue;
            };
            // Only list names that `get` can resolve back to this same file.
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if slugify(&flow.name) != stem {
                continue;
            }
            names.push(flow.name);
        }
        names.sort();
        Ok(names)
    }

    fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
        let path = self.file_path(collection, name)?;
        if !path.exists() {
            return Err(not_found(collection, name));
        }
        let flow = Self::read_flow(&path)?;
        // A different name that shares the slug is a different flow.
        if flow.name != name {
            return Err(not_found(collection, name));
        }
        Ok(flow)
    }

    fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
        let path = self.file_path(collection, &flow.name)?;
        // Refuse to overwrite a different flow whose name maps to the same file.
        if path.exists() {
            if let Ok(existing) = Self::read_flow(&path) {
                if existing.name != flow.name {
                    return Err(DomainError::Conflict(format!(
                        "Flow name '{}' collides with existing flow '{}' in collection '{collection}'",
                        flow.name, existing.name
                    )));
                }
            }
        }
        let yaml = serde_yaml::to_string(flow)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize flow: {e}")))?;
        atomic_write(&path, yaml.as_bytes())
            .map_err(|e| DomainError::Io(format!("Failed to write flow file: {e}")))
    }

    fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
        let path = self.file_path(collection, name)?;
        if !path.exists() {
            return Err(not_found(collection, name));
        }
        // Never delete a different flow that only shares the slug.
        if let Ok(existing) = Self::read_flow(&path) {
            if existing.name != name {
                return Err(not_found(collection, name));
            }
        }
        fs::remove_file(&path).map_err(|e| DomainError::Io(e.to_string()))
    }

    fn rename(&self, collection: &str, old_name: &str, new_name: &str) -> DomainResult<()> {
        self.rename_with(collection, old_name, new_name, &|path: &Path| {
            fs::remove_file(path)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_flow::{
        FlowEdge, FlowNode, FlowNodeKind, InlineHeader, InlineRequestData, NodePosition,
        RepeatUntil, RequestSource, SwitchCase,
    };
    use tempfile::TempDir;

    fn setup() -> (TempDir, FsFlowRepo) {
        let dir = TempDir::new().expect("create temp dir");
        let repo = FsFlowRepo::new(dir.path().to_path_buf());
        fs::create_dir_all(dir.path().join("acme")).expect("create collection dir");
        (dir, repo)
    }

    fn sample(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![FlowNode {
                id: "node-1".to_string(),
                kind: FlowNodeKind::Input {
                    label: "Base URL".to_string(),
                    value: rocket_shared::VariableValue::simple("https://api.example.com"),
                },
                position: NodePosition { x: 100.0, y: 200.0 },
            }],
            edges: Vec::new(),
            callback_host: None,
        }
    }

    #[test]
    fn list_on_collection_with_no_flows_dir_returns_empty() {
        let (_dir, repo) = setup();
        assert_eq!(repo.list("acme").expect("list"), Vec::<String>::new());
    }

    #[test]
    fn get_on_missing_flow_returns_not_found() {
        let (_dir, repo) = setup();
        let err = repo
            .get("acme", "no-such-flow")
            .expect_err("must not find a flow that was never saved");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn save_get_list_roundtrip() {
        let (_dir, repo) = setup();
        let flow = sample("Login Flow");
        repo.save("acme", &flow).expect("save");

        let loaded = repo.get("acme", "Login Flow").expect("get");
        assert_eq!(loaded, flow);
        assert_eq!(
            repo.list("acme").expect("list"),
            vec!["Login Flow".to_string()]
        );
    }

    #[test]
    fn save_under_same_name_replaces_not_duplicates() {
        let (_dir, repo) = setup();
        repo.save("acme", &sample("Login Flow"))
            .expect("save first");

        let mut updated = sample("Login Flow");
        updated.nodes.push(FlowNode {
            id: "node-2".to_string(),
            kind: FlowNodeKind::Output {
                label: "Result".to_string(),
            },
            position: NodePosition { x: 400.0, y: 200.0 },
        });
        repo.save("acme", &updated).expect("save update");

        let names = repo.list("acme").expect("list");
        assert_eq!(
            names.len(),
            1,
            "same flow name must replace, not append a second file"
        );
        assert_eq!(repo.get("acme", "Login Flow").expect("get").nodes.len(), 2);
    }

    #[test]
    fn delete_of_missing_flow_returns_not_found() {
        let (_dir, repo) = setup();
        let err = repo
            .delete("acme", "no-such-flow")
            .expect_err("deleting a flow that was never saved must error");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn delete_removes_the_flow() {
        let (_dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save");
        repo.delete("acme", "Login Flow").expect("delete");
        assert!(repo.list("acme").expect("list").is_empty());
    }

    #[test]
    fn flow_name_with_spaces_and_punctuation_slugifies_stably() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("My First Flow!")).expect("save");

        assert!(
            dir.path()
                .join("acme")
                .join("flows")
                .join("my-first-flow.yml")
                .exists(),
            "expected slugified filename my-first-flow.yml"
        );
        let loaded = repo
            .get("acme", "My First Flow!")
            .expect("get by original name");
        assert_eq!(loaded.name, "My First Flow!");
    }

    #[test]
    fn malformed_yaml_errors_clearly_instead_of_panicking() {
        let (dir, repo) = setup();
        let flows_dir = dir.path().join("acme").join("flows");
        fs::create_dir_all(&flows_dir).expect("create flows dir");
        fs::write(flows_dir.join("broken.yml"), b"not: valid: yaml: [")
            .expect("write malformed file");

        let err = repo
            .get("acme", "Broken")
            .expect_err("malformed YAML must error, not panic");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn on_disk_file_has_no_camelcase_field_names() {
        let (dir, repo) = setup();
        let mut flow = sample("Login Flow");
        flow.edges.push(FlowEdge {
            id: "edge-1".to_string(),
            source_node_id: "node-1".to_string(),
            target_node_id: "node-2".to_string(),
            target_field: "url".to_string(),
            expression: "response.body".to_string(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        });
        repo.save("acme", &flow).expect("save");
        let raw = fs::read_to_string(dir.path().join("acme").join("flows").join("login-flow.yml"))
            .expect("read saved flow file");
        assert!(
            raw.contains("source_node_id"),
            "expected snake_case field, got:\n{raw}"
        );
        assert!(
            !raw.contains("sourceNodeId"),
            "must not contain camelCase, got:\n{raw}"
        );
    }

    #[test]
    fn list_skips_malformed_entry_and_continues() {
        let (dir, repo) = setup();
        let flows_dir = dir.path().join("acme").join("flows");
        fs::create_dir_all(&flows_dir).expect("create flows dir");

        // Create one malformed file.
        fs::write(flows_dir.join("broken.yml"), b"not: valid: yaml: [")
            .expect("write malformed file");

        // Create one valid flow file.
        repo.save("acme", &sample("Good Flow"))
            .expect("save valid flow");

        // list() should skip the malformed file and return only the valid one.
        let names = repo
            .list("acme")
            .expect("list must succeed despite malformed file");
        assert_eq!(names, vec!["Good Flow".to_string()]);
    }

    #[test]
    fn request_node_with_saved_source_roundtrips() {
        let (_dir, repo) = setup();
        let flow = Flow {
            name: "Saved Source Flow".to_string(),
            nodes: vec![FlowNode {
                id: "node-1".to_string(),
                kind: FlowNodeKind::Request {
                    debug: false,
                    repeat_until: None,
                    label: "Get User".to_string(),
                    source: RequestSource::Saved {
                        request_path: "users/get-user.yml".to_string(),
                    },
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: Vec::new(),
            callback_host: None,
        };
        repo.save("acme", &flow).expect("save");
        let loaded = repo.get("acme", "Saved Source Flow").expect("get");
        assert_eq!(loaded, flow);
    }

    #[test]
    fn request_node_with_inline_source_roundtrips() {
        let (_dir, repo) = setup();
        let flow = Flow {
            name: "Inline Source Flow".to_string(),
            nodes: vec![FlowNode {
                id: "node-1".to_string(),
                kind: FlowNodeKind::Request {
                    debug: false,
                    repeat_until: None,
                    label: "Ad Hoc Login".to_string(),
                    source: RequestSource::Inline {
                        request: InlineRequestData {
                            method: "POST".to_string(),
                            url: "https://api.example.com/login".to_string(),
                            headers: vec![InlineHeader {
                                name: "Content-Type".to_string(),
                                value: "application/json".to_string(),
                            }],
                            body: Some("{\"user\":\"{{u}}\"}".to_string()),
                        },
                    },
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: Vec::new(),
            callback_host: None,
        };
        repo.save("acme", &flow).expect("save");
        let loaded = repo.get("acme", "Inline Source Flow").expect("get");
        assert_eq!(loaded, flow);
    }

    #[test]
    fn inline_request_with_no_body_roundtrips_as_none() {
        let (_dir, repo) = setup();
        let flow = Flow {
            name: "No Body Flow".to_string(),
            nodes: vec![FlowNode {
                id: "node-1".to_string(),
                kind: FlowNodeKind::Request {
                    debug: false,
                    repeat_until: None,
                    label: "Ping".to_string(),
                    source: RequestSource::Inline {
                        request: InlineRequestData {
                            method: "GET".to_string(),
                            url: "https://api.example.com/ping".to_string(),
                            headers: Vec::new(),
                            body: None,
                        },
                    },
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: Vec::new(),
            callback_host: None,
        };
        repo.save("acme", &flow).expect("save");
        let loaded = repo.get("acme", "No Body Flow").expect("get");
        match &loaded.nodes[0].kind {
            FlowNodeKind::Request {
                source: RequestSource::Inline { request },
                ..
            } => {
                assert_eq!(request.body, None);
                assert!(request.headers.is_empty());
            }
            other => panic!("expected an Inline Request node, got {other:?}"),
        }
    }

    #[test]
    fn all_three_node_kinds_in_one_flow_roundtrip_together() {
        let (_dir, repo) = setup();
        let mut flow = sample("Mixed Kinds Flow");
        flow.nodes.push(FlowNode {
            id: "node-2".to_string(),
            kind: FlowNodeKind::Request {
                debug: false,
                repeat_until: None,
                label: "Call".to_string(),
                source: RequestSource::Saved {
                    request_path: "call.yml".to_string(),
                },
            },
            position: NodePosition { x: 200.0, y: 0.0 },
        });
        flow.nodes.push(FlowNode {
            id: "node-3".to_string(),
            kind: FlowNodeKind::Output {
                label: "Result".to_string(),
            },
            position: NodePosition { x: 400.0, y: 0.0 },
        });
        repo.save("acme", &flow).expect("save");
        let loaded = repo.get("acme", "Mixed Kinds Flow").expect("get");
        assert_eq!(loaded.nodes.len(), 3);
        assert_eq!(loaded, flow);
    }

    #[test]
    fn colliding_slug_does_not_overwrite_or_leak_other_flow() {
        let (_dir, repo) = setup();
        repo.save("acme", &sample("Login Flow"))
            .expect("save original");

        let err = repo
            .save("acme", &sample("login-flow"))
            .expect_err("a different name with the same slug must not overwrite");
        assert!(matches!(err, DomainError::Conflict(_)));

        let err = repo
            .get("acme", "login-flow")
            .expect_err("must not return another flow");
        assert!(matches!(err, DomainError::NotFound(_)));
        let err = repo
            .delete("acme", "LOGIN FLOW")
            .expect_err("must not delete another flow");
        assert!(matches!(err, DomainError::NotFound(_)));

        assert_eq!(
            repo.get("acme", "Login Flow").expect("get").name,
            "Login Flow"
        );
    }

    #[test]
    fn name_with_empty_slug_is_rejected() {
        let (_dir, repo) = setup();
        for name in ["", "!!!", "日本語"] {
            let err = repo
                .save("acme", &sample(name))
                .expect_err("empty slug must be rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "name {name:?}");
        }
    }

    #[test]
    fn collection_name_that_escapes_base_dir_is_rejected() {
        let (_dir, repo) = setup();
        for collection in ["../escape", "a/b", ".hidden", ""] {
            let err = repo
                .save(collection, &sample("Login Flow"))
                .expect_err("unsafe collection name must be rejected");
            assert!(
                matches!(err, DomainError::InvalidInput(_)),
                "collection {collection:?}"
            );
            assert!(repo.list(collection).is_err(), "collection {collection:?}");
        }
    }

    #[test]
    fn list_skips_file_whose_name_does_not_match_its_filename() {
        let (dir, repo) = setup();
        let flows_dir = dir.path().join("acme").join("flows");
        fs::create_dir_all(&flows_dir).expect("create flows dir");
        fs::write(
            flows_dir.join("renamed.yml"),
            b"name: Other\nnodes: []\nedges: []\n",
        )
        .expect("write mismatched file");
        repo.save("acme", &sample("Good Flow"))
            .expect("save valid flow");

        assert_eq!(
            repo.list("acme").expect("list"),
            vec!["Good Flow".to_string()]
        );
    }

    #[test]
    fn transform_node_roundtrips_a_multiline_script() {
        let (_dir, repo) = setup();
        let mut flow = sample("Transform Flow");
        flow.nodes.push(FlowNode {
            id: "t1".to_string(),
            kind: FlowNodeKind::Transform {
                label: "Pick token".to_string(),
                script: "const t = response.body.token;\n\n\treturn \"a\" + 'b' + t;".to_string(),
            },
            position: NodePosition { x: 200.0, y: 0.0 },
        });
        flow.edges.push(FlowEdge {
            id: "e1".to_string(),
            source_node_id: "node-1".to_string(),
            target_node_id: "t1".to_string(),
            target_field: rocket_flow::handle::INPUT.to_string(),
            expression: String::new(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        });
        repo.save("acme", &flow).expect("save");
        assert_eq!(repo.get("acme", "Transform Flow").expect("get"), flow);
    }

    #[test]
    fn auth_node_roundtrips_through_yaml_without_a_token_field() {
        use rocket_shared::oauth2::{OAuth2ClientCredentials, OAuth2Flow};
        use rocket_shared::types::Auth;

        let (_dir, repo) = setup();
        let mut flow = sample("Auth Flow");
        flow.nodes.push(FlowNode {
            id: "a1".to_string(),
            kind: FlowNodeKind::Auth {
                label: "Sign in".to_string(),
                auth: Auth::OAuth2(Box::new(OAuth2Flow::ClientCredentials {
                    access_token_url: "https://idp.example.com/token".to_string(),
                    refresh_token_url: None,
                    credentials: OAuth2ClientCredentials {
                        client_id: "{{clientId}}".to_string(),
                        client_secret: "{{clientSecret}}".to_string(),
                        placement: None,
                    },
                    scope: Some("read".to_string()),
                    additional_parameters: None,
                    token_config: None,
                    settings: None,
                })),
                apply_to_inherit: true,
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        });
        repo.save("acme", &flow).expect("save");
        assert_eq!(repo.get("acme", "Auth Flow").expect("get"), flow);

        let raw = serde_yaml::to_string(&flow).expect("serialize flow to yaml");
        // The Auth config is on disk (so the key checks below see real output)...
        assert!(raw.contains("accessTokenUrl"), "got: {raw}");
        assert!(raw.contains("{{clientSecret}}"), "got: {raw}");
        // ...but no key that could hold a fetched token.
        let value: serde_yaml::Value = serde_yaml::from_str(&raw).expect("parse yaml");
        let mut keys = Vec::new();
        collect_yaml_keys(&value, &mut keys);
        assert!(
            keys.iter().any(|k| k == "accessTokenUrl"),
            "the key walk reaches the Auth config, keys: {keys:?}"
        );
        for forbidden in ["accessToken", "access_token", "refreshToken", "token"] {
            assert!(
                !keys.iter().any(|k| k == forbidden),
                "key {forbidden} persisted, keys: {keys:?}"
            );
        }
    }

    /// Every mapping key anywhere in `value`.
    fn collect_yaml_keys(value: &serde_yaml::Value, keys: &mut Vec<String>) {
        match value {
            serde_yaml::Value::Mapping(map) => {
                for (key, child) in map {
                    if let Some(key) = key.as_str() {
                        keys.push(key.to_string());
                    }
                    collect_yaml_keys(child, keys);
                }
            }
            serde_yaml::Value::Sequence(items) => {
                for item in items {
                    collect_yaml_keys(item, keys);
                }
            }
            serde_yaml::Value::Tagged(tagged) => collect_yaml_keys(&tagged.value, keys),
            _ => {}
        }
    }

    /// A flow file written before Auth nodes existed: Input, Request and
    /// Output nodes and one edge, in the on-disk shape existing flows use.
    const LEGACY_FLOW_YAML: &str = "\
name: Legacy Flow
nodes:
- id: in1
  kind:
    kind: Input
    label: Username
    value: alice
  position:
    x: 0.0
    y: 0.0
- id: req1
  kind:
    kind: Request
    label: Get user
    source:
      type: Saved
      request_path: users/get-user.yml
  position:
    x: 200.0
    y: 0.0
- id: out1
  kind:
    kind: Output
    label: Result
  position:
    x: 400.0
    y: 0.0
edges:
- id: e1
  source_node_id: req1
  target_node_id: out1
  target_field: value
  expression: response.body
";

    #[test]
    fn a_legacy_flow_without_auth_nodes_loads_and_reserializes_unchanged() {
        let flow: Flow = serde_yaml::from_str(LEGACY_FLOW_YAML).expect("legacy flow loads");
        assert_eq!(flow.nodes.len(), 3);
        assert_eq!(flow.edges.len(), 1);
        assert!(matches!(flow.nodes[0].kind, FlowNodeKind::Input { .. }));
        assert!(matches!(flow.nodes[1].kind, FlowNodeKind::Request { .. }));
        assert!(matches!(flow.nodes[2].kind, FlowNodeKind::Output { .. }));

        let resaved = serde_yaml::to_string(&flow).expect("serialize");
        let reloaded: Flow = serde_yaml::from_str(&resaved).expect("re-load");
        assert_eq!(reloaded, flow);
    }

    #[test]
    fn a_legacy_flow_file_loads_through_the_repo() {
        let (dir, repo) = setup();
        let flows_dir = dir.path().join("acme").join("flows");
        fs::create_dir_all(&flows_dir).expect("create flows dir");
        fs::write(flows_dir.join("legacy-flow.yml"), LEGACY_FLOW_YAML).expect("write legacy file");

        let loaded = repo.get("acme", "Legacy Flow").expect("load legacy file");
        let parsed: Flow = serde_yaml::from_str(LEGACY_FLOW_YAML).expect("parse");
        assert_eq!(loaded, parsed);
        repo.save("acme", &loaded).expect("re-save");
        assert_eq!(repo.get("acme", "Legacy Flow").expect("reload"), loaded);
    }

    #[test]
    fn if_and_switch_nodes_with_routed_edges_roundtrip() {
        let (_dir, repo) = setup();
        let mut flow = sample("Routing Flow");
        flow.nodes.push(FlowNode {
            id: "if1".to_string(),
            kind: FlowNodeKind::If {
                label: "Logged in?".to_string(),
                condition: "response.status === 200".to_string(),
            },
            position: NodePosition { x: 200.0, y: 0.0 },
        });
        flow.nodes.push(FlowNode {
            id: "sw1".to_string(),
            kind: FlowNodeKind::Switch {
                label: "Plan router".to_string(),
                value: "response.body.plan".to_string(),
                cases: vec![SwitchCase {
                    id: "c1".to_string(),
                    label: "Pro plan".to_string(),
                    matches: "pro".to_string(),
                }],
            },
            position: NodePosition { x: 400.0, y: 0.0 },
        });
        flow.edges.push(FlowEdge {
            id: "e1".to_string(),
            source_node_id: "node-1".to_string(),
            target_node_id: "if1".to_string(),
            target_field: rocket_flow::handle::INPUT.to_string(),
            expression: String::new(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        });
        flow.edges.push(FlowEdge {
            id: "e2".to_string(),
            source_node_id: "if1".to_string(),
            target_node_id: "sw1".to_string(),
            target_field: rocket_flow::handle::INPUT.to_string(),
            expression: String::new(),
            source_handle: rocket_flow::handle::TRUE.to_string(),
        });
        repo.save("acme", &flow).expect("save");
        assert_eq!(repo.get("acme", "Routing Flow").expect("get"), flow);
    }

    /// Mirrors the Phase 1 on-disk edge shape, which had no `source_handle`.
    #[derive(serde::Serialize)]
    struct Phase1Edge {
        id: String,
        source_node_id: String,
        target_node_id: String,
        target_field: String,
        expression: String,
    }

    /// Mirrors the Phase 1 on-disk flow shape.
    #[derive(serde::Serialize)]
    struct Phase1Flow {
        name: String,
        nodes: Vec<FlowNode>,
        edges: Vec<Phase1Edge>,
    }

    #[test]
    fn fs_repo_resaves_phase1_file_byte_identically() {
        let (dir, repo) = setup();
        let mut nodes = sample("Phase One").nodes;
        nodes.push(FlowNode {
            id: "node-2".to_string(),
            kind: FlowNodeKind::Output {
                label: "Result".to_string(),
            },
            position: NodePosition { x: 400.0, y: 0.0 },
        });
        let phase1 = Phase1Flow {
            name: "Phase One".to_string(),
            nodes,
            edges: vec![Phase1Edge {
                id: "edge-1".to_string(),
                source_node_id: "node-1".to_string(),
                target_node_id: "node-2".to_string(),
                target_field: "value".to_string(),
                expression: "response.body".to_string(),
            }],
        };
        let flows_dir = dir.path().join("acme").join("flows");
        fs::create_dir_all(&flows_dir).expect("create flows dir");
        let path = flows_dir.join("phase-one.yml");
        let original = serde_yaml::to_string(&phase1).expect("serialize Phase 1 flow");
        fs::write(&path, &original).expect("write Phase 1 file");

        let loaded = repo.get("acme", "Phase One").expect("load Phase 1 file");
        repo.save("acme", &loaded).expect("re-save");

        let resaved = fs::read_to_string(&path).expect("read re-saved file");
        assert_eq!(
            resaved, original,
            "re-saving a Phase 1 file must not change it"
        );
    }

    #[test]
    fn request_node_with_repeat_until_roundtrips() {
        let (_dir, repo) = setup();
        let flow = Flow {
            name: "Poll Flow".to_string(),
            nodes: vec![FlowNode {
                id: "node-1".to_string(),
                kind: FlowNodeKind::Request {
                    debug: false,
                    label: "Poll job".to_string(),
                    source: RequestSource::Saved {
                        request_path: "jobs/get-job.yml".to_string(),
                    },
                    repeat_until: Some(RepeatUntil {
                        condition: "response.body.status === \"done\"".to_string(),
                        interval_ms: 500,
                        max_attempts: 10,
                        timeout_ms: 5000,
                    }),
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: Vec::new(),
            callback_host: None,
        };
        repo.save("acme", &flow).expect("save");
        let loaded = repo.get("acme", "Poll Flow").expect("get");
        assert_eq!(loaded, flow);
    }

    #[test]
    fn fs_repo_resaves_request_without_repeat_until_byte_identically() {
        let (dir, repo) = setup();
        let flows_dir = dir.path().join("acme").join("flows");
        fs::create_dir_all(&flows_dir).expect("create flows dir");
        let path = flows_dir.join("plain.yml");
        let original = "name: Plain\nnodes:\n- id: n1\n  kind:\n    kind: Request\n    label: Get\n    source:\n      type: Saved\n      request_path: a.yml\n  position:\n    x: 0.0\n    y: 0.0\nedges: []\n";
        fs::write(&path, original).expect("write file");

        let loaded = repo.get("acme", "Plain").expect("load");
        repo.save("acme", &loaded).expect("re-save");

        let resaved = fs::read_to_string(&path).expect("read re-saved file");
        assert_eq!(
            resaved, original,
            "a Request without repeat_until must re-save unchanged"
        );
    }

    #[test]
    fn wait_for_callback_node_and_callback_host_roundtrip_on_disk() {
        let (dir, repo) = setup();
        let mut flow = sample("Callback Flow");
        flow.callback_host = Some("host.docker.internal".to_string());
        flow.nodes.push(FlowNode {
            id: "wait-1".to_string(),
            kind: FlowNodeKind::WaitForCallback {
                label: "Payment done".to_string(),
                name: "payment".to_string(),
                timeout_ms: 60_000,
                accept_when: Some("request.body.ok".to_string()),
            },
            position: NodePosition { x: 300.0, y: 200.0 },
        });
        repo.save("acme", &flow).expect("save");

        let loaded = repo.get("acme", "Callback Flow").expect("get");
        assert_eq!(loaded, flow);
        let raw = fs::read_to_string(
            dir.path()
                .join("acme")
                .join("flows")
                .join("callback-flow.yml"),
        )
        .expect("read saved flow file");
        assert!(
            raw.contains("callback_host: host.docker.internal"),
            "got:\n{raw}"
        );
        assert!(raw.contains("kind: WaitForCallback"), "got:\n{raw}");
        assert!(
            !raw.contains("timeoutMs"),
            "no camelCase on disk, got:\n{raw}"
        );
    }

    fn flow_files(dir: &TempDir) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir.path().join("acme").join("flows"))
            .expect("read flows dir")
            .map(|e| {
                e.expect("dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn rename_moves_the_flow_to_a_new_file_and_keeps_its_content() {
        let (dir, repo) = setup();
        let original = sample("Login Flow");
        repo.save("acme", &original).expect("save");

        repo.rename("acme", "Login Flow", "Sign In").expect("rename");

        assert_eq!(flow_files(&dir), vec!["sign-in.yml".to_string()]);
        assert!(matches!(
            repo.get("acme", "Login Flow"),
            Err(DomainError::NotFound(_))
        ));
        let loaded = repo.get("acme", "Sign In").expect("get renamed");
        assert_eq!(loaded.name, "Sign In");
        assert_eq!(loaded.nodes, original.nodes);
        assert_eq!(repo.list("acme").expect("list"), vec!["Sign In".to_string()]);
    }

    #[test]
    fn rename_that_only_changes_case_rewrites_the_same_file() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save");

        repo.rename("acme", "Login Flow", "login flow")
            .expect("case-only rename");

        assert_eq!(flow_files(&dir), vec!["login-flow.yml".to_string()]);
        assert_eq!(
            repo.list("acme").expect("list"),
            vec!["login flow".to_string()]
        );
        assert!(repo.get("acme", "Login Flow").is_err());
        assert!(repo.get("acme", "login flow").is_ok());
    }

    #[test]
    fn rename_that_only_changes_punctuation_rewrites_the_same_file() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("My Flow")).expect("save");

        repo.rename("acme", "My Flow", "My Flow!").expect("rename");

        assert_eq!(flow_files(&dir), vec!["my-flow.yml".to_string()]);
        assert_eq!(
            repo.list("acme").expect("list"),
            vec!["My Flow!".to_string()]
        );
    }

    #[test]
    fn rename_onto_an_existing_flow_is_a_conflict_and_changes_nothing() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save a");
        repo.save("acme", &sample("Other Flow")).expect("save b");

        let err = repo
            .rename("acme", "Login Flow", "Other Flow")
            .expect_err("target exists");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert_eq!(
            flow_files(&dir),
            vec!["login-flow.yml".to_string(), "other-flow.yml".to_string()]
        );
        assert!(repo.get("acme", "Login Flow").is_ok());
        assert!(repo.get("acme", "Other Flow").is_ok());
    }

    #[test]
    fn rename_onto_a_different_name_with_the_same_slug_is_a_conflict() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("My Flow")).expect("save a");
        repo.save("acme", &sample("Old")).expect("save b");

        // "my-flow" slugifies to the file that holds "My Flow".
        let err = repo
            .rename("acme", "Old", "my-flow")
            .expect_err("slug is taken");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert_eq!(
            flow_files(&dir),
            vec!["my-flow.yml".to_string(), "old.yml".to_string()]
        );
        assert_eq!(
            repo.get("acme", "My Flow").expect("get").name,
            "My Flow".to_string()
        );
    }

    #[test]
    fn rename_with_a_wrong_old_name_is_not_found_and_leaves_the_file_alone() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save");

        // Same slug, different name, so it is a different flow.
        let err = repo
            .rename("acme", "Login flow", "Sign In")
            .expect_err("wrong old name");
        assert!(matches!(err, DomainError::NotFound(_)));
        let err = repo
            .rename("acme", "No Such Flow", "Sign In")
            .expect_err("missing flow");
        assert!(matches!(err, DomainError::NotFound(_)));
        assert_eq!(flow_files(&dir), vec!["login-flow.yml".to_string()]);
        assert_eq!(
            repo.get("acme", "Login Flow").expect("get").name,
            "Login Flow".to_string()
        );
    }

    #[test]
    fn rename_to_a_name_with_an_empty_slug_is_rejected_and_keeps_the_old_file() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save");

        let err = repo
            .rename("acme", "Login Flow", "!!!")
            .expect_err("empty slug");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert_eq!(flow_files(&dir), vec!["login-flow.yml".to_string()]);
        assert!(repo.get("acme", "Login Flow").is_ok());
    }

    #[test]
    fn failed_removal_of_the_old_file_rolls_back_so_there_is_never_a_second_flow() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save");

        let err = repo
            .rename_with("acme", "Login Flow", "Sign In", &|_: &Path| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "locked",
                ))
            })
            .expect_err("removal fails");
        assert!(matches!(err, DomainError::Io(_)));

        // Only the original file remains, still under the old name, and no temp file is left.
        assert_eq!(flow_files(&dir), vec!["login-flow.yml".to_string()]);
        assert_eq!(
            repo.list("acme").expect("list"),
            vec!["Login Flow".to_string()]
        );
        assert!(matches!(
            repo.get("acme", "Sign In"),
            Err(DomainError::NotFound(_))
        ));
    }
}
