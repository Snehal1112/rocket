use rocket_flow::{Flow, FlowRepository};
use rocket_shared::error::DomainResult;

pub struct FlowService {
    flow_repo: Box<dyn FlowRepository>,
}

impl FlowService {
    pub fn new(flow_repo: Box<dyn FlowRepository>) -> Self {
        Self { flow_repo }
    }

    pub fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
        self.flow_repo.list(collection)
    }

    pub fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
        self.flow_repo.get(collection, name)
    }

    pub fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
        self.flow_repo.delete(collection, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_flow::{FlowNode, FlowNodeKind, NodePosition};
    use std::sync::Mutex;

    fn sample_flow() -> Flow {
        Flow {
            name: "Login Then Fetch".to_string(),
            nodes: vec![FlowNode {
                id: "n1".to_string(),
                kind: FlowNodeKind::Output {
                    label: "Result".to_string(),
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: vec![],
        }
    }

    struct FakeFlowRepo {
        flows: Mutex<Vec<(String, Flow)>>, // (collection, flow)
    }
    impl FakeFlowRepo {
        fn new() -> Self {
            Self {
                flows: Mutex::new(Vec::new()),
            }
        }
        fn seeded(collection: &str, flow: Flow) -> Self {
            let repo = Self::new();
            repo.flows
                .lock()
                .expect("lock FakeFlowRepo")
                .push((collection.to_string(), flow));
            repo
        }
    }
    impl FlowRepository for FakeFlowRepo {
        fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
            Ok(self
                .flows
                .lock()
                .expect("lock FakeFlowRepo")
                .iter()
                .filter(|(c, _)| c == collection)
                .map(|(_, f)| f.name.clone())
                .collect())
        }
        fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
            self.flows
                .lock()
                .expect("lock FakeFlowRepo")
                .iter()
                .find(|(c, f)| c == collection && f.name == name)
                .map(|(_, f)| f.clone())
                .ok_or_else(|| {
                    rocket_shared::error::DomainError::NotFound(format!(
                        "flow '{name}' not found in collection '{collection}'"
                    ))
                })
        }
        fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
            let mut guard = self.flows.lock().expect("lock FakeFlowRepo");
            guard.retain(|(c, f)| !(c == collection && f.name == flow.name));
            guard.push((collection.to_string(), flow.clone()));
            Ok(())
        }
        fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
            self.flows
                .lock()
                .expect("lock FakeFlowRepo")
                .retain(|(c, f)| !(c == collection && f.name == name));
            Ok(())
        }
    }

    #[test]
    fn list_returns_flow_names_for_collection() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::seeded("demo", sample_flow())));
        assert_eq!(
            svc.list("demo").expect("list"),
            vec!["Login Then Fetch".to_string()]
        );
        assert_eq!(svc.list("other").expect("list other"), Vec::<String>::new());
    }

    #[test]
    fn get_returns_not_found_for_missing_flow() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::new()));
        let err = svc.get("demo", "missing").expect_err("expected NotFound");
        assert!(matches!(err, rocket_shared::error::DomainError::NotFound(_)));
    }

    #[test]
    fn delete_removes_flow() {
        let repo = FakeFlowRepo::seeded("demo", sample_flow());
        let svc = FlowService::new(Box::new(repo));
        svc.delete("demo", "Login Then Fetch").expect("delete");
        assert!(svc.get("demo", "Login Then Fetch").is_err());
    }
}
