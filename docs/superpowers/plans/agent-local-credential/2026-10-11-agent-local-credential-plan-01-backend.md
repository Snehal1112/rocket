# Agent Local Credential Implementation Plan 01: Backend

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An agent config can take its credential from a RocketVault secret, a variable in Rocket's own environment, or an OS keychain entry saved from the app, with one explicit source per config.

**Architecture:** `rocket-acp` gets the `CredentialSource` enum, two new `AgentConfig` fields and a pure validation function. `AgentConfigService` resolves the credential per source and owns the keychain writes through an injected `SecretStore` and an injected environment reader. `rocket-infra` gets a new keychain namespace and `src-tauri` gets three IPC commands. The resolved value is the same `String` as today, so session env building and masking do not change.

**Tech Stack:** Rust (`rocket-acp`, `rocket-app`, `rocket-infra`, `src-tauri`), serde, `keyring` through the existing `KeyringSecretStore`.

**Spec:** [../../specs/2026-10-11-agent-local-credential-design.md](../../specs/2026-10-11-agent-local-credential-design.md). Index and rulings: [00-index.md](00-index.md).

## Global Constraints

- No credential value in an error message, a log line, an event, a `Debug` output, a test name or any value returned to the UI. Tests use a recognizable fake value and assert it is absent.
- One explicit source per config: `vault`, `env` or `keychain`. No fallback chain. A failure is reported as it is.
- `agent_configs.yml` holds only names. For `env` it holds a variable name, validated by the existing `validate_env_var_name` (letters, digits, underscore, not starting with a digit, at most 128 bytes, not `CLAUDE_CONFIG_DIR`).
- Old files without the new fields load as `vault`.
- A locked or unavailable keychain is an error (`get_strict`), never "not set". There is no plaintext fallback.
- Keychain value rules: trimmed; refuse empty, any control character, or more than 4096 bytes.
- `agent_configs.yml` is not part of any collection, so the OpenCollection reference read step does not apply.
- Rust: no panicking unwrap calls in production paths. Comments are short full sentences ending with a period.
- Cargo: always `-j4`. Never `cargo test --workspace` or `--all`.
- Commit through the `dev-workflow-skills:1-git-commit` skill with explicit paths.

## Review Focus

- An `env` config whose variable is unset or blank fails at session start and in Test with a message that names the variable and never a value (T2).
- A `keychain` config with no saved entry, and a locked keychain, are two different errors; the locked one is not "not set" (T2).
- Deleting a config, or switching it away from `keychain`, removes the keychain entry (T2).
- A value with control characters, an empty value or a 4097-byte value is refused and never echoed (T2).
- An old `agent_configs.yml` without the new fields still loads and resolves as a vault config (T1).

---

### Task 1: Model, validation and DTO

**Files:**
- Modify: `crates/rocket-acp/src/agent_config.rs`
- Modify: `crates/rocket-acp/src/lib.rs` (re-export)
- Modify: `crates/rocket-infra/src/fs_agent_config_repo.rs` (test literal and new test)
- Modify: `crates/rocket-app/src/acp_session_service.rs` (test literal)
- Modify: `crates/rocket-app/src/agent_config_service.rs` (test literal `sample_config` only)
- Modify: `src-tauri/src/commands/agent_configs.rs` (DTO, both `From` impls, test literal)

**Interfaces:**
- Consumes: the existing `validate_env_var_name`.
- Produces: `CredentialSource`, the two `AgentConfig` fields, `validate_credential_source`, and the DTO fields `credentialSource` and `sourceEnvVar`.

- [ ] **Step 1: Write the failing tests** at the end of `mod tests` in `crates/rocket-acp/src/agent_config.rs` (read the existing `sample()` helper in that module first and add the two fields to it in Step 3; the tests below assume a helper `sample()` returning a vault config):

```rust
    #[test]
    fn credential_source_uses_lowercase_wire_names_and_defaults_to_vault() {
        assert_eq!(serde_json::to_string(&CredentialSource::Env).expect("json"), "\"env\"");
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
        assert!(validate_credential_source(&env).is_ok(), "vault fields may be empty");
        for bad in [None, Some(String::new()), Some("1BAD".to_string()), Some("sk-ant-oat01-abc".to_string()), Some("CLAUDE_CONFIG_DIR".to_string())] {
            env.source_env_var = bad.clone();
            let err = validate_credential_source(&env).expect_err("bad env name");
            assert!(!err.contains("sk-ant-oat01-abc"), "{err}");
        }

        let mut keychain = sample();
        keychain.credential_source = CredentialSource::Keychain;
        keychain.vault_connection_id = String::new();
        assert!(validate_credential_source(&keychain).is_ok());
        keychain.source_env_var = Some("SOMETHING".into());
        assert!(validate_credential_source(&keychain).is_err(), "env name only for env");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p rocket-acp -j4 credential_source`
Expected: compile error, `CredentialSource` not found.

- [ ] **Step 3: Implement.** In `agent_config.rs`, above `AgentConfig`:

```rust
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
```

Add to `AgentConfig` (after `extra_env`):

```rust
    /// Where the credential comes from. Old files without it load as `vault`.
    #[serde(default)]
    pub credential_source: CredentialSource,
    /// For the `env` source: the name of the variable Rocket reads from its
    /// own environment.
    #[serde(default)]
    pub source_env_var: Option<String>,
```

Update the doc comment of `AgentConfig` (the credential value never lives here for any source). Add:

```rust
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
```

In the test helper `sample()` of this file and in every other `AgentConfig { .. }` literal listed under Files, add `credential_source: CredentialSource::Vault, source_env_var: None,` (import `CredentialSource` where needed). In `lib.rs` re-export `CredentialSource` and `validate_credential_source` next to the other `agent_config` items.

DTO in `src-tauri/src/commands/agent_configs.rs`: add to `AgentConfigDto`

```rust
    // Old callers may omit it.
    #[serde(default)]
    pub credential_source: CredentialSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_env_var: Option<String>,
```

and copy both fields in the two `From` impls. Add a DTO test:

```rust
    #[test]
    fn dto_carries_the_credential_source_in_camel_case() {
        let mut config = sample_domain();
        config.credential_source = CredentialSource::Env;
        config.source_env_var = Some("CLAUDE_CODE_OAUTH_TOKEN".to_string());
        let json = serde_json::to_value(AgentConfigDto::from(config)).expect("serialize");
        assert_eq!(json["credentialSource"], "env");
        assert_eq!(json["sourceEnvVar"], "CLAUDE_CODE_OAUTH_TOKEN");
        let old: AgentConfigDto = serde_json::from_value(serde_json::json!({
            "id": "a", "label": "A", "command": "c", "args": [], "credentialEnvVar": "K",
            "vaultConnectionId": "v", "vaultName": "n", "vaultSecretId": "s", "vaultSecretName": "sn"
        }))
        .expect("old caller");
        assert_eq!(AgentConfig::from(old).credential_source, CredentialSource::Vault);
    }
```

In `fs_agent_config_repo.rs` tests add a round trip with a non-default source and an old-file test:

```rust
    #[test]
    fn an_old_file_without_the_new_fields_loads_as_vault() {
        let (dir, repo) = setup();
        std::fs::write(
            dir.path().join("agent_configs.yml"),
            "- id: a\n  label: A\n  command: c\n  credential_env_var: K\n  vault_connection_id: v\n  vault_name: n\n  vault_secret_id: s\n  vault_secret_name: sn\n",
        )
        .expect("write old file");
        let loaded = repo.list().expect("list");
        assert_eq!(loaded[0].credential_source, CredentialSource::Vault);
        assert_eq!(loaded[0].source_env_var, None);
    }

    #[test]
    fn a_local_source_round_trips() {
        let (_dir, repo) = setup();
        let mut config = sample("a");
        config.credential_source = CredentialSource::Env;
        config.source_env_var = Some("CLAUDE_CODE_OAUTH_TOKEN".to_string());
        repo.save(&config).expect("save");
        assert_eq!(repo.get("a").expect("get"), Some(config));
    }
```

(Check how the repo file is shaped — a top-level list or a map — and write the old file accordingly.)

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p rocket-acp -j4 credential_source` and `cargo test -p rocket-infra -j4 fs_agent_config_repo` and `cargo test -p rocket -j4 --lib dto_carries_the_credential_source`.
Expected: PASS.

- [ ] **Step 5: Checks.** `cargo check --workspace --all-targets -j4`.

- [ ] **Step 6: Commit** via the commit skill. Paths: the six files above. Message: `feat(assistant): add the agent credential source model`.

---

### Task 2: `AgentConfigService` resolve, validate and keychain writes

**Files:**
- Modify: `crates/rocket-app/src/agent_config_service.rs`
- Modify: `crates/rocket-app/src/lib.rs` (export `EnvReader`, `process_env_reader`)
- Modify: `crates/rocket-app/CLAUDE.md`

**Interfaces:**
- Consumes: T1's `CredentialSource`, `validate_credential_source`; `rocket_environment::secret_store::SecretStore` (`get_strict`, `set`, `delete`).
- Produces: `EnvReader`, `process_env_reader()`, the new `AgentConfigService::new` signature, `set_local_credential`, `clear_local_credential`, `has_local_credential`.

- [ ] **Step 1: Write the failing tests.** In `mod tests` of `agent_config_service.rs` add test doubles and update `service_with`. Add:

```rust
    /// An in-memory keychain. `fail` makes every call an error, like a locked keychain.
    #[derive(Default)]
    struct MapStore {
        entries: Mutex<std::collections::HashMap<(String, String), String>>,
        fail: std::sync::atomic::AtomicBool,
    }
    impl SecretStore for MapStore {
        fn get(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
            self.get_strict(scope, key)
        }
        fn get_strict(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(DomainError::Internal("the keychain could not be read".to_string()));
            }
            Ok(self.entries.lock().expect("lock").get(&(scope.into(), key.into())).cloned())
        }
        fn set(&self, scope: &str, key: &str, value: &str) -> DomainResult<()> {
            self.entries.lock().expect("lock").insert((scope.into(), key.into()), value.into());
            Ok(())
        }
        fn delete(&self, scope: &str, key: &str) -> DomainResult<()> {
            self.entries.lock().expect("lock").remove(&(scope.to_string(), key.to_string()));
            Ok(())
        }
    }

    fn env_reader(pairs: &[(&str, &str)]) -> EnvReader {
        let map: std::collections::HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Arc::new(move |name| map.get(name).cloned())
    }

    fn local_service(store: Arc<MapStore>, env: EnvReader) -> AgentConfigService {
        let secret_manager = Arc::new(SecretManagerService::new(
            Box::new(FakeSecretManagerRepo(Mutex::new(Vec::new()))),
            Arc::new(FakeSecretStore),
            Arc::new(FakeVaultFetcher { secret_value_result: Ok(None) }),
        ));
        AgentConfigService::new(Box::new(FakeAgentConfigRepo::new()), secret_manager, store, env)
    }

    fn env_config(id: &str, var: &str) -> AgentConfig {
        let mut config = sample_config(id, "");
        config.credential_source = CredentialSource::Env;
        config.source_env_var = Some(var.to_string());
        config.vault_connection_id = String::new();
        config.vault_name = String::new();
        config.vault_secret_id = String::new();
        config
    }

    fn keychain_config(id: &str) -> AgentConfig {
        let mut config = env_config(id, "UNUSED");
        config.credential_source = CredentialSource::Keychain;
        config.source_env_var = None;
        config
    }

    const FAKE_TOKEN: &str = "sk-ant-oat01-FAKE-VALUE-FOR-TESTS";

    #[tokio::test]
    async fn an_env_config_reads_the_named_variable_trimmed() {
        let service = local_service(Arc::default(), env_reader(&[("MY_TOKEN", &format!("  {FAKE_TOKEN}\n"))]));
        service.save(env_config("a", "MY_TOKEN")).expect("save without a vault connection");
        assert_eq!(service.resolve_credential("a").await.expect("resolve"), FAKE_TOKEN);
    }

    #[tokio::test]
    async fn an_env_config_with_a_missing_or_blank_variable_names_it_and_never_a_value() {
        let service = local_service(Arc::default(), env_reader(&[("BLANK", "   ")]));
        service.save(env_config("a", "NOT_SET")).expect("save");
        service.save(env_config("b", "BLANK")).expect("save");
        for id in ["a", "b"] {
            let err = service.resolve_credential(id).await.expect_err("not set");
            assert!(matches!(err, DomainError::NotFound(_)), "{err:?}");
        }
        let message = service.resolve_credential("a").await.expect_err("not set").to_string();
        assert!(message.contains("NOT_SET"), "{message}");
    }

    #[tokio::test]
    async fn a_keychain_config_reads_the_saved_entry_and_separates_missing_from_locked() {
        let store = Arc::new(MapStore::default());
        let service = local_service(Arc::clone(&store), env_reader(&[]));
        service.save(keychain_config("a")).expect("save");

        let missing = service.resolve_credential("a").await.expect_err("no entry yet");
        assert!(matches!(missing, DomainError::NotFound(_)), "{missing:?}");

        service.set_local_credential("a", &format!("  {FAKE_TOKEN}  ")).expect("set");
        assert_eq!(service.resolve_credential("a").await.expect("resolve"), FAKE_TOKEN);

        store.fail.store(true, std::sync::atomic::Ordering::SeqCst);
        let locked = service.resolve_credential("a").await.expect_err("locked keychain");
        assert!(matches!(locked, DomainError::Internal(_)), "locked is not a miss: {locked:?}");
        assert!(!locked.to_string().contains(FAKE_TOKEN));
    }

    #[test]
    fn set_local_credential_enforces_the_rules_and_never_echoes_the_value() {
        let service = local_service(Arc::default(), env_reader(&[]));
        service.save(keychain_config("a")).expect("save");
        service.save(env_config("e", "X")).expect("save env");
        let too_long = "x".repeat(4097);
        let with_control = format!("{FAKE_TOKEN}\u{0007}");
        for (label, value) in [("empty", ""), ("blank", "  \n"), ("control", with_control.as_str()), ("too long", too_long.as_str())] {
            let err = service.set_local_credential("a", value).expect_err(label);
            assert!(matches!(err, DomainError::InvalidInput(_)), "{label}");
            assert!(!err.to_string().contains(FAKE_TOKEN), "{label}");
        }
        assert!(service.set_local_credential("e", FAKE_TOKEN).is_err(), "only a keychain config");
        assert!(matches!(service.set_local_credential("nope", FAKE_TOKEN), Err(DomainError::NotFound(_))));
        service.set_local_credential("a", &"x".repeat(4096)).expect("exactly the limit");
    }

    #[test]
    fn has_and_clear_follow_the_entry() {
        let service = local_service(Arc::default(), env_reader(&[]));
        service.save(keychain_config("a")).expect("save");
        service.save(env_config("e", "X")).expect("save env");
        assert!(!service.has_local_credential("a").expect("has"));
        service.set_local_credential("a", FAKE_TOKEN).expect("set");
        assert!(service.has_local_credential("a").expect("has"));
        service.clear_local_credential("a").expect("clear");
        assert!(!service.has_local_credential("a").expect("has"));
        assert!(!service.has_local_credential("e").expect("not a keychain config"));
    }

    #[test]
    fn deleting_or_switching_away_removes_the_keychain_entry() {
        let store = Arc::new(MapStore::default());
        let service = local_service(Arc::clone(&store), env_reader(&[]));
        service.save(keychain_config("a")).expect("save");
        service.set_local_credential("a", FAKE_TOKEN).expect("set");
        service.delete("a").expect("delete");
        assert!(store.entries.lock().expect("lock").is_empty(), "delete clears the entry");

        service.save(keychain_config("b")).expect("save");
        service.set_local_credential("b", FAKE_TOKEN).expect("set");
        service.save(env_config("b", "MY_TOKEN")).expect("switch to env");
        assert!(store.entries.lock().expect("lock").is_empty(), "switching clears the entry");
    }

    #[test]
    fn save_validates_the_source_fields() {
        let service = local_service(Arc::default(), env_reader(&[]));
        assert!(service.save(env_config("a", "")).is_err());
        assert!(service.save(env_config("a", "sk-ant-oat01-abc")).is_err());
        let mut vault = sample_config("a", "conn-1");
        vault.vault_secret_id = String::new();
        assert!(service.save(vault).is_err());
    }
```

Update the existing `service_with` to pass `Arc::new(MapStore::default())` and `env_reader(&[])` to the new constructor.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p rocket-app -j4 agent_config_service`
Expected: compile errors (constructor arity, missing methods).

- [ ] **Step 3: Implement** in `agent_config_service.rs`:

Imports: `use rocket_acp::{validate_credential_source, AgentConfig, AgentConfigRepository, CredentialSource};` and `use rocket_environment::secret_store::SecretStore;`.

```rust
/// Reads one variable from Rocket's own process environment. Injected so the
/// service holds no process state and tests need no real environment.
pub type EnvReader = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The reader used in production. A variable that is not valid Unicode counts
/// as not set.
pub fn process_env_reader() -> EnvReader {
    Arc::new(|name| std::env::var(name).ok())
}

/// The keychain scope that holds agent credentials, keyed by config id.
const LOCAL_CREDENTIAL_SCOPE: &str = "agent-credential";

/// Longest accepted keychain credential, in bytes.
const MAX_LOCAL_CREDENTIAL_BYTES: usize = 4096;
```

Struct and constructor:

```rust
pub struct AgentConfigService {
    repo: Box<dyn AgentConfigRepository>,
    secret_manager: Arc<SecretManagerService>,
    local_secrets: Arc<dyn SecretStore>,
    env_reader: EnvReader,
}

impl AgentConfigService {
    pub fn new(
        repo: Box<dyn AgentConfigRepository>,
        secret_manager: Arc<SecretManagerService>,
        local_secrets: Arc<dyn SecretStore>,
        env_reader: EnvReader,
    ) -> Self { Self { repo, secret_manager, local_secrets, env_reader } }
```

`save` (only the vault connection check stays source-specific):

```rust
    pub fn save(&self, config: AgentConfig) -> DomainResult<()> {
        validate_config(&config)?;
        if config.credential_source == CredentialSource::Vault {
            let known_connection = self
                .secret_manager
                .list()?
                .iter()
                .any(|c| c.id == config.vault_connection_id);
            if !known_connection {
                return Err(DomainError::InvalidInput(format!(
                    "agent '{}': vault_connection_id '{}' does not match any configured RocketVault connection",
                    config.label, config.vault_connection_id
                )));
            }
        }
        let previous = self.repo.get(&config.id)?;
        self.repo.save(&config)?;
        let left_keychain = previous.is_some_and(|p| {
            p.credential_source == CredentialSource::Keychain
                && config.credential_source != CredentialSource::Keychain
        });
        if left_keychain {
            self.forget_local_credential(&config.id);
        }
        Ok(())
    }

    pub fn delete(&self, id: &str) -> DomainResult<()> {
        let previous = self.repo.get(id)?;
        self.repo.delete(id)?;
        if previous.is_some_and(|p| p.credential_source == CredentialSource::Keychain) {
            self.forget_local_credential(id);
        }
        Ok(())
    }

    /// Removes the keychain entry of a config. A failure is logged without any
    /// value and does not fail the caller, because the config change already
    /// happened.
    fn forget_local_credential(&self, id: &str) {
        if let Err(e) = self.local_secrets.delete(LOCAL_CREDENTIAL_SCOPE, id) {
            tracing::warn!(error = %e, "could not remove an agent credential from the keychain");
        }
    }
```

`resolve_credential_for`:

```rust
    async fn resolve_credential_for(&self, config: &AgentConfig) -> DomainResult<String> {
        match config.credential_source {
            CredentialSource::Vault => self.resolve_vault_credential(config).await,
            CredentialSource::Env => {
                let name = config.source_env_var.as_deref().unwrap_or_default();
                (self.env_reader)(name)
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        DomainError::NotFound(format!(
                            "agent '{}': environment variable '{name}' is not set in Rocket's environment",
                            config.label
                        ))
                    })
            }
            CredentialSource::Keychain => self
                .local_secrets
                .get_strict(LOCAL_CREDENTIAL_SCOPE, &config.id)?
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    DomainError::NotFound(format!(
                        "agent '{}': no credential saved in the keychain; open the agent settings and save one",
                        config.label
                    ))
                }),
        }
    }
```

Rename the existing body to `resolve_vault_credential(&self, config)`. New public methods:

```rust
    /// Saves the credential of a `keychain` config in the OS keychain.
    pub fn set_local_credential(&self, config_id: &str, value: &str) -> DomainResult<()> {
        let config = self.get_config(config_id)?;
        require_keychain(&config)?;
        let value = value.trim();
        validate_local_credential(value)?;
        self.local_secrets.set(LOCAL_CREDENTIAL_SCOPE, config_id, value)
    }

    /// Removes the keychain entry of a config. Missing is fine.
    pub fn clear_local_credential(&self, config_id: &str) -> DomainResult<()> {
        self.get_config(config_id)?;
        self.local_secrets.delete(LOCAL_CREDENTIAL_SCOPE, config_id)
    }

    /// True when a `keychain` config has a saved entry. Other sources are false.
    pub fn has_local_credential(&self, config_id: &str) -> DomainResult<bool> {
        let config = self.get_config(config_id)?;
        if config.credential_source != CredentialSource::Keychain {
            return Ok(false);
        }
        Ok(self
            .local_secrets
            .get_strict(LOCAL_CREDENTIAL_SCOPE, config_id)?
            .is_some())
    }
```

Free functions; `validate_config` now requires only id, label, command and credential_env_var, then runs `validate_credential_source` and `validate_env_config`:

```rust
fn require_keychain(config: &AgentConfig) -> DomainResult<()> {
    if config.credential_source != CredentialSource::Keychain {
        return Err(DomainError::InvalidInput(format!(
            "agent '{}': its credential source is not the keychain",
            config.label
        )));
    }
    Ok(())
}

/// The error text never contains the value.
fn validate_local_credential(value: &str) -> DomainResult<()> {
    if value.is_empty() {
        return Err(DomainError::InvalidInput("credential must not be empty".to_string()));
    }
    if value.len() > MAX_LOCAL_CREDENTIAL_BYTES {
        return Err(DomainError::InvalidInput(format!(
            "credential is too long (at most {MAX_LOCAL_CREDENTIAL_BYTES} bytes)"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(DomainError::InvalidInput(
            "credential must not contain control characters".to_string(),
        ));
    }
    Ok(())
}
```

(`validate_config` calls `validate_credential_source(config).map_err(|why| DomainError::InvalidInput(format!("agent config {why}")))?;`.) Export `EnvReader, process_env_reader` from `crates/rocket-app/src/lib.rs`.

Note: `src-tauri/src/lib.rs` calls `AgentConfigService::new` with the old arity; Task 3 updates it. To keep the workspace compiling at the end of this task, update the two call sites there now with `Arc::new(rocket_environment::NullSecretStore)` and `rocket_app::process_env_reader()`, marked `// Replaced in plan 01 task 3.`

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p rocket-app -j4 agent_config_service`
Expected: PASS (the old tests and the 7 new ones).

- [ ] **Step 5: Checks.** `cargo check --workspace --all-targets -j4` and `cargo clippy -p rocket-app -j4 -- -D warnings`.

- [ ] **Step 6: Docs.** In `crates/rocket-app/CLAUDE.md`, extend the `AgentConfigService` row: credential source (`vault`, `env`, `keychain`), `resolve_credential` per source, `set/clear/has_local_credential`, the keychain scope `agent-credential` keyed by config id, delete and source-change cleanup, `get_strict` so a locked keychain is an error.

- [ ] **Step 7: Commit** via the commit skill. Paths: `crates/rocket-app/src/agent_config_service.rs`, `crates/rocket-app/src/lib.rs`, `crates/rocket-app/CLAUDE.md`, `src-tauri/src/lib.rs`. Message: `feat(assistant): resolve agent credentials per source`.

---

### Task 3: Keychain namespace, IPC commands and wiring

**Files:**
- Modify: `crates/rocket-infra/src/secret_store.rs`
- Modify: `src-tauri/src/commands/agent_configs.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `crates/rocket-acp/CLAUDE.md`
- Modify: `docs/features/assistant-followups.md`

**Interfaces:**
- Consumes: T2's `AgentConfigService` methods, `EnvReader`, `process_env_reader`.
- Produces: `KeyringSecretStore::new_agent_credentials()`, commands `set_agent_credential`, `clear_agent_credential`, `has_agent_credential`.

- [ ] **Step 1: Write the failing test** in `mod tests` of `secret_store.rs`, next to the other namespace tests:

```rust
    #[test]
    fn the_agent_credentials_namespace_is_distinct() {
        let agent = KeyringSecretStore::new_agent_credentials();
        assert_eq!(agent.service, "com.rocketapi.agent-credentials");
        for other in [
            KeyringSecretStore::new_env_secrets(),
            KeyringSecretStore::new_vault_connections(),
            KeyringSecretStore::new_proxy(),
        ] {
            assert_ne!(agent.service, other.service);
        }
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p rocket-infra -j4 agent_credentials_namespace`
Expected: compile error.

- [ ] **Step 3: Implement.**

`secret_store.rs`, next to `new_proxy`:

```rust
    /// Backs agent credentials saved from the agent dialog. A separate keychain
    /// service from the other namespaces so the entries can never collide.
    pub fn new_agent_credentials() -> Self {
        Self {
            service: "com.rocketapi.agent-credentials",
        }
    }
```

`agent_configs.rs` commands (no command returns a value; the arguments are camelCase on the wire through Tauri's default):

```rust
#[tauri::command]
pub async fn set_agent_credential(
    config_id: String,
    value: String,
    svc: State<'_, AgentConfigService>,
) -> Result<(), DomainError> {
    svc.set_local_credential(&config_id, &value)
}

#[tauri::command]
pub async fn clear_agent_credential(
    config_id: String,
    svc: State<'_, AgentConfigService>,
) -> Result<(), DomainError> {
    svc.clear_local_credential(&config_id)
}

#[tauri::command]
pub async fn has_agent_credential(
    config_id: String,
    svc: State<'_, AgentConfigService>,
) -> Result<bool, DomainError> {
    svc.has_local_credential(&config_id)
}
```

Check that `DomainError`'s `Serialize` output for a failed `set_local_credential` carries the same text as the service error (it never holds the value; the service tests pin that).

`src-tauri/src/lib.rs`: create one shared store above the two service constructions and use it in both, replacing T2's temporary arguments:

```rust
            let agent_credential_store: Arc<dyn rocket_environment::secret_store::SecretStore> =
                Arc::new(rocket_infra::KeyringSecretStore::new_agent_credentials());
            let agent_env_reader = rocket_app::process_env_reader();
```

and pass `Arc::clone(&agent_credential_store), Arc::clone(&agent_env_reader)` as the last two arguments of both `AgentConfigService::new` calls. Register the three commands in the `invoke_handler` list after `test_agent_config`.

Add a wiring-level test in `agent_configs.rs` `mod tests` only if the module already tests commands through a `tauri::test` app; otherwise rely on the service tests (the commands are one-line delegations). State which in the report.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p rocket-infra -j4 agent_credentials_namespace`.
Expected: PASS.

- [ ] **Step 5: Checks.** `cargo check --workspace --all-targets -j4` and `cargo clippy -p rocket -p rocket-infra -j4 -- -D warnings`. Confirm no `Replaced in plan 01 task 3` comment remains (`grep -rn "Replaced in plan 01 task 3" src-tauri crates`).

- [ ] **Step 6: Docs.**
  - `crates/rocket-acp/CLAUDE.md`: `AgentConfig` holds `credential_source` and `source_env_var`; the key design point now covers all three sources (the value never lives on the struct); env names are validated; add the limit "the env source sees only what Rocket's process was started with".
  - `docs/features/assistant-followups.md`: add open items — stored conversations do not scrub agent credentials from the env or keychain sources (same as vault values); the commands run the keychain call on a runtime thread (no `spawn_blocking`); a token typed into a prompt is stored.

- [ ] **Step 7: Commit** via the commit skill. Paths: the five files above. Message: `feat(assistant): add keychain credential commands`.
