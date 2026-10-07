# Folder Settings, Plan 03: Script flow setting

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `CollectionSettings.script_flow` is read from and written to `extensions.bruno.scripts.flow` in `opencollection.yml`, and it survives every settings save path (the collection overview tab, the sandbox popover, script-driven collection variable writes). No UI control for the flow is added in this plan; plan 06 consumes the value at run time.

**Architecture:** Two private helpers in `crates/rocket-infra/src/fs_collection/settings.rs`, `script_flow_from_extensions` and `set_script_flow_in_extensions`, mirror the existing `sandbox_mode_from_extensions` and `set_script_roots_in_extensions` technique: walk the free-form `OcCollection.extensions` value, clone only the sub-mappings that are touched, and insert them back so every other key stays in place. Read is tolerant: absent, unknown or wrong-type values mean `Sandwich`. Write touches the value only when the stored flow reads differently from the target, writes `flow: sequential` for `Sequential`, and for `Sandwich` removes a `sequential` value and prunes the `scripts` and `bruno` mappings it emptied. So a collection that never used the setting stays byte-identical. The IPC path needs no new DTO: `get_collection_settings` and `save_collection_settings` already pass the domain `CollectionSettings`, whose camelCase JSON gains `scriptFlow` from plan 01. The frontend adds `scriptFlow?: ScriptFlow` to the `CollectionSettings` TS type and carries it in `buildSettingsForSave`. `SandboxPopover` already spreads the full settings, so it carries the field for free.

**Tech Stack:** Rust (serde, serde_yaml 0.9), Tauri 2 IPC, React + TypeScript, Vitest. Run Rust tests with `cargo test -j4 -p <crate> <name>`.

**Spec:** `docs/superpowers/specs/2026-10-07-folder-settings-design.md` (Decisions: "Script order is sandwich by default. `extensions.bruno.scripts.flow` ... is read, honored and preserved on write"). Locked names: `docs/superpowers/plans/folder-settings/00-plan-index.md`, section "Script flow".

**Depends on:** Plan 01 (`rocket_collection::ScriptFlow { Sandwich (default), Sequential }`, serde lowercase, re-exported at the crate root, and `CollectionSettings.script_flow: ScriptFlow` with `#[serde(default)]`). Plan 02 (folder.yml persistence; this plan touches no file plan 02 owns except an append to `schema_shape_tests.rs`).

## Global Constraints

- `OcCollection` is a persistence struct. Do not add a typed `extensions` struct and never add `#[serde(rename_all = "camelCase")]` to anything in `crates/rocket-infra/src/oc/`. `extensions` stays `Option<serde_yaml::Value>`.
- The only key this plan may write is `extensions.bruno.scripts.flow`, with the value `sequential`. Nothing is written for `Sandwich`. No key is added under `request`, `info` or the root.
- Every key under `extensions` that this plan does not own is kept, including `extensions.rocketapi.sandboxMode`, `extensions.rocketapi.scripts.additionalContextRoots`, unknown `extensions.rocketapi.*` keys, other keys under `extensions.bruno` and `extensions.bruno.scripts`, and other tools' namespaces.
- A stored value that is unknown (`flow: yolo`) or of the wrong type (`flow: 1`) reads as `Sandwich` and is left untouched by a `Sandwich` save.
- No bare `unwrap` calls in production paths. Tests use `.expect("reason")`.
- Always pass `-j4` to cargo, one crate per command. Never run `cargo test --workspace`.
- Commit with the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only (never `git add -A` or `.`), and commit with a pathspec. Peer sessions share this repo's index.
- Frontend: no new UI. Narrow Zustand selectors, shadcn/ui and `lucide-react` rules are unaffected because no component markup changes.

## Review Focus

1. A collection that never set a flow must save without any `bruno` key, so existing and Rocket-created collections stay byte-identical (Task 1 tests `settings_default_save_writes_no_bruno_extension` and `set_script_flow_sandwich_leaves_matching_values_untouched`).
2. Saving `Sequential` must keep every `extensions.rocketapi.*` key and every foreign namespace (Task 1 tests `set_script_flow_sequential_keeps_sibling_keys` and `settings_script_flow_sequential_roundtrips_and_keeps_other_extensions`).
3. Switching back to `Sandwich` removes only `flow` and prunes only the stubs it emptied (Task 1 tests `set_script_flow_sandwich_removes_only_the_flow_key` and `settings_script_flow_back_to_sandwich_removes_the_bruno_stub`).
4. Absent, unknown and wrong-type values read as `Sandwich` without an error (Task 1 test `script_flow_from_extensions_defaults_to_sandwich_for_absent_unknown_or_wrong_type`).
5. Saving from `CollectionOverviewTab` (a full replace) must not reset a stored `sequential` flow (Task 2 test `keeps the script flow from the fresh settings`), and an older frontend payload without `scriptFlow` must still deserialize (Task 2 test `ipc_json_carries_script_flow_and_defaults_when_absent`).
6. The written `opencollection.yml` passes the schema guard and puts the flow nowhere but `extensions.bruno.scripts.flow` (Task 3 test `script_flow_is_written_only_under_bruno_extensions`).

---

## Task 1: Read and write `extensions.bruno.scripts.flow` in rocket-infra

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection/settings.rs` (import on line 4; new helpers after `set_script_roots_in_extensions`, which ends at line 122; `get_settings` lines 137-166; `save_settings` lines 239-241; unit tests module at the end)
- Test: `crates/rocket-infra/src/fs_collection/settings.rs` (`mod tests`), `crates/rocket-infra/src/fs_collection/tests.rs` (append at the end of the file)
- No change: `crates/rocket-infra/src/conversions/folder.rs`. `oc_collection_to_collection` is test-only (`#[allow(dead_code)]`, bundled layout) and already hardcodes `sandbox_mode: SandboxMode::Safe` instead of reading `extensions`; plan 01 gives it the default `script_flow`, and this plan leaves it that way.

**Interfaces:**
- Consumes: `rocket_collection::ScriptFlow` (plan 01), `CollectionSettings.script_flow` (plan 01), `OcCollection.extensions: Option<serde_yaml::Value>` (`crates/rocket-infra/src/oc/collection.rs:91`), `FsCollectionRepo::settings_path(&self, name: &str)` (used by existing tests in `tests.rs`).
- Produces (module-private to `fs_collection::settings`):
  - `fn script_flow_from_extensions(extensions: &Option<serde_yaml::Value>) -> ScriptFlow`
  - `fn set_script_flow_in_extensions(extensions: Option<serde_yaml::Value>, flow: &ScriptFlow) -> Option<serde_yaml::Value>`
  - `get_settings` fills `script_flow`; `save_settings` persists it.

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (section 2.1: `extensions: {}` is the free-form escape hatch; every other object is `additionalProperties: false`).

- [ ] **Step 2: Write the failing unit tests**

Append to the `mod tests` block at the end of `crates/rocket-infra/src/fs_collection/settings.rs` (after `script_roots_from_extensions_ignores_non_string_entries`):

```rust
    fn ext(yaml: &str) -> Option<serde_yaml::Value> {
        Some(serde_yaml::from_str(yaml).expect("parse fixture yaml"))
    }

    #[test]
    fn script_flow_from_extensions_defaults_to_sandwich_for_absent_unknown_or_wrong_type() {
        assert_eq!(script_flow_from_extensions(&None), ScriptFlow::Sandwich);
        for yaml in [
            "rocketapi:\n  sandboxMode: safe\n",
            "bruno:\n  scripts:\n    flow: yolo\n",
            "bruno:\n  scripts:\n    flow: 1\n",
            "bruno:\n  scripts:\n    - flow\n",
            "bruno: 7\n",
            "- bruno\n",
        ] {
            assert_eq!(
                script_flow_from_extensions(&ext(yaml)),
                ScriptFlow::Sandwich,
                "{yaml}"
            );
        }
        assert_eq!(
            script_flow_from_extensions(&ext("bruno:\n  scripts:\n    flow: sequential\n")),
            ScriptFlow::Sequential
        );
        assert_eq!(
            script_flow_from_extensions(&ext("bruno:\n  scripts:\n    flow: sandwich\n")),
            ScriptFlow::Sandwich
        );
    }

    #[test]
    fn set_script_flow_sequential_keeps_sibling_keys() {
        let input = ext(
            "rocketapi:\n  sandboxMode: developer\n  keep: me\n  scripts:\n    additionalContextRoots:\n      - ../shared\nbruno:\n  other: 1\n  scripts:\n    keep: true\nsomeOtherTool:\n  foo: bar\n",
        );
        let out = set_script_flow_in_extensions(input, &ScriptFlow::Sequential)
            .expect("extensions value");
        let expected: serde_yaml::Value = serde_yaml::from_str(
            "rocketapi:\n  sandboxMode: developer\n  keep: me\n  scripts:\n    additionalContextRoots:\n      - ../shared\nbruno:\n  other: 1\n  scripts:\n    keep: true\n    flow: sequential\nsomeOtherTool:\n  foo: bar\n",
        )
        .expect("parse expected yaml");
        assert_eq!(out, expected);
        assert_eq!(
            script_flow_from_extensions(&Some(out)),
            ScriptFlow::Sequential
        );
    }

    #[test]
    fn set_script_flow_sequential_on_empty_extensions_creates_only_the_flow_key() {
        let out = set_script_flow_in_extensions(None, &ScriptFlow::Sequential)
            .expect("extensions value");
        let expected: serde_yaml::Value =
            serde_yaml::from_str("bruno:\n  scripts:\n    flow: sequential\n")
                .expect("parse expected yaml");
        assert_eq!(out, expected);
    }

    #[test]
    fn set_script_flow_sandwich_removes_only_the_flow_key() {
        let out = set_script_flow_in_extensions(
            ext("bruno:\n  other: 1\n  scripts:\n    flow: sequential\n    keep: true\n"),
            &ScriptFlow::Sandwich,
        )
        .expect("extensions value");
        let expected: serde_yaml::Value =
            serde_yaml::from_str("bruno:\n  other: 1\n  scripts:\n    keep: true\n")
                .expect("parse expected yaml");
        assert_eq!(out, expected);

        // Emptied `scripts` and `bruno` stubs are pruned, siblings stay.
        let out = set_script_flow_in_extensions(
            ext("rocketapi:\n  sandboxMode: safe\nbruno:\n  scripts:\n    flow: sequential\n"),
            &ScriptFlow::Sandwich,
        )
        .expect("extensions value");
        let expected: serde_yaml::Value =
            serde_yaml::from_str("rocketapi:\n  sandboxMode: safe\n").expect("parse expected yaml");
        assert_eq!(out, expected);

        // Nothing left at all gives no `extensions` key.
        assert_eq!(
            set_script_flow_in_extensions(
                ext("bruno:\n  scripts:\n    flow: sequential\n"),
                &ScriptFlow::Sandwich
            ),
            None
        );
    }

    #[test]
    fn set_script_flow_sandwich_leaves_matching_values_untouched() {
        assert_eq!(set_script_flow_in_extensions(None, &ScriptFlow::Sandwich), None);
        for yaml in [
            "rocketapi:\n  sandboxMode: safe\n",
            "bruno:\n  scripts:\n    flow: sandwich\n",
            "bruno:\n  scripts:\n    flow: yolo\n",
            "bruno:\n  scripts:\n    flow: 1\n",
        ] {
            let input = ext(yaml);
            assert_eq!(
                set_script_flow_in_extensions(input.clone(), &ScriptFlow::Sandwich),
                input,
                "{yaml}"
            );
        }
        let sequential = ext("bruno:\n  scripts:\n    flow: sequential\n  other: 1\n");
        assert_eq!(
            set_script_flow_in_extensions(sequential.clone(), &ScriptFlow::Sequential),
            sequential
        );
    }
```

Append at the end of `crates/rocket-infra/src/fs_collection/tests.rs` (`fs`, `SandboxMode` and `CollectionVariable` are already imported at the top of that file):

```rust
fn bruno_flow(yaml: &str) -> Option<String> {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse opencollection.yml");
    doc.get("extensions")
        .and_then(|v| v.get("bruno"))
        .and_then(|v| v.get("scripts"))
        .and_then(|v| v.get("flow"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

#[test]
fn settings_default_save_writes_no_bruno_extension() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let settings = repo.get_settings("col").expect("get");
    assert_eq!(settings.script_flow, rocket_collection::ScriptFlow::Sandwich);
    repo.save_settings("col", &settings).expect("save");
    let yaml = fs::read_to_string(repo.settings_path("col")).expect("read");
    assert!(!yaml.contains("bruno"), "no bruno key for sandwich: {yaml}");
    assert!(!yaml.contains("flow"), "no flow key for sandwich: {yaml}");
}

#[test]
fn settings_script_flow_reads_a_bruno_authored_file() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let path = repo.settings_path("col");
    let existing = fs::read_to_string(&path).expect("read");
    fs::write(
        &path,
        format!("{existing}extensions:\n  bruno:\n    scripts:\n      flow: sequential\n"),
    )
    .expect("write fixture");
    let loaded = repo.get_settings("col").expect("get");
    assert_eq!(loaded.script_flow, rocket_collection::ScriptFlow::Sequential);
}

#[test]
fn settings_script_flow_sequential_roundtrips_and_keeps_other_extensions() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let path = repo.settings_path("col");
    let existing = fs::read_to_string(&path).expect("read");
    fs::write(
        &path,
        format!(
            "{existing}extensions:\n  rocketapi:\n    sandboxMode: developer\n    keep: me\n    scripts:\n      additionalContextRoots:\n        - ../shared\n  bruno:\n    other: 1\n  other:\n    x: 1\n"
        ),
    )
    .expect("write fixture");

    let mut settings = repo.get_settings("col").expect("get");
    assert_eq!(settings.sandbox_mode, SandboxMode::Developer);
    assert_eq!(settings.script_context_roots, vec!["../shared"]);
    settings.script_flow = rocket_collection::ScriptFlow::Sequential;
    repo.save_settings("col", &settings).expect("save");

    let loaded = repo.get_settings("col").expect("reload");
    assert_eq!(loaded.script_flow, rocket_collection::ScriptFlow::Sequential);
    assert_eq!(loaded.sandbox_mode, SandboxMode::Developer);
    assert_eq!(loaded.script_context_roots, vec!["../shared"]);
    let yaml = fs::read_to_string(&path).expect("read back");
    assert_eq!(bruno_flow(&yaml).as_deref(), Some("sequential"), "{yaml}");
    assert!(yaml.contains("keep: me"), "rocketapi keys kept: {yaml}");
    assert!(yaml.contains("other: 1"), "bruno siblings kept: {yaml}");
    assert!(yaml.contains("x: 1"), "foreign namespaces kept: {yaml}");

    // A read-modify-write that only edits variables (the script-side
    // `rok.setCollectionVar` path) keeps the flow.
    let mut again = repo.get_settings("col").expect("get again");
    again.variables.push(CollectionVariable {
        key: "k".into(),
        value: "v".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    });
    repo.save_settings("col", &again).expect("save variables");
    assert_eq!(
        repo.get_settings("col").expect("reload").script_flow,
        rocket_collection::ScriptFlow::Sequential
    );
}

#[test]
fn settings_script_flow_back_to_sandwich_removes_the_bruno_stub() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let mut settings = repo.get_settings("col").expect("get");
    settings.script_flow = rocket_collection::ScriptFlow::Sequential;
    repo.save_settings("col", &settings).expect("save sequential");
    settings.script_flow = rocket_collection::ScriptFlow::Sandwich;
    repo.save_settings("col", &settings).expect("save sandwich");
    let yaml = fs::read_to_string(repo.settings_path("col")).expect("read");
    assert!(!yaml.contains("bruno"), "bruno stub removed: {yaml}");
    assert!(yaml.contains("sandboxMode: safe"), "rocketapi kept: {yaml}");
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra script_flow`
Expected: FAIL to compile with `cannot find function 'script_flow_from_extensions' in this scope` and `cannot find function 'set_script_flow_in_extensions' in this scope` (and `ScriptFlow` unresolved in the `settings.rs` test module).

- [ ] **Step 4: Implement the helpers**

In `crates/rocket-infra/src/fs_collection/settings.rs`, change the import on line 4 to:

```rust
use rocket_collection::{Collection, CollectionSettings, CollectionVariable, ScriptFlow};
```

Insert after `set_script_roots_in_extensions` (after line 122, before `pub(super) fn get_settings`):

```rust
/// Reads Bruno's script order from `extensions.bruno.scripts.flow`. A missing key, an
/// unknown string or a value of the wrong type all mean `Sandwich`, Bruno's default.
fn script_flow_from_extensions(extensions: &Option<serde_yaml::Value>) -> ScriptFlow {
    match extensions
        .as_ref()
        .and_then(|v| v.get("bruno"))
        .and_then(|v| v.get("scripts"))
        .and_then(|v| v.get("flow"))
        .and_then(|v| v.as_str())
    {
        Some("sequential") => ScriptFlow::Sequential,
        _ => ScriptFlow::Sandwich,
    }
}

/// Writes the script order to `extensions.bruno.scripts.flow`, keeping every other key.
/// A value that already reads as `flow` is left as it is, so a save never rewrites it.
/// `Sequential` writes `flow: sequential`. `Sandwich` is the default, so it removes the
/// key and prunes the `scripts` and `bruno` mappings that this leaves empty.
fn set_script_flow_in_extensions(
    extensions: Option<serde_yaml::Value>,
    flow: &ScriptFlow,
) -> Option<serde_yaml::Value> {
    if script_flow_from_extensions(&extensions) == *flow {
        return extensions;
    }
    let mut root = match extensions {
        Some(serde_yaml::Value::Mapping(map)) => map,
        _ => serde_yaml::Mapping::new(),
    };
    let bruno_key = serde_yaml::Value::String("bruno".into());
    let scripts_key = serde_yaml::Value::String("scripts".into());
    let flow_key = serde_yaml::Value::String("flow".into());
    let mut bruno = match root.get(&bruno_key) {
        Some(serde_yaml::Value::Mapping(map)) => map.clone(),
        _ => serde_yaml::Mapping::new(),
    };
    let mut scripts = match bruno.get(&scripts_key) {
        Some(serde_yaml::Value::Mapping(map)) => map.clone(),
        _ => serde_yaml::Mapping::new(),
    };
    match flow {
        ScriptFlow::Sequential => {
            scripts.insert(flow_key, serde_yaml::Value::String("sequential".into()));
        }
        ScriptFlow::Sandwich => {
            scripts.remove(&flow_key);
        }
    }
    if scripts.is_empty() {
        bruno.remove(&scripts_key);
    } else {
        bruno.insert(scripts_key, serde_yaml::Value::Mapping(scripts));
    }
    if bruno.is_empty() {
        root.remove(&bruno_key);
    } else {
        root.insert(bruno_key, serde_yaml::Value::Mapping(bruno));
    }
    if root.is_empty() {
        None
    } else {
        Some(serde_yaml::Value::Mapping(root))
    }
}
```

In `get_settings`, the block from `let sandbox_mode = ...` (line 137) to the end of the `if let Some(defaults) = oc.request { ... } else { ... }` (line 166) builds `CollectionSettings`; after plan 01 it sets `script_flow` to its default. Replace that whole block with:

```rust
    let sandbox_mode = sandbox_mode_from_extensions(&oc.extensions);
    let script_context_roots = script_roots_from_extensions(&oc.extensions);
    let script_flow = script_flow_from_extensions(&oc.extensions);

    if let Some(defaults) = oc.request {
        Ok(CollectionSettings {
            docs: oc.docs,
            auth: defaults.auth.map(rocket_shared::types::Auth::from),
            headers: defaults
                .headers
                .unwrap_or_default()
                .into_iter()
                .map(rocket_shared::types::Header::from)
                .collect(),
            variables: defaults
                .variables
                .unwrap_or_default()
                .into_iter()
                .map(CollectionVariable::from)
                .collect(),
            sandbox_mode,
            script_context_roots,
            script_flow,
        })
    } else {
        Ok(CollectionSettings {
            docs: oc.docs,
            sandbox_mode,
            script_context_roots,
            script_flow,
            ..CollectionSettings::default()
        })
    }
```

In `save_settings`, after the two existing `oc.extensions = ...` statements (lines 239-241), add:

```rust
    oc.extensions = set_script_flow_in_extensions(oc.extensions.take(), &settings.script_flow);
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra script_flow`
Expected: PASS, 9 tests (`script_flow_from_extensions_defaults_to_sandwich_for_absent_unknown_or_wrong_type`, `set_script_flow_sequential_keeps_sibling_keys`, `set_script_flow_sequential_on_empty_extensions_creates_only_the_flow_key`, `set_script_flow_sandwich_removes_only_the_flow_key`, `set_script_flow_sandwich_leaves_matching_values_untouched`, `settings_default_save_writes_no_bruno_extension`, `settings_script_flow_reads_a_bruno_authored_file`, `settings_script_flow_sequential_roundtrips_and_keeps_other_extensions`, `settings_script_flow_back_to_sandwich_removes_the_bruno_stub`).

Run: `cargo test -j4 -p rocket-infra settings`
Expected: PASS (existing `settings_roundtrip`, `save_settings_preserves_unrelated_extensions_data`, `settings_script_context_roots_*` unchanged).

Run: `cargo clippy -j4 -p rocket-infra --tests`
Expected: no new warnings in `fs_collection/settings.rs` or `fs_collection/tests.rs`.

- [ ] **Step 6: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add crates/rocket-infra/src/fs_collection/settings.rs crates/rocket-infra/src/fs_collection/tests.rs
git commit --only -m "feat(infra): persist script flow in opencollection extensions" -- crates/rocket-infra/src/fs_collection/settings.rs crates/rocket-infra/src/fs_collection/tests.rs
```

---

## Task 2: Carry `scriptFlow` through IPC and the frontend settings save

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Test: `crates/rocket-collection/src/settings.rs` (`mod tests`, append)
- Modify: `src/lib/tauri-api.ts` (the `SandboxMode` type and `CollectionSettings` interface, lines 81-91)
- Modify: `src/lib/collection-settings-save.ts` (`buildSettingsForSave`)
- Modify: `src/components/collections/CollectionOverviewTab.tsx` (comment above `saveSettings`, lines 258-262)
- Test: `src/lib/__tests__/collection-settings-save.test.ts`
- No change: `src-tauri/src/commands/collections.rs`. `get_collection_settings` (line 342) returns and `save_collection_settings` (line 350) accepts `rocket_collection::CollectionSettings` directly; there is no separate DTO, and the domain struct's camelCase JSON is the IPC contract here. `src/components/layout/SandboxPopover.tsx` builds its payload as `{ ...current, sandboxMode: nextMode }` (line 73), so it already carries `scriptFlow`.

**Interfaces:**
- Consumes: `CollectionSettings.script_flow` JSON as `"scriptFlow": "sandwich" | "sequential"` (plan 01: struct-level `rename_all = "camelCase"`, enum `rename_all = "lowercase"`, field `#[serde(default)]`). `getCollectionSettings(name)` and `saveCollectionSettings(collection, settings)` in `src/lib/tauri-api.ts`.
- Produces:
  - `export type ScriptFlow = 'sandwich' | 'sequential';` in `src/lib/tauri-api.ts`.
  - `CollectionSettings.scriptFlow?: ScriptFlow` (optional, like `scriptContextRoots`, so existing test fixtures keep compiling).
  - `buildSettingsForSave(current, edited)` returns `scriptFlow: current.scriptFlow`.

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

Append to the `mod tests` block in `crates/rocket-collection/src/settings.rs` (after `sandbox_mode_developer_roundtrips_as_camel_case`):

```rust
    #[test]
    fn ipc_json_carries_script_flow_and_defaults_when_absent() {
        use crate::ScriptFlow;

        let settings = CollectionSettings {
            script_flow: ScriptFlow::Sequential,
            ..Default::default()
        };
        let json = serde_json::to_value(&settings).expect("serialize");
        assert_eq!(json["scriptFlow"], "sequential");

        let round: CollectionSettings =
            serde_json::from_value(json).expect("deserialize");
        assert_eq!(round.script_flow, ScriptFlow::Sequential);

        // A payload from a frontend that does not know the field yet.
        let old: CollectionSettings =
            serde_json::from_str(r#"{"headers":[],"variables":[],"sandboxMode":"safe"}"#)
                .expect("old payload deserializes");
        assert_eq!(old.script_flow, ScriptFlow::Sandwich);
    }
```

Replace the contents of `src/lib/__tests__/collection-settings-save.test.ts` with:

```ts
import { describe, expect, it } from 'vitest';
import { buildSettingsForSave } from '@/lib/collection-settings-save';
import type { CollectionSettings } from '@/lib/tauri-api';

describe('buildSettingsForSave', () => {
  const current: CollectionSettings = {
    headers: [],
    variables: [],
    sandboxMode: 'developer',
    scriptContextRoots: ['../shared'],
    scriptFlow: 'sequential',
  };

  it('keeps the sandbox mode and script context roots from the fresh settings', () => {
    const payload = buildSettingsForSave(current, {
      headers: [{ key: 'X-A', value: '1', enabled: true }],
      variables: [],
      docs: 'hello',
    });

    expect(payload.sandboxMode).toBe('developer');
    expect(payload.scriptContextRoots).toEqual(['../shared']);
    expect(payload.docs).toBe('hello');
    expect(payload.headers).toHaveLength(1);
  });

  it('keeps the script flow from the fresh settings', () => {
    const payload = buildSettingsForSave(current, {
      headers: [],
      variables: [],
    });

    expect(payload.scriptFlow).toBe('sequential');
  });

  it('leaves the script flow out when the backend did not send one', () => {
    const withoutFlow: CollectionSettings = {
      headers: [],
      variables: [],
      sandboxMode: 'safe',
    };
    const payload = buildSettingsForSave(withoutFlow, { headers: [], variables: [] });

    expect(payload.scriptFlow).toBeUndefined();
    expect(JSON.parse(JSON.stringify(payload))).not.toHaveProperty('scriptFlow');
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-collection ipc_json_carries_script_flow`
Expected: PASS if plan 01 gave `script_flow` `#[serde(default)]` and lowercase variants. This test is the IPC contract guard, not new behavior. If it FAILS with `missing field 'scriptFlow'` or a wrong value, fix the serde attributes on the plan 01 field and enum before going on.

Run: `yarn test --run collection-settings-save`
Expected: FAIL on `keeps the script flow from the fresh settings` with `expected undefined to be 'sequential'`. (`yarn tsc --noEmit` also fails, because `scriptFlow` does not exist on `CollectionSettings` yet.)

- [ ] **Step 4: Implement the passthrough**

In `src/lib/tauri-api.ts`, replace lines 81-91 (`SandboxMode` and the `CollectionSettings` interface) with:

```ts
export type SandboxMode = 'safe' | 'developer';

/** Script order across collection, folders and request. Absent means sandwich. */
export type ScriptFlow = 'sandwich' | 'sequential';

export interface CollectionSettings {
  docs?: string;
  auth?: Auth;
  headers: Header[];
  variables: CollectionVariable[];
  sandboxMode: SandboxMode;
  /** Extra directories scripts may require() from, in Developer mode only. */
  scriptContextRoots?: string[];
  /** Persisted at extensions.bruno.scripts.flow in opencollection.yml. */
  scriptFlow?: ScriptFlow;
}
```

In `src/lib/collection-settings-save.ts`, replace the `return` object of `buildSettingsForSave` with:

```ts
  return {
    ...edited,
    sandboxMode: current.sandboxMode,
    scriptContextRoots: current.scriptContextRoots,
    scriptFlow: current.scriptFlow,
  };
```

In `src/components/collections/CollectionOverviewTab.tsx`, replace the comment above `saveSettings` (lines 258-262) with:

```ts
  // Persist all settings to disk (no auto-save). saveCollectionSettings is a full
  // replace on the backend, so sandboxMode, scriptContextRoots and scriptFlow are read
  // fresh here immediately before saving rather than from `collection` (loaded once on
  // mount). Otherwise a change made elsewhere in the meantime, such as a mode change
  // via the toolbar's SandboxPopover, would be silently wiped by this save.
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `yarn test --run collection-settings-save`
Expected: PASS, 3 tests.

Run: `cargo test -j4 -p rocket-collection ipc_json_carries_script_flow`
Expected: PASS.

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn check`
Expected: no errors. If Biome reports formatting only, run `yarn format` and re-run `yarn check`.

- [ ] **Step 6: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add crates/rocket-collection/src/settings.rs src/lib/tauri-api.ts src/lib/collection-settings-save.ts src/components/collections/CollectionOverviewTab.tsx src/lib/__tests__/collection-settings-save.test.ts
git commit --only -m "fix(collections): keep script flow when saving collection settings" -- crates/rocket-collection/src/settings.rs src/lib/tauri-api.ts src/lib/collection-settings-save.ts src/components/collections/CollectionOverviewTab.tsx src/lib/__tests__/collection-settings-save.test.ts
```

---

## Task 3: Schema guard for the script flow key

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Test: `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs` (append at the end of the file; plan 02 also appends here, so on rebase keep both)

**Interfaces:**
- Consumes: `setup() -> (TempDir, FsCollectionRepo)`, `read_yaml(path: &Path) -> Value`, `Violations`, `check_collection_root(v: &mut Violations, at: &str, doc: &Value)` (all private to `schema_shape_tests.rs`), `rocket_collection::ScriptFlow`, and `SandboxMode`, `CollectionSettings`, `Header`, `Value` (already imported at the top of the file), plus the Task 1 save path.
- Produces: test `script_flow_is_written_only_under_bruno_extensions`.

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (section 2.1, `RequestDefaults` keys and `extensions`).

- [ ] **Step 2: Write the guard test**

Append at the end of `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`:

```rust
#[test]
fn script_flow_is_written_only_under_bruno_extensions() {
    use rocket_collection::ScriptFlow;

    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    let path = dir.path().join("api/opencollection.yml");
    let mut settings = CollectionSettings {
        headers: vec![Header::new("X-Tenant", "acme")],
        sandbox_mode: SandboxMode::Developer,
        script_context_roots: vec!["../shared".into()],
        script_flow: ScriptFlow::Sequential,
        ..Default::default()
    };
    repo.save_settings("api", &settings).expect("save sequential");

    let doc = read_yaml(&path);
    let mut v = Violations::default();
    check_collection_root(&mut v, "opencollection.yml", &doc);
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));

    // The flow lives only under Bruno's namespace, and Rocket adds nothing else there.
    let bruno = doc
        .get("extensions")
        .and_then(|e| e.get("bruno"))
        .expect("extensions.bruno written for sequential");
    let expected: Value =
        serde_yaml::from_str("scripts:\n  flow: sequential\n").expect("parse expected yaml");
    assert_eq!(bruno, &expected);
    let root = doc.as_mapping().expect("root mapping");
    for key in ["flow", "scriptFlow", "script_flow", "bruno"] {
        assert!(
            !root.contains_key(Value::String(key.into())),
            "`{key}` must not be a root key"
        );
    }
    let request = doc.get("request").expect("request defaults written for headers");
    assert!(request.get("scripts").is_none(), "flow is not a RequestDefaults key");
    assert!(
        doc.get("extensions")
            .and_then(|e| e.get("rocketapi"))
            .and_then(|r| r.get("sandboxMode"))
            .is_some(),
        "rocketapi extensions are still written"
    );

    // Back to the default: the Bruno namespace disappears again.
    settings.script_flow = ScriptFlow::Sandwich;
    repo.save_settings("api", &settings).expect("save sandwich");
    let doc = read_yaml(&path);
    assert!(
        doc.get("extensions").and_then(|e| e.get("bruno")).is_none(),
        "no bruno key for sandwich"
    );
}
```

- [ ] **Step 3: Run the test and prove it can fail**

Run: `cargo test -j4 -p rocket-infra script_flow_is_written_only_under_bruno_extensions`
Expected: PASS once Task 1 is in. This is a regression guard, so prove it bites: temporarily change the `"sequential"` string literal inside `set_script_flow_in_extensions` (Task 1) to `"Sequential"`, rerun, expect FAIL on `assert_eq!(bruno, &expected)`, then revert that change and rerun to PASS. Do not commit the temporary change.

- [ ] **Step 4: Run the full schema guard and settings tests**

Run: `cargo test -j4 -p rocket-infra schema_shape`
Expected: PASS (every test in `schema_shape_tests.rs`, including `written_collection_files_only_use_schema_keys`).

Run: `cargo test -j4 -p rocket-infra settings`
Expected: PASS.

Run: `cargo check -j4 -p rocket-infra`
Expected: no errors.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add crates/rocket-infra/src/fs_collection/schema_shape_tests.rs
git commit --only -m "test(infra): guard script flow placement in opencollection.yml" -- crates/rocket-infra/src/fs_collection/schema_shape_tests.rs
```

---

## Next Plan

**Execution order:** this is plan 03 of 12. The series runs strictly one plan at a time, in numeric order (see [00-plan-index.md](00-plan-index.md)).

**Next to execute:** [Plan 04: IPC commands, DTOs, event and TS bindings](2026-10-07-folder-settings-plan-04-ipc-commands-and-bindings.md).

**Instruction for the executing Claude:** when every task in this plan is done and its verification commands (listed in the last step of each task) pass, record "Task N: complete" for each task in the ledger at `.superpowers/sdd/folder-settings/progress.md`, then open Plan 04 and start its first task immediately. Do not ask the user for consent between plans. Stop and report to the user instead if a verification command fails, if a plan's stated prerequisite is missing, or if you must deviate from the locked contract in the index.

**Notes for the next plan (from the plan author):**

`docs/superpowers/plans/folder-settings/2026-10-07-folder-settings-plan-04-ipc-commands-and-bindings.md` (Plan 04: IPC commands, DTOs, `FolderSettingsSaved` event and TS bindings). Plan 06 reads `CollectionSettings.script_flow` through the path this plan made reliable and passes it to `chain_scripts`.
