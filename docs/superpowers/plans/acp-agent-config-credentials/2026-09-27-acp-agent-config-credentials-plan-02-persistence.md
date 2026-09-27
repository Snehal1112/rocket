# ACP Agent Config Plan 02: Persistence — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `AgentConfigRepository` as a flat-YAML-file filesystem
repository, `FsAgentConfigRepo`, in `rocket-infra`.

**Architecture:** Byte-for-byte the same pattern as
`crates/rocket-infra/src/fs_secret_manager_repo.rs`: the full `AgentConfig`
list is stored as one YAML file, read fully and rewritten fully on every
mutation via this crate's existing `atomic_write` helper.

**Tech Stack:** Rust, serde_yaml, tempfile (tests).

**Spec:** `docs/superpowers/specs/2026-09-27-acp-agent-config-credentials-design.md`
(Persistence section). Plan index:
`docs/superpowers/plans/acp-agent-config-credentials/00-plan-index.md`.

## Global Constraints

- 📖 Before starting this task, read
  `docs/superpowers/specs/opencollection-spec-reference.md` — `rocket-infra`
  is on this repo's OpenCollection-spec trigger list
  (`.claude/rules/rust-ddd-boundaries.md`), even though `agent_configs.yml`
  has nothing to do with collections or environments; the rule triggers on
  the crate touched, not the content.
- `FsAgentConfigRepo::new` takes the full path to the YAML file directly
  (e.g. `<app_data_dir>/agent_configs.yml`), not a containing directory —
  matching `FsSecretManagerRepo::new`'s documented convention
  (`crates/rocket-infra/src/fs_secret_manager_repo.rs:21-24`), not
  `FsWorkspaceRepo::new`'s (which takes the app data directory and joins the
  filename itself).
- Use this crate's existing `atomic_write(path: &Path, content: &[u8]) ->
  std::io::Result<()>` helper (`crates/rocket-infra/src/atomic_write.rs`) for
  every write — do not write the file directly with `std::fs::write`.
- A missing or zero-byte file is not an error — treat both as an empty list,
  matching `FsSecretManagerRepo::read_all`'s handling exactly.
- Test code uses `.expect("message")` for fallible calls, never the bare
  panicking shorthand.

## Review Focus

- Loading a zero-byte file left behind by an interrupted first write (not
  just a fully-missing file) must not error — `FsSecretManagerRepo` treats
  this case explicitly; `FsAgentConfigRepo` must too.
- `save` on an existing id must replace that entry in place, not append a
  duplicate — this is the one behavior most likely to silently corrupt the
  file if implemented naively as "always push and write."
- `delete` of an id that was never saved must be a no-op, not an error — a
  caller retrying a delete after a partial failure should not get a new,
  different error on the second attempt.
- Malformed/corrupt YAML in `agent_configs.yml` (e.g. hand-edited by a user)
  must surface as a clear `DomainError::InvalidInput`, not panic the whole
  app on startup — this crate's read path must handle a parse failure the
  same way `FsSecretManagerRepo::read_all` does.
- The persisted YAML must never contain anything that looks like a raw
  credential value — since `AgentConfig` has no field capable of holding one
  (only `vault_secret_id`/`vault_secret_name`, references, not values), this
  is a compile-time guarantee; write a defensive regression test anyway, the
  same way `fs_secret_manager_repo.rs` does for `client_secret`, so a future
  edit that accidentally added a raw-value field to `AgentConfig` would be
  caught here.

---

## Task 1: `FsAgentConfigRepo`

**Files:**
- Create: `crates/rocket-infra/src/fs_agent_config_repo.rs`
- Modify: `crates/rocket-infra/src/lib.rs`
- Modify: `crates/rocket-infra/Cargo.toml`

**Interfaces:**
- Consumes: `AgentConfig`, `AgentConfigRepository` from `rocket-acp` (Plan 01).
- Produces: `FsAgentConfigRepo::new(path: PathBuf) -> Self` implementing
  `AgentConfigRepository` — consumed by Plan 04's `src-tauri/src/lib.rs`
  wiring.

- [ ] **Step 1: Add the `rocket-acp` dependency**

In `crates/rocket-infra/Cargo.toml`, add `rocket-acp.workspace = true` to
`[dependencies]`, alongside the existing `rocket-environment.workspace =
true` line.

Run: `cargo check -p rocket-infra -j4`
Expected: succeeds (no code uses the new dependency yet, but it must resolve).

- [ ] **Step 2: Write the failing tests**

```rust
// crates/rocket-infra/src/fs_agent_config_repo.rs
use std::fs;
use std::path::PathBuf;

use rocket_acp::{AgentConfig, AgentConfigRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

pub struct FsAgentConfigRepo {
    path: PathBuf,
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
        fs::write(dir.path().join("agent_configs.yml"), b"not: valid: agent: configs: [")
            .expect("write malformed file");
        let err = repo.list().expect_err("malformed YAML must error, not panic");
        assert!(matches!(err, DomainError::InvalidInput(_)));
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

        let raw = fs::read_to_string(dir.path().join("agent_configs.yml"))
            .expect("read persisted file");
        for forbidden in ["credential_value", "api_key_value", "secret_value"] {
            assert!(
                !raw.contains(forbidden),
                "agent_configs.yml must never contain a raw credential field: {raw}"
            );
        }
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rocket-infra fs_agent_config_repo -j4`
Expected: FAIL — `FsAgentConfigRepo::new` and the trait methods don't exist
yet (compile error).

- [ ] **Step 4: Implement the repository**

```rust
// crates/rocket-infra/src/fs_agent_config_repo.rs (add above the tests module,
// replacing the bare `pub struct FsAgentConfigRepo { path: PathBuf }` stub)

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
```

- [ ] **Step 5: Register the module and add the `serde_yaml`/`tempfile`
  imports it needs**

In `crates/rocket-infra/src/lib.rs`, add alongside the existing module
declarations:

```rust
pub mod fs_agent_config_repo;
pub use fs_agent_config_repo::FsAgentConfigRepo;
```

`serde_yaml` is already a `rocket-infra` dependency (see its `Cargo.toml`);
`tempfile` is already a `[dev-dependencies]` entry — no further manifest
changes are needed beyond Step 1's `rocket-acp` addition.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-infra fs_agent_config_repo -j4`
Expected: PASS — 9 tests.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-infra
git commit -m "feat(infra): add FsAgentConfigRepo"
```

---

## Next Plan

[Plan 03: AgentConfigService + credential resolution](2026-09-27-acp-agent-config-credentials-plan-03-app-service.md) —
adds `SecretManagerService::resolve_secret_value` and the new
`AgentConfigService` in `rocket-app`, consuming `FsAgentConfigRepo` from this
plan.

## Post-Implementation Review

Before starting Plan 03, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `crates/rocket-infra/Cargo.toml`, `crates/rocket-infra/src/lib.rs`,
> `crates/rocket-infra/src/fs_agent_config_repo.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — does `FsAgentConfigRepo`
>    implement `AgentConfigRepository` exactly as the plan index's locked
>    interface contract promises Plan 03/04 will consume, including the
>    `PathBuf`-to-file (not directory) constructor convention?
> 2. Code quality — naming, error handling, and test coverage versus this
>    plan's Review Focus section (zero-byte file, replace-not-duplicate,
>    no-op delete of a missing id, malformed-YAML error path, no raw
>    credential field ever reaching disk).
> 3. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    specifically that all I/O stays in `rocket-infra` and this file only
>    uses the existing `atomic_write` helper, never a direct
>    `std::fs::write`.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-infra fs_agent_config_repo -j4`
> and `cargo check -p rocket-infra -j4`, and confirm they still pass. Report
> what you found and fixed.

Only proceed to Plan 03 once this review comes back clean (or its fixes are
applied and re-verified).
