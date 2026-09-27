use crate::node::{FlowNodeKind, NodePosition};
use rocket_shared::error::DomainResult;
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
}

/// The Flow aggregate. `name` is its identity within a collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub name: String,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
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
            }],
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
}
