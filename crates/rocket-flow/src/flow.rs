use crate::node::{FlowNodeKind, NodePosition};
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

/// A placed node. `id` is the only identity key within a `Flow`'s `nodes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowNode {
    pub id: String,
    pub kind: FlowNodeKind,
    pub position: NodePosition,
}

/// A wire from one node's captured output into one field of another node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowEdge {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    /// Path into the target node's own field set, e.g. "url",
    /// "headers[1].value", "body". Not a fixed enum, so new wireable fields
    /// on a node type don't require a schema change here.
    pub target_field: String,
    /// JS expression evaluated against the source node's captured output
    /// (Plan 05). This crate only carries it as data and never evaluates it.
    pub expression: String,
    /// Which exit of the source node this edge leaves from, e.g. "result",
    /// "true" or "case:<id>". Phase 1 files have no such key, so it defaults
    /// to "result", and it is left out when "result" so those files re-save
    /// without a diff.
    #[serde(
        default = "default_source_handle",
        skip_serializing_if = "is_result_handle"
    )]
    pub source_handle: String,
}

fn default_source_handle() -> String {
    crate::handle::RESULT.to_string()
}

fn is_result_handle(handle: &str) -> bool {
    handle == crate::handle::RESULT
}

/// The Flow aggregate. `name` is its identity within a collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub name: String,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    /// Host used in callback URLs. `None` means this machine's LAN IP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_host: Option<String>,
}

/// Persistence boundary for `Flow`. No I/O in this crate —
/// `rocket-infra`'s `FsFlowRepo` (Plan 03) implements this. Flows are keyed
/// by `(collection, flow.name)`; `save` replaces any flow with the same key.
pub trait FlowRepository: Send + Sync {
    /// Returns the names of all flows in `collection`.
    fn list(&self, collection: &str) -> DomainResult<Vec<String>>;
    fn get(&self, collection: &str, name: &str) -> DomainResult<Flow>;
    fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()>;
    fn delete(&self, collection: &str, name: &str) -> DomainResult<()>;

    /// Renames a flow. The default body (get, check the target, save under the
    /// new name, delete the old one) suits name-keyed stores. File-backed
    /// stores whose keys are derived from the name must override it, because
    /// two names can map to one key.
    fn rename(&self, collection: &str, old_name: &str, new_name: &str) -> DomainResult<()> {
        let mut flow = self.get(collection, old_name)?;
        if self.get(collection, new_name).is_ok() {
            return Err(DomainError::Conflict(format!(
                "Flow '{new_name}' already exists in collection '{collection}'"
            )));
        }
        flow.name = new_name.to_string();
        self.save(collection, &flow)?;
        self.delete(collection, old_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::RequestSource;
    use std::sync::Mutex;

    fn sample_flow() -> Flow {
        Flow {
            name: "Login then fetch profile".to_string(),
            nodes: vec![
                FlowNode {
                    id: "node-1".to_string(),
                    kind: FlowNodeKind::Request {
                        debug: false,
                        repeat_until: None,
                        label: "Login".to_string(),
                        source: RequestSource::Saved {
                            request_path: "auth/login.yml".to_string(),
                        },
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                FlowNode {
                    id: "node-2".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "Token".to_string(),
                    },
                    position: NodePosition { x: 200.0, y: 0.0 },
                },
            ],
            edges: vec![FlowEdge {
                id: "edge-1".to_string(),
                source_node_id: "node-1".to_string(),
                target_node_id: "node-2".to_string(),
                target_field: "value".to_string(),
                expression: "response.body.token".to_string(),
                source_handle: crate::handle::RESULT.to_string(),
            }],
            callback_host: None,
        }
    }

    struct FakeRepo(Mutex<Vec<(String, Flow)>>); // (collection, flow)
    impl FakeRepo {
        fn new() -> Self {
            Self(Mutex::new(Vec::new()))
        }
    }
    impl FlowRepository for FakeRepo {
        fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeRepo")
                .iter()
                .filter(|(c, _)| c == collection)
                .map(|(_, f)| f.name.clone())
                .collect())
        }
        fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
            self.0
                .lock()
                .expect("lock FakeRepo")
                .iter()
                .find(|(c, f)| c == collection && f.name == name)
                .map(|(_, f)| f.clone())
                .ok_or_else(|| rocket_shared::error::DomainError::NotFound(name.to_string()))
        }
        fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeRepo");
            guard.retain(|(c, f)| !(c == collection && f.name == flow.name));
            guard.push((collection.to_string(), flow.clone()));
            Ok(())
        }
        fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock FakeRepo")
                .retain(|(c, f)| !(c == collection && f.name == name));
            Ok(())
        }
    }

    #[test]
    fn repository_trait_save_get_delete_roundtrip() {
        let repo = FakeRepo::new();
        let flow = sample_flow();
        repo.save("my-collection", &flow).expect("save flow");
        let fetched = repo.get("my-collection", &flow.name).expect("get flow");
        assert_eq!(fetched, flow);
        assert_eq!(
            repo.list("my-collection").expect("list flows"),
            vec![flow.name.clone()]
        );
        repo.delete("my-collection", &flow.name)
            .expect("delete flow");
        assert!(repo.get("my-collection", &flow.name).is_err());
    }

    #[test]
    fn repository_save_replaces_existing_entry_with_same_name_instead_of_duplicating() {
        let repo = FakeRepo::new();
        let mut flow = sample_flow();
        repo.save("my-collection", &flow).expect("save first");
        flow.nodes[0].position = NodePosition { x: 500.0, y: 500.0 };
        repo.save("my-collection", &flow).expect("save update");
        let all = repo.list("my-collection").expect("list");
        assert_eq!(all.len(), 1, "same name must replace, not append");
        let fetched = repo.get("my-collection", &flow.name).expect("get updated");
        assert_eq!(
            fetched.nodes[0].position,
            NodePosition { x: 500.0, y: 500.0 }
        );
    }

    #[test]
    fn repository_keeps_different_names_and_collections_separate() {
        let repo = FakeRepo::new();
        let first = sample_flow();
        let mut second = sample_flow();
        second.name = "Another flow".to_string();
        second.nodes.truncate(1);
        repo.save("my-collection", &first).expect("save first");
        repo.save("my-collection", &second).expect("save second");
        repo.save("other-collection", &first)
            .expect("save in other collection");

        let mut names = repo.list("my-collection").expect("list");
        names.sort();
        assert_eq!(names, vec![second.name.clone(), first.name.clone()]);
        assert_eq!(
            repo.get("my-collection", &first.name).expect("get first"),
            first
        );
        assert_eq!(
            repo.get("my-collection", &second.name).expect("get second"),
            second
        );

        repo.delete("other-collection", &first.name)
            .expect("delete other");
        assert_eq!(
            repo.get("my-collection", &first.name)
                .expect("still present"),
            first
        );
    }

    #[test]
    fn two_nodes_with_same_kind_and_different_ids_are_not_conflated() {
        let repo = FakeRepo::new();
        let mut flow = sample_flow();
        flow.nodes.push(FlowNode {
            id: "node-3".to_string(),
            kind: FlowNodeKind::Output {
                label: "Token".to_string(),
            },
            position: NodePosition { x: 200.0, y: 100.0 },
        });
        repo.save("my-collection", &flow).expect("save flow");
        let fetched = repo.get("my-collection", &flow.name).expect("get flow");
        assert_eq!(fetched.nodes.len(), 3);
        let by_id = |id: &str| {
            fetched
                .nodes
                .iter()
                .find(|n| n.id == id)
                .expect("node exists")
        };
        assert!(matches!(by_id("node-1").kind, FlowNodeKind::Request { .. }));
        assert_eq!(by_id("node-2").position, NodePosition { x: 200.0, y: 0.0 });
        assert_eq!(
            by_id("node-3").position,
            NodePosition { x: 200.0, y: 100.0 }
        );
    }

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn FlowRepository>) {}
    }

    fn plain_edge_json() -> &'static str {
        r#"{"id":"e1","source_node_id":"a","target_node_id":"b","target_field":"url","expression":"response.body"}"#
    }

    #[test]
    fn edge_without_source_handle_defaults_to_result() {
        let edge: FlowEdge = serde_json::from_str(plain_edge_json()).expect("deserialize edge");
        assert_eq!(edge.source_handle, crate::handle::RESULT);
    }

    #[test]
    fn result_source_handle_is_omitted_on_serialize() {
        let edge: FlowEdge = serde_json::from_str(plain_edge_json()).expect("deserialize edge");
        let json = serde_json::to_string(&edge).expect("serialize edge");
        assert!(!json.contains("source_handle"), "got: {json}");
    }

    #[test]
    fn non_result_source_handle_is_serialized() {
        let mut edge: FlowEdge = serde_json::from_str(plain_edge_json()).expect("deserialize edge");
        edge.source_handle = crate::handle::TRUE.to_string();
        let json = serde_json::to_string(&edge).expect("serialize edge");
        assert!(json.contains(r#""source_handle":"true""#), "got: {json}");
        let back: FlowEdge = serde_json::from_str(&json).expect("deserialize edge");
        assert_eq!(back, edge);
    }

    /// Mirrors the Phase 1 on-disk edge shape, which had no `source_handle`.
    #[derive(Serialize)]
    struct Phase1Edge {
        id: String,
        source_node_id: String,
        target_node_id: String,
        target_field: String,
        expression: String,
    }

    /// Mirrors the Phase 1 on-disk flow shape.
    #[derive(Serialize)]
    struct Phase1Flow {
        name: String,
        nodes: Vec<FlowNode>,
        edges: Vec<Phase1Edge>,
    }

    #[test]
    fn phase1_yaml_reserializes_byte_identically() {
        let phase1 = Phase1Flow {
            name: "Login then fetch profile".to_string(),
            nodes: sample_flow().nodes,
            edges: vec![Phase1Edge {
                id: "edge-1".to_string(),
                source_node_id: "node-1".to_string(),
                target_node_id: "node-2".to_string(),
                target_field: "value".to_string(),
                expression: "response.body.token".to_string(),
            }],
        };
        let phase1_yaml = serde_yaml::to_string(&phase1).expect("serialize Phase 1 flow");

        let loaded: Flow = serde_yaml::from_str(&phase1_yaml).expect("load Phase 1 flow");
        assert_eq!(loaded.edges[0].source_handle, crate::handle::RESULT);

        let resaved = serde_yaml::to_string(&loaded).expect("re-serialize flow");
        assert_eq!(
            resaved, phase1_yaml,
            "a Phase 1 file must re-save without any diff"
        );
    }

    fn single_request_flow(debug: bool) -> Flow {
        Flow {
            name: "Debug flow".to_string(),
            nodes: vec![FlowNode {
                id: "n1".to_string(),
                kind: FlowNodeKind::Request {
                    label: "Login".to_string(),
                    source: RequestSource::Saved {
                        request_path: "auth/login.yml".to_string(),
                    },
                    debug,
                    repeat_until: None,
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: vec![],
            callback_host: None,
        }
    }

    #[test]
    fn a_request_node_without_debug_round_trips_without_a_debug_key() {
        let yaml = serde_yaml::to_string(&single_request_flow(false)).expect("serialize");
        assert!(!yaml.contains("debug:"), "got {yaml}");
        let flow: Flow = serde_yaml::from_str(&yaml).expect("old yaml loads");
        let FlowNodeKind::Request { debug, .. } = &flow.nodes[0].kind else {
            panic!("request")
        };
        assert!(!debug);
        let out = serde_yaml::to_string(&flow).expect("serialize");
        assert_eq!(out, yaml);
    }

    #[test]
    fn a_debug_request_node_round_trips() {
        let yaml = serde_yaml::to_string(&single_request_flow(true)).expect("serialize");
        assert!(yaml.contains("debug: true"), "got {yaml}");
        let flow: Flow = serde_yaml::from_str(&yaml).expect("loads");
        let out = serde_yaml::to_string(&flow).expect("serialize");
        assert!(out.contains("debug: true"), "got {out}");
    }

    #[test]
    fn flow_without_callback_host_saves_without_the_key() {
        let flow = Flow {
            name: "f".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            callback_host: None,
        };
        let yaml = serde_yaml::to_string(&flow).expect("serialize");
        assert!(!yaml.contains("callback_host"), "got:\n{yaml}");
        let old_file = "name: f\nnodes: []\nedges: []\n";
        let loaded: Flow = serde_yaml::from_str(old_file).expect("an old file still loads");
        assert_eq!(loaded.callback_host, None);
    }

    #[test]
    fn flow_with_callback_host_roundtrips() {
        let flow = Flow {
            name: "f".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            callback_host: Some("host.docker.internal".to_string()),
        };
        let yaml = serde_yaml::to_string(&flow).expect("serialize");
        assert!(
            yaml.contains("callback_host: host.docker.internal"),
            "got:\n{yaml}"
        );
        let back: Flow = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(back, flow);
    }

    #[test]
    fn default_rename_moves_the_flow_to_the_new_name() {
        let repo = FakeRepo::new();
        let flow = sample_flow();
        repo.save("my-collection", &flow).expect("save");
        repo.rename("my-collection", &flow.name, "Renamed")
            .expect("rename");
        assert!(repo.get("my-collection", &flow.name).is_err());
        let renamed = repo.get("my-collection", "Renamed").expect("get renamed");
        assert_eq!(renamed.name, "Renamed");
        assert_eq!(renamed.nodes, flow.nodes);
        assert_eq!(repo.list("my-collection").expect("list").len(), 1);
    }

    #[test]
    fn default_rename_onto_an_existing_flow_is_a_conflict() {
        let repo = FakeRepo::new();
        let flow = sample_flow();
        repo.save("my-collection", &flow).expect("save first");
        let mut other = sample_flow();
        other.name = "Other".to_string();
        repo.save("my-collection", &other).expect("save other");
        let err = repo
            .rename("my-collection", &flow.name, "Other")
            .expect_err("target exists");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::Conflict(_)
        ));
        assert_eq!(repo.list("my-collection").expect("list").len(), 2);
    }

    #[test]
    fn default_rename_of_a_missing_flow_is_not_found() {
        let repo = FakeRepo::new();
        let err = repo
            .rename("my-collection", "nope", "Renamed")
            .expect_err("missing");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::NotFound(_)
        ));
    }
}
