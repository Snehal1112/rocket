use std::fs;
use std::path::{Path, PathBuf};

#[allow(unused_imports)]
use rocket_flow::{Flow, FlowEdge, FlowNode, FlowNodeKind, FlowRepository, NodePosition};
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

    fn flows_dir(&self, collection: &str) -> PathBuf {
        self.base_dir.join(collection).join("flows")
    }

    fn file_path(&self, collection: &str, name: &str) -> PathBuf {
        self.flows_dir(collection).join(format!("{}.yml", slugify(name)))
    }

    fn read_flow(&self, path: &Path, not_found_label: &str) -> DomainResult<Flow> {
        if !path.exists() {
            return Err(DomainError::NotFound(not_found_label.to_string()));
        }
        let content = fs::read_to_string(path)
            .map_err(|e| DomainError::Io(format!("Failed to read flow file: {e}")))?;
        serde_yaml::from_str(&content)
            .map_err(|e| DomainError::InvalidInput(format!("Failed to parse flow YAML: {e}")))
    }
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
        let dir = self.flows_dir(collection);
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
            let content = fs::read_to_string(&path).map_err(|e| DomainError::Io(e.to_string()))?;
            let flow: Flow = serde_yaml::from_str(&content).map_err(|e| {
                DomainError::InvalidInput(format!("Failed to parse flow YAML: {e}"))
            })?;
            names.push(flow.name);
        }
        names.sort();
        Ok(names)
    }

    fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
        let path = self.file_path(collection, name);
        self.read_flow(&path, &format!("Flow '{name}' in collection '{collection}'"))
    }

    fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
        let dir = self.flows_dir(collection);
        fs::create_dir_all(&dir).map_err(|e| DomainError::Io(e.to_string()))?;
        let yaml = serde_yaml::to_string(flow)
            .map_err(|e| DomainError::InvalidInput(format!("Failed to serialize flow: {e}")))?;
        atomic_write(&self.file_path(collection, &flow.name), yaml.as_bytes())
            .map_err(|e| DomainError::Io(format!("Failed to write flow file: {e}")))
    }

    fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
        let path = self.file_path(collection, name);
        if !path.exists() {
            return Err(DomainError::NotFound(format!(
                "Flow '{name}' in collection '{collection}'"
            )));
        }
        fs::remove_file(&path).map_err(|e| DomainError::Io(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let err = repo.get("acme", "no-such-flow").expect_err("must not find a flow that was never saved");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn save_get_list_roundtrip() {
        let (_dir, repo) = setup();
        let flow = sample("Login Flow");
        repo.save("acme", &flow).expect("save");

        let loaded = repo.get("acme", "Login Flow").expect("get");
        assert_eq!(loaded, flow);
        assert_eq!(repo.list("acme").expect("list"), vec!["Login Flow".to_string()]);
    }

    #[test]
    fn save_under_same_name_replaces_not_duplicates() {
        let (_dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save first");

        let mut updated = sample("Login Flow");
        updated.nodes.push(FlowNode {
            id: "node-2".to_string(),
            kind: FlowNodeKind::Output { label: "Result".to_string() },
            position: NodePosition { x: 400.0, y: 200.0 },
        });
        repo.save("acme", &updated).expect("save update");

        let names = repo.list("acme").expect("list");
        assert_eq!(names.len(), 1, "same flow name must replace, not append a second file");
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
            dir.path().join("acme").join("flows").join("my-first-flow.yml").exists(),
            "expected slugified filename my-first-flow.yml"
        );
        let loaded = repo.get("acme", "My First Flow!").expect("get by original name");
        assert_eq!(loaded.name, "My First Flow!");
    }

    #[test]
    fn malformed_yaml_errors_clearly_instead_of_panicking() {
        let (dir, repo) = setup();
        let flows_dir = dir.path().join("acme").join("flows");
        fs::create_dir_all(&flows_dir).expect("create flows dir");
        fs::write(flows_dir.join("broken.yml"), b"not: valid: yaml: [").expect("write malformed file");

        let err = repo.get("acme", "broken").expect_err("malformed YAML must error, not panic");
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
        });
        repo.save("acme", &flow).expect("save");
        let raw = fs::read_to_string(dir.path().join("acme").join("flows").join("login-flow.yml"))
            .expect("read saved flow file");
        assert!(raw.contains("source_node_id"), "expected snake_case field, got:\n{raw}");
        assert!(!raw.contains("sourceNodeId"), "must not contain camelCase, got:\n{raw}");
    }
}
