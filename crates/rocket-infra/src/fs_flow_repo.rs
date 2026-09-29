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
}
