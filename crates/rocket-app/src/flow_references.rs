//! Rewrites and looks up the Saved request paths used by a collection's flows.

use crate::flow_service::FlowService;
use rocket_collection::Collection;
use rocket_flow::{rewrite_saved_paths, uses_saved_path};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::FlowRewriteFailure;

const ALL_FLOWS: &str = "(all flows)";

/// The outcome of one rewrite pass over a collection's flows.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FlowRewriteReport {
    pub rewritten: Vec<String>,
    pub failed: Vec<FlowRewriteFailure>,
}

pub trait FlowReferenceUpdater: Send + Sync {
    /// Rewrites every Saved reference at or below `from` to `to` in the flows
    /// of `collection`. Never fails: problems are listed in the report.
    fn rewrite_request_paths(&self, collection: &str, from: &str, to: &str) -> FlowRewriteReport;

    /// Names the flows of `collection` that use `path` or something below it.
    fn flows_using(&self, collection: &str, path: &str) -> DomainResult<Vec<String>>;
}

fn validate_lookup(collection: &str, path: &str) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    if path.is_empty() {
        return Err(DomainError::InvalidInput("Path must not be empty".into()));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(DomainError::InvalidInput("Path must be relative".into()));
    }
    if path.split(['/', '\\']).any(|part| part == "..") {
        return Err(DomainError::InvalidInput(
            "Path must not contain '..'".into(),
        ));
    }
    Ok(())
}

impl FlowReferenceUpdater for FlowService {
    fn rewrite_request_paths(&self, collection: &str, from: &str, to: &str) -> FlowRewriteReport {
        let mut report = FlowRewriteReport::default();
        let repo = self.repo();
        let names = match repo.list(collection) {
            Ok(names) => names,
            Err(err) => {
                report.failed.push(FlowRewriteFailure {
                    flow: ALL_FLOWS.to_string(),
                    message: err.to_string(),
                });
                return report;
            }
        };
        for name in names {
            let mut flow = match repo.get(collection, &name) {
                Ok(flow) => flow,
                Err(err) => {
                    report.failed.push(FlowRewriteFailure {
                        flow: name,
                        message: err.to_string(),
                    });
                    continue;
                }
            };
            if rewrite_saved_paths(&mut flow, from, to) == 0 {
                continue;
            }
            match repo.save(collection, &flow) {
                Ok(()) => report.rewritten.push(name),
                Err(err) => report.failed.push(FlowRewriteFailure {
                    flow: name,
                    message: err.to_string(),
                }),
            }
        }
        report
    }

    fn flows_using(&self, collection: &str, path: &str) -> DomainResult<Vec<String>> {
        validate_lookup(collection, path)?;
        let repo = self.repo();
        let mut found = Vec::new();
        for name in repo.list(collection)? {
            if let Ok(flow) = repo.get(collection, &name) {
                if uses_saved_path(&flow, path) {
                    found.push(name);
                }
            }
        }
        found.sort();
        found.dedup();
        Ok(found)
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use rocket_flow::{Flow, FlowNode, FlowNodeKind, FlowRepository, NodePosition, RequestSource};
    use rocket_shared::error::{DomainError, DomainResult};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    pub(crate) struct ScriptedFlowRepo {
        pub flows: Mutex<Vec<(String, Flow)>>,
        pub fail_get: Mutex<Vec<String>>,
        pub fail_save: Mutex<Vec<String>>,
        pub fail_list: Mutex<bool>,
        pub saved: Mutex<Vec<String>>,
        pub list_calls: Mutex<usize>,
    }

    impl ScriptedFlowRepo {
        pub(crate) fn seeded(collection: &str, flows: Vec<Flow>) -> Arc<Self> {
            let repo = Self::default();
            *repo.flows.lock().expect("lock flows") = flows
                .into_iter()
                .map(|f| (collection.to_string(), f))
                .collect();
            Arc::new(repo)
        }

        pub(crate) fn flow(&self, name: &str) -> Option<Flow> {
            self.flows
                .lock()
                .expect("lock flows")
                .iter()
                .find(|(_, f)| f.name == name)
                .map(|(_, f)| f.clone())
        }
    }

    fn boom() -> DomainError {
        DomainError::Internal("boom".into())
    }

    impl FlowRepository for ScriptedFlowRepo {
        fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
            *self.list_calls.lock().expect("lock list_calls") += 1;
            if *self.fail_list.lock().expect("lock fail_list") {
                return Err(boom());
            }
            Ok(self
                .flows
                .lock()
                .expect("lock flows")
                .iter()
                .filter(|(c, _)| c == collection)
                .map(|(_, f)| f.name.clone())
                .collect())
        }

        fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
            if self
                .fail_get
                .lock()
                .expect("lock fail_get")
                .iter()
                .any(|n| n == name)
            {
                return Err(boom());
            }
            self.flows
                .lock()
                .expect("lock flows")
                .iter()
                .find(|(c, f)| c == collection && f.name == name)
                .map(|(_, f)| f.clone())
                .ok_or_else(|| DomainError::NotFound(name.to_string()))
        }

        fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
            if self
                .fail_save
                .lock()
                .expect("lock fail_save")
                .contains(&flow.name)
            {
                return Err(boom());
            }
            self.saved
                .lock()
                .expect("lock saved")
                .push(flow.name.clone());
            let mut guard = self.flows.lock().expect("lock flows");
            guard.retain(|(c, f)| !(c == collection && f.name == flow.name));
            guard.push((collection.to_string(), flow.clone()));
            Ok(())
        }

        fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
            self.flows
                .lock()
                .expect("lock flows")
                .retain(|(c, f)| !(c == collection && f.name == name));
            Ok(())
        }
    }

    /// A shareable handle the test keeps while the service owns a box of it.
    pub(crate) struct Shared(pub Arc<ScriptedFlowRepo>);

    impl FlowRepository for Shared {
        fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
            self.0.list(collection)
        }
        fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
            self.0.get(collection, name)
        }
        fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
            self.0.save(collection, flow)
        }
        fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
            self.0.delete(collection, name)
        }
    }

    pub(crate) fn flow_with(name: &str, paths: &[&str]) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: paths
                .iter()
                .enumerate()
                .map(|(i, p)| FlowNode {
                    id: format!("n{i}"),
                    kind: FlowNodeKind::Request {
                        label: format!("r{i}"),
                        source: RequestSource::Saved {
                            request_path: (*p).to_string(),
                        },
                        debug: false,
                        repeat_until: None,
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                })
                .collect(),
            edges: Vec::new(),
            callback_host: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{flow_with, ScriptedFlowRepo, Shared};
    use super::*;
    use rocket_flow::{FlowEdge, FlowNodeKind, RequestSource};
    use std::sync::Arc;

    fn service(repo: &Arc<ScriptedFlowRepo>) -> FlowService {
        FlowService::new(Box::new(Shared(Arc::clone(repo))))
    }

    fn saved_path(repo: &ScriptedFlowRepo, name: &str) -> String {
        let flow = repo.flow(name).expect("flow exists");
        match &flow.nodes[0].kind {
            FlowNodeKind::Request {
                source: RequestSource::Saved { request_path },
                ..
            } => request_path.clone(),
            _ => panic!("not a saved request"),
        }
    }

    #[test]
    fn rewrite_request_paths_rewrites_and_saves_matching_flows() {
        let repo = ScriptedFlowRepo::seeded(
            "c",
            vec![
                flow_with("A", &["auth/login.yml"]),
                flow_with("B", &["x.yml"]),
            ],
        );
        let report = service(&repo).rewrite_request_paths("c", "auth", "identity");
        assert_eq!(report.rewritten, vec!["A".to_string()]);
        assert!(report.failed.is_empty());
        assert_eq!(saved_path(&repo, "A"), "identity/login.yml");
    }

    #[test]
    fn rewrite_request_paths_skips_a_flow_that_does_not_match() {
        let repo = ScriptedFlowRepo::seeded(
            "c",
            vec![
                flow_with("A", &["auth/login.yml"]),
                flow_with("B", &["x.yml"]),
            ],
        );
        service(&repo).rewrite_request_paths("c", "auth", "identity");
        assert_eq!(*repo.saved.lock().expect("lock"), vec!["A".to_string()]);
    }

    #[test]
    fn rewrite_request_paths_reports_a_failed_save_and_continues() {
        let repo = ScriptedFlowRepo::seeded(
            "c",
            vec![
                flow_with("A", &["auth/a.yml"]),
                flow_with("C", &["auth/c.yml"]),
            ],
        );
        *repo.fail_save.lock().expect("lock") = vec!["A".to_string()];
        let report = service(&repo).rewrite_request_paths("c", "auth", "identity");
        assert_eq!(report.rewritten, vec!["C".to_string()]);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].flow, "A");
        assert!(report.failed[0].message.contains("boom"));
    }

    #[test]
    fn rewrite_request_paths_does_not_write_a_flow_it_could_not_read() {
        let repo = ScriptedFlowRepo::seeded(
            "c",
            vec![
                flow_with("A", &["auth/a.yml"]),
                flow_with("C", &["auth/c.yml"]),
            ],
        );
        *repo.fail_get.lock().expect("lock") = vec!["A".to_string()];
        let report = service(&repo).rewrite_request_paths("c", "auth", "identity");
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].flow, "A");
        assert!(!repo.saved.lock().expect("lock").contains(&"A".to_string()));
        assert_eq!(report.rewritten, vec!["C".to_string()]);
    }

    #[test]
    fn rewrite_request_paths_reports_a_list_error() {
        let repo = ScriptedFlowRepo::seeded("c", vec![flow_with("A", &["auth/a.yml"])]);
        *repo.fail_list.lock().expect("lock") = true;
        let report = service(&repo).rewrite_request_paths("c", "auth", "identity");
        assert!(report.rewritten.is_empty());
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].flow, "(all flows)");
    }

    #[test]
    fn rewrite_request_paths_saves_a_flow_that_would_fail_validation() {
        let mut flow = flow_with("A", &["auth/a.yml"]);
        flow.edges.push(FlowEdge {
            id: "e1".to_string(),
            source_node_id: "n0".to_string(),
            target_node_id: "ghost".to_string(),
            target_field: "value".to_string(),
            expression: "response.body".to_string(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        });
        let repo = ScriptedFlowRepo::seeded("c", vec![flow]);
        let report = service(&repo).rewrite_request_paths("c", "auth", "identity");
        assert_eq!(report.rewritten, vec!["A".to_string()]);
        assert_eq!(*repo.saved.lock().expect("lock"), vec!["A".to_string()]);
        assert_eq!(saved_path(&repo, "A"), "identity/a.yml");
    }

    #[test]
    fn flows_using_lists_each_flow_once_sorted() {
        let repo = ScriptedFlowRepo::seeded(
            "c",
            vec![
                flow_with("B", &["auth/b.yml"]),
                flow_with("A", &["auth/a.yml", "auth/x.yml"]),
                flow_with("D", &["other.yml"]),
            ],
        );
        let svc = service(&repo);
        assert_eq!(
            svc.flows_using("c", "auth").expect("lookup"),
            vec!["A".to_string(), "B".to_string()]
        );
        *repo.fail_list.lock().expect("lock") = true;
        assert!(svc.flows_using("c", "auth").is_err());
    }

    #[test]
    fn flows_using_rejects_bad_arguments() {
        let repo = ScriptedFlowRepo::seeded("c", vec![flow_with("A", &["a.yml"])]);
        let svc = service(&repo);
        for (collection, path) in [
            ("c", ""),
            ("c", "/abs.yml"),
            ("c", "../x.yml"),
            ("../c", "a.yml"),
        ] {
            let result = svc.flows_using(collection, path);
            assert!(
                matches!(result, Err(DomainError::InvalidInput(_))),
                "{collection:?} {path:?}"
            );
        }
        assert_eq!(*repo.list_calls.lock().expect("lock"), 0);
    }
}
