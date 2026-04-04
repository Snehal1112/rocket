# OpenCollection YAML Compliance — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make all Rocket-produced YAML files spec-compliant with OpenCollection 1.0.0, enabling Bruno↔Rocket workspace interchange.

**Architecture:** Three independent fixes — a one-liner version string fix, a serialization layer swap for environments (using existing `OcEnvironment` conversions), and a new `OcWorkspaceConfig` serde struct with dual-read fallback. No domain crate changes; all changes are in `rocket-infra`.

**Spec:** `docs/superpowers/specs/2026-04-04-opencollection-yaml-compliance-design.md`

**Tech Stack:** Rust, serde_yaml, `rocket-infra` crate, `tempfile` for tests

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-infra/src/oc_conversions.rs` | Fix `"0.1"` → `"1.0.0"` in `collection_to_oc_collection`; add `From` impls for `WorkspaceConfig ↔ OcWorkspaceConfig` and `CollectionReference ↔ OcWorkspaceCollectionRef` |
| `crates/rocket-infra/src/opencollection.rs` | Add `OcWorkspaceInfo`, `OcWorkspaceCollectionRef`, `OcWorkspaceEnvironments`, `OcWorkspaceConfig` structs |
| `crates/rocket-infra/src/fs_environment_repo.rs` | Save via `OcEnvironment`; dual-read in `list`/`get` |
| `crates/rocket-infra/src/fs_workspace_config_repo.rs` | Save via `OcWorkspaceConfig`; dual-read in `load` |

---

## Task 1: Fix opencollection.yml version string

**Files:**
- Modify: `crates/rocket-infra/src/oc_conversions.rs` (around line 1300)
- Test: `crates/rocket-infra/src/oc_conversions.rs` (existing tests section)

- [ ] **Step 1: Write the failing test**

  Add to the `#[cfg(test)]` module at the bottom of `oc_conversions.rs`:

  ```rust
  #[test]
  fn collection_to_oc_has_correct_version() {
      use rocket_collection::Collection;
      use rocket_collection::Folder;
      use rocket_collection::CollectionSettings;
      let col = Collection {
          name: "Test".into(),
          root: Folder { uid: "uid".into(), name: "Test".into(), items: vec![] },
          settings: CollectionSettings::default(),
      };
      let oc = super::collection_to_oc_collection(col);
      assert_eq!(oc.opencollection.as_deref(), Some("1.0.0"));
  }
  ```

- [ ] **Step 2: Run test to verify it fails**

  ```bash
  cd /home/numericlabs/data/rocket/rocket
  cargo test -p rocket-infra collection_to_oc_has_correct_version 2>&1 | tail -20
  ```
  Expected: FAIL — assertion `Some("0.1") == Some("1.0.0")` fails.

- [ ] **Step 3: Fix the version string**

  In `crates/rocket-infra/src/oc_conversions.rs`, find the line:
  ```rust
          opencollection: Some("0.1".into()),
  ```
  Change it to:
  ```rust
          opencollection: Some("1.0.0".into()),
  ```

- [ ] **Step 4: Run test to verify it passes**

  ```bash
  cargo test -p rocket-infra collection_to_oc_has_correct_version 2>&1 | tail -10
  ```
  Expected: PASS

- [ ] **Step 5: Run all infra tests**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -20
  ```
  Expected: all pass

- [ ] **Step 6: Commit**

  ```bash
  git add crates/rocket-infra/src/oc_conversions.rs
  git commit -m "fix: opencollection.yml version string 0.1 → 1.0.0"
  ```

---

## Task 2: Fix environment file serialization

**Files:**
- Modify: `crates/rocket-infra/src/fs_environment_repo.rs`
- Test: same file (existing `mod tests` block)

- [ ] **Step 1: Write the failing tests**

  Add to the `mod tests` block in `fs_environment_repo.rs`:

  ```rust
  #[test]
  fn save_writes_spec_field_names() {
      let (_dir, repo) = setup();
      let mut env = Environment::new("prod");
      let mut var = Variable::new("BASE_URL", "https://api.example.com");
      // enabled by default
      env.set_variable(var.clone());
      var = Variable::new("DISABLED_VAR", "x");
      var.enabled = false;
      env.set_variable(var);
      repo.save(&env).unwrap();

      // Read raw YAML to confirm field names
      let dir = repo.dir.clone();
      let raw = std::fs::read_to_string(dir.join("prod.yml")).unwrap();
      assert!(raw.contains("name: BASE_URL"), "expected 'name:' field, got:\n{raw}");
      assert!(!raw.contains("key:"), "should not contain 'key:' field");
      assert!(raw.contains("disabled: true"), "expected 'disabled: true'");
      assert!(!raw.contains("enabled:"), "should not contain 'enabled:' field");
  }

  #[test]
  fn save_then_load_roundtrip_via_oc_format() {
      let (_dir, repo) = setup();
      let mut env = Environment::new("staging");
      env.set_variable(Variable::new("HOST", "staging.example.com"));
      repo.save(&env).unwrap();
      let loaded = repo.get("staging").unwrap();
      assert_eq!(loaded.get_value("HOST"), Some("staging.example.com"));
  }

  #[test]
  fn load_old_format_with_key_field_still_works() {
      // Backward-compat: old files used `key:` and `enabled:` field names
      let (dir, repo) = setup();
      let old_yaml = "name: legacy\nvariables:\n- key: OLD_VAR\n  value: hello\n  enabled: true\n";
      std::fs::write(dir.path().join("legacy.yml"), old_yaml).unwrap();
      let env = repo.get("legacy").unwrap();
      assert_eq!(env.get_value("OLD_VAR"), Some("hello"));
  }
  ```

  **Note:** This requires `repo.dir` to be accessible. If `dir` is private, expose it as `pub(crate)` or use a helper that returns the dir path for tests. Check the struct — if `dir: PathBuf` needs to be `pub(crate)`, add the visibility modifier.

- [ ] **Step 2: Run tests to verify they fail**

  ```bash
  cargo test -p rocket-infra save_writes_spec_field_names 2>&1 | tail -20
  ```
  Expected: FAIL — output YAML has `key:` not `name:`.

- [ ] **Step 3: Import OcEnvironment in fs_environment_repo.rs**

  Add to the top of `crates/rocket-infra/src/fs_environment_repo.rs`:
  ```rust
  use crate::opencollection::OcEnvironment;
  ```

- [ ] **Step 4: Update `save()` to serialize via OcEnvironment**

  Replace the `save` method body:
  ```rust
  fn save(&self, env: &Environment) -> DomainResult<()> {
      fs::create_dir_all(&self.dir)?;
      let oc: OcEnvironment = env.clone().into();
      let yaml = serde_yaml::to_string(&oc)
          .map_err(|e| DomainError::Internal(format!("Failed to serialize environment: {e}")))?;
      fs::write(self.file_path(&env.name), yaml)?;
      Ok(())
  }
  ```

- [ ] **Step 5: Update `list()` to dual-read**

  Replace the line in `list()` that reads environments:
  ```rust
  // Old:
  if let Ok(env) = serde_yaml::from_str::<Environment>(&content) {
      result.push(env);
  }
  ```
  With:
  ```rust
  // New: try OcEnvironment first (spec format), fall back to domain Environment (legacy)
  if let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) {
      result.push(Environment::from(oc));
  } else if let Ok(env) = serde_yaml::from_str::<Environment>(&content) {
      result.push(env);
  }
  ```

- [ ] **Step 6: Update `get()` to dual-read**

  Replace the deserialization line in `get()`:
  ```rust
  // Old:
  let env: Environment = serde_yaml::from_str(&content)
      .map_err(|e| DomainError::Internal(format!("Failed to parse environment YAML: {e}")))?;
  Ok(env)
  ```
  With:
  ```rust
  // New: try OcEnvironment first (spec format), fall back to domain Environment (legacy)
  if let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) {
      return Ok(Environment::from(oc));
  }
  serde_yaml::from_str::<Environment>(&content)
      .map_err(|e| DomainError::Internal(format!("Failed to parse environment YAML: {e}")))
  ```

- [ ] **Step 7: Make `dir` field pub(crate) for tests (if needed)**

  In `FsEnvironmentRepo`, change:
  ```rust
  pub struct FsEnvironmentRepo {
      dir: PathBuf,
  }
  ```
  To:
  ```rust
  pub struct FsEnvironmentRepo {
      pub(crate) dir: PathBuf,
  }
  ```

- [ ] **Step 8: Run tests to verify they pass**

  ```bash
  cargo test -p rocket-infra -- fs_environment 2>&1 | tail -20
  ```
  Expected: all environment repo tests pass, including the three new ones.

- [ ] **Step 9: Run all infra tests**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -20
  ```
  Expected: all pass

- [ ] **Step 10: Commit**

  ```bash
  git add crates/rocket-infra/src/fs_environment_repo.rs
  git commit -m "fix: environment files use spec field names (name/disabled) via OcEnvironment"
  ```

---

## Task 3: Add OcWorkspaceConfig serde structs

**Files:**
- Modify: `crates/rocket-infra/src/opencollection.rs` (add after `OcCollection` struct, around line 970)

- [ ] **Step 1: Add structs to opencollection.rs**

  Add after the closing brace of `OcCollection` (after the `pub bundled:` field block and its closing brace), before the `#[cfg(test)]` block:

  ```rust
  // ============================================================
  // Workspace file format (workspace.yml) — Bruno-compatible extension
  // ============================================================

  /// workspace.yml — info block.
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct OcWorkspaceInfo {
      pub name: String,
      #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
      pub workspace_type: Option<String>,
  }

  /// workspace.yml — single collection entry.
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct OcWorkspaceCollectionRef {
      pub name: String,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub path: Option<std::path::PathBuf>,
  }

  /// workspace.yml — environments block (Rocket extension).
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct OcWorkspaceEnvironments {
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub active_environment: Option<String>,
  }

  /// Top-level workspace.yml document.
  /// Follows Bruno's OpenCollection workspace extension.
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct OcWorkspaceConfig {
      /// Spec version — always "1.0.0" when written by Rocket.
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub opencollection: Option<String>,
      /// Required: workspace name and type.
      pub info: OcWorkspaceInfo,
      #[serde(default, skip_serializing_if = "Vec::is_empty")]
      pub collections: Vec<OcWorkspaceCollectionRef>,
      /// Human-readable description (spec field name is `docs`).
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub docs: Option<String>,
      /// Active environment selection (Rocket extension).
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub environments: Option<OcWorkspaceEnvironments>,
      /// Global environment override (Rocket extension).
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub global_environment: Option<String>,
  }
  ```

- [ ] **Step 2: Verify it compiles**

  ```bash
  cargo check -p rocket-infra 2>&1 | tail -20
  ```
  Expected: no errors

- [ ] **Step 3: Add YAML roundtrip test in opencollection.rs test module**

  Add inside `#[cfg(test)] mod tests { ... }`:

  ```rust
  #[test]
  fn oc_workspace_config_roundtrip() {
      let yaml = r#"
  opencollection: "1.0.0"
  info:
    name: Acme API
    type: workspace
  collections:
  - name: main-api
    path: collections/main-api
  - name: external
    path: /abs/path/to/ext
  docs: Project description
  environments:
    activeEnvironment: Staging
  globalEnvironment: Production
  "#;
      let cfg: OcWorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
      assert_eq!(cfg.info.name, "Acme API");
      assert_eq!(cfg.collections.len(), 2);
      assert_eq!(cfg.collections[0].name, "main-api");
      assert_eq!(cfg.docs.as_deref(), Some("Project description"));
      assert_eq!(cfg.environments.as_ref().unwrap().active_environment.as_deref(), Some("Staging"));
      assert_eq!(cfg.global_environment.as_deref(), Some("Production"));
      // Roundtrip
      let back: OcWorkspaceConfig = serde_yaml::from_str(&serde_yaml::to_string(&cfg).unwrap()).unwrap();
      assert_eq!(cfg, back);
  }

  #[test]
  fn oc_workspace_config_requires_info_field() {
      // Old format (no `info:` key) must fail to parse as OcWorkspaceConfig
      let old_yaml = "name: My Workspace\ncollections: []\n";
      let result = serde_yaml::from_str::<OcWorkspaceConfig>(old_yaml);
      assert!(result.is_err(), "old format should not parse as OcWorkspaceConfig");
  }
  ```

- [ ] **Step 4: Run tests**

  ```bash
  cargo test -p rocket-infra oc_workspace_config 2>&1 | tail -10
  ```
  Expected: both tests pass

- [ ] **Step 5: Commit**

  ```bash
  git add crates/rocket-infra/src/opencollection.rs
  git commit -m "feat: add OcWorkspaceConfig serde structs for spec-compliant workspace.yml"
  ```

---

## Task 4: Add WorkspaceConfig ↔ OcWorkspaceConfig conversions

**Files:**
- Modify: `crates/rocket-infra/src/oc_conversions.rs` (add after the `OcEnvironment ↔ Environment` block)

- [ ] **Step 1: Write the failing tests**

  Add to the test module at the bottom of `oc_conversions.rs`:

  ```rust
  #[cfg(test)]
  mod workspace_conversion_tests {
      use super::*;
      use rocket_workspace::{WorkspaceConfig, CollectionRefType};
      use std::path::PathBuf;

      #[test]
      fn workspace_config_to_oc_workspace_config() {
          let mut cfg = WorkspaceConfig::new("My API");
          cfg.description = Some("A great API".into());
          cfg.add_embedded_collection("users");
          cfg.add_external_collection("shared", PathBuf::from("/abs/path/shared"));
          cfg.environments.active_environment = Some("Production".into());
          cfg.global_environment = Some("Prod Global".into());

          let oc = OcWorkspaceConfig::from(cfg);
          assert_eq!(oc.opencollection.as_deref(), Some("1.0.0"));
          assert_eq!(oc.info.name, "My API");
          assert_eq!(oc.info.workspace_type.as_deref(), Some("workspace"));
          assert_eq!(oc.docs.as_deref(), Some("A great API"));
          assert_eq!(oc.collections.len(), 2);
          // Embedded → relative path collections/<name>
          assert_eq!(oc.collections[0].path, Some(PathBuf::from("collections/users")));
          // External → absolute path preserved
          assert_eq!(oc.collections[1].path, Some(PathBuf::from("/abs/path/shared")));
          assert_eq!(oc.environments.as_ref().unwrap().active_environment.as_deref(), Some("Production"));
          assert_eq!(oc.global_environment.as_deref(), Some("Prod Global"));
      }

      #[test]
      fn oc_workspace_config_to_workspace_config() {
          // OcWorkspace* types via `super::*` (glob from opencollection.rs)
          let oc = OcWorkspaceConfig {
              opencollection: Some("1.0.0".into()),
              info: OcWorkspaceInfo { name: "Acme".into(), workspace_type: Some("workspace".into()) },
              collections: vec![
                  OcWorkspaceCollectionRef { name: "api".into(), path: Some(PathBuf::from("collections/api")) },
                  OcWorkspaceCollectionRef { name: "ext".into(), path: Some(PathBuf::from("/abs/ext")) },
              ],
              docs: Some("Docs here".into()),
              environments: Some(OcWorkspaceEnvironments { active_environment: Some("Staging".into()) }),
              global_environment: Some("Global".into()),
          };
          let cfg = WorkspaceConfig::from(oc);
          assert_eq!(cfg.name, "Acme");
          assert_eq!(cfg.description.as_deref(), Some("Docs here"));
          assert_eq!(cfg.collections.len(), 2);
          // Relative path → Embedded
          assert_eq!(cfg.collections[0].ref_type, CollectionRefType::Embedded);
          // Absolute path → External
          assert_eq!(cfg.collections[1].ref_type, CollectionRefType::External);
          assert_eq!(cfg.environments.active_environment.as_deref(), Some("Staging"));
          assert_eq!(cfg.global_environment.as_deref(), Some("Global"));
      }
  }
  ```

- [ ] **Step 2: Run tests to verify they fail (because impls don't exist yet)**

  ```bash
  cargo test -p rocket-infra workspace_conversion_tests 2>&1 | tail -10
  ```
  Expected: FAIL — compile error "no impl of From<WorkspaceConfig> for OcWorkspaceConfig"

- [ ] **Step 3: Add the imports and From impls to oc_conversions.rs**

  Add after the `impl From<Environment> for OcEnvironment { ... }` block.

  The `crate::opencollection::*` glob already covers the `OcWorkspace*` structs. Add only the `rocket_workspace` domain type imports at the top of the additions:

  ```rust
  // ============================================================
  // WorkspaceConfig ↔ OcWorkspaceConfig conversions
  // ============================================================

  use rocket_workspace::{WorkspaceConfig, WorkspaceEnvironmentsConfig, CollectionReference, CollectionRefType};

  impl From<OcWorkspaceCollectionRef> for CollectionReference {
      fn from(r: OcWorkspaceCollectionRef) -> Self {
          match r.path {
              Some(p) if p.is_absolute() => CollectionReference {
                  name: r.name,
                  ref_type: CollectionRefType::External,
                  path: Some(p),
              },
              _ => CollectionReference {
                  name: r.name,
                  ref_type: CollectionRefType::Embedded,
                  path: None,
              },
          }
      }
  }

  impl From<CollectionReference> for OcWorkspaceCollectionRef {
      fn from(r: CollectionReference) -> Self {
          OcWorkspaceCollectionRef {
              path: match r.ref_type {
                  CollectionRefType::Embedded => {
                      Some(std::path::PathBuf::from(format!("collections/{}", r.name)))
                  }
                  CollectionRefType::External => r.path,
              },
              name: r.name,
          }
      }
  }

  impl From<OcWorkspaceConfig> for WorkspaceConfig {
      fn from(oc: OcWorkspaceConfig) -> Self {
          WorkspaceConfig {
              name: oc.info.name,
              description: oc.docs,
              collections: oc.collections.into_iter().map(CollectionReference::from).collect(),
              environments: WorkspaceEnvironmentsConfig {
                  active_environment: oc.environments.and_then(|e| e.active_environment),
              },
              global_environment: oc.global_environment,
          }
      }
  }

  impl From<WorkspaceConfig> for OcWorkspaceConfig {
      fn from(w: WorkspaceConfig) -> Self {
          let has_active_env = w.environments.active_environment.is_some();
          OcWorkspaceConfig {
              opencollection: Some("1.0.0".into()),
              info: OcWorkspaceInfo {
                  name: w.name,
                  workspace_type: Some("workspace".into()),
              },
              collections: w.collections.into_iter().map(OcWorkspaceCollectionRef::from).collect(),
              docs: w.description,
              environments: if has_active_env {
                  Some(OcWorkspaceEnvironments {
                      active_environment: w.environments.active_environment,
                  })
              } else {
                  None
              },
              global_environment: w.global_environment,
          }
      }
  }
  ```

  **Note:** `rocket-workspace` is already in `rocket-infra/Cargo.toml`.

- [ ] **Step 4: Run tests to verify they pass**

  ```bash
  cargo test -p rocket-infra workspace_conversion_tests 2>&1 | tail -20
  ```
  Expected: both tests pass

- [ ] **Step 5: Run all infra tests**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -20
  ```
  Expected: all pass

- [ ] **Step 6: Commit**

  ```bash
  git add crates/rocket-infra/src/oc_conversions.rs
  git commit -m "feat: add WorkspaceConfig <-> OcWorkspaceConfig conversions"
  ```

---

## Task 5: Update FsWorkspaceConfigRepo to use new format

**Files:**
- Modify: `crates/rocket-infra/src/fs_workspace_config_repo.rs`

- [ ] **Step 1: Write the failing tests**

  Add to the `mod tests` block in `fs_workspace_config_repo.rs`:

  ```rust
  #[test]
  fn save_writes_new_format() {
      let tmp = TempDir::new().unwrap();
      let ws_path = tmp.path().join("my-api");
      let repo = FsWorkspaceConfigRepo::new();
      let mut cfg = WorkspaceConfig::new("My API");
      cfg.description = Some("Great docs".into());
      repo.save(&ws_path, &cfg).unwrap();

      let raw = fs::read_to_string(ws_path.join("workspace.yml")).unwrap();
      assert!(raw.contains("opencollection:"), "missing opencollection header:\n{raw}");
      assert!(raw.contains("info:"), "missing info block:\n{raw}");
      assert!(raw.contains("name: My API") || raw.contains("name: \"My API\""),
          "missing info.name:\n{raw}");
      assert!(raw.contains("docs:"), "missing docs field:\n{raw}");
      assert!(!raw.contains("\nname:"), "unexpected root-level name:\n{raw}");
      assert!(!raw.contains("description:"), "description field should be renamed to docs:\n{raw}");
  }

  #[test]
  fn load_new_format_roundtrip() {
      let tmp = TempDir::new().unwrap();
      let ws_path = tmp.path().join("ws");
      let repo = FsWorkspaceConfigRepo::new();
      let mut cfg = WorkspaceConfig::new("Roundtrip WS");
      cfg.description = Some("My desc".into());
      cfg.add_embedded_collection("my-api");
      repo.save(&ws_path, &cfg).unwrap();

      let loaded = repo.load(&ws_path).unwrap();
      assert_eq!(loaded.name, "Roundtrip WS");
      assert_eq!(loaded.description.as_deref(), Some("My desc"));
      assert_eq!(loaded.collections.len(), 1);
      assert_eq!(loaded.collections[0].name, "my-api");
  }

  #[test]
  fn load_old_format_backward_compat() {
      let tmp = TempDir::new().unwrap();
      let ws_path = tmp.path().join("old-ws");
      fs::create_dir_all(&ws_path).unwrap();
      // Old flat format
      let old_yaml = "name: Old Workspace\ndescription: Legacy\ncollections:\n- name: users\n  type: embedded\n";
      fs::write(ws_path.join("workspace.yml"), old_yaml).unwrap();

      let repo = FsWorkspaceConfigRepo::new();
      let cfg = repo.load(&ws_path).unwrap();
      assert_eq!(cfg.name, "Old Workspace");
      assert_eq!(cfg.description.as_deref(), Some("Legacy"));
      assert_eq!(cfg.collections.len(), 1);
  }
  ```

- [ ] **Step 2: Run tests to verify they fail**

  ```bash
  cargo test -p rocket-infra fs_workspace_config 2>&1 | tail -20
  ```
  Expected: `save_writes_new_format` FAIL — sees `name: Old Workspace` not `opencollection:` header.

- [ ] **Step 3: Add imports to fs_workspace_config_repo.rs**

  Add at the top of the file after the existing `use` statements:
  ```rust
  use crate::opencollection::OcWorkspaceConfig;
  ```

- [ ] **Step 4: Update `save()` to serialize as OcWorkspaceConfig**

  Replace the `save` method:
  ```rust
  fn save(&self, workspace_path: &Path, config: &WorkspaceConfig) -> DomainResult<()> {
      fs::create_dir_all(workspace_path).map_err(|e| {
          DomainError::Io(format!("Failed to create workspace directory: {e}"))
      })?;

      let config_path = workspace_path.join("workspace.yml");
      let oc = OcWorkspaceConfig::from(config.clone());
      let content = serde_yaml::to_string(&oc).map_err(|e| {
          DomainError::InvalidInput(format!("Failed to serialize workspace.yml: {e}"))
      })?;

      fs::write(&config_path, content).map_err(|e| {
          DomainError::Io(format!("Failed to write workspace.yml: {e}"))
      })
  }
  ```

- [ ] **Step 5: Update `load()` to dual-read**

  Replace the `serde_yaml::from_str` call inside `load()`:
  ```rust
  // Old:
  serde_yaml::from_str(&content).map_err(|e| {
      DomainError::InvalidInput(format!("Failed to parse workspace.yml: {e}"))
  })
  ```
  With:
  ```rust
  // Try new format (has info.name block)
  if let Ok(oc) = serde_yaml::from_str::<OcWorkspaceConfig>(&content) {
      return Ok(WorkspaceConfig::from(oc));
  }
  // Fall back to old format (flat name: at root)
  serde_yaml::from_str::<WorkspaceConfig>(&content).map_err(|e| {
      DomainError::InvalidInput(format!("Failed to parse workspace.yml: {e}"))
  })
  ```

- [ ] **Step 6: Run tests to verify they pass**

  ```bash
  cargo test -p rocket-infra fs_workspace_config 2>&1 | tail -20
  ```
  Expected: all workspace config repo tests pass, including the three new ones.

- [ ] **Step 7: Run all infra tests**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -20
  ```
  Expected: all pass

- [ ] **Step 8: Cargo check the whole workspace**

  ```bash
  cargo check 2>&1 | tail -20
  ```
  Expected: no errors

- [ ] **Step 9: Commit**

  ```bash
  git add crates/rocket-infra/src/fs_workspace_config_repo.rs
  git commit -m "fix: workspace.yml uses spec-compliant format (info.name, docs, no type discriminant)"
  ```

---

## Final Verification

- [ ] **Run all Rust tests**

  ```bash
  cargo test 2>&1 | tail -30
  ```
  Expected: all pass, no regressions

- [ ] **TypeScript / frontend check (unchanged by this work, just confirm)**

  ```bash
  yarn tsc --noEmit && yarn check
  ```
  Expected: 0 errors, 0 warnings

- [ ] **Smoke test: create a workspace, verify workspace.yml**

  ```bash
  cargo test -p rocket-infra save_writes_new_format -- --nocapture 2>&1 | tail -20
  ```

- [ ] **Final commit summary**

  At this point these are the committed changes:
  1. `fix: opencollection.yml version string 0.1 → 1.0.0`
  2. `fix: environment files use spec field names (name/disabled) via OcEnvironment`
  3. `feat: add OcWorkspaceConfig serde structs for spec-compliant workspace.yml`
  4. `feat: add WorkspaceConfig <-> OcWorkspaceConfig conversions`
  5. `fix: workspace.yml uses spec-compliant format (info.name, docs, no type discriminant)`
