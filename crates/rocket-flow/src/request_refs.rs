//! Pure path helpers for Saved request references inside flows.

use crate::flow::Flow;
use crate::node::{FlowNodeKind, RequestSource};

fn clean_target(target: &str) -> &str {
    target.strip_suffix('/').unwrap_or(target)
}

/// True when `request_path` is `target` or lies below it, on whole segments.
/// A trailing `/` on `target` is ignored. An empty target never matches.
pub fn path_is_within(request_path: &str, target: &str) -> bool {
    let target = clean_target(target);
    if target.is_empty() {
        return false;
    }
    match request_path.strip_prefix(target) {
        Some("") => true,
        Some(rest) => rest.starts_with('/'),
        None => false,
    }
}

/// Replaces the `from` prefix with `to` on every Saved request node.
/// Returns how many nodes changed.
pub fn rewrite_saved_paths(flow: &mut Flow, from: &str, to: &str) -> usize {
    let from = clean_target(from);
    let to = clean_target(to);
    let mut changed = 0;
    for node in &mut flow.nodes {
        if let FlowNodeKind::Request {
            source: RequestSource::Saved { request_path },
            ..
        } = &mut node.kind
        {
            if path_is_within(request_path, from) {
                let rest = &request_path[from.len()..];
                *request_path = format!("{to}{rest}");
                changed += 1;
            }
        }
    }
    changed
}

/// True when any Saved request node of `flow` is at or below `target`.
pub fn uses_saved_path(flow: &Flow, target: &str) -> bool {
    flow.nodes.iter().any(|node| {
        matches!(
            &node.kind,
            FlowNodeKind::Request {
                source: RequestSource::Saved { request_path },
                ..
            } if path_is_within(request_path, target)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowNode;
    use crate::node::{InlineRequestData, NodePosition};

    fn flow_with(paths: &[&str]) -> Flow {
        Flow {
            name: "f".to_string(),
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
                        allow_wired_host: false,
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                })
                .collect(),
            edges: Vec::new(),
            callback_host: None,
        }
    }

    fn saved_paths(flow: &Flow) -> Vec<String> {
        flow.nodes
            .iter()
            .filter_map(|n| match &n.kind {
                FlowNodeKind::Request {
                    source: RequestSource::Saved { request_path },
                    ..
                } => Some(request_path.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn path_is_within_matches_exact_and_folder_prefix() {
        assert!(path_is_within("auth/login.yml", "auth/login.yml"));
        assert!(path_is_within("auth/login.yml", "auth"));
        assert!(path_is_within("auth/login.yml", "auth/"));
        assert!(path_is_within("auth", "auth"));
    }

    #[test]
    fn path_is_within_does_not_match_a_longer_sibling_name() {
        assert!(!path_is_within("author/x.yml", "auth"));
    }

    #[test]
    fn path_is_within_never_matches_an_empty_target() {
        assert!(!path_is_within("a.yml", ""));
        assert!(!path_is_within("a.yml", "/"));
        assert!(!path_is_within("", ""));
    }

    #[test]
    fn rewrite_saved_paths_replaces_a_folder_prefix() {
        let mut f = flow_with(&["auth/login.yml", "auth/sub/me.yml", "other/x.yml"]);
        assert_eq!(rewrite_saved_paths(&mut f, "auth", "identity"), 2);
        assert_eq!(
            saved_paths(&f),
            ["identity/login.yml", "identity/sub/me.yml", "other/x.yml"]
        );
    }

    #[test]
    fn rewrite_saved_paths_replaces_an_exact_request_path() {
        let mut f = flow_with(&["a.yml"]);
        assert_eq!(rewrite_saved_paths(&mut f, "a.yml", "b/a.yml"), 1);
        assert_eq!(saved_paths(&f), ["b/a.yml"]);
    }

    #[test]
    fn rewrite_saved_paths_ignores_inline_requests() {
        let mut f = flow_with(&[]);
        f.nodes.push(FlowNode {
            id: "i".to_string(),
            kind: FlowNodeKind::Request {
                label: "inline".to_string(),
                source: RequestSource::Inline {
                    request: InlineRequestData {
                        method: "GET".to_string(),
                        url: "https://a.example".to_string(),
                        headers: Vec::new(),
                        body: None,
                    },
                },
                debug: false,
                repeat_until: None,
                allow_wired_host: false,
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        });
        f.nodes.push(FlowNode {
            id: "o".to_string(),
            kind: FlowNodeKind::Output {
                label: "out".to_string(),
            },
            position: NodePosition { x: 1.0, y: 0.0 },
        });
        let before = f.clone();
        assert_eq!(rewrite_saved_paths(&mut f, "a", "b"), 0);
        assert_eq!(f, before);
    }

    #[test]
    fn uses_saved_path_is_true_for_a_nested_request() {
        let f = flow_with(&["auth/login.yml"]);
        assert!(uses_saved_path(&f, "auth"));
        assert!(uses_saved_path(&f, "auth/login.yml"));
        assert!(!uses_saved_path(&f, "other"));
    }
}
