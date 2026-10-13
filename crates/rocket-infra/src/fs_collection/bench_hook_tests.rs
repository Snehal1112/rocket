//! The bench hook must be a pure delegation to `validate_path`.

use super::*;
use tempfile::TempDir;

fn repo_with_file() -> (TempDir, FsCollectionRepo, PathBuf) {
    let dir = TempDir::new().expect("create temp dir");
    let base = dir.path().join("col");
    std::fs::create_dir_all(base.join("folder")).expect("create folder");
    std::fs::write(base.join("folder/req.yml"), "x").expect("write file");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    (dir, repo, base)
}

#[test]
fn hook_returns_what_validate_path_returns_for_every_input_class() {
    let (_dir, repo, base) = repo_with_file();
    let inputs = [
        "folder/req.yml",
        "folder/new.yml",
        "missing/deep/new.yml",
        "../outside.yml",
        "/etc/passwd",
        "",
    ];
    for input in inputs {
        let path = Path::new(input);
        assert_eq!(
            repo.validate_path(&base, path),
            repo.validate_path_for_bench(&base, path),
            "hook differs for input {input:?}"
        );
    }
}

#[test]
fn hook_keeps_rejecting_traversal() {
    let (_dir, repo, base) = repo_with_file();
    let result = repo.validate_path_for_bench(&base, Path::new("../outside.yml"));
    assert_eq!(
        result,
        Err(DomainError::InvalidInput("Path traversal detected".into()))
    );
}

#[test]
fn hook_returns_a_canonical_path_inside_the_base() {
    let (_dir, repo, base) = repo_with_file();
    let canonical_base = base.canonicalize().expect("canonicalize base");
    let resolved = repo
        .validate_path_for_bench(&base, Path::new("folder/req.yml"))
        .expect("existing file validates");
    assert_eq!(resolved, canonical_base.join("folder/req.yml"));
}

#[test]
fn hook_reports_a_missing_base_like_validate_path() {
    let (dir, repo, _base) = repo_with_file();
    let gone = dir.path().join("no-such-collection");
    let expected = Err(DomainError::NotFound("Base dir not found".into()));
    assert_eq!(repo.validate_path(&gone, Path::new("a.yml")), expected);
    assert_eq!(repo.validate_path_for_bench(&gone, Path::new("a.yml")), expected);
}
