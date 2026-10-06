//! Pure path logic for `require()` of local `.js` files.
//!
//! The specifier is first normalised lexically and checked against the roots
//! without touching the disk. Only then is the path canonicalised and checked
//! again, so `..` segments and symlinks cannot escape the allowed roots.

use std::path::{Component, Path, PathBuf};

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

/// Resolves `.` and `..` textually, with no disk access.
///
/// Returns `None` for a path with a prefix (UNC or drive) or a `..` above the root.
pub(crate) fn lexical_normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(_) => return None,
            Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                out.pop();
            }
            Component::Normal(part) => {
                depth += 1;
                out.push(part);
            }
        }
    }
    Some(out)
}

fn denied(roots: &LocalRoots, name: &str) -> String {
    let allowed = roots
        .roots
        .iter()
        .map(|r| r.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("Access to '{name}' is outside the allowed script roots: {allowed}")
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

    // The root check below depends only on the specifier text, never on the disk.
    let base = match lexical_normalize(&base) {
        Some(normal) if roots.roots.iter().any(|root| normal.starts_with(root)) => normal,
        _ => return Err(denied(roots, &name)),
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
        return Err(denied(roots, &name));
    }
    if found.extension().and_then(|e| e.to_str()) != Some("js") {
        return Err(format!("Only .js files can be required: '{name}'"));
    }

    let source =
        std::fs::read_to_string(&found).map_err(|e| format!("Cannot read module '{name}': {e}"))?;
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
        assert!(
            err.contains("outside the allowed script roots"),
            "got: {err}"
        );
        assert!(
            err.contains(&f.col.display().to_string()),
            "lists roots: {err}"
        );
    }

    #[test]
    fn absolute_path_outside_roots_is_denied() {
        let f = fixture();
        let r = roots(&f, SandboxMode::Safe);
        let abs = f.outer.join("outer.js").display().to_string();
        let err = resolve_local_module(&r, &r.collection_root, &abs).expect_err("absolute");
        assert!(
            err.contains("outside the allowed script roots"),
            "got: {err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_denied() {
        let f = fixture();
        std::os::unix::fs::symlink(f.outer.join("outer.js"), f.col.join("link.js"))
            .expect("symlink");
        let r = roots(&f, SandboxMode::Safe);
        let err = resolve_local_module(&r, &r.collection_root, "./link.js").expect_err("symlink");
        assert!(
            err.contains("outside the allowed script roots"),
            "got: {err}"
        );
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
        assert!(
            err.contains("outside the allowed script roots"),
            "got: {err}"
        );

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

    fn denial(r: &LocalRoots, name: &str) -> String {
        let allowed = r
            .roots
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("Access to '{name}' is outside the allowed script roots: {allowed}")
    }

    #[test]
    fn outside_paths_give_the_same_denial_whether_or_not_they_exist() {
        let f = fixture();
        let r = roots(&f, SandboxMode::Safe);
        let existing = f.outer.join("outer.js").display().to_string();
        let missing = f.outer.join("missing.js").display().to_string();
        let a = resolve_local_module(&r, &r.collection_root, &existing).expect_err("existing");
        let b = resolve_local_module(&r, &r.collection_root, &missing).expect_err("missing");
        assert_eq!(a, denial(&r, &existing));
        assert_eq!(b, denial(&r, &missing));
    }

    #[test]
    fn lexical_escapes_are_denied_without_the_file_existing() {
        let f = fixture();
        let r = roots(&f, SandboxMode::Safe);
        for name in ["/etc/../etc/hostname", "./../../x", "./../nope-missing.js"] {
            let err = resolve_local_module(&r, &r.collection_root, name).expect_err(name);
            assert_eq!(err, denial(&r, name));
        }
    }

    #[test]
    fn unc_style_specifiers_are_denied() {
        let f = fixture();
        let r = roots(&f, SandboxMode::Safe);
        for name in ["//attacker/share/x.js", "\\\\attacker\\share\\x.js"] {
            let err = resolve_local_module(&r, &r.collection_root, name).expect_err(name);
            assert_eq!(err, denial(&r, &name.replace('\\', "/")));
        }
    }

    #[test]
    fn in_root_dotdot_segments_still_load() {
        let f = fixture();
        fs::create_dir_all(f.col.join("a")).expect("mkdir a");
        fs::write(f.col.join("b.js"), "module.exports = 5;").expect("write b");
        let r = roots(&f, SandboxMode::Safe);
        let m = resolve_local_module(&r, &r.collection_root, "./a/../b.js").expect("loads");
        assert_eq!(m.source, "module.exports = 5;");
    }

    #[test]
    fn lexical_normalize_resolves_dots_without_the_disk() {
        let n = |s: &str| lexical_normalize(Path::new(s));
        assert_eq!(n("/a/./b/../c"), Some(PathBuf::from("/a/c")));
        assert_eq!(n("/a/b/.."), Some(PathBuf::from("/a")));
        assert_eq!(n("/a/../.."), None);
        assert_eq!(n("/.."), None);
        assert_eq!(n("a/../../b"), None);
        assert_eq!(n("//host/share/x"), Some(PathBuf::from("/host/share/x")));
    }

    #[cfg(windows)]
    #[test]
    fn lexical_normalize_rejects_prefixes() {
        assert_eq!(lexical_normalize(Path::new(r"\\host\share\x")), None);
        assert_eq!(lexical_normalize(Path::new(r"C:\x")), None);
    }
}
