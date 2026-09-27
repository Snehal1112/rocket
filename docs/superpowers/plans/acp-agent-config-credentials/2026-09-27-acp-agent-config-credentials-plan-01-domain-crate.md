# ACP Agent Config Plan 01: Domain Crate — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the new `rocket-acp` crate holding `AgentConfig` (the config
shape for one registered ACP agent binary + its RocketVault credential
reference) and `AgentConfigRepository` (the persistence boundary trait).

**Architecture:** A pure domain crate — no I/O, no network — modeled directly
on `rocket-environment`'s `secret_manager.rs`: one plain serde struct plus one
trait for the persistence boundary. `rocket-infra` (Plan 02) implements the
trait; `rocket-app` (Plan 03) orchestrates it.

**Tech Stack:** Rust, serde, serde_json (tests).

**Spec:** `docs/superpowers/specs/2026-09-27-acp-agent-config-credentials-design.md`
(Domain Model section). Plan index:
`docs/superpowers/plans/acp-agent-config-credentials/00-plan-index.md` (has
the full locked interface contract every later plan in this series depends
on).

## Global Constraints

- `AgentConfig` does **not** get `#[serde(rename_all = "camelCase")]` — it
  persists to its own app-level `agent_configs.yml` (Plan 02), not the
  OpenCollection format, so this repo's general "no camelCase on persistence
  structs" rule applies, exactly like `SecretManagerConnection`
  (`crates/rocket-environment/src/secret_manager.rs:13`).
- `rocket-acp` depends only on `rocket-shared`, `serde`, and `serde_json` — no
  cross-domain-crate imports. Other entities (a RocketVault connection, a
  vault secret) are referenced by plain `String` id, the same loose-coupling
  convention `rocket-environment` and the other domain crates already use.
- `args: Vec<String>` and `working_dir: Option<String>` both need
  `#[serde(default)]` so a minimal hand-written YAML/JSON fixture without
  those fields still deserializes — this crate has no legacy files to be
  backward-compatible with yet, but every other domain crate in this repo
  applies this rule to `Vec`/`Option` fields as standard practice, and Plan 02
  and Plan 04's DTO both build on this.
- Test code uses `.expect("message")` for fallible calls, never the bare
  panicking shorthand, matching this repo's stricter Rust safety convention
  even in test paths.

## Review Focus

- Blank/whitespace-only required string fields (`label`, `command`,
  `credential_env_var`, `vault_secret_id`) are not rejected by this crate —
  that validation lives in `AgentConfigService` (Plan 03), not here. No test
  in this plan should expect construction itself to reject them; a reviewer
  should not flag their absence as a gap in this plan.
- `args: Vec<String>` empty-vs-populated must round-trip through serde without
  the field disappearing when empty.
- `working_dir: Option<String>` absent from the input JSON must deserialize to
  `None`, not error — confirm this with an explicit test rather than assuming
  serde's default `Option` behavior.
- Two `AgentConfig`s with different `id` but the same `label` must not be
  conflated by `AgentConfigRepository::save` — id is the only identity key.
- `AgentConfigRepository` must be object-safe (`Box<dyn AgentConfigRepository>`
  compiles) since `AgentConfigService` (Plan 03) holds it as a trait object —
  a non-object-safe trait here would silently break Plan 03 later rather than
  failing fast in this plan.

---

## Task 1: Crate scaffold + `AgentConfig` struct

**Files:**
- Create: `crates/rocket-acp/Cargo.toml`
- Create: `crates/rocket-acp/src/lib.rs`
- Create: `crates/rocket-acp/src/agent_config.rs`
- Modify: `Cargo.toml` (root workspace manifest)

**Interfaces:**
- Produces: `AgentConfig { id, label, command, args, working_dir,
  credential_env_var, vault_connection_id, vault_name, vault_secret_id,
  vault_secret_name }` — consumed by Task 2 of this plan and every later plan
  in this series.

- [ ] **Step 1: Scaffold the crate and register it in the workspace**

Create `crates/rocket-acp/Cargo.toml`:

```toml
[package]
name = "rocket-acp"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared.workspace = true
serde.workspace = true
serde_json.workspace = true
```

Create `crates/rocket-acp/src/lib.rs`:

```rust
pub mod agent_config;
```

In the root `Cargo.toml`, add `"crates/rocket-acp",` to the `[workspace]`
`members` list (alongside the existing `"crates/rocket-scripting",` entry —
add it right after that line), and add `rocket-acp = { path =
"crates/rocket-acp" }` to the `[workspace.dependencies]` internal-crates
section (alongside `rocket-scripting = { path = "crates/rocket-scripting" }`
— add it right after that line).

Run: `cargo check -p rocket-acp -j4`
Expected: succeeds (empty crate, compiles with just the empty `agent_config`
module — create an empty `crates/rocket-acp/src/agent_config.rs` file first
if the compiler complains about the missing module file).

- [ ] **Step 2: Write the failing tests**

```rust
// crates/rocket-acp/src/agent_config.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AgentConfig {
        AgentConfig {
            id: "agent-1".to_string(),
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
        let back: AgentConfig = serde_json::from_str(&json).expect("deserialize AgentConfig");
        assert!(back.args.is_empty());
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
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rocket-acp agent_config::tests -j4`
Expected: FAIL with "cannot find struct `AgentConfig`" (compile error — the
type doesn't exist yet).

- [ ] **Step 4: Implement the struct**

```rust
// crates/rocket-acp/src/agent_config.rs (add above the tests module)
use serde::{Deserialize, Serialize};

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
    pub vault_connection_id: String,
    pub vault_name: String,
    pub vault_secret_id: String,
    pub vault_secret_name: String,
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-acp agent_config::tests -j4`
Expected: PASS — 3 tests.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/rocket-acp
git commit -m "feat(acp): add rocket-acp crate with AgentConfig"
```

---

## Task 2: `AgentConfigRepository` trait + crate docs

**Files:**
- Modify: `crates/rocket-acp/src/agent_config.rs`
- Modify: `crates/rocket-acp/src/lib.rs`
- Create: `crates/rocket-acp/CLAUDE.md`

**Interfaces:**
- Consumes: `AgentConfig` from Task 1.
- Produces: `AgentConfigRepository { list, get, save, delete }` — consumed by
  Plan 02 (`FsAgentConfigRepo` impl) and Plan 03 (`AgentConfigService`).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-acp/src/agent_config.rs (add to the existing tests module)
use rocket_shared::error::DomainResult;
use std::sync::Mutex;

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
fn trait_is_object_safe() {
    fn _assert(_: Box<dyn AgentConfigRepository>) {}
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-acp agent_config::tests -j4`
Expected: FAIL with "cannot find trait `AgentConfigRepository`".

- [ ] **Step 3: Implement the trait**

```rust
// crates/rocket-acp/src/agent_config.rs (add above the tests module)
use rocket_shared::error::DomainResult;

/// Persistence boundary for `AgentConfig`. No I/O in this crate —
/// `rocket-infra`'s `FsAgentConfigRepo` (Plan 02) implements this.
pub trait AgentConfigRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<AgentConfig>>;
    fn get(&self, id: &str) -> DomainResult<Option<AgentConfig>>;
    fn save(&self, config: &AgentConfig) -> DomainResult<()>;
    fn delete(&self, id: &str) -> DomainResult<()>;
}
```

- [ ] **Step 4: Register the module export**

In `crates/rocket-acp/src/lib.rs`:

```rust
pub mod agent_config;
pub use agent_config::{AgentConfig, AgentConfigRepository};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-acp -j4`
Expected: PASS — 6 tests total (3 from Task 1, 3 from this task).

- [ ] **Step 6: Write the crate CLAUDE.md**

Create `crates/rocket-acp/CLAUDE.md`, mirroring the structure of
`crates/rocket-environment/CLAUDE.md`:

```markdown
# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

The `rocket-acp` crate is a pure domain crate in the Rocket HTTP client
workspace. It owns the `AgentConfig` entity (a registered ACP agent
binary/command and a reference to where RocketVault holds its API key) and
the `AgentConfigRepository` trait. It has no I/O — the filesystem
implementation lives in `rocket-infra` (`FsAgentConfigRepo`).

## Commands

\`\`\`bash
# Check this crate
cargo check -p rocket-acp -j4

# Run all tests in this crate
cargo test -p rocket-acp -j4
\`\`\`

## Architecture

### Module Map

| Module | Responsibility |
|---|---|
| `agent_config.rs` | `AgentConfig` struct + `AgentConfigRepository` trait |

### Key Design Points

- `AgentConfig` never holds a credential *value* — only
  `vault_connection_id`/`vault_name`/`vault_secret_id`, a reference resolved
  on demand by `rocket-app`'s `AgentConfigService` through the existing
  RocketVault `SecretManagerService`/`VaultSecretFetcher` machinery.
- No cross-domain-crate dependencies — other entities are referenced by plain
  `String` id, not by importing another domain crate's types.
- Plain (non-camelCase) field names — this struct persists to its own
  app-level `agent_configs.yml`, not the OpenCollection format.

### Dependencies

- `rocket-shared` — `DomainResult`
- `serde` / `serde_json` — serialization
```

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-acp
git commit -m "feat(acp): add AgentConfigRepository trait"
```

---

## Next Plan

[Plan 02: FsAgentConfigRepo persistence](2026-09-27-acp-agent-config-credentials-plan-02-persistence.md) —
implements `AgentConfigRepository` on top of a flat YAML file in
`rocket-infra`, mirroring `FsSecretManagerRepo`.

## Post-Implementation Review

Before starting Plan 02, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `Cargo.toml` (root), `crates/rocket-acp/Cargo.toml`,
> `crates/rocket-acp/src/lib.rs`, `crates/rocket-acp/src/agent_config.rs`,
> `crates/rocket-acp/CLAUDE.md`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — does `AgentConfig` and
>    `AgentConfigRepository` match exactly what the plan index's locked
>    interface contract promises Plan 02/03/04 will consume?
> 2. Code quality — naming, doc comments, test coverage versus this plan's
>    Review Focus section (blank-field validation deliberately absent here;
>    empty-`args`/absent-`working_dir` roundtrip; per-id save semantics;
>    trait object-safety).
> 3. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    specifically that `rocket-acp` has zero cross-domain-crate dependencies
>    and contains no I/O.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-acp -j4` and
> `cargo check -p rocket-acp -j4`, and confirm they still pass. Report what
> you found and fixed.

Only proceed to Plan 02 once this review comes back clean (or its fixes are
applied and re-verified).
