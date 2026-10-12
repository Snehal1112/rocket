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

/// Where an agent's credential value comes from. It never holds the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CredentialSource {
    /// A RocketVault secret named by the `vault_*` fields.
    #[default]
    Vault,
    /// A variable in Rocket's own process environment, named by `source_env_var`.
    Env,
    /// An entry in the OS keychain, keyed by the config id.
    Keychain,
}

/// Which ACP agent a config runs. It decides how Rocket isolates and
/// instructs the assistant session. Old files without it load as `Claude`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    /// Claude Code through its ACP adapter. Isolation travels in `_meta`.
    #[default]
    Claude,
    /// Gemini CLI in ACP mode. Isolation travels in files and env: a private
    /// home with a lockdown settings file, and a `GEMINI.md` in the cwd.
    Gemini,
    /// Any other ACP agent. Rocket sets no isolation: the agent keeps its own
    /// tools and settings, and gets the Rocket instructions in its first prompt.
    Generic,
}

impl AgentKind {
    /// Environment variable names Rocket sets for this kind. A config of this
    /// kind may not claim them, as a credential name or as extra env.
    pub fn reserved_env_vars(self) -> &'static [&'static str] {
        match self {
            AgentKind::Claude | AgentKind::Generic => &[],
            AgentKind::Gemini => &[
                "GEMINI_CLI_HOME",
                "GEMINI_CLI_TRUST_WORKSPACE",
                "GEMINI_CLI_IDE_WORKSPACE_PATH",
            ],
        }
    }
}

/// Checks the credential variable name and every extra variable name against
/// the names Rocket sets for `kind`. `CLAUDE_CONFIG_DIR` is checked for every
/// kind by `validate_env_var_name`. The error is a short reason and never
/// contains a value.
pub fn validate_kind_env(
    kind: AgentKind,
    credential_env_var: &str,
    extra_env: &[EnvPair],
) -> Result<(), String> {
    let reserved = kind.reserved_env_vars();
    let is_reserved = |name: &str| reserved.iter().any(|r| name.eq_ignore_ascii_case(r));
    if is_reserved(credential_env_var) {
        return Err("credential_env_var is reserved by Rocket".to_string());
    }
    for (i, pair) in extra_env.iter().enumerate() {
        if is_reserved(&pair.name) {
            return Err(format!("extra_env #{} name is reserved by Rocket", i + 1));
        }
    }
    Ok(())
}

/// Shortest and longest accepted idle limit, in seconds.
pub const MIN_IDLE_TIMEOUT_SECS: u32 = 30;
pub const MAX_IDLE_TIMEOUT_SECS: u32 = 1800;

/// Longest accepted pinned model name, in bytes.
pub const MAX_MODEL_NAME_LEN: usize = 128;

/// How hard the agent should reason. Rocket maps it to each agent's own
/// control: Claude's `effort` option, Gemini's thinking settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningLevel {
    Low,
    Medium,
    #[default]
    High,
}

/// Per-agent knobs that tune how a session runs. Old files without this
/// struct load with every field at its default.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AgentTuning {
    /// Seconds a prompt turn may stay silent before the session is ended.
    /// `None` keeps the built-in 120 seconds. A deep reasoning turn on a
    /// slow model may need more.
    #[serde(default)]
    pub idle_timeout_secs: Option<u32>,
    /// How hard the agent should reason. Old files without it load as `high`.
    #[serde(default)]
    pub reasoning: ReasoningLevel,
    /// A model name to pin, written into a Gemini agent's settings. `None`
    /// keeps the agent's own choice.
    #[serde(default)]
    pub model: Option<String>,
}

impl AgentTuning {
    /// Checks every field. The error is a short reason and never contains a value.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(secs) = self.idle_timeout_secs {
            if !(MIN_IDLE_TIMEOUT_SECS..=MAX_IDLE_TIMEOUT_SECS).contains(&secs) {
                return Err(format!(
                    "tuning idle_timeout_secs must be between {MIN_IDLE_TIMEOUT_SECS} and {MAX_IDLE_TIMEOUT_SECS}"
                ));
            }
        }
        if let Some(model) = &self.model {
            let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '/');
            if model.is_empty() || model.len() > MAX_MODEL_NAME_LEN || !model.chars().all(plain) {
                return Err(format!(
                    "tuning model must be 1 to {MAX_MODEL_NAME_LEN} characters: letters, digits and . _ - : /"
                ));
            }
        }
        Ok(())
    }
}

/// One registered ACP agent binary/command and where to find its API key.
/// The credential *value* never lives on this struct for any source. It holds
/// only a reference: the `vault_*` fields for a RocketVault secret, or
/// `source_env_var` for an environment variable. The value is resolved on
/// demand by `AgentConfigService`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub label: String,
    /// The agent family. Old files without this field load as `claude`.
    #[serde(default)]
    pub kind: AgentKind,
    /// Per-agent session tuning. Old files without it load with the defaults.
    #[serde(default)]
    pub tuning: AgentTuning,
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
    /// Where the credential comes from. Old files without it load as `vault`.
    #[serde(default)]
    pub credential_source: CredentialSource,
    /// For the `env` source: the name of the variable Rocket reads from its
    /// own environment.
    #[serde(default)]
    pub source_env_var: Option<String>,
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

/// Checks the fields that depend on the credential source. The vault fields
/// are required only for `vault`; `source_env_var` is required only for `env`
/// and must be absent otherwise. The error is a short reason and never
/// contains a value.
pub fn validate_credential_source(config: &AgentConfig) -> Result<(), String> {
    let has_env_name = config
        .source_env_var
        .as_deref()
        .is_some_and(|name| !name.is_empty());
    match config.credential_source {
        CredentialSource::Vault => {
            let required = [
                ("vault_connection_id", &config.vault_connection_id),
                ("vault_name", &config.vault_name),
                ("vault_secret_id", &config.vault_secret_id),
            ];
            for (field, value) in required {
                if value.trim().is_empty() {
                    return Err(format!("{field} must not be empty"));
                }
            }
            if has_env_name {
                return Err("source_env_var is only used by the env source".to_string());
            }
        }
        CredentialSource::Env => {
            let Some(name) = config.source_env_var.as_deref().filter(|n| !n.is_empty()) else {
                return Err("source_env_var must not be empty for the env source".to_string());
            };
            validate_env_var_name(name).map_err(|why| format!("source_env_var {why}"))?;
        }
        CredentialSource::Keychain => {
            if has_env_name {
                return Err("source_env_var is only used by the env source".to_string());
            }
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
            kind: AgentKind::default(),
            command: "claude-agent-acp".to_string(),
            args: vec!["--stdio".to_string()],
            working_dir: None,
            credential_env_var: "ANTHROPIC_API_KEY".to_string(),
            extra_env: Vec::new(),
            credential_source: CredentialSource::Vault,
            source_env_var: None,
            vault_connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            vault_secret_id: "b6f1c2e0-1234-4a5b-9abc-000000000001".to_string(),
            vault_secret_name: "anthropic-api-key".to_string(),
            tuning: AgentTuning::default(),
        }
    }

    #[test]
    fn a_config_without_tuning_loads_with_the_defaults() {
        let json = serde_json::json!({
            "id": "a", "label": "A", "command": "c", "credential_env_var": "K",
            "vault_connection_id": "v", "vault_name": "n", "vault_secret_id": "s",
            "vault_secret_name": "sn"
        });
        let config: AgentConfig = serde_json::from_value(json).expect("old shape");
        assert_eq!(config.tuning, AgentTuning::default());
        assert_eq!(config.tuning.idle_timeout_secs, None);
    }

    #[test]
    fn the_idle_timeout_must_stay_between_30_and_1800_seconds() {
        let tuning = |secs: Option<u32>| AgentTuning {
            idle_timeout_secs: secs,
            ..AgentTuning::default()
        };
        assert!(tuning(None).validate().is_ok());
        assert!(tuning(Some(MIN_IDLE_TIMEOUT_SECS)).validate().is_ok());
        assert!(tuning(Some(MAX_IDLE_TIMEOUT_SECS)).validate().is_ok());
        for bad in [0, 5, 29, 1801, 99_999] {
            let err = tuning(Some(bad)).validate().expect_err("out of range");
            assert!(err.contains("idle_timeout_secs"), "{err}");
        }
    }

    #[test]
    fn reasoning_defaults_to_high_and_uses_lowercase_wire_names() {
        assert_eq!(ReasoningLevel::default(), ReasoningLevel::High);
        assert_eq!(
            serde_json::to_string(&ReasoningLevel::Medium).expect("json"),
            "\"medium\""
        );
        assert_eq!(
            serde_json::from_str::<ReasoningLevel>("\"low\"").expect("parse"),
            ReasoningLevel::Low
        );
    }

    #[test]
    fn a_tuning_saved_before_reasoning_existed_loads_as_high_with_no_model() {
        let tuning: AgentTuning =
            serde_json::from_value(serde_json::json!({ "idle_timeout_secs": 300 })).expect("old");
        assert_eq!(tuning.reasoning, ReasoningLevel::High);
        assert_eq!(tuning.model, None);
        assert_eq!(tuning.idle_timeout_secs, Some(300));
    }

    #[test]
    fn a_pinned_model_must_be_a_plain_model_name() {
        let with_model = |m: &str| AgentTuning {
            model: Some(m.to_string()),
            ..AgentTuning::default()
        };
        for good in ["gemini-2.5-pro", "gemini-3-pro-preview", "models/gemini-2.5-flash", "a_b:c.d"] {
            assert!(with_model(good).validate().is_ok(), "{good}");
        }
        for bad in ["", "   ", "has space", "quote\"d", "new\nline", "back\\slash", "{x}"] {
            let err = with_model(bad).validate().expect_err(bad);
            assert!(err.contains("tuning model"), "{err}");
            assert!(!err.contains(bad.trim()) || bad.trim().is_empty(), "never echoes the value: {err}");
        }
        let too_long = "a".repeat(MAX_MODEL_NAME_LEN + 1);
        assert!(with_model(&too_long).validate().is_err());
        assert!(with_model(&"a".repeat(MAX_MODEL_NAME_LEN)).validate().is_ok());
    }

    #[test]
    fn tuning_roundtrips_in_snake_case() {
        let mut c = sample();
        c.tuning.idle_timeout_secs = Some(300);
        let json = serde_json::to_string(&c).expect("serialize AgentConfig");
        assert!(json.contains("\"idle_timeout_secs\":300"), "got: {json}");
        let back: AgentConfig = serde_json::from_str(&json).expect("deserialize AgentConfig");
        assert_eq!(back.tuning.idle_timeout_secs, Some(300));
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

    #[test]
    fn agent_kind_defaults_to_claude_and_uses_lowercase_wire_names() {
        assert_eq!(AgentKind::default(), AgentKind::Claude);
        assert_eq!(
            serde_json::to_string(&AgentKind::Generic).expect("json"),
            "\"generic\""
        );
        assert_eq!(
            serde_json::from_str::<AgentKind>("\"claude\"").expect("parse"),
            AgentKind::Claude
        );
    }

    #[test]
    fn a_config_without_kind_loads_as_claude() {
        let json = serde_json::json!({
            "id": "a", "label": "A", "command": "c", "credential_env_var": "ANTHROPIC_API_KEY",
            "vault_connection_id": "v", "vault_name": "n", "vault_secret_id": "s",
            "vault_secret_name": "sn"
        });
        let config: AgentConfig = serde_json::from_value(json).expect("old shape");
        assert_eq!(config.kind, AgentKind::Claude);
    }

    #[test]
    fn kind_roundtrips_in_snake_case_config_json() {
        let mut c = sample();
        c.kind = AgentKind::Generic;
        let json = serde_json::to_string(&c).expect("serialize AgentConfig");
        assert!(json.contains("\"kind\":\"generic\""), "got: {json}");
        let back: AgentConfig = serde_json::from_str(&json).expect("deserialize AgentConfig");
        assert_eq!(back.kind, AgentKind::Generic);
    }

    #[test]
    fn gemini_kind_uses_its_lowercase_wire_name() {
        assert_eq!(
            serde_json::to_string(&AgentKind::Gemini).expect("json"),
            "\"gemini\""
        );
    }

    #[test]
    fn kind_env_check_refuses_the_reserved_names_of_that_kind_only() {
        let pair = |name: &str| EnvPair {
            name: name.to_string(),
            value: "v".to_string(),
        };
        let err = validate_kind_env(AgentKind::Gemini, "KEY", &[pair("GEMINI_CLI_HOME")])
            .expect_err("reserved extra env");
        assert!(
            err.contains("extra_env #1") && err.contains("reserved"),
            "{err}"
        );
        assert!(
            validate_kind_env(AgentKind::Gemini, "gemini_cli_trust_workspace", &[]).is_err(),
            "the credential name is checked too, in any case"
        );
        assert!(validate_kind_env(
            AgentKind::Gemini,
            "KEY",
            &[pair("GEMINI_CLI_IDE_WORKSPACE_PATH")]
        )
        .is_err());
        assert!(
            validate_kind_env(AgentKind::Gemini, "GEMINI_API_KEY", &[pair("HTTPS_PROXY")]).is_ok()
        );
        assert!(validate_kind_env(AgentKind::Claude, "KEY", &[pair("GEMINI_CLI_HOME")]).is_ok());
        assert!(validate_kind_env(AgentKind::Generic, "KEY", &[pair("GEMINI_CLI_HOME")]).is_ok());
        let err = validate_kind_env(AgentKind::Gemini, "GEMINI_CLI_HOME", &[pair("X")])
            .expect_err("reserved credential name");
        assert!(!err.contains('/'), "never echoes a value: {err}");
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

    #[test]
    fn credential_source_uses_lowercase_wire_names_and_defaults_to_vault() {
        assert_eq!(
            serde_json::to_string(&CredentialSource::Env).expect("json"),
            "\"env\""
        );
        assert_eq!(
            serde_json::from_str::<CredentialSource>("\"keychain\"").expect("parse"),
            CredentialSource::Keychain
        );
        assert_eq!(CredentialSource::default(), CredentialSource::Vault);
    }

    #[test]
    fn a_config_without_the_new_fields_deserializes_as_vault() {
        let json = serde_json::json!({
            "id": "a", "label": "A", "command": "c", "credential_env_var": "ANTHROPIC_API_KEY",
            "vault_connection_id": "v", "vault_name": "n", "vault_secret_id": "s",
            "vault_secret_name": "sn"
        });
        let config: AgentConfig = serde_json::from_value(json).expect("old shape");
        assert_eq!(config.credential_source, CredentialSource::Vault);
        assert_eq!(config.source_env_var, None);
    }

    #[test]
    fn validate_credential_source_checks_each_source() {
        let mut vault = sample();
        assert!(validate_credential_source(&vault).is_ok());
        vault.vault_secret_id = String::new();
        assert!(validate_credential_source(&vault).is_err());

        let mut env = sample();
        env.credential_source = CredentialSource::Env;
        env.vault_connection_id = String::new();
        env.vault_name = String::new();
        env.vault_secret_id = String::new();
        env.source_env_var = Some("CLAUDE_CODE_OAUTH_TOKEN".into());
        assert!(
            validate_credential_source(&env).is_ok(),
            "vault fields may be empty"
        );
        for bad in [
            None,
            Some(String::new()),
            Some("1BAD".to_string()),
            Some("sk-ant-oat01-abc".to_string()),
            Some("CLAUDE_CONFIG_DIR".to_string()),
        ] {
            env.source_env_var = bad.clone();
            let err = validate_credential_source(&env).expect_err("bad env name");
            assert!(!err.contains("sk-ant-oat01-abc"), "{err}");
        }

        let mut keychain = sample();
        keychain.credential_source = CredentialSource::Keychain;
        keychain.vault_connection_id = String::new();
        assert!(validate_credential_source(&keychain).is_ok());
        keychain.source_env_var = Some("SOMETHING".into());
        assert!(
            validate_credential_source(&keychain).is_err(),
            "env name only for env"
        );
    }
}
