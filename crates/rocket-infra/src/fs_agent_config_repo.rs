//! Filesystem persistence for ACP agent configurations.
//! Stores the full `AgentConfig` list as one flat YAML file
//! (`agent_configs.yml`), mirroring `FsSecretManagerRepo`. Credential values
//! never appear here. `AgentConfig` only holds RocketVault references, and
//! the value is resolved on demand by `AgentConfigService`.

use std::fs;
use std::path::PathBuf;

use rocket_acp::{AgentConfig, AgentConfigRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

pub struct FsAgentConfigRepo {
    path: PathBuf,
}

impl FsAgentConfigRepo {
    /// `path` should point directly at the YAML file (e.g.
    /// `<app_data_dir>/agent_configs.yml`), not a containing directory.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read_all(&self) -> DomainResult<Vec<AgentConfig>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&self.path)
            .map_err(|e| DomainError::Io(format!("Failed to read agent_configs.yml: {e}")))?;
        if content.trim().is_empty() {
            // A zero-byte file (e.g. left behind by an interrupted first
            // write) is not an error. Treat it the same as a missing file.
            return Ok(Vec::new());
        }
        serde_yaml::from_str(&content).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to parse agent_configs.yml: {e}"))
        })
    }

    fn write_all(&self, configs: &[AgentConfig]) -> DomainResult<()> {
        let yaml = serde_yaml::to_string(configs).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to serialize agent_configs.yml: {e}"))
        })?;
        atomic_write(&self.path, yaml.as_bytes())
            .map_err(|e| DomainError::Io(format!("Failed to write agent_configs.yml: {e}")))
    }
}

impl AgentConfigRepository for FsAgentConfigRepo {
    fn list(&self) -> DomainResult<Vec<AgentConfig>> {
        self.read_all()
    }

    fn get(&self, id: &str) -> DomainResult<Option<AgentConfig>> {
        Ok(self.read_all()?.into_iter().find(|c| c.id == id))
    }

    fn save(&self, config: &AgentConfig) -> DomainResult<()> {
        let mut configs = self.read_all()?;
        configs.retain(|c| c.id != config.id);
        configs.push(config.clone());
        self.write_all(&configs)
    }

    fn delete(&self, id: &str) -> DomainResult<()> {
        let mut configs = self.read_all()?;
        configs.retain(|c| c.id != id);
        self.write_all(&configs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FsAgentConfigRepo) {
        let dir = TempDir::new().expect("create temp dir");
        let repo = FsAgentConfigRepo::new(dir.path().join("agent_configs.yml"));
        (dir, repo)
    }

    fn sample(id: &str) -> AgentConfig {
        AgentConfig {
            id: id.to_string(),
            label: "Claude Agent".to_string(),
            command: "claude-agent-acp".to_string(),
            args: vec!["--stdio".to_string()],
            working_dir: None,
            credential_env_var: "ANTHROPIC_API_KEY".to_string(),
            vault_connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            vault_secret_id: "b6f1c2e0-1234-4a5b-9abc-000000000001".to_string(),
            vault_secret_name: "anthropic-api-key".to_string(),
        }
    }

    #[test]
    fn list_on_missing_file_returns_empty() {
        let (_dir, repo) = setup();
        assert_eq!(repo.list().expect("list"), Vec::new());
    }

    #[test]
    fn list_on_zero_byte_file_returns_empty() {
        let (dir, repo) = setup();
        fs::write(dir.path().join("agent_configs.yml"), b"").expect("write empty file");
        assert_eq!(repo.list().expect("list"), Vec::new());
    }

    #[test]
    fn get_on_missing_file_returns_none() {
        let (_dir, repo) = setup();
        assert_eq!(repo.get("agent-1").expect("get"), None);
    }

    #[test]
    fn save_get_list_delete_roundtrip() {
        let (_dir, repo) = setup();
        let cfg = sample("agent-1");
        repo.save(&cfg).expect("save");

        assert_eq!(repo.get("agent-1").expect("get"), Some(cfg.clone()));
        assert_eq!(repo.list().expect("list"), vec![cfg.clone()]);

        repo.delete("agent-1").expect("delete");
        assert_eq!(repo.get("agent-1").expect("get after delete"), None);
        assert!(repo.list().expect("list after delete").is_empty());
    }

    #[test]
    fn save_replaces_existing_entry_with_same_id_instead_of_duplicating() {
        let (_dir, repo) = setup();
        repo.save(&sample("agent-1")).expect("save first");

        let mut updated = sample("agent-1");
        updated.label = "Renamed".to_string();
        repo.save(&updated).expect("save update");

        let all = repo.list().expect("list");
        assert_eq!(all.len(), 1, "same id must replace, not append");
        assert_eq!(all[0].label, "Renamed");
    }

    #[test]
    fn save_appends_distinct_ids() {
        let (_dir, repo) = setup();
        repo.save(&sample("agent-1")).expect("save agent-1");
        repo.save(&sample("agent-2")).expect("save agent-2");
        assert_eq!(repo.list().expect("list").len(), 2);
    }

    #[test]
    fn delete_of_missing_id_is_a_no_op() {
        let (_dir, repo) = setup();
        repo.save(&sample("agent-1")).expect("save");
        repo.delete("no-such-id")
            .expect("delete of missing id must not error");
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    #[test]
    fn malformed_yaml_errors_clearly_instead_of_panicking() {
        let (dir, repo) = setup();
        fs::write(
            dir.path().join("agent_configs.yml"),
            b"not: valid: agent: configs: [",
        )
        .expect("write malformed file");
        let err = repo
            .list()
            .expect_err("malformed YAML must error, not panic");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn save_on_malformed_yaml_errors_without_overwriting_the_file() {
        // A hand-edited, corrupt file must not be silently replaced by a
        // save. The user would otherwise lose every other entry in it.
        let (dir, repo) = setup();
        let path = dir.path().join("agent_configs.yml");
        let corrupt = b"not: valid: agent: configs: [";
        fs::write(&path, corrupt).expect("write malformed file");

        let err = repo
            .save(&sample("agent-1"))
            .expect_err("save over malformed YAML must error");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert_eq!(fs::read(&path).expect("read file back"), corrupt);
    }

    #[test]
    fn persisted_yaml_never_contains_a_raw_credential_field() {
        // AgentConfig (rocket-acp, Plan 01) has no field capable of holding a
        // raw credential value — only vault_secret_id/vault_secret_name,
        // which are references, not values. This is really a compile-time
        // guarantee since the struct has no such field to leak; this test is
        // a defensive regression guard so a future edit that added a
        // raw-value field to AgentConfig would be caught here, at the point
        // it would first reach disk in cleartext.
        let (dir, repo) = setup();
        repo.save(&sample("agent-1")).expect("save");

        let raw =
            fs::read_to_string(dir.path().join("agent_configs.yml")).expect("read persisted file");
        for forbidden in ["credential_value", "api_key_value", "secret_value"] {
            assert!(
                !raw.contains(forbidden),
                "agent_configs.yml must never contain a raw credential field: {raw}"
            );
        }

        // A name blocklist only catches the names it guesses. Also pin the
        // exact persisted key set, so any new field fails here and must be
        // reviewed for credential leakage before it can reach disk.
        let parsed: Vec<serde_yaml::Mapping> =
            serde_yaml::from_str(&raw).expect("parse persisted file");
        let mut keys: Vec<String> = parsed
            .first()
            .expect("one persisted entry")
            .keys()
            .map(|k| k.as_str().expect("string key").to_string())
            .collect();
        keys.sort();
        let mut expected = vec![
            "id",
            "label",
            "command",
            "args",
            "working_dir",
            "credential_env_var",
            "vault_connection_id",
            "vault_name",
            "vault_secret_id",
            "vault_secret_name",
        ];
        expected.sort();
        assert_eq!(
            keys, expected,
            "new AgentConfig field reached disk; confirm it cannot hold a credential value"
        );
    }
}
