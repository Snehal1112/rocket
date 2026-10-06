# JS Script Files Plan 01: Local require() Engine

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scripts can `require('./x.js')` and `require('../shared/y.js')`, with reads limited to the collection root (Safe mode) or the collection root plus `additionalContextRoots` (Developer mode).

**Architecture:** `rocket-scripting` gains a plain-data `ScriptFileScope` on `ScriptContext`. `rocket-infra` gets a pure resolver (`local_modules.rs`), a sync Deno op `op_require_local` that reads the roots from `OpState`, and a `bootstrap.js` loader with a per-run cache. `rocket-app` builds the scope from collection settings and the new `CollectionRepository::collection_root_path`.

**Tech Stack:** Rust, deno_core ops, `tempfile` for fixtures, `serde_yaml` for `opencollection.yml`.

**Spec:** `docs/superpowers/specs/2026-10-06-js-script-files-design.md` (Sections 1 and "Config"). Plan index: `00-plan-index.md`.

## Global Constraints

- `-j4` on every cargo invocation. No `cargo test --workspace`.
- No `unwrap()` in production paths. Tests may use `expect("...")`.
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only. `CollectionSettings` is already an IPC DTO. The persistence layer is the `Oc*` structs and the `extensions` YAML value, which get no rename.
- Persistence stays backward compatible: a missing `scripts` key means an empty roots list.
- Code comments: short full sentences ending in a period.
- Commits: conventional commits, created through `dev-workflow-skills:1-git-commit`, staging by path.
- Each task starts with: read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Review Focus

- A symlink inside the collection that points outside it: must be denied (Task 1 test, Task 2 engine test).
- `require('./data.txt')` or `require('../.env')`: must throw "Only .js files can be required", never run or echo file content.
- A module that throws while loading, then is required again: the second require must retry, not return a half-built object (Task 2).
- Two modules that require each other: no infinite loop, partial exports (Task 2).
- `additionalContextRoots` entry that does not exist: ignored, no crash (Task 1).
- Collection with `opencollection.yml` that has other `extensions` keys: round trip keeps them (Task 3).

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/rocket-scripting/src/context.rs` | modify | `ScriptFileScope`, `file_scope` field, `with_file_scope` |
| `crates/rocket-scripting/src/lib.rs` | modify | export `ScriptFileScope` |
| `crates/rocket-infra/src/scripting/local_modules.rs` | create | pure root building and path resolution |
| `crates/rocket-infra/src/scripting/ops/modules.rs` | create | `op_require_local` |
| `crates/rocket-infra/src/scripting/ops/mod.rs` | modify | `pub mod modules;` |
| `crates/rocket-infra/src/scripting/mod.rs` | modify | `pub mod local_modules;` |
| `crates/rocket-infra/src/scripting/state.rs` | modify | `local_roots` on `ScriptInputState` |
| `crates/rocket-infra/src/scripting/engine.rs` | modify | register op, seed state, tests |
| `crates/rocket-infra/src/scripting/bootstrap.js` | modify | local loader and cache |
| `crates/rocket-collection/src/settings.rs` | modify | `script_context_roots` |
| `crates/rocket-collection/src/repository.rs` | modify | `collection_root_path` default method |
| `crates/rocket-infra/src/fs_collection/settings.rs` | modify | read and write `extensions.rocketapi.scripts` |
| `crates/rocket-infra/src/fs_collection/mod.rs` | modify | implement `collection_root_path` |
| `crates/rocket-infra/src/shared_path_collection_repo.rs` | modify | delegate `collection_root_path` |
| `crates/rocket-app/src/execution_service.rs` | modify | build and pass the scope |

---

### Task 1: Scope type and pure resolver

**Files:**
- Modify: `crates/rocket-scripting/src/context.rs`, `crates/rocket-scripting/src/lib.rs`
- Create: `crates/rocket-infra/src/scripting/local_modules.rs`
- Modify: `crates/rocket-infra/src/scripting/mod.rs`
- Test: inline `#[cfg(test)]` module in `local_modules.rs`; a context test in `context.rs`

**Interfaces:**
- Produces (`rocket_scripting`):
  - `pub struct ScriptFileScope { pub collection_root: PathBuf, pub additional_roots: Vec<PathBuf> }` (`Debug, Clone, PartialEq, Eq`)
  - `ScriptContext.file_scope: Option<ScriptFileScope>` and `fn with_file_scope(self, scope: Option<ScriptFileScope>) -> Self`
- Produces (`rocket_infra::scripting::local_modules`):
  - `pub struct LocalRoots { pub collection_root: PathBuf, pub roots: Vec<PathBuf> }`
  - `pub struct ResolvedModule { pub path: PathBuf, pub dir: PathBuf, pub source: String }`
  - `pub fn build_roots(scope: &ScriptFileScope, mode: SandboxMode) -> Result<LocalRoots, String>`
  - `pub fn resolve_local_module(roots: &LocalRoots, from_dir: &Path, name: &str) -> Result<ResolvedModule, String>`
  - `pub fn is_local_specifier(name: &str) -> bool`

- [ ] **Step 1: Read the spec reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Add the scope type to `rocket-scripting`**

In `crates/rocket-scripting/src/context.rs` add `use std::path::PathBuf;` to the imports, then add this above `pub struct ScriptContext`:

```rust
/// Where a script may load local `.js` files from.
///
/// Plain data only. `rocket-infra` does the file reading and the root checks.
/// Whether `additional_roots` is honoured depends on `ScriptContext.sandbox_mode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptFileScope {
    /// Absolute path of the collection directory.
    pub collection_root: PathBuf,
    /// Extra roots from `additionalContextRoots`. Relative entries are resolved
    /// against `collection_root` by the engine.
    pub additional_roots: Vec<PathBuf>,
}
```

Add the field to `ScriptContext` after `path_params`:

```rust
    /// Local-file `require()` scope. `None` disables local requires.
    pub file_scope: Option<ScriptFileScope>,
```

In each of the three constructors (`before_request`, `after_response`, `tests`) add `file_scope: None,` after `path_params,`. Add the builder after `with_sandbox_mode`:

```rust
    /// Sets the local-file `require()` scope. `rocket-app` builds it from the
    /// collection's location and `additionalContextRoots` setting.
    pub fn with_file_scope(mut self, scope: Option<ScriptFileScope>) -> Self {
        self.file_scope = scope;
        self
    }
```

In `crates/rocket-scripting/src/lib.rs` change the context export to:

```rust
pub use context::{ExecutionMode, SandboxMode, ScriptContext, ScriptFileScope};
```

Add this test inside the existing `tests` module of `context.rs`:

```rust
    #[test]
    fn file_scope_defaults_to_none_and_can_be_set() {
        let ctx = ScriptContext::before_request(
            String::new(),
            VariableContext::default(),
            stub_request(),
            None,
            String::new(),
            vec![],
            vec![],
        );
        assert!(ctx.file_scope.is_none());
        let scope = ScriptFileScope {
            collection_root: "/tmp/col".into(),
            additional_roots: vec!["../shared".into()],
        };
        let ctx = ctx.with_file_scope(Some(scope.clone()));
        assert_eq!(ctx.file_scope, Some(scope));
    }
```

- [ ] **Step 3: Fix the struct-literal call sites**

Run: `cargo check -j4 -p rocket-infra --tests`
Expected: FAIL with "missing field `file_scope`" in `crates/rocket-infra/src/scripting/engine.rs` (the `minimal_ctx` helper and three other `ScriptContext { ... }` literals near lines 335, 1190, 1571, 1595, 1632). Add `file_scope: None,` after `sandbox_mode: ...,` in each literal. Re-run until it compiles.

- [ ] **Step 4: Write the failing resolver tests**

Create `crates/rocket-infra/src/scripting/local_modules.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    struct Fixture {
        _tmp: TempDir,
        col: PathBuf,
        outer: PathBuf,
        shared: PathBuf,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = tmp.path().canonicalize().expect("canonicalize");
        let col = base.join("col");
        let shared = base.join("shared");
        fs::create_dir_all(col.join("lib")).expect("mkdir col/lib");
        fs::create_dir_all(&shared).expect("mkdir shared");
        fs::write(col.join("utils.js"), "module.exports = 1;").expect("write utils");
        fs::write(col.join("lib/helper.js"), "module.exports = 2;").expect("write helper");
        fs::write(col.join("data.txt"), "secret").expect("write data");
        fs::write(base.join("outer.js"), "module.exports = 3;").expect("write outer");
        fs::write(shared.join("common.js"), "module.exports = 4;").expect("write common");
        Fixture {
            _tmp: tmp,
            col,
            outer: base,
            shared,
        }
    }

    fn roots(f: &Fixture, mode: SandboxMode) -> LocalRoots {
        let scope = ScriptFileScope {
            collection_root: f.col.clone(),
            additional_roots: vec![PathBuf::from("../shared")],
        };
        build_roots(&scope, mode).expect("build roots")
    }

    #[test]
    fn resolves_with_and_without_js_extension() {
        let f = fixture();
        let r = roots(&f, SandboxMode::Safe);
        let a = resolve_local_module(&r, &r.collection_root, "./utils.js").expect("with ext");
        let b = resolve_local_module(&r, &r.collection_root, "./utils").expect("without ext");
        assert_eq!(a.path, b.path);
        assert_eq!(a.source, "module.exports = 1;");
        assert_eq!(a.dir, r.collection_root);
    }

    #[test]
    fn resolves_relative_to_the_requiring_directory() {
        let f = fixture();
        let r = roots(&f, SandboxMode::Safe);
        let from = r.collection_root.join("lib");
        let m = resolve_local_module(&r, &from, "../utils").expect("parent require");
        assert_eq!(m.source, "module.exports = 1;");
        let m = resolve_local_module(&r, &r.collection_root, "./lib/helper").expect("child");
        assert_eq!(m.dir, r.collection_root.join("lib"));
    }

    #[test]
    fn missing_module_reports_cannot_find() {
        let f = fixture();
        let r = roots(&f, SandboxMode::Safe);
        let err = resolve_local_module(&r, &r.collection_root, "./nope").expect_err("missing");
        assert!(err.contains("Cannot find module './nope'"), "got: {err}");
    }

    #[test]
    fn parent_escape_is_denied() {
        let f = fixture();
        let r = roots(&f, SandboxMode::Safe);
        let err = resolve_local_module(&r, &r.collection_root, "../outer.js").expect_err("escape");
        assert!(err.contains("outside the allowed script roots"), "got: {err}");
        assert!(err.contains(&f.col.display().to_string()), "lists roots: {err}");
    }

    #[test]
    fn absolute_path_outside_roots_is_denied() {
        let f = fixture();
        let r = roots(&f, SandboxMode::Safe);
        let abs = f.outer.join("outer.js").display().to_string();
        let err = resolve_local_module(&r, &r.collection_root, &abs).expect_err("absolute");
        assert!(err.contains("outside the allowed script roots"), "got: {err}");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_denied() {
        let f = fixture();
        std::os::unix::fs::symlink(f.outer.join("outer.js"), f.col.join("link.js"))
            .expect("symlink");
        let r = roots(&f, SandboxMode::Safe);
        let err = resolve_local_module(&r, &r.collection_root, "./link.js").expect_err("symlink");
        assert!(err.contains("outside the allowed script roots"), "got: {err}");
    }

    #[test]
    fn non_js_extension_is_rejected_without_reading() {
        let f = fixture();
        let r = roots(&f, SandboxMode::Safe);
        let err = resolve_local_module(&r, &r.collection_root, "./data.txt").expect_err("txt");
        assert!(err.contains("Only .js files can be required"), "got: {err}");
        assert!(!err.contains("secret"), "must not echo content: {err}");
    }

    #[test]
    fn additional_roots_only_apply_in_developer_mode() {
        let f = fixture();
        let safe = roots(&f, SandboxMode::Safe);
        let err = resolve_local_module(&safe, &safe.collection_root, "../shared/common.js")
            .expect_err("safe denies");
        assert!(err.contains("outside the allowed script roots"), "got: {err}");

        let dev = roots(&f, SandboxMode::Developer);
        let m = resolve_local_module(&dev, &dev.collection_root, "../shared/common.js")
            .expect("developer allows");
        assert_eq!(m.source, "module.exports = 4;");
        assert_eq!(dev.roots.len(), 2);
        assert!(dev.roots.contains(&f.shared));
    }

    #[test]
    fn missing_additional_root_is_ignored() {
        let f = fixture();
        let scope = ScriptFileScope {
            collection_root: f.col.clone(),
            additional_roots: vec![PathBuf::from("../does-not-exist")],
        };
        let r = build_roots(&scope, SandboxMode::Developer).expect("build");
        assert_eq!(r.roots.len(), 1);
    }

    #[test]
    fn missing_collection_root_is_an_error() {
        let scope = ScriptFileScope {
            collection_root: "/definitely/not/here".into(),
            additional_roots: vec![],
        };
        assert!(build_roots(&scope, SandboxMode::Safe).is_err());
    }

    #[test]
    fn local_specifier_detection() {
        assert!(is_local_specifier("./a"));
        assert!(is_local_specifier("../a"));
        assert!(is_local_specifier("/abs/a.js"));
        assert!(is_local_specifier(".\\a"));
        assert!(!is_local_specifier("lodash"));
        assert!(!is_local_specifier("crypto-js"));
    }
}
```

Add `pub mod local_modules;` to `crates/rocket-infra/src/scripting/mod.rs`.

- [ ] **Step 5: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra local_modules`
Expected: FAIL to compile (`build_roots`, `LocalRoots` not found).

- [ ] **Step 6: Implement the resolver**

Put this at the top of `local_modules.rs`, above the test module:

```rust
//! Pure path logic for `require()` of local `.js` files.
//!
//! Every path is canonicalised before the root check, so `..` segments and
//! symlinks cannot escape the allowed roots.

use std::path::{Path, PathBuf};

use rocket_scripting::{SandboxMode, ScriptFileScope};

/// The canonical directories a script may read `.js` files from.
#[derive(Debug, Clone)]
pub struct LocalRoots {
    /// Canonical collection directory. Top-level scripts resolve from here.
    pub collection_root: PathBuf,
    /// Every allowed canonical root, the collection root first.
    pub roots: Vec<PathBuf>,
}

/// A local module that passed every check.
#[derive(Debug, Clone)]
pub struct ResolvedModule {
    /// Canonical file path.
    pub path: PathBuf,
    /// Canonical directory of the file, used as the base for its own requires.
    pub dir: PathBuf,
    /// File text.
    pub source: String,
}

/// Returns true for `./x`, `../x`, `/abs` and their backslash forms.
pub fn is_local_specifier(name: &str) -> bool {
    name == "."
        || name == ".."
        || name.starts_with("./")
        || name.starts_with("../")
        || name.starts_with('/')
        || name.starts_with(".\\")
        || name.starts_with("..\\")
}

/// Builds the allowed roots. Safe mode allows the collection root only.
/// Developer mode adds each existing `additional_roots` entry.
pub fn build_roots(scope: &ScriptFileScope, mode: SandboxMode) -> Result<LocalRoots, String> {
    let collection_root = scope
        .collection_root
        .canonicalize()
        .map_err(|e| format!("collection directory is not available: {e}"))?;
    let mut roots = vec![collection_root.clone()];
    if mode == SandboxMode::Developer {
        for extra in &scope.additional_roots {
            let joined = if extra.is_absolute() {
                extra.clone()
            } else {
                collection_root.join(extra)
            };
            // A root that does not exist is skipped, not an error.
            if let Ok(canonical) = joined.canonicalize() {
                if !roots.contains(&canonical) {
                    roots.push(canonical);
                }
            }
        }
    }
    Ok(LocalRoots {
        collection_root,
        roots,
    })
}

fn with_js_suffix(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_owned();
    os.push(".js");
    PathBuf::from(os)
}

/// Resolves `name` against `from_dir` and loads the file.
///
/// Lookup order: the name as written when it has an extension, then `name.js`.
pub fn resolve_local_module(
    roots: &LocalRoots,
    from_dir: &Path,
    name: &str,
) -> Result<ResolvedModule, String> {
    let name = name.replace('\\', "/");
    let requested = Path::new(&name);
    let base = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        from_dir.join(requested)
    };

    let mut tries = Vec::new();
    if base.extension().is_some() {
        tries.push(base.clone());
    }
    tries.push(with_js_suffix(&base));

    let found = tries
        .iter()
        .find_map(|candidate| {
            candidate
                .canonicalize()
                .ok()
                .filter(|canonical| canonical.is_file())
        })
        .ok_or_else(|| format!("Cannot find module '{name}'"))?;

    if !roots.roots.iter().any(|root| found.starts_with(root)) {
        let allowed = roots
            .roots
            .iter()
            .map(|r| r.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Access to '{name}' is outside the allowed script roots: {allowed}"
        ));
    }
    if found.extension().and_then(|e| e.to_str()) != Some("js") {
        return Err(format!("Only .js files can be required: '{name}'"));
    }

    let source = std::fs::read_to_string(&found)
        .map_err(|e| format!("Cannot read module '{name}': {e}"))?;
    let dir = found
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("Cannot find module '{name}'"))?;
    Ok(ResolvedModule {
        path: found,
        dir,
        source,
    })
}
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra local_modules && cargo test -j4 -p rocket-scripting file_scope`
Expected: PASS for all.

- [ ] **Step 8: Commit**

Invoke `dev-workflow-skills:1-git-commit` for:
`git add crates/rocket-scripting/src/context.rs crates/rocket-scripting/src/lib.rs crates/rocket-infra/src/scripting/local_modules.rs crates/rocket-infra/src/scripting/mod.rs crates/rocket-infra/src/scripting/engine.rs`
Suggested subject: `feat(scripting): add local module resolver and file scope`

---

### Task 2: Op, bootstrap loader and engine tests

**Files:**
- Create: `crates/rocket-infra/src/scripting/ops/modules.rs`
- Modify: `crates/rocket-infra/src/scripting/ops/mod.rs`, `state.rs`, `engine.rs`, `bootstrap.js`
- Test: `crates/rocket-infra/src/scripting/engine.rs` (inline tests)

**Interfaces:**
- Consumes: `build_roots`, `resolve_local_module`, `LocalRoots` from Task 1; `ScriptContext.file_scope`.
- Produces: JS `require('./x')` behaviour; `ScriptInputState.local_roots: Option<LocalRoots>`; op `op_require_local(from_dir: String, name: String) -> String` returning JSON `{"path","dir","source"}`.

- [ ] **Step 1: Read the spec reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing engine tests**

In `crates/rocket-infra/src/scripting/engine.rs`, inside `mod tests`, add a helper and the tests below (place them after `unknown_require_returns_error`):

```rust
    use rocket_scripting::ScriptFileScope;

    /// Builds a context with a local-file scope rooted at `root`.
    fn scoped_ctx(
        code: &str,
        root: &std::path::Path,
        additional: Vec<std::path::PathBuf>,
        mode: SandboxMode,
    ) -> ScriptContext {
        let mut ctx = minimal_ctx(code);
        ctx.sandbox_mode = mode;
        ctx.file_scope = Some(ScriptFileScope {
            collection_root: root.to_path_buf(),
            additional_roots: additional,
        });
        ctx
    }

    fn write_file(root: &std::path::Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, content).expect("write file");
    }

    async fn run(ctx: ScriptContext) -> rocket_scripting::ScriptResult {
        DenoScriptEngine::new().execute(ctx).await.expect("execute")
    }

    #[tokio::test]
    async fn require_local_sibling_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_file(
            tmp.path(),
            "utils.js",
            "module.exports = { greet: (n) => 'hi ' + n };",
        );
        let ctx = scoped_ctx(
            "const { greet } = require('./utils.js'); console.log(greet('bob'));",
            tmp.path(),
            vec![],
            SandboxMode::Safe,
        );
        let result = run(ctx).await;
        assert!(result.error.is_none(), "error: {:?}", result.error);
        assert_eq!(result.console_entries[0].message, "hi bob");
    }

    #[tokio::test]
    async fn require_local_nested_resolves_from_the_requiring_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_file(tmp.path(), "root.js", "module.exports = 'root';");
        write_file(tmp.path(), "lib/b.js", "module.exports = 'b';");
        write_file(
            tmp.path(),
            "lib/a.js",
            "module.exports = require('./b') + '+' + require('../root');",
        );
        let ctx = scoped_ctx(
            "console.log(require('./lib/a'));",
            tmp.path(),
            vec![],
            SandboxMode::Safe,
        );
        let result = run(ctx).await;
        assert!(result.error.is_none(), "error: {:?}", result.error);
        assert_eq!(result.console_entries[0].message, "b+root");
    }

    #[tokio::test]
    async fn require_local_supports_module_exports_reassignment_and_dirname() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_file(
            tmp.path(),
            "lib/fn.js",
            "module.exports = function () { return __dirname.endsWith('lib') && __filename.endsWith('fn.js'); };",
        );
        let ctx = scoped_ctx(
            "console.log(String(require('./lib/fn')()));",
            tmp.path(),
            vec![],
            SandboxMode::Safe,
        );
        let result = run(ctx).await;
        assert!(result.error.is_none(), "error: {:?}", result.error);
        assert_eq!(result.console_entries[0].message, "true");
    }

    #[tokio::test]
    async fn require_local_circular_gets_partial_exports() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_file(
            tmp.path(),
            "a.js",
            "exports.early = 1; const b = require('./b'); exports.fromB = b.sawEarly;",
        );
        write_file(
            tmp.path(),
            "b.js",
            "const a = require('./a'); exports.sawEarly = a.early;",
        );
        let ctx = scoped_ctx(
            "console.log(String(require('./a').fromB));",
            tmp.path(),
            vec![],
            SandboxMode::Safe,
        );
        let result = run(ctx).await;
        assert!(result.error.is_none(), "error: {:?}", result.error);
        assert_eq!(result.console_entries[0].message, "1");
    }

    #[tokio::test]
    async fn require_local_runs_a_module_once_per_script_run() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_file(
            tmp.path(),
            "once.js",
            "console.log('loaded'); module.exports = {};",
        );
        let ctx = scoped_ctx(
            "require('./once'); require('./once.js'); require('./once');",
            tmp.path(),
            vec![],
            SandboxMode::Safe,
        );
        let result = run(ctx).await;
        assert!(result.error.is_none(), "error: {:?}", result.error);
        assert_eq!(result.console_entries.len(), 1);
    }

    #[tokio::test]
    async fn require_local_failed_load_is_retried_not_cached() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_file(tmp.path(), "bad.js", "throw new Error('boom');");
        let ctx = scoped_ctx(
            "let n = 0; for (let i = 0; i < 2; i++) { try { require('./bad'); } catch (e) { n++; } } console.log(String(n));",
            tmp.path(),
            vec![],
            SandboxMode::Safe,
        );
        let result = run(ctx).await;
        assert!(result.error.is_none(), "error: {:?}", result.error);
        assert_eq!(result.console_entries[0].message, "2");
    }

    #[tokio::test]
    async fn require_local_missing_file_reports_cannot_find() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ctx = scoped_ctx("require('./nope');", tmp.path(), vec![], SandboxMode::Safe);
        let result = run(ctx).await;
        let err = result.error.expect("must fail");
        assert!(err.contains("Cannot find module './nope'"), "got: {err}");
    }

    #[tokio::test]
    async fn require_local_without_scope_is_an_error() {
        let result = run(minimal_ctx("require('./x');")).await;
        let err = result.error.expect("must fail");
        assert!(err.contains("local file requires are not available"), "got: {err}");
    }

    #[tokio::test]
    async fn require_local_parent_escape_is_denied() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = tmp.path().canonicalize().expect("canonicalize");
        write_file(&base, "outer.js", "module.exports = 1;");
        let col = base.join("col");
        std::fs::create_dir_all(&col).expect("mkdir col");
        let ctx = scoped_ctx("require('../outer.js');", &col, vec![], SandboxMode::Safe);
        let result = run(ctx).await;
        let err = result.error.expect("must fail");
        assert!(err.contains("outside the allowed script roots"), "got: {err}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn require_local_symlink_escape_is_denied() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = tmp.path().canonicalize().expect("canonicalize");
        write_file(&base, "outer.js", "module.exports = 1;");
        let col = base.join("col");
        std::fs::create_dir_all(&col).expect("mkdir col");
        std::os::unix::fs::symlink(base.join("outer.js"), col.join("link.js")).expect("symlink");
        let ctx = scoped_ctx("require('./link.js');", &col, vec![], SandboxMode::Safe);
        let result = run(ctx).await;
        let err = result.error.expect("must fail");
        assert!(err.contains("outside the allowed script roots"), "got: {err}");
    }

    #[tokio::test]
    async fn require_local_non_js_file_is_rejected() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_file(tmp.path(), "data.txt", "TOPSECRET");
        let ctx = scoped_ctx("require('./data.txt');", tmp.path(), vec![], SandboxMode::Safe);
        let result = run(ctx).await;
        let err = result.error.expect("must fail");
        assert!(err.contains("Only .js files can be required"), "got: {err}");
        assert!(!err.contains("TOPSECRET"), "must not echo content: {err}");
    }

    #[tokio::test]
    async fn additional_roots_are_developer_mode_only() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = tmp.path().canonicalize().expect("canonicalize");
        write_file(&base, "shared/common.js", "module.exports = 'shared';");
        let col = base.join("col");
        std::fs::create_dir_all(&col).expect("mkdir col");
        let extra = vec![std::path::PathBuf::from("../shared")];
        let code = "console.log(require('../shared/common.js'));";

        let safe = run(scoped_ctx(code, &col, extra.clone(), SandboxMode::Safe)).await;
        let err = safe.error.expect("safe must deny");
        assert!(err.contains("outside the allowed script roots"), "got: {err}");

        let dev = run(scoped_ctx(code, &col, extra, SandboxMode::Developer)).await;
        assert!(dev.error.is_none(), "error: {:?}", dev.error);
        assert_eq!(dev.console_entries[0].message, "shared");
    }

    #[tokio::test]
    async fn bundled_modules_still_resolve_with_a_scope_present() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ctx = scoped_ctx(
            "console.log(typeof require('lodash').get);",
            tmp.path(),
            vec![],
            SandboxMode::Safe,
        );
        let result = run(ctx).await;
        assert!(result.error.is_none(), "error: {:?}", result.error);
        assert_eq!(result.console_entries[0].message, "function");
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra require_local`
Expected: FAIL (`Module not found: ./utils.js` and similar).

- [ ] **Step 4: Add the op**

Create `crates/rocket-infra/src/scripting/ops/modules.rs`:

```rust
use std::path::PathBuf;

use deno_core::{op2, OpState};

use crate::scripting::local_modules::resolve_local_module;
use crate::scripting::ops::ScriptOpError;
use crate::scripting::state::ScriptInputState;

/// Backs `require()` for local `.js` files.
///
/// `from_dir` is the directory of the requiring file, or an empty string for the
/// top-level script. Returns a JSON object string `{ path, dir, source }` that
/// `bootstrap.js` parses. All root checks happen in `resolve_local_module`, so a
/// forged `from_dir` cannot widen access.
#[op2]
#[string]
pub fn op_require_local(
    state: &mut OpState,
    #[string] from_dir: String,
    #[string] name: String,
) -> Result<String, ScriptOpError> {
    let input = state.borrow::<ScriptInputState>();
    let Some(roots) = input.local_roots.as_ref() else {
        return Err(ScriptOpError(format!(
            "Cannot require '{name}': local file requires are not available in this script"
        )));
    };
    let from = if from_dir.is_empty() {
        roots.collection_root.clone()
    } else {
        PathBuf::from(from_dir)
    };
    let module = resolve_local_module(roots, &from, &name).map_err(ScriptOpError)?;
    Ok(serde_json::json!({
        "path": module.path.to_string_lossy(),
        "dir": module.dir.to_string_lossy(),
        "source": module.source,
    })
    .to_string())
}
```

In `ops/mod.rs` add `pub mod modules;` after `pub mod fs;`.

In `state.rs` add the import `use crate::scripting::local_modules::LocalRoots;` and this field to `ScriptInputState`, after `path_params`:

```rust
    /// Allowed roots for local `require()`. `None` when the context has no file
    /// scope or the collection directory is unavailable.
    pub local_roots: Option<LocalRoots>,
```

In `engine.rs`:
1. Add `modules` to the ops import: `use crate::scripting::ops::{console, fs, modules, process, redact, req, res, rok};` and `use crate::scripting::local_modules::build_roots;`.
2. Register the op in the `rocket_scripting_ext` list, after `op_require_module,`: `modules::op_require_local,`.
3. In `run_script`, before the `// Seed OpState` block, add:

```rust
    let local_roots = ctx
        .file_scope
        .as_ref()
        .and_then(|scope| build_roots(scope, sandbox_mode).ok());
```
and add `local_roots,` to the `ScriptInputState { ... }` literal after `path_params: ctx.path_params,`. (`ctx.file_scope` is borrowed before the field moves, so keep this statement above the `state.put` block.)

- [ ] **Step 5: Replace the `require` loader in `bootstrap.js`**

Replace the whole block from the `// ── require() module loader` comment through the closing `};` of `globalThis.require` with:

```js
  // ── require() module loader ───────────────────────────────────────────────────
  // The new Function(...) body's only lexical parent is the global scope, so a
  // vendored module cannot see `__ops` — and does not need to. None of them
  // reference the Deno global; they use globalThis.crypto, navigator, btoa and
  // atob, all of which survive this file untouched.
  //
  // Bare names load vendored modules. `./x`, `../x` and absolute paths load local
  // `.js` files through op_require_local, which enforces the allowed roots.
  const isLocalSpecifier = (name) =>
    name === '.' ||
    name === '..' ||
    name.startsWith('./') ||
    name.startsWith('../') ||
    name.startsWith('/') ||
    name.startsWith('.\\') ||
    name.startsWith('..\\');

  const loadBundled = function(name) {
    const src = __ops.op_require_module(name);
    if (!src) throw new Error(`Module not found: ${name}`);
    const mod = { exports: {} };
    const fn = new Function("module", "exports", "require", src);
    fn(mod, mod.exports, globalThis.require);
    return mod.exports;
  };

  // One entry per canonical file path for the lifetime of this script run. The
  // entry is stored before the module body runs, so circular requires see the
  // partial exports. A module that throws is evicted so a later require retries.
  const localCache = new Map();

  const makeRequire = function(fromDir) {
    return function require(name) {
      if (typeof name !== 'string') {
        throw new TypeError('require() expects a string module name');
      }
      if (isLocalSpecifier(name)) return loadLocal(fromDir, name);
      return loadBundled(name);
    };
  };

  const loadLocal = function(fromDir, name) {
    const info = JSON.parse(__ops.op_require_local(fromDir, name));
    const cached = localCache.get(info.path);
    if (cached) return cached.exports;
    const mod = { exports: {} };
    localCache.set(info.path, mod);
    try {
      const fn = new Function(
        "module", "exports", "require", "__filename", "__dirname", info.source
      );
      fn(mod, mod.exports, makeRequire(info.dir), info.path, info.dir);
    } catch (e) {
      localCache.delete(info.path);
      throw e;
    }
    return mod.exports;
  };

  globalThis.require = makeRequire('');
```

Note: the `const _chai = require('chai');` line that follows keeps working because `globalThis.require` is defined above it.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra require_ && cargo test -j4 -p rocket-infra scripting`
Expected: PASS, including the pre-existing `require_chai_and_use_expect`, `require_lodash_group_by_and_get` and `unknown_require_returns_error`.

- [ ] **Step 7: Commit**

Invoke `dev-workflow-skills:1-git-commit` for:
`git add crates/rocket-infra/src/scripting/ops/modules.rs crates/rocket-infra/src/scripting/ops/mod.rs crates/rocket-infra/src/scripting/state.rs crates/rocket-infra/src/scripting/engine.rs crates/rocket-infra/src/scripting/bootstrap.js`
Suggested subject: `feat(scripting): require local .js files with root checks`

---

### Task 3: Config and wiring from ExecutionService

**Files:**
- Modify: `crates/rocket-collection/src/settings.rs`, `crates/rocket-collection/src/repository.rs`
- Modify: `crates/rocket-infra/src/fs_collection/settings.rs`, `crates/rocket-infra/src/fs_collection/mod.rs`, `crates/rocket-infra/src/shared_path_collection_repo.rs`
- Modify: `crates/rocket-app/src/execution_service.rs`
- Test: `crates/rocket-infra/src/fs_collection/tests.rs`, `crates/rocket-app/src/execution_service.rs` (inline)

**Interfaces:**
- Consumes: `ScriptFileScope`, `ScriptContext::with_file_scope` from Task 1.
- Produces:
  - `CollectionSettings.script_context_roots: Vec<String>` (`#[serde(default)]`).
  - `CollectionRepository::collection_root_path(&self, name: &str) -> DomainResult<PathBuf>` (default body returns `DomainError::Internal`). Plan 02 relies on it.
  - `ExecutionService` passes `Some(ScriptFileScope)` to every phase.

- [ ] **Step 1: Read the spec reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing persistence tests**

Append to `crates/rocket-infra/src/fs_collection/tests.rs`, using the `setup()` helper:

```rust
#[test]
fn settings_script_context_roots_roundtrip_and_keep_other_extensions() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let path = repo.settings_path("col");
    let existing = std::fs::read_to_string(&path).expect("read");
    let with_other = format!("{existing}extensions:\n  rocketapi:\n    sandboxMode: safe\n    keep: me\n  other:\n    x: 1\n");
    std::fs::write(&path, with_other).expect("write fixture");

    let mut settings = repo.get_settings("col").expect("get");
    assert!(settings.script_context_roots.is_empty());
    settings.script_context_roots = vec!["../shared".into(), "./more".into()];
    repo.save_settings("col", &settings).expect("save");

    let loaded = repo.get_settings("col").expect("reload");
    assert_eq!(loaded.script_context_roots, vec!["../shared", "./more"]);
    let yaml = std::fs::read_to_string(&path).expect("read back");
    assert!(yaml.contains("keep: me"), "other rocketapi keys kept: {yaml}");
    assert!(yaml.contains("other:"), "other extensions kept: {yaml}");
    assert!(yaml.contains("additionalContextRoots"), "key written: {yaml}");
}

#[test]
fn settings_script_context_roots_empty_removes_the_key() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let mut settings = repo.get_settings("col").expect("get");
    settings.script_context_roots = vec!["../shared".into()];
    repo.save_settings("col", &settings).expect("save");
    settings.script_context_roots.clear();
    repo.save_settings("col", &settings).expect("save empty");
    let yaml = std::fs::read_to_string(repo.settings_path("col")).expect("read");
    assert!(!yaml.contains("additionalContextRoots"), "key removed: {yaml}");
}
```

`setup()` is the existing fixture helper at the top of `tests.rs`; it returns `(TempDir, FsCollectionRepo)`.

Also add this unit test to the `tests` module of `crates/rocket-infra/src/fs_collection/settings.rs`:

```rust
    #[test]
    fn script_roots_from_extensions_ignores_non_string_entries() {
        let yaml = "rocketapi:\n  scripts:\n    additionalContextRoots:\n      - ../shared\n      - 42\n      - ./more\n";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(
            script_roots_from_extensions(&Some(value)),
            vec!["../shared".to_string(), "./more".to_string()]
        );
        assert!(script_roots_from_extensions(&None).is_empty());
    }
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -j4 -p rocket-infra script_roots script_context_roots`
Expected: FAIL to compile (field and helper missing).

- [ ] **Step 4: Add the settings field and fix literals**

In `crates/rocket-collection/src/settings.rs`, add to `CollectionSettings` after `sandbox_mode`:

```rust
    /// Extra directories scripts may `require()` from, in Developer sandbox mode only.
    /// Relative entries resolve against the collection directory.
    /// Persisted at `extensions.rocketapi.scripts.additionalContextRoots`.
    #[serde(default)]
    pub script_context_roots: Vec<String>,
```

Run `cargo check -j4 --workspace --tests` and add `script_context_roots: vec![],` (or `..Default::default()`) to every `CollectionSettings { ... }` literal it flags (about 27 sites across `rocket-collection`, `rocket-infra`, `rocket-app`, `src-tauri`). Use `..Default::default()` where the literal already spreads or is a test fixture.

- [ ] **Step 5: Persist the field**

In `crates/rocket-infra/src/fs_collection/settings.rs`, add next to `sandbox_mode_from_extensions`:

```rust
/// Reads `extensions.rocketapi.scripts.additionalContextRoots`. Non-string entries
/// are dropped and a missing key gives an empty list.
fn script_roots_from_extensions(extensions: &Option<serde_yaml::Value>) -> Vec<String> {
    extensions
        .as_ref()
        .and_then(|v| v.get("rocketapi"))
        .and_then(|v| v.get("scripts"))
        .and_then(|v| v.get("additionalContextRoots"))
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Writes the roots into `extensions.rocketapi.scripts.additionalContextRoots`, keeping
/// every other key. An empty list removes the `scripts` key it owns so no empty stub is left.
fn set_script_roots_in_extensions(
    extensions: Option<serde_yaml::Value>,
    roots: &[String],
) -> Option<serde_yaml::Value> {
    let mut root = match extensions {
        Some(serde_yaml::Value::Mapping(map)) => map,
        _ => serde_yaml::Mapping::new(),
    };
    let rocketapi_key = serde_yaml::Value::String("rocketapi".into());
    let mut rocketapi = match root.get(&rocketapi_key) {
        Some(serde_yaml::Value::Mapping(map)) => map.clone(),
        _ => serde_yaml::Mapping::new(),
    };
    let scripts_key = serde_yaml::Value::String("scripts".into());
    let mut scripts = match rocketapi.get(&scripts_key) {
        Some(serde_yaml::Value::Mapping(map)) => map.clone(),
        _ => serde_yaml::Mapping::new(),
    };
    let roots_key = serde_yaml::Value::String("additionalContextRoots".into());
    if roots.is_empty() {
        scripts.remove(&roots_key);
    } else {
        scripts.insert(
            roots_key,
            serde_yaml::Value::Sequence(
                roots
                    .iter()
                    .map(|r| serde_yaml::Value::String(r.clone()))
                    .collect(),
            ),
        );
    }
    if scripts.is_empty() {
        rocketapi.remove(&scripts_key);
    } else {
        rocketapi.insert(scripts_key, serde_yaml::Value::Mapping(scripts));
    }
    root.insert(rocketapi_key, serde_yaml::Value::Mapping(rocketapi));
    Some(serde_yaml::Value::Mapping(root))
}
```

In `get_settings`, after `let sandbox_mode = ...;` add `let script_context_roots = script_roots_from_extensions(&oc.extensions);` and add `script_context_roots,` to both `CollectionSettings { ... }` literals returned there (the second one already spreads `..CollectionSettings::default()`, add the field before the spread). In `save_settings`, after the `oc.extensions = set_sandbox_mode_in_extensions(...)` line add:

```rust
    oc.extensions = set_script_roots_in_extensions(oc.extensions.take(), &settings.script_context_roots);
```

- [ ] **Step 6: Run to verify the persistence tests pass**

Run: `cargo test -j4 -p rocket-infra script_roots script_context_roots settings_sandbox`
Expected: PASS (the pre-existing sandbox tests must still pass).

- [ ] **Step 7: Add `collection_root_path`**

In `crates/rocket-collection/src/repository.rs` add `use std::path::PathBuf;` and `use rocket_shared::error::{DomainError, DomainResult};`, then add inside the trait:

```rust
    /// Absolute directory of a collection. Used to scope local-file `require()`.
    /// The default body keeps test doubles compiling; real repositories override it.
    fn collection_root_path(&self, _name: &str) -> DomainResult<PathBuf> {
        Err(DomainError::Internal(
            "collection root path is not available".into(),
        ))
    }
```

In `crates/rocket-infra/src/fs_collection/mod.rs`, inside `impl CollectionRepository for FsCollectionRepo`:

```rust
    fn collection_root_path(&self, name: &str) -> DomainResult<PathBuf> {
        Collection::validate_name(name)?;
        let path = self.collection_path(name);
        if !path.is_dir() {
            return Err(DomainError::NotFound(format!("Collection '{name}' not found")));
        }
        Ok(path)
    }
```

(Add any missing imports: `std::path::PathBuf`, `Collection`, `DomainError`; they are likely present already.) In `shared_path_collection_repo.rs` add:

```rust
    fn collection_root_path(&self, name: &str) -> DomainResult<std::path::PathBuf> {
        self.repo().collection_root_path(name)
    }
```

Add a test to `fs_collection/tests.rs`:

```rust
#[test]
fn collection_root_path_returns_the_directory_and_rejects_unknown() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let path = repo.collection_root_path("col").expect("exists");
    assert!(path.is_dir());
    assert!(repo.collection_root_path("missing").is_err());
    assert!(repo.collection_root_path("../escape").is_err());
}
```

Run: `cargo test -j4 -p rocket-infra collection_root_path`. Expected: PASS.

- [ ] **Step 8: Write the failing ExecutionService test**

In `crates/rocket-app/src/execution_service.rs` tests, next to `before_request_script_receives_collection_sandbox_mode`, add a probe and test. The stub repo needs to return a root path, so give `StubCollectionRepo` an optional root: add a field `root: Option<std::path::PathBuf>` defaulting to `None` in its constructors (`empty()`, `with_settings(...)`), and override the method in its `impl CollectionRepository`:

```rust
        fn collection_root_path(&self, _: &str) -> DomainResult<std::path::PathBuf> {
            self.root
                .clone()
                .ok_or_else(|| DomainError::NotFound("no root".into()))
        }
```

Add a builder `fn with_root(mut self, root: &str) -> Self { self.root = Some(root.into()); self }` on `StubCollectionRepo`. Then:

```rust
    struct ScopeProbeEngine {
        seen: Mutex<Vec<Option<rocket_scripting::ScriptFileScope>>>,
    }

    #[async_trait]
    impl ScriptEngine for ScopeProbeEngine {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.seen.lock().expect("lock").push(ctx.file_scope.clone());
            Ok(ScriptResult::default())
        }
    }

    struct SharedScopeProbe(Arc<ScopeProbeEngine>);
    #[async_trait]
    impl ScriptEngine for SharedScopeProbe {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.0.execute(ctx).await
        }
    }

    #[tokio::test]
    async fn scripts_receive_the_collection_file_scope_in_every_phase() {
        let engine = Arc::new(ScopeProbeEngine {
            seen: Mutex::new(vec![]),
        });
        let repo = StubCollectionRepo::with_settings(CollectionSettings {
            script_context_roots: vec!["../shared".into()],
            ..Default::default()
        })
        .with_root("/work/my-api");
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(repo),
            Box::new(SharedScopeProbe(Arc::clone(&engine))),
        );

        let mut input = sample_input("https://example.com", None);
        input.collection = Some("my-api".into());
        input.pre_request_script = Some("// pre".into());
        input.post_response_script = Some("// post".into());
        input.tests_script = Some("// tests".into());
        svc.execute(input).await.expect("execute");

        let expected = Some(rocket_scripting::ScriptFileScope {
            collection_root: "/work/my-api".into(),
            additional_roots: vec!["../shared".into()],
        });
        let seen = engine.seen.lock().expect("lock").clone();
        assert_eq!(seen, vec![expected.clone(), expected.clone(), expected]);
    }

    #[tokio::test]
    async fn scripts_get_no_file_scope_without_a_collection() {
        let engine = Arc::new(ScopeProbeEngine {
            seen: Mutex::new(vec![]),
        });
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedScopeProbe(Arc::clone(&engine))),
        );
        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        svc.execute(input).await.expect("execute");
        assert_eq!(engine.seen.lock().expect("lock").clone(), vec![None]);
    }
```

Run: `cargo test -j4 -p rocket-app file_scope`. Expected: FAIL (scope is always `None`).

- [ ] **Step 9: Build and pass the scope**

In `execution_service.rs`:
1. Add `use rocket_scripting::ScriptFileScope;` next to the existing `rocket_scripting` imports.
2. Add to `PhaseState` after `sandbox_mode`:

```rust
    /// Local-file `require()` scope, resolved once in `begin_phases`.
    pub file_scope: Option<ScriptFileScope>,
```
3. Replace the `let sandbox_mode = match input.collection.as_deref() { ... };` block in `begin_phases` with:

```rust
        let (sandbox_mode, file_scope) = match input.collection.as_deref() {
            Some(col) => {
                let settings = self.collection_repo.get_settings(col).unwrap_or_default();
                let mode = match settings.sandbox_mode {
                    CollectionSandboxMode::Safe => SandboxMode::Safe,
                    CollectionSandboxMode::Developer => SandboxMode::Developer,
                };
                // A collection whose directory cannot be resolved just gets no scope.
                let scope = self
                    .collection_repo
                    .collection_root_path(col)
                    .ok()
                    .map(|root| ScriptFileScope {
                        collection_root: root,
                        additional_roots: settings
                            .script_context_roots
                            .iter()
                            .map(std::path::PathBuf::from)
                            .collect(),
                    });
                (mode, scope)
            }
            None => (SandboxMode::Safe, None),
        };
```
4. Add `file_scope,` to the `PhaseState { ... }` literal (and to any other `PhaseState { ... }` literal the compiler flags, using `file_scope: None`).
5. At each of the three `.with_sandbox_mode(state.sandbox_mode)` call sites (before-request, after-response, tests) add `.with_file_scope(state.file_scope.clone())` on the next line.

- [ ] **Step 10: Run the checks**

Run: `cargo test -j4 -p rocket-app file_scope && cargo test -j4 -p rocket-app sandbox_mode && cargo check -j4 --workspace --tests`
Expected: PASS and a clean check.

- [ ] **Step 11: Commit**

Invoke `dev-workflow-skills:1-git-commit` for:
`git add crates/rocket-collection/src/settings.rs crates/rocket-collection/src/repository.rs crates/rocket-infra/src/fs_collection crates/rocket-infra/src/shared_path_collection_repo.rs crates/rocket-app/src/execution_service.rs` plus any other file the literal fixes touched (list them with `git status --short` and add by path).
Suggested subject: `feat(scripting): scope local require to the collection`

---

## Plan 01 Self-Review

- Spec Section 1 covered: context type (T1), resolution and cache (T2), roots and Safe/Developer split (T1, T2), config (T3), wiring (T3).
- Type names used across tasks: `ScriptFileScope`, `LocalRoots`, `ResolvedModule`, `build_roots`, `resolve_local_module`, `op_require_local`, `script_context_roots`, `collection_root_path`, `with_file_scope`. All defined in the task that produces them.
- Verification after the plan: `cargo check -j4 --workspace --tests`, `cargo test -j4 -p rocket-scripting`, `cargo test -j4 -p rocket-infra scripting`, `cargo test -j4 -p rocket-app file_scope`.
