# Folder settings, Plan 12: Bruno compatibility verification and docs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove, with tests, that the shipped Folder Settings feature keeps `folder.yml` strictly OpenCollection (so a Rocket collection still opens in Bruno), that a Bruno-authored `folder.yml` loads and survives Rocket saves, and that a request nested two folders deep really receives the merged headers, inherited auth and chained scripts. Then document the feature (spec reference, `.claude/folder-settings.md`, crate guides) and hand the user a manual verification checklist.

**Architecture:** This is the closing plan, so it adds no feature code except one small compatibility fix that the new tests may expose in `rocket-infra` (a variable conversion helper). Task 1 extends the existing schema guard in `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs` and adds Bruno-style fixtures. Task 2 adds one child test module to `rocket-app`'s `execution_service` that runs the real `FsCollectionRepo` and the real `ReqwestExecutor` against a wiremock server. Task 3 is documentation only.

**Tech Stack:** Rust (serde_yaml, tokio, wiremock, tempfile), Markdown. Run Rust tests with `cargo test -j4 -p <crate> <name>`.

**Spec:** `docs/superpowers/specs/2026-10-07-folder-settings-design.md`. Locked contract: `docs/superpowers/plans/folder-settings/00-plan-index.md`. Depends on plans 01 to 11 being merged. Everything this plan references by name (`FolderSettings`, `ScriptFlow`, `resolve_folder_auth`, `get_folder_settings`, `save_folder_settings`, `CollectionSettings.script_flow`) is the index's locked contract.

## Global Constraints

- Never call `unwrap` in production paths. Tests may use `.expect("reason")`.
- Never apply `#[serde(rename_all = "camelCase")]` to the `Oc*` persistence structs. Nothing in this plan adds a DTO.
- `folder.yml` carries only OpenCollection keys. The only tolerated deviations are the ones already in `KNOWN_DEFERRED` in `schema_shape_tests.rs`. This plan does not add to that list. It asserts that folder files do not use `Variable.initial`, even though that key is deferred for other files.
- Always pass `-j4` to cargo. Target one crate. Never `cargo test --workspace` or `--all`.
- Commit with the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only (never `git add -A` or `git add .`) and commit with a pathspec. Conventional commit messages. Peer sessions share this repo's index.
- Docs go under `.claude/`, and the project `CLAUDE.md` gets one link line only.
- Comments in code are short full sentences that end with a period.
- `schema_shape_tests.rs` is also edited by plans 02 and 03. Before starting, run `git status --short` and `git log --oneline -8`, then read the tail of that file so new tests are appended after theirs and no helper is defined twice.
- A failing test in this plan that points at earlier-plan behavior is a regression in that plan: fix it there, do not weaken the test.

## Review Focus

1. A `folder.yml` with every section populated, saved for every auth type in `sample_auths()`, uses only schema keys, has only `info`, `request` and `docs` at the top level, and carries no Rocket-only key (Task 1 test `populated_folder_yml_only_uses_schema_keys`).
2. An empty section is omitted, not written as an empty list (Task 1 test `empty_folder_sections_are_omitted`).
3. The schema guard itself catches a bad script type, an extra script key, a Rocket key on `request` and an extra key on `docs`. Without this the guard could pass vacuously (Task 1 test `checker_flags_bad_folder_scripts_docs_and_rocket_keys`).
4. A Bruno-authored `folder.yml` (disabled headers, bearer auth, variables with descriptions, three script types and a `hooks` entry, `docs` as `{content, type}`) loads into the right `FolderSettings` fields (Task 1 test `bruno_authored_folder_loads_into_folder_settings`).
5. After a Rocket save, that file keeps `info.seq`, `request.metadata`, `request.settings`, the `hooks` script, header descriptions and variable descriptions, and gains neither an `initial` variable key nor an `info.uid` (Task 1 test `bruno_folder_keeps_untyped_sections_across_a_save`).
6. A second save is byte-identical to the first (Task 1 test `second_folder_save_is_byte_stable`).
7. A folder with `auth: inherit` and a plain-string `docs` loads, resolves to no folder auth, and saves back without a concrete auth (Task 1 test `bruno_inherit_folder_resolves_to_no_folder_auth`).
8. `extensions.bruno.scripts.flow: sequential` and its sibling keys survive a collection settings save and a folder settings save (Task 1 test `script_flow_survives_settings_and_folder_saves`).
9. A request two folders deep receives merged headers (inner replaces outer, a disabled folder header does not shadow, the request wins), the innermost folder's auth, and scripts in sandwich order, then in sequential order when the collection says so (Task 2 tests `nested_request_gets_merged_headers_inherited_auth_and_sandwich_scripts` and `sequential_flow_runs_every_phase_outer_to_inner`).
10. The docs match the shipped behavior, including the known limits (collection-level scripts are preserved but not run, `inherit` and `none` both take the parent's auth, header keys match exactly) (Task 3 step 3 review).

---

## Task 1: Compatibility test suite in `rocket-infra`

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify (Test): `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`
- Modify (only if steps 7 to 9 show the failures described there): `crates/rocket-infra/src/fs_collection/variables.rs` plus the file that holds `save_folder_settings`, found with `rg -n "fn save_folder_settings" crates/rocket-infra/src/fs_collection/`

**Interfaces:**
- Consumes (locked contract, `rocket_collection` crate root): `FolderSettings` (fields `headers`, `auth`, `variables`, `pre_request_script`, `post_response_script`, `tests_script`, `docs`; derives `Debug, Clone, PartialEq, Default`), `ScriptFlow { Sandwich, Sequential }` (derives `Debug, Clone, Copy, PartialEq, Eq, Default`), `resolve_folder_auth(&[FolderSettings]) -> Option<Auth>`, `CollectionSettings.script_flow: ScriptFlow`, and `CollectionRepository::{get_folder_settings, save_folder_settings, get_settings, save_settings, create, create_folder}`.
- Consumes (already in this file): `Violations`, `check_folder`, `check_request_defaults`, `check_auth`, `seq`, `setup`, `read_yaml`, `sample_auths`, and the constants `SCRIPT`, `FOLDER`, `REQUEST_DEFAULTS`.
- Produces (test-private): `check_scripts`, `check_docs`, `SCRIPT_TYPES`, `DOCS_OBJECT`, `populated_settings`, `assert_only_folder_sections`, `write_folder_fixture`, `BRUNO_FOLDER`, `BRUNO_INHERIT_FOLDER`, and the tests named in Review Focus 1 to 8.
- Produces (only if needed): `pub(super) fn folder_oc_variables(vars: &[CollectionVariable], existing: Option<&[OcVariable]>) -> Option<Vec<OcVariable>>` in `fs_collection/variables.rs`.

How the guard works, so the steps make sense: `Violations::keys(ty, at, value, allowed)` records every key of a mapping that is neither in `allowed` nor listed as `"<Type>.<key>"` in `KNOWN_DEFERRED`. `KNOWN_DEFERRED` holds keys Rocket still writes that the vendored OpenCollection v1.0.0 schema rejects (for example `FolderInfo.uid` and `Variable.initial`), each tracked in an earlier plan's Deferred section. `check_folder` currently validates `Folder`, `FolderInfo`, and through `check_request_defaults` the headers, variables and auth of `request`. It does not look at `request.scripts` or `docs`, which is the gap step 3 closes.

- [ ] **Step 1: Write the failing guard test and add the imports**

In `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`, replace

```rust
use rocket_collection::{
    CollectionItem, CollectionRepository, CollectionSettings, CollectionVariable, Request,
};
```

with

```rust
use rocket_collection::{
    resolve_folder_auth, CollectionItem, CollectionRepository, CollectionSettings,
    CollectionVariable, FolderSettings, Request, ScriptFlow,
};
use rocket_shared::description::Description;
```

If plans 02 or 03 already added some of these names to the import, keep one copy of each. Append this test at the end of the file:

```rust
#[test]
fn checker_flags_bad_folder_scripts_docs_and_rocket_keys() {
    let mut v = Violations::default();
    let doc: Value = serde_yaml::from_str(
        "info:\n  name: f\n  type: folder\nrequest:\n  scripts:\n  - type: pre-request\n    code: x\n  - type: tests\n    code: y\n    extra: z\n  scriptFlow: sequential\ndocs:\n  content: a\n  format: md\n",
    )
    .expect("fixture yaml");
    check_folder(&mut v, "bad folder", &doc);
    // `pre-request` is not a script type (1), `extra` on a script (1), `scriptFlow` on
    // RequestDefaults (1) and `format` on docs (1).
    assert_eq!(v.0.len(), 4, "{:#?}", v.0);
}
```

- [ ] **Step 2: Run it, expect a failure**

Run: `cargo test -j4 -p rocket-infra checker_flags_bad_folder_scripts_docs_and_rocket_keys`
Expected: FAIL with `left: 1, right: 4`. Only `scriptFlow` is flagged today, because scripts and docs are never inspected.

- [ ] **Step 3: Extend the guard**

Next to the other constants (after `const SCRIPT: &[&str] = &["type", "code"];`) add:

```rust
/// `Script.type` values in the spec. `hooks` entries are kept untouched by Rocket.
const SCRIPT_TYPES: &[&str] = &["before-request", "after-response", "tests", "hooks"];
/// A `docs` object has exactly these keys. A plain string is also legal.
const DOCS_OBJECT: &[&str] = &["content", "type"];
```

Before `fn check_request_defaults`, add:

```rust
fn check_scripts(v: &mut Violations, at: &str, scripts: Option<&Value>) {
    for script in seq(scripts) {
        v.keys("Script", at, script, SCRIPT);
        let ty = script.get("type").and_then(Value::as_str).unwrap_or("");
        if !SCRIPT_TYPES.contains(&ty) {
            v.0.push(format!("{at}: script type `{ty}` is not a spec Script type"));
        }
    }
}

fn check_docs(v: &mut Violations, at: &str, docs: Option<&Value>) {
    if let Some(docs) = docs {
        // A string has no keys, so `keys` skips it.
        v.keys("Docs", at, docs, DOCS_OBJECT);
    }
}
```

In `check_request_defaults`, after the `variables` loop and before the `auth` block, add `check_scripts(v, at, req.get("scripts"));`. In `check_folder`, after the `request` block, add `check_docs(v, at, doc.get("docs"));`.

If plan 02 already added a scripts or docs check to these two functions, keep one copy of each and delete the duplicate.

- [ ] **Step 4: Run it, expect a pass, and confirm nothing else broke**

Run: `cargo test -j4 -p rocket-infra schema_shape_tests`
Expected: all tests in the module pass, including `checker_flags_known_bad_shapes` (its count of 5 is unaffected because its folder has no scripts or docs).

- [ ] **Step 5: Write the populated-folder and empty-folder tests**

Append to `schema_shape_tests.rs`:

```rust
fn populated_settings(auth: Auth) -> FolderSettings {
    FolderSettings {
        headers: vec![
            Header::new("X-Team", "billing"),
            Header {
                key: "X-Off".into(),
                value: "1".into(),
                enabled: false,
                description: Some(Description::text("Debug only")),
            },
        ],
        auth: Some(auth),
        variables: vec![
            CollectionVariable {
                key: "region".into(),
                value: "eu".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            },
            CollectionVariable {
                key: "legacy".into(),
                value: "old".into(),
                initial_value: String::new(),
                enabled: false,
                secret: false,
            },
        ],
        pre_request_script: Some("console.log('pre');".into()),
        post_response_script: Some("console.log('post');".into()),
        tests_script: Some("test('ok', function () {});".into()),
        docs: Some("# Billing\n\nNotes.".into()),
    }
}

/// Asserts the `folder.yml` has only spec sections and none of the names Rocket uses in memory.
fn assert_only_folder_sections(raw: &Value, at: &str) {
    let top = raw.as_mapping().expect("folder.yml is a mapping");
    for key in top.keys().filter_map(Value::as_str) {
        assert!(
            ["info", "request", "docs"].contains(&key),
            "{at}: unexpected top-level key `{key}`"
        );
    }
    let request = raw
        .get("request")
        .and_then(Value::as_mapping)
        .expect("request block");
    for key in request.keys().filter_map(Value::as_str) {
        assert!(
            REQUEST_DEFAULTS.contains(&key),
            "{at}: unexpected request key `{key}`"
        );
    }
    let text = serde_yaml::to_string(raw).expect("yaml text");
    for banned in [
        "scriptFlow",
        "script_flow",
        "preRequestScript",
        "postResponseScript",
        "testsScript",
        "initialValue",
    ] {
        assert!(!text.contains(banned), "{at}: Rocket-only name `{banned}`");
    }
}

#[test]
fn populated_folder_yml_only_uses_schema_keys() {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    let mut v = Violations::default();
    for (name, auth) in sample_auths() {
        let folder = format!("f-{name}");
        repo.create_folder("api", &folder).expect("create folder");
        repo.save_folder_settings("api", &folder, &populated_settings(auth))
            .expect("save folder settings");
        let rel = format!("{folder}/folder.yml");
        let raw = read_yaml(&dir.path().join("api").join(&rel));
        check_folder(&mut v, &rel, &raw);
        assert_only_folder_sections(&raw, &rel);
        let mut types: Vec<&str> = raw["request"]["scripts"]
            .as_sequence()
            .expect("scripts written")
            .iter()
            .filter_map(|s| s["type"].as_str())
            .collect();
        types.sort_unstable();
        assert_eq!(types, vec!["after-response", "before-request", "tests"], "{rel}");
    }
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
}

#[test]
fn empty_folder_sections_are_omitted() {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    repo.create_folder("api", "empty").expect("create folder");
    repo.save_folder_settings("api", "empty", &FolderSettings::default())
        .expect("save empty settings");
    let raw = read_yaml(&dir.path().join("api/empty/folder.yml"));
    let request = raw.get("request");
    for section in ["headers", "auth", "variables", "scripts"] {
        assert!(
            request.and_then(|r| r.get(section)).is_none(),
            "empty `{section}` must be omitted: {raw:?}"
        );
    }
    assert!(raw.get("docs").is_none(), "empty docs must be omitted: {raw:?}");
}
```

- [ ] **Step 6: Write the Bruno fixtures and the load, save, byte-stable and inherit tests**

Append to `schema_shape_tests.rs`:

```rust
/// A `folder.yml` as Bruno writes it: every section, a disabled header and variable, descriptions,
/// all three script types plus `hooks`, untyped `metadata` and `settings`, and typed `docs`.
const BRUNO_FOLDER: &str = r#"info:
  name: Billing
  type: folder
  seq: 2
request:
  headers:
  - name: X-Team
    value: billing
    description: Owning team
  - name: X-Debug
    value: '1'
    disabled: true
  auth:
    type: bearer
    token: '{{billingToken}}'
  variables:
  - name: region
    value: eu
    description: Deployment region
  - name: legacy
    value: old
    disabled: true
  scripts:
  - type: before-request
    code: |-
      console.log('folder pre');
  - type: after-response
    code: |-
      console.log('folder post');
  - type: tests
    code: |-
      test('ok', function () {});
  - type: hooks
    code: |-
      // hook body kept as written
  metadata:
  - name: x-trace
    value: '1'
  settings:
    timeout: 5000
    followRedirects: false
docs:
  content: |-
    # Billing

    Folder notes.
  type: text/markdown
"#;

/// A Bruno folder that inherits auth and uses the plain-string form of `docs`.
const BRUNO_INHERIT_FOLDER: &str = "info:\n  name: Inner\n  type: folder\nrequest:\n  auth: inherit\n  headers:\n  - name: X-Inner\n    value: '1'\ndocs: Inner notes\n";

/// Creates `api` with one folder whose `folder.yml` is the given text, and returns its path.
fn write_folder_fixture(
    folder: &str,
    yaml: &str,
) -> (TempDir, FsCollectionRepo, std::path::PathBuf) {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    repo.create_folder("api", folder).expect("create folder");
    let path = dir.path().join("api").join(folder).join("folder.yml");
    fs::write(&path, yaml).expect("write fixture");
    (dir, repo, path)
}

#[test]
fn bruno_authored_folder_loads_into_folder_settings() {
    let (_dir, repo, _path) = write_folder_fixture("billing", BRUNO_FOLDER);
    let loaded = repo
        .get_folder_settings("api", "billing")
        .expect("a Bruno folder.yml loads");

    assert_eq!(loaded.headers.len(), 2);
    assert_eq!(loaded.headers[0].key, "X-Team");
    assert_eq!(
        loaded.headers[0].description.as_ref().and_then(Description::content),
        Some("Owning team")
    );
    assert!(!loaded.headers[1].enabled, "a disabled header stays disabled");
    assert_eq!(
        loaded.auth,
        Some(Auth::Bearer {
            token: "{{billingToken}}".into()
        })
    );
    assert_eq!(loaded.variables.len(), 2);
    assert_eq!(loaded.variables[0].key, "region");
    assert!(!loaded.variables[1].enabled);
    assert_eq!(
        loaded.pre_request_script.as_deref(),
        Some("console.log('folder pre');")
    );
    assert_eq!(
        loaded.post_response_script.as_deref(),
        Some("console.log('folder post');")
    );
    assert_eq!(
        loaded.tests_script.as_deref(),
        Some("test('ok', function () {});")
    );
    assert_eq!(loaded.docs.as_deref(), Some("# Billing\n\nFolder notes."));
}

#[test]
fn bruno_folder_keeps_untyped_sections_across_a_save() {
    let (_dir, repo, path) = write_folder_fixture("billing", BRUNO_FOLDER);
    let loaded = repo.get_folder_settings("api", "billing").expect("load");
    repo.save_folder_settings("api", "billing", &loaded)
        .expect("save");

    let raw = read_yaml(&path);
    let req = &raw["request"];
    assert_eq!(raw["info"]["seq"].as_u64(), Some(2));
    assert!(
        raw["info"].get("uid").is_none(),
        "a save must not stamp a uid on a Bruno folder: {raw:?}"
    );
    assert_eq!(req["settings"]["timeout"].as_u64(), Some(5000));
    assert_eq!(req["settings"]["followRedirects"].as_bool(), Some(false));
    assert_eq!(req["metadata"][0]["name"].as_str(), Some("x-trace"));

    let scripts = req["scripts"].as_sequence().expect("scripts");
    assert_eq!(scripts.len(), 4, "three typed scripts plus the hooks entry");
    let hooks = scripts
        .iter()
        .find(|s| s["type"].as_str() == Some("hooks"))
        .expect("hooks entry kept");
    assert_eq!(hooks["code"].as_str(), Some("// hook body kept as written"));

    assert_eq!(req["headers"][0]["description"].as_str(), Some("Owning team"));
    assert_eq!(req["headers"][1]["disabled"].as_bool(), Some(true));
    assert_eq!(req["auth"]["type"].as_str(), Some("bearer"));
    let vars = req["variables"].as_sequence().expect("variables");
    assert_eq!(vars[0]["description"].as_str(), Some("Deployment region"));
    assert_eq!(vars[1]["disabled"].as_bool(), Some(true));
    assert!(
        vars.iter().all(|var| var.get("initial").is_none()),
        "folder variables must not gain the deferred `initial` key: {vars:?}"
    );
    assert_eq!(
        raw["docs"].as_str().or_else(|| raw["docs"]["content"].as_str()),
        Some("# Billing\n\nFolder notes.")
    );

    let mut v = Violations::default();
    check_folder(&mut v, "billing/folder.yml", &raw);
    assert_only_folder_sections(&raw, "billing/folder.yml");
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
}

#[test]
fn second_folder_save_is_byte_stable() {
    let (_dir, repo, path) = write_folder_fixture("billing", BRUNO_FOLDER);
    let loaded = repo.get_folder_settings("api", "billing").expect("load");
    repo.save_folder_settings("api", "billing", &loaded)
        .expect("first save");
    let first = fs::read_to_string(&path).expect("read first save");

    let reloaded = repo.get_folder_settings("api", "billing").expect("reload");
    assert_eq!(reloaded, loaded, "a save must not change what loads");
    repo.save_folder_settings("api", "billing", &reloaded)
        .expect("second save");
    let second = fs::read_to_string(&path).expect("read second save");
    assert_eq!(second, first, "the second save must be byte-identical");
}

#[test]
fn bruno_inherit_folder_resolves_to_no_folder_auth() {
    let (_dir, repo, path) = write_folder_fixture("inner", BRUNO_INHERIT_FOLDER);
    let loaded = repo.get_folder_settings("api", "inner").expect("load");
    assert!(
        matches!(loaded.auth, None | Some(Auth::Inherit)),
        "auth: inherit is no folder auth, got {:?}",
        loaded.auth
    );
    assert_eq!(resolve_folder_auth(std::slice::from_ref(&loaded)), None);
    assert_eq!(loaded.headers.len(), 1);
    assert_eq!(loaded.docs.as_deref(), Some("Inner notes"));

    repo.save_folder_settings("api", "inner", &loaded)
        .expect("save");
    let raw = read_yaml(&path);
    let auth = &raw["request"]["auth"];
    assert!(
        auth.is_null() || auth.as_str() == Some("inherit"),
        "auth must stay absent or `inherit`, got {auth:?}"
    );
    assert_eq!(raw["docs"].as_str(), Some("Inner notes"));
}
```

- [ ] **Step 7: Run them and read the failures**

Run: `cargo test -j4 -p rocket-infra schema_shape_tests`
Expected on a correct earlier-plan stack: the guard, populated, empty, loads, inherit and byte-stable tests pass. Two assertions are known risks and may fail first time in `bruno_folder_keeps_untyped_sections_across_a_save`:
- `vars[0]["description"]`: `CollectionVariable` has no description field, so the plain conversion drops it.
- the `initial` assertion: loading a Bruno variable fills `initial_value` from `value` (`From<OcVariable> for CollectionVariable`), and `From<CollectionVariable> for OcVariable` then writes `initial: eu`, a key the schema rejects (it is only tolerated through `KNOWN_DEFERRED`).

If both assertions already pass (plan 02 handled them), skip steps 8 and 9 and go to step 10.

- [ ] **Step 8: Add the variable conversion helper**

In `crates/rocket-infra/src/fs_collection/variables.rs`, after the imports and before `get_folder_chain_variables`, add:

```rust
/// Builds `request.variables` for a `folder.yml` from the in-memory variables.
///
/// `CollectionVariable` cannot carry a description, so each entry keeps the description of the
/// existing entry with the same name. Loading fills `initial_value` from `value`, so an equal
/// pair means "no initial value" and `initial` is left out. That keeps the file free of a key
/// the OpenCollection schema rejects.
pub(super) fn folder_oc_variables(
    vars: &[CollectionVariable],
    existing: Option<&[OcVariable]>,
) -> Option<Vec<OcVariable>> {
    if vars.is_empty() {
        return None;
    }
    Some(
        vars.iter()
            .map(|cv| {
                let mut oc = OcVariable::from(cv.clone());
                if cv.initial_value == cv.value {
                    oc.initial = None;
                }
                oc.description = existing
                    .and_then(|list| list.iter().find(|e| e.name == cv.key))
                    .and_then(|e| e.description.clone());
                oc
            })
            .collect(),
    )
}
```

- [ ] **Step 9: Use the helper everywhere a folder's `request.variables` is built**

Run: `rg -n "OcVariable::from" crates/rocket-infra/src/fs_collection/`
Expected folder call sites: `save_folder_variables` in `variables.rs` (it builds `oc_vars` from `vars`) and `save_folder_settings`. Leave the request-level and collection-level call sites alone.

In `save_folder_variables`, read the existing variables before they are replaced and use the helper. Replace

```rust
    let oc_vars: Vec<OcVariable> = vars.into_iter().map(OcVariable::from).collect();
    let req_defaults = oc_folder.request.take().unwrap_or_default();
    oc_folder.request = Some(OcRequestDefaults {
        variables: if oc_vars.is_empty() {
            None
        } else {
            Some(oc_vars)
        },
        ..req_defaults
    });
```

with

```rust
    let req_defaults = oc_folder.request.take().unwrap_or_default();
    let oc_vars = folder_oc_variables(&vars, req_defaults.variables.as_deref());
    oc_folder.request = Some(OcRequestDefaults {
        variables: oc_vars,
        ..req_defaults
    });
```

In `save_folder_settings`, apply the same change to the place that sets `variables` on the `OcRequestDefaults`: pass `settings.variables.as_slice()` and the existing `request.variables.as_deref()` read from the file before it is overwritten.

Run: `cargo check -j4 -p rocket-infra`
Expected: compiles. Remove any import the change leaves unused.

- [ ] **Step 10: Write the script flow test**

Append to `schema_shape_tests.rs`:

```rust
#[test]
fn script_flow_survives_settings_and_folder_saves() {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    let oc_path = dir.path().join("api/opencollection.yml");

    // Author the Bruno extension by hand, with a sibling key that Rocket does not own.
    let mut doc = read_yaml(&oc_path);
    let root = doc.as_mapping_mut().expect("root mapping");
    let ext_key = Value::String("extensions".into());
    let mut ext = match root.get(&ext_key) {
        Some(Value::Mapping(map)) => map.clone(),
        _ => serde_yaml::Mapping::new(),
    };
    ext.insert(
        Value::String("bruno".into()),
        serde_yaml::from_str("scripts:\n  flow: sequential\nother: keep\n").expect("bruno ext"),
    );
    root.insert(ext_key, Value::Mapping(ext));
    fs::write(&oc_path, serde_yaml::to_string(&doc).expect("yaml")).expect("write opencollection");

    let mut settings = repo.get_settings("api").expect("settings");
    assert_eq!(settings.script_flow, ScriptFlow::Sequential);

    // A collection settings save keeps the flow and the sibling key.
    settings.docs = Some("changed".into());
    repo.save_settings("api", &settings).expect("save settings");
    let after = read_yaml(&oc_path);
    assert_eq!(
        after["extensions"]["bruno"]["scripts"]["flow"].as_str(),
        Some("sequential")
    );
    assert_eq!(after["extensions"]["bruno"]["other"].as_str(), Some("keep"));

    // A folder settings save never touches opencollection.yml.
    let before = fs::read_to_string(&oc_path).expect("read before folder save");
    repo.create_folder("api", "users").expect("create folder");
    repo.save_folder_settings("api", "users", &populated_settings(Auth::None))
        .expect("save folder settings");
    assert_eq!(
        fs::read_to_string(&oc_path).expect("read after folder save"),
        before
    );
    assert_eq!(
        repo.get_settings("api").expect("settings").script_flow,
        ScriptFlow::Sequential
    );

    // Switching back to the default is read back, and the sibling key still survives.
    settings.script_flow = ScriptFlow::Sandwich;
    repo.save_settings("api", &settings).expect("save sandwich");
    assert_eq!(
        repo.get_settings("api").expect("settings").script_flow,
        ScriptFlow::Sandwich
    );
    let last = read_yaml(&oc_path);
    assert_eq!(last["extensions"]["bruno"]["other"].as_str(), Some("keep"));
}
```

- [ ] **Step 11: Run the full module, format, lint**

Run: `cargo test -j4 -p rocket-infra schema_shape_tests`
Expected: all tests in the module pass.

Run: `cargo test -j4 -p rocket-infra fs_collection`
Expected: all pass. If an older test pinned that a folder variable keeps an `initial` equal to its value, update that test to the new rule (equal means omitted) and say so in the commit body.

Run: `cargo fmt -p rocket-infra` then `cargo clippy -j4 -p rocket-infra --tests`
Expected: no warnings.

- [ ] **Step 12: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only, then commit with the same pathspec:

```bash
git add crates/rocket-infra/src/fs_collection/schema_shape_tests.rs \
  crates/rocket-infra/src/fs_collection/variables.rs
git commit --only -m "..." -- crates/rocket-infra/src/fs_collection/schema_shape_tests.rs \
  crates/rocket-infra/src/fs_collection/variables.rs
```

If step 9 touched another file (the one that holds `save_folder_settings`), add it to both path lists. If steps 8 and 9 were skipped, drop `variables.rs`. Suggested subject: `test(infra): verify folder.yml stays Bruno compatible`.

---

## Task 2: End-to-end execution test through the real repo and executor

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create (Test): `crates/rocket-app/src/execution_service/folder_chain_e2e_tests.rs`
- Modify: `crates/rocket-app/src/execution_service.rs` (one `mod` line, next to `pub mod websocket_resolution;` at line 27)

**Interfaces:**
- Consumes: `RequestExecutionService::{new, with_script_engine, execute}`, `ExecuteRequestInput` (all fields are `pub`), `crate::graphql_request::test_support::input(url: &str) -> ExecuteRequestInput` (`pub(crate)`, test only), `crate::test_doubles::{EmptySecretManagerRepo, InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo, SharedHistoryRepo}`, `rocket_infra::{FsCollectionRepo, ReqwestExecutor}` (rocket-infra is a dev-dependency of rocket-app, as are `wiremock` and `tempfile`), `rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult}`, and the locked contract's `FolderSettings`, `ScriptFlow`, `CollectionSettings.script_flow`, `CollectionRepository::save_folder_settings`.
- Produces: the module `folder_chain_e2e_tests` with a `CodeRecorder` script engine and the tests `nested_request_gets_merged_headers_inherited_auth_and_sandwich_scripts` and `sequential_flow_runs_every_phase_outer_to_inner`.

Design notes. The real script engine (deno) is not constructed in `rocket-app` tests: `rocket-app` holds `Box<dyn ScriptEngine>` and every existing test uses a double. This test follows that convention and says so here. Its `CodeRecorder` records `(phase, ctx.code)` for each engine call. What is under test is the order in which code reaches the engine, so the assertions flatten the recorded code per phase and compare marker positions. That holds whether plan 06 hands the engine one call per script or one joined script. Everything else is real: the collection lives in a tempdir behind `FsCollectionRepo`, folder settings are written through `save_folder_settings`, and the HTTP request goes through `ReqwestExecutor` to a wiremock server whose recorded request is inspected.

- [ ] **Step 1: Register the module**

In `crates/rocket-app/src/execution_service.rs`, directly after `pub mod websocket_resolution;` add:

```rust
#[cfg(test)]
mod folder_chain_e2e_tests;
```

- [ ] **Step 2: Write the test module**

Create `crates/rocket-app/src/execution_service/folder_chain_e2e_tests.rs`:

```rust
//! Runs a request two folders deep through the real `FsCollectionRepo`, the real
//! `ReqwestExecutor` and a wiremock server. Scripts go to a recording engine, because only the
//! order in which code reaches the engine is under test.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rocket_collection::{
    CollectionRepository, CollectionSettings, FolderSettings, Request, ScriptFlow,
};
use rocket_infra::{FsCollectionRepo, ReqwestExecutor};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::error::DomainResult;
use rocket_shared::events::NullEventPublisher;
use rocket_shared::types::{Auth, Header, HttpMethod};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::RequestExecutionService;
use crate::graphql_request::test_support::input;
use crate::test_doubles::{
    EmptySecretManagerRepo, InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo, SharedHistoryRepo,
};

/// Records `(phase, code)` for every engine call and returns an empty result.
#[derive(Default)]
struct CodeRecorder {
    calls: Mutex<Vec<(String, String)>>,
}

#[async_trait]
impl ScriptEngine for CodeRecorder {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        self.calls
            .lock()
            .expect("lock")
            .push((ctx.phase.as_str().to_string(), ctx.code));
        Ok(ScriptResult::default())
    }
}

/// Hands one `Arc<CodeRecorder>` to a service expecting a `Box<dyn ScriptEngine>`.
struct SharedRecorder(Arc<CodeRecorder>);

#[async_trait]
impl ScriptEngine for SharedRecorder {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        self.0.execute(ctx).await
    }
}

/// Builds the collection `api` with folders `outer` and `outer/inner`, sends `outer/inner/ping.yml`
/// to a wiremock server, and returns the request the server saw plus the recorded script calls.
async fn run_ping(flow: ScriptFlow) -> (wiremock::Request, Vec<(String, String)>) {
    let dir = TempDir::new().expect("tempdir");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    repo.create("api").expect("create collection");
    repo.save_settings(
        "api",
        &CollectionSettings {
            headers: vec![Header::new("X-Col", "col"), Header::new("X-Shared", "col")],
            auth: Some(Auth::Bearer {
                token: "col-token".into(),
            }),
            script_flow: flow,
            ..Default::default()
        },
    )
    .expect("save collection settings");
    repo.create_folder("api", "outer").expect("create outer");
    repo.create_folder("api", "outer/inner")
        .expect("create inner");
    repo.save_folder_settings(
        "api",
        "outer",
        &FolderSettings {
            headers: vec![
                Header::new("X-Outer", "outer"),
                Header::new("X-Shared", "outer"),
                // A disabled folder header must not shadow the collection's X-Col.
                Header::disabled("X-Col", "shadow"),
            ],
            auth: Some(Auth::Basic {
                username: "u".into(),
                password: "p".into(),
            }),
            pre_request_script: Some("// outer-pre".into()),
            post_response_script: Some("// outer-post".into()),
            tests_script: Some("// outer-tests".into()),
            ..Default::default()
        },
    )
    .expect("save outer settings");
    repo.save_folder_settings(
        "api",
        "outer/inner",
        &FolderSettings {
            headers: vec![
                Header::new("X-Inner", "inner"),
                Header::new("X-Shared", "inner"),
            ],
            auth: Some(Auth::Bearer {
                token: "inner-token".into(),
            }),
            pre_request_script: Some("// inner-pre".into()),
            post_response_script: Some("// inner-post".into()),
            tests_script: Some("// inner-tests".into()),
            ..Default::default()
        },
    )
    .expect("save inner settings");

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ping"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let url = format!("{}/ping", server.uri());
    let rel = repo
        .save_request(
            "api",
            "outer/inner/ping.yml",
            &Request::new("Ping", HttpMethod::Get, url.clone()),
        )
        .expect("save request");

    let engine = Arc::new(CodeRecorder::default());
    let svc = RequestExecutionService::new(
        Box::new(NullEnvRepo),
        Arc::new(ReqwestExecutor::new()),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(repo),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(SharedRecorder(Arc::clone(&engine))));

    let mut req = input(&url);
    req.method = HttpMethod::Get;
    req.headers = vec![
        Header::new("X-Req", "req"),
        // The request replaces the inner folder's X-Inner.
        Header::new("X-Inner", "request"),
    ];
    req.auth = Auth::Inherit;
    req.collection = Some("api".into());
    req.request_path = Some(rel);
    req.request_name = Some("Ping".into());
    req.pre_request_script = Some("// req-pre".into());
    req.post_response_script = Some("// req-post".into());
    req.tests_script = Some("// req-tests".into());
    svc.execute(req).await.expect("execute");

    let mut seen = server
        .received_requests()
        .await
        .expect("request recording is on");
    assert_eq!(seen.len(), 1, "exactly one request reaches the server");
    let calls = engine.calls.lock().expect("lock").clone();
    (seen.remove(0), calls)
}

fn header<'r>(request: &'r wiremock::Request, name: &str) -> Option<&'r str> {
    request.headers.get(name).and_then(|v| v.to_str().ok())
}

/// The markers found in the code handed over for `phase`, in the order they appear.
fn order(calls: &[(String, String)], phase: &str, markers: &[&str]) -> Vec<String> {
    let text = calls
        .iter()
        .filter(|(p, _)| p == phase)
        .map(|(_, code)| code.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut found: Vec<(usize, &str)> = markers
        .iter()
        .filter_map(|m| text.find(m).map(|at| (at, *m)))
        .collect();
    found.sort_unstable();
    found.into_iter().map(|(_, m)| m.to_string()).collect()
}

#[tokio::test]
async fn nested_request_gets_merged_headers_inherited_auth_and_sandwich_scripts() {
    let (received, calls) = run_ping(ScriptFlow::Sandwich).await;

    assert_eq!(
        header(&received, "X-Col"),
        Some("col"),
        "a disabled folder header must not shadow the collection header"
    );
    assert_eq!(header(&received, "X-Outer"), Some("outer"));
    assert_eq!(header(&received, "X-Shared"), Some("inner"), "inner beats outer");
    assert_eq!(header(&received, "X-Inner"), Some("request"), "request wins");
    assert_eq!(header(&received, "X-Req"), Some("req"));
    assert_eq!(
        header(&received, "Authorization"),
        Some("Bearer inner-token"),
        "the innermost folder auth beats the outer folder and the collection"
    );

    assert_eq!(
        order(&calls, "before-request", &["outer-pre", "inner-pre", "req-pre"]),
        vec!["outer-pre", "inner-pre", "req-pre"]
    );
    assert_eq!(
        order(&calls, "after-response", &["outer-post", "inner-post", "req-post"]),
        vec!["req-post", "inner-post", "outer-post"]
    );
    assert_eq!(
        order(&calls, "tests", &["outer-tests", "inner-tests", "req-tests"]),
        vec!["req-tests", "inner-tests", "outer-tests"]
    );
}

#[tokio::test]
async fn sequential_flow_runs_every_phase_outer_to_inner() {
    let (_received, calls) = run_ping(ScriptFlow::Sequential).await;

    assert_eq!(
        order(&calls, "before-request", &["outer-pre", "inner-pre", "req-pre"]),
        vec!["outer-pre", "inner-pre", "req-pre"]
    );
    assert_eq!(
        order(&calls, "after-response", &["outer-post", "inner-post", "req-post"]),
        vec!["outer-post", "inner-post", "req-post"]
    );
    assert_eq!(
        order(&calls, "tests", &["outer-tests", "inner-tests", "req-tests"]),
        vec!["outer-tests", "inner-tests", "req-tests"]
    );
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -j4 -p rocket-app folder_chain_e2e_tests`
Expected on a correct plan 05 to 07 stack: both tests PASS on the first run, because this is a verification test of behavior built earlier. A compile error on `rocket_collection::ScriptFlow` or `save_folder_settings` means plan 01 or 02 did not re-export or implement the locked contract: fix it there. A failure on a header means plan 05 regressed. A failure on script order means plan 06 regressed.

- [ ] **Step 4: Prove the test can fail**

Temporarily delete the line `Header::new("X-Outer", "outer"),` from the `outer` settings in `run_ping`. Run `cargo test -j4 -p rocket-app nested_request_gets_merged_headers`.
Expected: FAIL at `assert_eq!(header(&received, "X-Outer"), Some("outer"))` with `left: None`. Restore the line and re-run: PASS.

- [ ] **Step 5: Format and lint**

Run: `cargo fmt -p rocket-app` then `cargo clippy -j4 -p rocket-app --tests`
Expected: no warnings. Run `cargo test -j4 -p rocket-app execution_service` once to confirm the existing execution tests still pass next to the new module.

- [ ] **Step 6: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add crates/rocket-app/src/execution_service.rs \
  crates/rocket-app/src/execution_service/folder_chain_e2e_tests.rs
git commit --only -m "..." -- crates/rocket-app/src/execution_service.rs \
  crates/rocket-app/src/execution_service/folder_chain_e2e_tests.rs
```

Suggested subject: `test(app): run a nested request through the folder chain`.

---

## Task 3: Documentation and manual verification

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `docs/superpowers/specs/opencollection-spec-reference.md`
- Create: `.claude/folder-settings.md`
- Modify: `CLAUDE.md` (one line)
- Modify: `crates/rocket-collection/CLAUDE.md`, `crates/rocket-infra/CLAUDE.md`

**Interfaces:** none (documentation only). Every name used below is in the locked contract.

- [ ] **Step 1: Confirm the real file locations**

Run these and keep the output next to you. The doc in step 4 names files, and a guessed path is worse than none.

```bash
rg -n "fn get_folder_settings|fn save_folder_settings|fn get_folder_chain_settings" crates --glob '*.rs'
rg -n "FolderSettingsSaved" crates src-tauri/src --glob '*.rs' -l
rg -n "get_folder_settings|save_folder_settings" src-tauri/src --glob '*.rs' -l
rg -n "openFolderTab|useFolderSettings|FolderSettingsTab" src -l
rg -n "chain_scripts|inherited_headers|resolve_folder_auth" crates/rocket-app/src -l
```

If a file named in step 4 differs from the output, use the output.

- [ ] **Step 2: Update the spec reference**

In `docs/superpowers/specs/opencollection-spec-reference.md` make these five edits with the Edit tool.

**2a. Section 2.7.** Replace

````markdown
request:                # RequestDefaults — inherited by children
  headers, metadata, auth, variables, scripts, settings
docs: string | { content, type } | null
```

### 2.8 ScriptFile
````

with

````markdown
request:                # RequestDefaults — inherited by children
  headers, metadata, auth, variables, scripts, settings
docs: string | { content, type } | null
```

**What Rocket edits in `folder.yml`** (the Folder Settings tab; design in `docs/superpowers/specs/2026-10-07-folder-settings-design.md`):

| Section | Shape | Notes |
|---|---|---|
| `request.headers` | `[ { name, value, description, disabled } ]` | Disabled entries never shadow an outer header. |
| `request.auth` | `Auth` or `"inherit"` | Every auth type is allowed on a folder. Absent and `inherit` both mean "no folder auth". |
| `request.variables` | `[ Variable ]` | Pre-request only. There is no post-response variable slot in `RequestDefaults`. |
| `request.scripts` | `[ { type, code } ]` | Types `before-request`, `after-response`, `tests`. A `hooks` entry is kept untouched. |
| `docs` | string or `{ content, type }` | Rocket reads both. It writes a plain string. |

Rocket leaves `request.metadata`, `request.settings`, `info.seq` and any `hooks` script exactly as it found them on every save. An empty section is omitted, never written as an empty list. Folder `request.variables` never carry `initial`, and each keeps the description of the existing entry with the same name. `info.uid` is the existing `FolderInfo.uid` deviation and is never added to a file that lacks it. Rocket has no `folder.bru` importer: a Bruno folder imported from `.bru` files loses its folder-level settings (a tracked follow-up).

### 2.8 ScriptFile
````

**2b. Section 5, `opencollection.yml` bullet.** Replace

```markdown
- `opencollection.yml` at collection root — contains `info`, `config`, `request` (defaults), `docs`. The `items[]` array is NOT written here; items live as individual files.
```

with

```markdown
- `opencollection.yml` at collection root — contains `info`, `config`, `request` (defaults), `docs`. The `items[]` array is NOT written here; items live as individual files.
- `extensions.bruno.scripts.flow` in `opencollection.yml` is `sequential`, or absent for the default `sandwich`. It is the script order for the whole collection, and it is the only place the order is stored (there is no per-folder flow). Rocket reads it, honors it and keeps it, with every other key under `extensions`, on every settings save. `extensions` is the schema's free-form object, so this is not a deviation.
```

**2c. Section 5, `folder.yml` bullet.** Replace

```markdown
- `folder.yml` at each folder root — contains `info`, `request` (defaults), `docs`. No `items[]` array.
```

with

```markdown
- `folder.yml` at each folder root — contains `info`, `request` (defaults), `docs`. No `items[]` array. Rocket writes `request.headers`, `request.auth`, `request.variables`, `request.scripts` and `docs` here (see section 2.7), and keeps `request.metadata` and `request.settings` as found. No Rocket-only fields.
```

**2d. Section 6.** Directly after the line `- Runtime variables exist in memory only — never serialised to disk.` insert:

````markdown

### 6.1 Headers, auth and scripts follow the same chain

Variables are not the only thing a folder contributes. The chain is always collection, then folders from the outermost to the innermost, then the request.

**Headers.** Lowest to highest: collection `request.headers`, each folder's `request.headers`, the request's own headers. A more specific level replaces an enabled header of the same name from a less specific level. The match is on the exact header name, the same as today's collection-versus-request merge, so `x-id` and `X-Id` are two headers. A disabled header never replaces another one.

**Auth.** A request whose auth is `inherit` (or `none`, which Rocket treats the same, see section 3.1) takes the auth of the nearest folder whose auth is set and is not `inherit`, then the collection's auth, then none. A folder whose auth is absent or `inherit` passes the lookup to its parent. An explicit request auth always wins.

**Scripts.** Each phase gets the request's script plus every folder's script of that phase, in an order set by `extensions.bruno.scripts.flow`:

| Phase | `sandwich` (default) | `sequential` |
|---|---|---|
| before-request | outer folder, inner folder, request | outer folder, inner folder, request |
| after-response | request, inner folder, outer folder | outer folder, inner folder, request |
| tests | request, inner folder, outer folder | outer folder, inner folder, request |

A blank script is skipped. Folder scripts use the same sandbox mode, `require()` rules and secret-use scan as request scripts, and an error in one surfaces as `script_error` in phase order. Scripts stored in `opencollection.yml` `request.scripts` are preserved on save but are not run: `CollectionSettings` has no script field.

A `folder.yml` that fails to parse during a send is an error that names the folder. It is never silently dropped.
````

**2e. Section 10.** Replace

```markdown
- Always walk the full ancestor folder chain when resolving folder variables.
```

with

```markdown
- Always walk the full ancestor folder chain when resolving folder variables, headers, auth and scripts.
```

- [ ] **Step 3: Review the spec reference against shipped behavior**

Read the five edits once against the code, not against memory:

```bash
rg -n "fn merge_headers|fn merge_auth|fn inherited_headers|fn resolve_folder_auth|fn chain_scripts" crates --glob '*.rs'
```

Check each of these and correct the doc if the code disagrees: header names match exactly (no lowercasing in `merge_headers` or `inherited_headers`); `Auth::None` and `Auth::Inherit` on a request both take the parent's auth; a blank script is skipped in `chain_scripts`. If `inherited_headers` lowercases names or `chain_scripts` runs a collection script, change the sentence in 2d to match the code. Do not change the code in this plan.

- [ ] **Step 4: Create `.claude/folder-settings.md`**

Create `.claude/folder-settings.md` with exactly this content (fix any path that step 1 showed differently):

```markdown
# Folder settings

Clicking a folder in the sidebar opens a Folder Settings tab with the sub-tabs Headers, Script, Test, Vars, Auth and Docs. The settings live in the folder's `folder.yml` and apply to every request below it when it runs.

Design: `docs/superpowers/specs/2026-10-07-folder-settings-design.md`. Plans and the locked names: `docs/superpowers/plans/folder-settings/00-plan-index.md`. On-disk shape and resolution order: `docs/superpowers/specs/opencollection-spec-reference.md` sections 2.7, 5 and 6.

## Where it lives

- `rocket-collection`, module `folder_settings`: `FolderSettings`, `ScriptFlow`, `ScriptPhase` and the pure helpers `inherited_headers`, `resolve_folder_auth`, `chain_scripts`. No I/O. `CollectionSettings.script_flow` holds the collection's flow.
- `rocket-collection` `CollectionRepository`: `get_folder_settings`, `save_folder_settings`, `get_folder_chain_settings`. All three have defaults, so existing test doubles still compile. `get_folder_variables`, `save_folder_variables` and `get_folder_chain_variables` read and write the same `request.variables`.
- `rocket-infra` `fs_collection/folder_file.rs`: reads and writes `folder.yml` (spec shape, legacy shape fallback). The settings methods are in `fs_collection/folders.rs` and `fs_collection/variables.rs`. `fs_collection/settings.rs` reads and writes `extensions.bruno.scripts.flow` in `opencollection.yml`.
- `rocket-shared` events: `DomainEvent::FolderSettingsSaved { collection, folder_path }`, published after a save.
- `rocket-app` `execution_service.rs`: `resolve_request` merges folder headers and auth, and the script phases use `chain_scripts`. The folder chain is loaded once per execution with `get_folder_chain_settings`. Folder scope for scripts is the `rok` folder-variable getter (plan 07).
- `src-tauri/src/commands/`: thin `get_folder_settings` and `save_folder_settings` commands with a camelCase `FolderSettingsDto`.
- Frontend: `src/lib/tauri-api.ts` (`getFolderSettings`, `saveFolderSettings`, the `FolderSettings` type), `src/stores/pane-store.ts` (`openFolderTab`, `updateFolderSection`), `src/hooks/useFolderSettings.ts`, `src/components/collections/FolderSettingsTab.tsx` with its sub-tab components beside it, and `src/components/collections/FolderNode.tsx` (click opens the tab, the chevron still expands).
- Tests: Bruno compatibility in `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`; end to end execution in `crates/rocket-app/src/execution_service/folder_chain_e2e_tests.rs`.

## Rules that are easy to break

- `folder.yml` is strictly OpenCollection (`additionalProperties: false`). No Rocket-only keys. The schema guard in `schema_shape_tests.rs` fails if one appears. Do not add to `KNOWN_DEFERRED` for a folder key.
- A save reads the file first and only replaces `headers`, `auth`, `variables`, the three typed scripts and `docs`. `request.metadata`, `request.settings`, `info.seq` and `hooks` scripts must survive. An empty section is omitted.
- `CollectionVariable` has no description field. Folder saves keep each variable's existing description by name (`folder_oc_variables` in `fs_collection/variables.rs`). Loading fills `initial_value` from `value`, so an equal pair means no initial value and `initial` is not written.
- `docs` is read as a string or `{ content, type }` and written as a plain string.
- Absent auth and `inherit` both mean no folder auth. A request set to `none` also inherits (existing known difference, spec reference section 3.1).
- Header names match exactly (no case folding), like the collection-versus-request merge. A disabled header never shadows another.
- Script flow is per collection, at `extensions.bruno.scripts.flow`. There is no per-folder flow and no UI for it. `save_settings` must keep every other key under `extensions`.
- Collection-level scripts in `opencollection.yml` are preserved but not run. Only folder and request scripts are chained.
- Folder variables are pre-request only. A script can still set variables after a response.
- A `folder.yml` that does not parse during a send is an error naming the folder. The sidebar tree load is more forgiving and must stay light, which is why `FolderSettings` is not a field of `Folder`.
- Scripts that use `bru.*` stay non-portable. There is no `bru` alias, parity goes in the `rok` namespace.

## Deferred

`folder.bru` import, folder post-response variables, per-folder script flow, a UI for the script flow.
```

- [ ] **Step 5: Link it from the project CLAUDE.md**

In `CLAUDE.md`, replace

```markdown
See `.claude/script-files.md` for shared .js script files and local require().
```

with

```markdown
See `.claude/script-files.md` for shared .js script files and local require().
See `.claude/folder-settings.md` for the folder settings tab, `folder.yml` sections and inheritance.
```

- [ ] **Step 6: Update the crate guides**

In `crates/rocket-collection/CLAUDE.md`, replace

```markdown
- **`docs: Option<String>`** — optional markdown documentation for the collection (maps to `docs:` in opencollection.yml).
```

with

```markdown
- **`docs: Option<String>`** — optional markdown documentation for the collection (maps to `docs:` in opencollection.yml).
- **`script_flow: ScriptFlow`** — `Sandwich` (default) or `Sequential`, serde lowercase. Stored at `extensions.bruno.scripts.flow` in opencollection.yml, not in a Rocket-only field.

## FolderSettings

`FolderSettings` (module `folder_settings`, re-exported at the crate root) is the editable content of a folder's `folder.yml`: `headers`, `auth`, `variables` (pre-request only), `pre_request_script`, `post_response_script`, `tests_script` and `docs`. It is a separate value object, not a field of `Folder`, so the sidebar load stays light. Pure helpers, no I/O:

- `inherited_headers(collection, folders)`: collection then folders outermost first; inner replaces outer by exact name; a disabled entry never shadows.
- `resolve_folder_auth(folders)`: innermost folder auth that is not `None` or `Inherit`.
- `chain_scripts(phase, flow, folders, request_script)`: scripts for one `ScriptPhase` in execution order. Sandwich runs post-response and tests request first, then folders inner to outer. Blank scripts are skipped.

`CollectionRepository::{get_folder_settings, save_folder_settings, get_folder_chain_settings}` all have defaults (`Err(Internal)`, `Err(Internal)`, `Ok(vec![])`) so existing test doubles keep compiling. `folder_path` is relative to the collection root, `""` for the root, and the chain is outermost first.
```

In `crates/rocket-infra/CLAUDE.md`, replace

```markdown
**UID storage.** UIDs are stored inside `opencollection.yml` and `folder.yml`.
```

with

```markdown
**Folder settings.** `get_folder_settings`, `save_folder_settings` and `get_folder_chain_settings` read and write `folder.yml`'s `request.headers`, `request.auth`, `request.variables`, `request.scripts` (types `before-request`, `after-response`, `tests`) and `docs`. A save starts from the file as it is, so `request.metadata`, `request.settings`, `info.seq` and `hooks` scripts are kept, and an empty section is omitted. Folder variables go through `folder_oc_variables` (`fs_collection/variables.rs`): `CollectionVariable` has no description, so each entry keeps the existing description of the same name, and `initial` is left out when it equals `value`. `docs` is written as a plain string. A `folder.yml` that fails to parse is an error naming the folder during execution. The collection's script flow is read and written at `extensions.bruno.scripts.flow` in `opencollection.yml` by `fs_collection/settings.rs`, and every other `extensions` key survives a settings save. Bruno compatibility is guarded by `fs_collection/schema_shape_tests.rs` (see `.claude/folder-settings.md`).

**UID storage.** UIDs are stored inside `opencollection.yml` and `folder.yml`.
```

If `folder_oc_variables` was not needed in Task 1 (steps 8 and 9 skipped), delete the `folder_oc_variables` sentence from both `.claude/folder-settings.md` and the `rocket-infra` paragraph, and describe what plan 02 does instead.

- [ ] **Step 7: Check the docs**

Run: `git diff --stat -- docs/superpowers/specs/opencollection-spec-reference.md CLAUDE.md crates/rocket-collection/CLAUDE.md crates/rocket-infra/CLAUDE.md` and `git status --short .claude/folder-settings.md`
Expected: five files changed or created, no source files. Confirm `CLAUDE.md` grew by one line.

Run: `rg -n "TBD|TODO|XXX" .claude/folder-settings.md`
Expected: no matches.

- [ ] **Step 8: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add docs/superpowers/specs/opencollection-spec-reference.md .claude/folder-settings.md \
  CLAUDE.md crates/rocket-collection/CLAUDE.md crates/rocket-infra/CLAUDE.md
git commit --only -m "..." -- docs/superpowers/specs/opencollection-spec-reference.md \
  .claude/folder-settings.md CLAUDE.md crates/rocket-collection/CLAUDE.md \
  crates/rocket-infra/CLAUDE.md
```

Suggested subject: `docs: document folder settings and update the spec reference`.

- [ ] **Step 9: Final automated gate**

Run, in this order (single crate each, never `--workspace`):

```bash
cargo check -j4 -p rocket-collection
cargo check -j4 -p rocket-infra
cargo check -j4 -p rocket-app
cargo test -j4 -p rocket-infra schema_shape_tests
cargo test -j4 -p rocket-app folder_chain_e2e_tests
yarn tsc --noEmit
yarn check
```

Expected: everything passes. A frontend failure here belongs to plans 08 to 11: fix it there, not by touching this plan's files.

- [ ] **Step 10: Manual verification checklist (the user performs this)**

Automated tests cannot see the real window, so this step is for the user. Do not tick it on the user's behalf.

Run `yarn tauri dev`, then in a throwaway collection with a folder `outer` that contains a folder `inner` and a request `ping` in `inner` pointing at an echo endpoint (for example `https://httpbin.org/anything`):

- [ ] Click folder `outer` in the sidebar. A Folder Settings tab opens with Headers, Script, Test, Vars, Auth and Docs. Clicking the chevron still expands and collapses without opening a tab. The folder menu has Settings.
- [ ] Clicking `outer` again focuses the same tab and does not open a second one. Open `inner` and confirm it gets its own tab.
- [ ] Headers: in `outer` add `X-Outer: outer` and a disabled `X-Col: shadow`. In `inner` add `X-Shared: inner`. Save. The Save button clears its dirty marker. Send `ping` and confirm the echo shows `X-Outer` and `X-Shared`, and no `X-Col`.
- [ ] Vars: add `region = eu` in `outer`, save, reload the app window, and confirm it is still there. Use `{{region}}` in the request URL or a header and confirm it resolves.
- [ ] Auth: set `outer` to Bearer `outer-token`, then set `inner` to `inherit`. Set the request's auth to Inherit. Send `ping`. The echo shows `Authorization: Bearer outer-token`. Change `inner` to Bearer `inner-token`, save, send again: it shows `inner-token`.
- [ ] Script: in `outer` add a pre-request script `console.log('outer pre')` and in `inner` `console.log('inner pre')`, and a post-response script in each that logs `outer post` and `inner post`. Add `console.log('req post')` to the request's own post-response script. Send. The console shows pre: outer, inner. Post: request, inner, outer.
- [ ] Test: add `test('folder test', function () {})` in `outer`'s Test tab. Send and confirm it appears among the test results.
- [ ] Docs: write Markdown in `outer`'s Docs tab, save, reload, and confirm it renders.
- [ ] Open `outer/folder.yml` in a text editor. It has only `info`, `request` and `docs`. `request` has only `headers`, `auth`, `variables`, `scripts`. No `scriptFlow`, `initialValue` or empty lists.
- [ ] Edit `outer/folder.yml` by hand: add a `request.settings.timeout: 5000` and a `hooks` script, reload the collection, change a header in the tab and save. Both hand-added keys are still in the file.
- [ ] Add `extensions:\n  bruno:\n    scripts:\n      flow: sequential` to `opencollection.yml`, reload, and send `ping` again. The post-response order is now outer, inner, request.
- [ ] Optional: open the collection folder in Bruno. The folder settings show in its folder dialog and the collection opens without a schema error.

Report any failing item with the folder tab's state and the `folder.yml` text, so the failure can be traced to the owning plan.

---

## Next Plan

**Execution order:** this is plan 12 of 12, the last plan. There is no next plan to start.

**Instruction for the executing Claude:** when every task in this plan is done, record it in the ledger at `.superpowers/sdd/folder-settings/progress.md`, run the final whole-branch review, and then **stop and report to the user**. Do not start any other work, and do not merge until the user has finished the manual checklist in Task 3 and approved.

**Notes (from the plan author):**

None. This is plan 12 of 12, so the folder settings series is complete. Mark it done in the plan ledger (`.superpowers/sdd/folder-settings/progress.md`) and merge after the user finishes the manual checklist in Task 3 step 10 and a final whole-branch review.

Deferred follow-ups from the spec, each to be planned on its own when wanted:

1. Import of Bruno `.bru`-format `folder.bru` files (folder headers, auth, scripts, vars and docs from a `.bru` collection).
2. Folder post-response variables. The spec's `RequestDefaults` has no slot for them, so this needs a spec decision first. Scripts can already set variables after a response.
3. A per-folder script flow. Bruno sets the flow per collection, so this would be a Rocket-only concept and must not be written into `folder.yml`.
4. A UI for the script flow (`extensions.bruno.scripts.flow`), for example a setting on the collection overview tab. Today it is read, honored and preserved, but only edited by hand in `opencollection.yml`.
