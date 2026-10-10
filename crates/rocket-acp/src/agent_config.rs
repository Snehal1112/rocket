use rocket_shared::error::DomainResult;
use serde::{Deserialize, Serialize};
use std::fmt;

/// The environment variable Rocket's session isolation sets. An agent config
/// may not claim it.
pub const RESERVED_ENV_VAR: &str = "CLAUDE_CONFIG_DIR";

/// Longest accepted environment variable name, in bytes.
pub const MAX_ENV_VAR_NAME_LEN: usize = 128;

/// One plain (non-secret) environment variable passed to the agent process.
/// `Debug` prints the name only, so a value never reaches a log by accident.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvPair {
    pub name: String,
    pub value: String,
}

impl fmt::Debug for EnvPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnvPair")
            .field("name", &self.name)
            .field("value", &"***")
            .finish()
    }
}

/// One registered ACP agent binary/command and where to find its API key.
/// The credential *value* never lives on this struct — only a reference to
/// where RocketVault holds it (`vault_connection_id`/`vault_name`/
/// `vault_secret_id`), resolved on demand by `AgentConfigService`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    pub credential_env_var: String,
    /// Extra plain environment variables, in the order the user gave them.
    /// Old files without this field load as an empty list.
    #[serde(default)]
    pub extra_env: Vec<EnvPair>,
    pub vault_connection_id: String,
    pub vault_name: String,
    pub vault_secret_id: String,
    pub vault_secret_name: String,
}

/// Checks one environment variable name: letters, digits and underscore,
/// not starting with a digit, 1 to 128 bytes, and not `CLAUDE_CONFIG_DIR`.
/// The error is a short reason and never contains a value.
pub fn validate_env_var_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("must not be empty");
    }
    if name.len() > MAX_ENV_VAR_NAME_LEN {
        return Err("is too long (at most 128 characters)");
    }
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Err("must not start with a digit");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("may contain only letters, digits and underscores");
    }
    // Windows compares names without regard to case, so Rocket does too.
    if name.eq_ignore_ascii_case(RESERVED_ENV_VAR) {
        return Err("is reserved by Rocket");
    }
    Ok(())
}

/// Checks the credential variable name and every extra variable name. Also
/// refuses an extra name equal to the credential name and duplicate extra
/// names. The message names the field and never echoes a value.
pub fn validate_env_config(credential_env_var: &str, extra_env: &[EnvPair]) -> Result<(), String> {
    validate_env_var_name(credential_env_var).map_err(|why| format!("credential_env_var {why}"))?;
    for (i, pair) in extra_env.iter().enumerate() {
        let n = i + 1;
        validate_env_var_name(&pair.name).map_err(|why| format!("extra_env #{n} name {why}"))?;
        if pair.name.eq_ignore_ascii_case(credential_env_var) {
            return Err(format!(
                "extra_env #{n} name must differ from credential_env_var"
            ));
        }
        if extra_env[..i]
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&pair.name))
        {
            return Err(format!("extra_env #{n} name is a duplicate"));
        }
    }
    Ok(())
}

/// Persistence boundary for `AgentConfig`. No I/O in this crate —
/// `rocket-infra`'s `FsAgentConfigRepo` (Plan 02) implements this.
///
/// `id` is the only identity key. Two configs that share a `label` but have
/// different ids are distinct entries.
pub trait AgentConfigRepository: Send + Sync {
    /// Returns every stored config. An empty store yields an empty list.
    fn list(&self) -> DomainResult<Vec<AgentConfig>>;
    /// Returns the config with this id, or `None` if it does not exist.
    fn get(&self, id: &str) -> DomainResult<Option<AgentConfig>>;
    /// Inserts the config, or replaces the existing entry with the same id.
    fn save(&self, config: &AgentConfig) -> DomainResult<()>;
    /// Removes the config with this id. Deleting an unknown id is a no-op.
    fn delete(&self, id: &str) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn sample() -> AgentConfig {
        AgentConfig {
            id: "agent-1".to_string(),
            label: "Claude Agent".to_string(),
            command: "claude-agent-acp".to_string(),
            args: vec!["--stdio".to_string()],
            working_dir: None,
            credential_env_var: "ANTHROPIC_API_KEY".to_string(),
            extra_env: Vec::new(),
            vault_connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            vault_secret_id: "b6f1c2e0-1234-4a5b-9abc-000000000001".to_string(),
            vault_secret_name: "anthropic-api-key".to_string(),
        }
    }

    #[test]
    fn agent_config_serde_roundtrip_no_camelcase() {
        let c = sample();
        let json = serde_json::to_string(&c).expect("serialize AgentConfig");
        assert!(
            json.contains("\"credential_env_var\""),
            "expected snake_case field, got: {json}"
        );
        assert!(
            json.contains("\"vault_connection_id\""),
            "expected snake_case field, got: {json}"
        );
        let back: AgentConfig = serde_json::from_str(&json).expect("deserialize AgentConfig");
        assert_eq!(c, back);
    }

    #[test]
    fn agent_config_args_roundtrips_when_empty() {
        let mut c = sample();
        c.args = Vec::new();
        let json = serde_json::to_string(&c).expect("serialize AgentConfig");
        assert!(
            json.contains("\"args\":[]"),
            "empty args must still be serialized, got: {json}"
        );
        let back: AgentConfig = serde_json::from_str(&json).expect("deserialize AgentConfig");
        assert!(back.args.is_empty());
        assert_eq!(c, back);
    }

    #[test]
    fn agent_config_working_dir_defaults_to_none_when_absent() {
        let json = r#"{
            "id":"agent-1","label":"Claude Agent","command":"claude-agent-acp",
            "credential_env_var":"ANTHROPIC_API_KEY","vault_connection_id":"conn-1",
            "vault_name":"prod-vault","vault_secret_id":"b6f1c2e0-1234-4a5b-9abc-000000000001",
            "vault_secret_name":"anthropic-api-key"
        }"#;
        let c: AgentConfig = serde_json::from_str(json).expect("deserialize minimal AgentConfig");
        assert_eq!(c.working_dir, None);
        assert!(c.args.is_empty());
    }

    fn pair(name: &str, value: &str) -> EnvPair {
        EnvPair {
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn env_var_name_accepts_valid_names() {
        for name in ["A", "_x", "ANTHROPIC_API_KEY", "a1_B2"] {
            assert!(validate_env_var_name(name).is_ok(), "{name}");
        }
        assert!(validate_env_var_name(&"A".repeat(128)).is_ok());
    }

    #[test]
    fn env_var_name_refuses_bad_names() {
        for name in [
            "",
            "1ABC",
            "A=B",
            "A B",
            " A",
            "A\tB",
            "A-B",
            "CLAUDE_CONFIG_DIR",
            "\u{e9}t\u{e9}",
        ] {
            assert!(validate_env_var_name(name).is_err(), "{name:?}");
        }
        assert!(validate_env_var_name(&"A".repeat(129)).is_err());
    }

    #[test]
    fn env_config_refuses_credential_clash_and_duplicates() {
        let ok = [pair("HTTPS_PROXY", "v"), pair("OTHER", "w")];
        assert!(validate_env_config("KEY", &ok).is_ok());
        let clash = validate_env_config("KEY", &[pair("KEY", "v")]).expect_err("clash");
        assert!(clash.contains("extra_env #1"));
        let dup = validate_env_config("KEY", &[pair("A", "1"), pair("A", "2")]).expect_err("dup");
        assert!(dup.contains("#2") && dup.contains("duplicate"));
        let reserved =
            validate_env_config("KEY", &[pair("CLAUDE_CONFIG_DIR", "x")]).expect_err("reserved");
        assert!(reserved.contains("reserved"));
        assert!(validate_env_config("CLAUDE_CONFIG_DIR", &[]).is_err());
    }

    #[test]
    fn env_names_are_compared_case_insensitively() {
        assert!(validate_env_var_name("claude_config_dir").is_err());
        assert!(validate_env_var_name("Claude_Config_Dir").is_err());
        assert!(validate_env_config("KEY", &[pair("key", "v")]).is_err());
        assert!(
            validate_env_config("Anthropic_Api_Key", &[pair("ANTHROPIC_API_KEY", "v")]).is_err()
        );
        assert!(validate_env_config("KEY", &[pair("Path", "1"), pair("PATH", "2")]).is_err());
    }

    #[test]
    fn env_config_error_never_echoes_a_value() {
        let err =
            validate_env_config("KEY", &[pair("1BAD", "top-secret-value")]).expect_err("bad name");
        assert!(!err.contains("top-secret-value"));
        assert!(!err.contains("1BAD"));
    }

    #[test]
    fn debug_hides_extra_env_values() {
        let mut c = sample();
        c.extra_env = vec![pair("PROXY", "top-secret-value")];
        let shown = format!("{c:?}");
        assert!(shown.contains("PROXY"));
        assert!(!shown.contains("top-secret-value"));
    }

    #[test]
    fn extra_env_defaults_to_empty_and_roundtrips_in_order() {
        let json = r#"{
            "id":"a","label":"l","command":"c","credential_env_var":"K",
            "vault_connection_id":"c","vault_name":"v","vault_secret_id":"s",
            "vault_secret_name":"n"
        }"#;
        let c: AgentConfig = serde_json::from_str(json).expect("old shape loads");
        assert!(c.extra_env.is_empty());
        let mut c = sample();
        c.extra_env = vec![pair("B", "2"), pair("A", "1")];
        let back: AgentConfig =
            serde_json::from_str(&serde_json::to_string(&c).expect("ser")).expect("de");
        assert_eq!(back.extra_env, c.extra_env);
    }

    // In-memory fake exercising the trait contract.
    struct FakeRepo(Mutex<Vec<AgentConfig>>);
    impl FakeRepo {
        fn new() -> Self {
            Self(Mutex::new(Vec::new()))
        }
    }
    impl AgentConfigRepository for FakeRepo {
        fn list(&self) -> DomainResult<Vec<AgentConfig>> {
            Ok(self.0.lock().expect("lock FakeRepo").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<AgentConfig>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeRepo")
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        fn save(&self, config: &AgentConfig) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeRepo");
            guard.retain(|c| c.id != config.id);
            guard.push(config.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.0.lock().expect("lock FakeRepo").retain(|c| c.id != id);
            Ok(())
        }
    }

    #[test]
    fn repository_trait_save_get_delete_roundtrip() {
        let repo = FakeRepo::new();
        let c = sample();
        repo.save(&c).expect("save config");
        assert_eq!(repo.get("agent-1").expect("get config"), Some(c.clone()));
        assert_eq!(repo.list().expect("list configs"), vec![c]);
        repo.delete("agent-1").expect("delete config");
        assert_eq!(repo.get("agent-1").expect("get after delete"), None);
    }

    #[test]
    fn repository_save_replaces_existing_entry_with_same_id_instead_of_duplicating() {
        let repo = FakeRepo::new();
        repo.save(&sample()).expect("save first");
        let mut updated = sample();
        updated.label = "Renamed".to_string();
        repo.save(&updated).expect("save update");
        let all = repo.list().expect("list");
        assert_eq!(all.len(), 1, "same id must replace, not append");
        assert_eq!(all[0].label, "Renamed");
    }

    #[test]
    fn repository_save_keeps_distinct_ids_with_the_same_label() {
        let repo = FakeRepo::new();
        let first = sample();
        let mut second = sample();
        second.id = "agent-2".to_string();
        repo.save(&first).expect("save first");
        repo.save(&second).expect("save second");
        assert_eq!(
            repo.list().expect("list").len(),
            2,
            "id, not label, is identity"
        );
        assert_eq!(repo.get("agent-1").expect("get first"), Some(first));
        assert_eq!(repo.get("agent-2").expect("get second"), Some(second));
    }

    #[test]
    fn repository_delete_of_unknown_id_is_a_no_op() {
        let repo = FakeRepo::new();
        repo.save(&sample()).expect("save");
        repo.delete("no-such-id")
            .expect("delete of unknown id must not error");
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    #[test]
    fn trait_is_object_safe() {
        let repo: Box<dyn AgentConfigRepository> = Box::new(FakeRepo::new());
        assert!(repo.list().expect("list via trait object").is_empty());
    }
}
