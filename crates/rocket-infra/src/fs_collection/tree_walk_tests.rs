//! Pins what the tree walk loads and skips, so work on `tree.rs` cannot change it.

use std::fs;

use rocket_collection::{CollectionItem, CollectionRepository, Folder};
use rocket_shared::types::HttpMethod;
use tempfile::TempDir;

use super::tree::build_tree;
use super::FsCollectionRepo;

fn setup() -> (TempDir, FsCollectionRepo) {
    let dir = TempDir::new().expect("temp dir");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    (dir, repo)
}

/// Saves one HTTP request into collection `col`.
fn save(repo: &FsCollectionRepo, path: &str, name: &str) {
    let req = rocket_collection::Request::new(name, HttpMethod::Get, "https://example.test");
    repo.save_request("col", path, &req).expect("save request");
}

/// Writes `text` to `rel` below the temp dir and creates the parent folders.
fn write(dir: &TempDir, rel: &str, text: &str) {
    let path = dir.path().join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent folders");
    }
    fs::write(path, text).expect("write file");
}

fn folder_yml(name: &str, uid: &str) -> String {
    format!("info:\n  name: {name}\n  uid: {uid}\n")
}

/// Renders a folder as text: `dir:<dir_name>|<name>|<uid>` then one line per item.
/// The root uid is random, so the root line shows `uid-set` or `uid-empty`.
fn outline(folder: &Folder) -> String {
    let mut lines = Vec::new();
    outline_into(folder, 0, &mut lines);
    lines.join("\n")
}

fn outline_into(folder: &Folder, depth: usize, lines: &mut Vec<String>) {
    let pad = "  ".repeat(depth);
    let uid = if depth == 0 {
        if folder.uid.is_empty() {
            "uid-empty"
        } else {
            "uid-set"
        }
    } else {
        folder.uid.as_str()
    };
    let dir_name = folder.dir_name.as_deref().unwrap_or("-");
    lines.push(format!("{pad}dir:{dir_name}|{}|{uid}", folder.name));
    for item in &folder.items {
        match item {
            CollectionItem::Folder(f) => outline_into(f, depth + 1, lines),
            CollectionItem::Request(r) => lines.push(format!(
                "{pad}  req:{}|{}",
                r.file_name.as_deref().unwrap_or("-"),
                r.name
            )),
            CollectionItem::Summary(s) => lines.push(format!(
                "{pad}  sum:{}|{}",
                s.file_name.as_deref().unwrap_or("-"),
                s.name
            )),
            CollectionItem::ScriptFile(s) => lines.push(format!("{pad}  js:{}", s.file_name)),
            _ => lines.push(format!("{pad}  other")),
        }
    }
}

/// The summaries load shows requests as `sum:` lines.
fn as_summary(outline: &str) -> String {
    outline.replace("req:", "sum:")
}

/// A collection `col` with one request, `req-a.yml`.
fn small() -> (TempDir, FsCollectionRepo) {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    save(&repo, "req-a.yml", "Alpha");
    (dir, repo)
}

/// Asserts both loads give `expected` (the summaries load shows `sum:` for `req:`).
fn assert_both_loads(repo: &FsCollectionRepo, expected: &str) {
    let full = repo.get("col").expect("full load");
    assert_eq!(outline(&full.root), expected, "full load");
    let summaries = repo.get_summaries("col").expect("summaries load");
    assert_eq!(outline(&summaries.root), as_summary(expected), "summaries load");
}

const SMALL_ONLY: &str = "dir:col|col|uid-set\n  req:req-a.yml|Alpha";

/// A collection with folders, a script file, nested `flows`, and entries the tree hides.
fn rich() -> (TempDir, FsCollectionRepo) {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    save(&repo, "req-a.yml", "Alpha");
    save(&repo, "req-c.yml", "Gamma");
    write(&dir, "col/lib.js", "module.exports = 1;");
    write(&dir, "col/a-folder/folder.yml", &folder_yml("Alpha Display", "uid-a"));
    save(&repo, "a-folder/x.yml", "X");
    write(&dir, "col/b-folder/folder.yml", &folder_yml("b-folder", "uid-b"));
    write(&dir, "col/b-folder/nested/folder.yml", &folder_yml("nested", "uid-n"));
    write(&dir, "col/b-folder/nested/flows/folder.yml", &folder_yml("flows", "uid-f"));
    save(&repo, "b-folder/nested/flows/inner.yml", "Inner");
    // Entries the tree must hide.
    write(&dir, "col/.hidden/h.yml", "x");
    write(&dir, "col/node_modules/n.js", "x");
    write(&dir, "col/environments/e.yml", "x");
    write(&dir, "col/flows/skipped.yml", "x");
    (dir, repo)
}

const RICH_BY_NAME: &str = "dir:col|col|uid-set
  dir:a-folder|Alpha Display|uid-a
    req:x.yml|X
  dir:b-folder|b-folder|uid-b
    dir:nested|nested|uid-n
      dir:flows|flows|uid-f
        req:inner.yml|Inner
  js:lib.js
  req:req-a.yml|Alpha
  req:req-c.yml|Gamma";

#[test]
fn walk_hides_the_hidden_entries_and_keeps_a_nested_flows_folder() {
    let (_dir, repo) = rich();
    assert_both_loads(&repo, RICH_BY_NAME);
}

#[cfg(unix)]
#[test]
fn walk_skips_a_symlinked_directory() {
    let (dir, repo) = small();
    let valid = fs::read_to_string(dir.path().join("col/req-a.yml")).expect("read request");
    write(&dir, "outside/secret.yml", &valid);
    std::os::unix::fs::symlink(dir.path().join("outside"), dir.path().join("col/linked"))
        .expect("symlink");
    assert_both_loads(&repo, SMALL_ONLY);
}

#[cfg(unix)]
#[test]
fn walk_skips_a_symlinked_directory_named_like_a_request_file() {
    let (dir, repo) = small();
    let valid = fs::read_to_string(dir.path().join("col/req-a.yml")).expect("read request");
    write(&dir, "outside/secret.yml", &valid);
    std::os::unix::fs::symlink(dir.path().join("outside"), dir.path().join("col/dirlink.yml"))
        .expect("symlink");
    assert_both_loads(&repo, SMALL_ONLY);
    let strict = repo.get_strict("col").expect("strict load");
    assert_eq!(outline(&strict.root), SMALL_ONLY, "strict load");
}

#[cfg(unix)]
#[test]
fn walk_skips_a_symlinked_directory_named_like_a_script() {
    let (dir, repo) = small();
    let valid = fs::read_to_string(dir.path().join("col/req-a.yml")).expect("read request");
    write(&dir, "outside/secret.yml", &valid);
    std::os::unix::fs::symlink(dir.path().join("outside"), dir.path().join("col/tools.js"))
        .expect("symlink");
    assert_both_loads(&repo, SMALL_ONLY);
}

#[cfg(unix)]
#[test]
fn walk_skips_a_symlinked_js_file() {
    let (dir, repo) = small();
    write(&dir, "outside/real.js", "module.exports = 1;");
    std::os::unix::fs::symlink(dir.path().join("outside/real.js"), dir.path().join("col/link.js"))
        .expect("symlink");
    assert_both_loads(&repo, SMALL_ONLY);
}

#[cfg(unix)]
#[test]
fn walk_still_lists_a_symlinked_request_file() {
    let (dir, repo) = small();
    let valid = fs::read_to_string(dir.path().join("col/req-a.yml")).expect("read request");
    write(&dir, "outside/target.yml", &valid);
    std::os::unix::fs::symlink(
        dir.path().join("outside/target.yml"),
        dir.path().join("col/link.yml"),
    )
    .expect("symlink");
    assert_both_loads(
        &repo,
        "dir:col|col|uid-set\n  req:link.yml|Alpha\n  req:req-a.yml|Alpha",
    );
}

#[cfg(unix)]
#[test]
fn walk_keeps_its_behavior_for_a_dangling_symlinked_request_file() {
    let (dir, repo) = small();
    std::os::unix::fs::symlink(dir.path().join("nowhere.yml"), dir.path().join("col/dead.yml"))
        .expect("symlink");
    // The full load reads the file with `?`, so a dangling link is an error today.
    assert!(repo.get("col").is_err(), "full load errors on a dangling link");
    // The summaries load skips an unreadable file.
    let summaries = repo.get_summaries("col").expect("summaries load");
    assert_eq!(outline(&summaries.root), as_summary(SMALL_ONLY));
}

#[test]
fn a_file_removed_after_the_listing_fails_a_loader_that_reads_with_a_question_mark() {
    let (dir, repo) = small();
    save(&repo, "req-b.yml", "Bravo");
    let root = dir.path().join("col");
    // `a` sorts before `b`, so `b` is gone by the time the walk reaches it.
    let result = build_tree(&root, &mut |path, name| {
        if name == "req-a.yml" {
            fs::remove_file(root.join("req-b.yml")).expect("remove the later file");
        }
        let _text = fs::read_to_string(path)?;
        Ok(None)
    });
    assert!(result.is_err(), "the full-load style loader errors on the removed file");
}

#[test]
fn a_file_removed_after_the_listing_is_skipped_by_a_lenient_loader() {
    let (dir, repo) = small();
    save(&repo, "req-b.yml", "Bravo");
    let root = dir.path().join("col");
    let mut seen: Vec<String> = Vec::new();
    let result = build_tree(&root, &mut |path, name| {
        if name == "req-a.yml" {
            fs::remove_file(root.join("req-b.yml")).expect("remove the later file");
        }
        if fs::read_to_string(path).is_ok() {
            seen.push(name.to_string());
        }
        Ok(None)
    });
    assert!(result.is_ok(), "the summaries style loader skips the removed file");
    assert_eq!(seen, vec!["req-a.yml".to_string()]);
}
