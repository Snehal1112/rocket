//! Callback endpoints for one Flow run. `run` owns a `RunCallbacks`, so
//! every endpoint closes when the run ends, on any path.

use std::collections::HashMap;

use rocket_flow::{Flow, FlowNodeKind, CALLBACK_VAR_PREFIX};
use rocket_shared::error::{DomainError, DomainResult};

use crate::callback_listener::{CallbackEndpoint, CallbackListener};

pub(crate) struct RunCallbacks {
    /// `callback.<name>` → endpoint URL, for every request in the run.
    vars: HashMap<String, String>,
    /// Open endpoints by Wait for callback node id.
    endpoints: HashMap<String, CallbackEndpoint>,
}

impl RunCallbacks {
    /// Opens one endpoint per Wait for callback node, in node order. A flow
    /// without such nodes opens nothing.
    pub(crate) async fn open_all(
        listener: &dyn CallbackListener,
        flow: &Flow,
    ) -> DomainResult<Self> {
        let mut vars = HashMap::new();
        let mut endpoints = HashMap::new();
        for node in &flow.nodes {
            let FlowNodeKind::WaitForCallback { name, .. } = &node.kind else {
                continue;
            };
            let endpoint = listener
                .open(flow.callback_host.as_deref())
                .await
                .map_err(|e| {
                    DomainError::Internal(format!("could not open callback listener: {e}"))
                })?;
            vars.insert(format!("{CALLBACK_VAR_PREFIX}{name}"), endpoint.url.clone());
            endpoints.insert(node.id.clone(), endpoint);
        }
        Ok(Self { vars, endpoints })
    }

    pub(crate) fn vars(&self) -> &HashMap<String, String> {
        &self.vars
    }

    pub(crate) fn endpoint_mut(&mut self, node_id: &str) -> Option<&mut CallbackEndpoint> {
        self.endpoints.get_mut(node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::FakeCallbackListener;
    use rocket_flow::{FlowNode, NodePosition};
    use std::sync::Arc;

    fn wait_node(id: &str, name: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::WaitForCallback {
                label: format!("Wait {id}"),
                name: name.to_string(),
                timeout_ms: 1000,
                accept_when: None,
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    #[tokio::test]
    async fn each_wait_node_owns_the_endpoint_its_variable_names() {
        let flow = Flow {
            name: "cb".to_string(),
            nodes: vec![wait_node("w1", "first"), wait_node("w2", "second")],
            edges: Vec::new(),
            callback_host: None,
        };
        let fake = FakeCallbackListener::new();
        let listener: Box<dyn CallbackListener> = Box::new(Arc::clone(&fake));

        let mut callbacks = RunCallbacks::open_all(listener.as_ref(), &flow)
            .await
            .expect("open");

        let first_url = callbacks.vars().get("callback.first").cloned();
        let endpoint = callbacks.endpoint_mut("w1").expect("w1 endpoint");
        assert_eq!(Some(endpoint.url.clone()), first_url);
        assert!(callbacks.endpoint_mut("unknown").is_none());
    }
}
