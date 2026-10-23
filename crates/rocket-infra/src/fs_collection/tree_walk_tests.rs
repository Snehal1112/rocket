//! Pins what the tree walk loads and skips, so work on `tree.rs` cannot change it.

use std::fs;

use rocket_collection::{CollectionItem, CollectionRepository, Folder};
use rocket_shared::types::HttpMethod;
use tempfile::TempDir;

use super::tree::{build_tree, read_order_list};
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
    assert_eq!(
        outline(&summaries.root),
        as_summary(expected),
        "summaries load"
    );
}

const SMALL_ONLY: &str = "dir:col|col|uid-set\n  req:req-a.yml|Alpha";

/// A collection with folders, a script file, nested `flows`, and entries the tree hides.
fn rich() -> (TempDir, FsCollectionRepo) {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    save(&repo, "req-a.yml", "Alpha");
    save(&repo, "req-c.yml", "Gamma");
    write(&dir, "col/lib.js", "module.exports = 1;");
    write(
        &dir,
        "col/a-folder/folder.yml",
        &folder_yml("Alpha Display", "uid-a"),
    );
    save(&repo, "a-folder/x.yml", "X");
    write(
        &dir,
        "col/b-folder/folder.yml",
        &folder_yml("b-folder", "uid-b"),
    );
    write(
        &dir,
        "col/b-folder/nested/folder.yml",
        &folder_yml("nested", "uid-n"),
    );
    write(
        &dir,
        "col/b-folder/nested/flows/folder.yml",
        &folder_yml("flows", "uid-f"),
    );
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
    std::os::unix::fs::symlink(
        dir.path().join("outside"),
        dir.path().join("col/dirlink.yml"),
    )
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
    std::os::unix::fs::symlink(
        dir.path().join("outside/real.js"),
        dir.path().join("col/link.js"),
    )
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
    std::os::unix::fs::symlink(
        dir.path().join("nowhere.yml"),
        dir.path().join("col/dead.yml"),
    )
    .expect("symlink");
    // The full load reads the file with `?`, so a dangling link is an error today.
    assert!(
        repo.get("col").is_err(),
        "full load errors on a dangling link"
    );
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
    assert!(
        result.is_err(),
        "the full-load style loader errors on the removed file"
    );
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
    assert!(
        result.is_ok(),
        "the summaries style loader skips the removed file"
    );
    assert_eq!(seen, vec!["req-a.yml".to_string()]);
}

const RICH_ORDERED: &str = "dir:col|col|uid-set
  req:req-c.yml|Gamma
  js:lib.js
  dir:b-folder|b-folder|uid-b
    dir:nested|nested|uid-n
      dir:flows|flows|uid-f
        req:inner.yml|Inner
  dir:a-folder|Alpha Display|uid-a
    req:x.yml|X
  req:req-a.yml|Alpha";

#[test]
fn walk_applies_order_yml_and_puts_unlisted_names_last() {
    let (dir, repo) = rich();
    write(
        &dir,
        "col/_order.yml",
        "- req-c.yml\n- lib.js\n- b-folder\n",
    );
    assert_both_loads(&repo, RICH_ORDERED);
}

#[test]
fn walk_applies_order_json_when_there_is_no_order_yml() {
    let (dir, repo) = rich();
    write(
        &dir,
        "col/_order.json",
        "[\"req-c.yml\", \"lib.js\", \"b-folder\"]",
    );
    assert_both_loads(&repo, RICH_ORDERED);
}

#[test]
fn walk_prefers_order_yml_over_order_json() {
    let (dir, repo) = rich();
    write(
        &dir,
        "col/_order.yml",
        "- req-c.yml\n- lib.js\n- b-folder\n",
    );
    write(&dir, "col/_order.json", "[\"req-a.yml\"]");
    assert_both_loads(&repo, RICH_ORDERED);
}

#[test]
fn walk_falls_back_to_name_order_for_an_unparseable_order_yml_without_reading_the_json() {
    let (dir, repo) = rich();
    write(&dir, "col/_order.yml", "{{{ not a list");
    write(&dir, "col/_order.json", "[\"req-c.yml\", \"lib.js\"]");
    assert_both_loads(&repo, RICH_BY_NAME);
}

#[test]
fn walk_gives_name_order_when_order_yml_is_a_directory_and_does_not_read_the_json() {
    let (dir, repo) = rich();
    // The directory is listed as a folder like any other. A folder.yml gives it a fixed uid.
    write(
        &dir,
        "col/_order.yml/folder.yml",
        &folder_yml("_order.yml", "uid-o"),
    );
    write(&dir, "col/_order.json", "[\"req-c.yml\", \"lib.js\"]");
    let expected = RICH_BY_NAME.replacen(
        "\n  dir:a-folder",
        "\n  dir:_order.yml|_order.yml|uid-o\n  dir:a-folder",
        1,
    );
    assert_both_loads(&repo, &expected);
}

#[test]
fn walk_keeps_the_last_position_of_a_duplicate_and_ignores_unknown_names() {
    let (dir, repo) = rich();
    // req-c.yml is listed first and last, so it counts at position 3. The unknown name changes nothing.
    write(
        &dir,
        "col/_order.yml",
        "- req-c.yml\n- not-there.yml\n- req-a.yml\n- req-c.yml\n",
    );
    assert_both_loads(
        &repo,
        "dir:col|col|uid-set
  req:req-a.yml|Alpha
  req:req-c.yml|Gamma
  dir:a-folder|Alpha Display|uid-a
    req:x.yml|X
  dir:b-folder|b-folder|uid-b
    dir:nested|nested|uid-n
      dir:flows|flows|uid-f
        req:inner.yml|Inner
  js:lib.js",
    );
}

#[cfg(unix)]
#[test]
fn walk_lists_a_non_utf8_name_and_sorts_it_after_the_listed_names() {
    use std::os::unix::ffi::OsStrExt;
    let (dir, repo) = small();
    save(&repo, "req-b.yml", "Bravo");
    let valid = fs::read_to_string(dir.path().join("col/req-a.yml")).expect("read request");
    let odd = std::ffi::OsStr::from_bytes(b"req-\xFF.yml");
    fs::write(dir.path().join("col").join(odd), &valid).expect("write the odd name");
    write(&dir, "col/_order.yml", "- req-b.yml\n");
    // The listed name comes first, then the unlisted names by file name. The odd name shows lossily.
    assert_both_loads(
        &repo,
        "dir:col|col|uid-set\n  req:req-b.yml|Bravo\n  req:req-a.yml|Alpha\n  req:req-\u{FFFD}.yml|Alpha",
    );
}

fn first_subfolder(folder: &Folder) -> &Folder {
    folder
        .items
        .iter()
        .find_map(|i| match i {
            CollectionItem::Folder(f) => Some(f),
            _ => None,
        })
        .expect("a subfolder")
}

#[test]
fn walk_takes_the_root_uid_from_opencollection_yml() {
    let (dir, repo) = small();
    let first = repo.get("col").expect("first load").root.uid;
    let text = fs::read_to_string(dir.path().join("col/opencollection.yml"))
        .expect("read opencollection.yml");
    assert!(!first.is_empty());
    assert!(
        text.contains(&first),
        "the root uid is the one stored in opencollection.yml"
    );
    let second = repo.get_summaries("col").expect("second load").root.uid;
    assert_eq!(first, second);
}

#[test]
fn walk_takes_uid_and_display_name_from_folder_yml() {
    let (_dir, repo) = rich();
    let full = repo.get("col").expect("full load");
    let folder = first_subfolder(&full.root);
    assert_eq!(folder.dir_name.as_deref(), Some("a-folder"));
    assert_eq!(folder.name, "Alpha Display");
    assert_eq!(folder.uid, "uid-a");
}

#[test]
fn walk_falls_back_to_the_legacy_uid_file_and_migrates_it_into_folder_yml() {
    let (dir, repo) = small();
    write(&dir, "col/legacy/folder.yml", "info:\n  name: legacy\n");
    write(&dir, "col/legacy/.uid", "legacy-uid\n");
    let full = repo.get("col").expect("full load");
    assert_eq!(first_subfolder(&full.root).uid, "legacy-uid");
    // The load wrote the uid into folder.yml and deleted the legacy file, as it does today.
    let migrated =
        fs::read_to_string(dir.path().join("col/legacy/folder.yml")).expect("read folder.yml");
    assert!(
        migrated.contains("legacy-uid"),
        "folder.yml now holds the uid: {migrated}"
    );
    assert!(
        !dir.path().join("col/legacy/.uid").exists(),
        "the .uid file is deleted"
    );
}

#[test]
fn walk_uses_the_legacy_uid_file_for_a_folder_without_folder_yml_and_leaves_it_alone() {
    let (dir, repo) = small();
    write(&dir, "col/plain/.uid", "plain-uid\n");
    write(&dir, "col/plain/p.yml", "not used");
    let full = repo.get_summaries("col").expect("summaries load");
    let folder = first_subfolder(&full.root);
    assert_eq!(folder.uid, "plain-uid");
    assert_eq!(folder.name, "plain");
    assert!(
        dir.path().join("col/plain/.uid").exists(),
        "no YAML was written, so .uid stays"
    );
    assert!(
        !dir.path().join("col/plain/folder.yml").exists(),
        "no folder.yml is created"
    );
}

#[test]
fn walk_keeps_the_directory_name_for_an_unparseable_folder_yml_and_does_not_touch_it() {
    let (dir, repo) = small();
    write(&dir, "col/broken/folder.yml", "info: [unclosed");
    let full = repo.get_summaries("col").expect("summaries load");
    let folder = first_subfolder(&full.root);
    assert_eq!(folder.name, "broken");
    assert!(!folder.uid.is_empty(), "a uid is generated");
    let text =
        fs::read_to_string(dir.path().join("col/broken/folder.yml")).expect("read folder.yml");
    assert_eq!(text, "info: [unclosed");
}

#[test]
fn walk_of_a_missing_folder_is_the_empty_folder() {
    let (dir, _repo) = setup();
    let gone = dir.path().join("no-such-folder");
    let folder =
        super::tree::build_folder_tree(&gone, false).expect("a missing folder is not an error");
    assert!(folder.items.is_empty());
    assert_eq!(folder.name, "no-such-folder");
    assert_eq!(folder.dir_name.as_deref(), Some("no-such-folder"));
}

#[test]
fn read_order_list_prefers_yml_and_falls_back_to_json_only_when_yml_is_absent() {
    let (dir, _repo) = setup();
    write(&dir, "d/_order.yml", "- a.yml\n- b.yml\n");
    write(&dir, "d/_order.json", "[\"z.yml\"]");
    assert_eq!(
        read_order_list(&dir.path().join("d")),
        Some(vec!["a.yml".to_string(), "b.yml".to_string()])
    );
    write(&dir, "j/_order.json", "[\"z.yml\", \"y.yml\"]");
    assert_eq!(
        read_order_list(&dir.path().join("j")),
        Some(vec!["z.yml".to_string(), "y.yml".to_string()])
    );
}

#[test]
fn read_order_list_is_none_for_garbage_a_directory_or_no_file() {
    let (dir, _repo) = setup();
    write(&dir, "g/_order.yml", "{{{ not a list");
    write(&dir, "g/_order.json", "[\"z.yml\"]");
    assert_eq!(read_order_list(&dir.path().join("g")), None);
    fs::create_dir_all(dir.path().join("dd/_order.yml")).expect("order dir");
    write(&dir, "dd/_order.json", "[\"z.yml\"]");
    assert_eq!(read_order_list(&dir.path().join("dd")), None);
    write(&dir, "e/other.yml", "x");
    assert_eq!(read_order_list(&dir.path().join("e")), None);
}

#[test]
fn summaries_list_example_names_in_order() {
    let (dir, repo) = setup();
    repo.create("col").expect("create collection");
    save(&repo, "a.yml", "A");
    let path = dir.path().join("col").join("a.yml");
    let mut text = fs::read_to_string(&path).expect("read request");
    text.push_str("examples:\n  - name: 200 OK\n  - name: '401'\n");
    fs::write(&path, text).expect("write examples");

    let summaries = repo.get_summaries("col").expect("summaries load");
    let names: Vec<_> = summaries
        .root
        .items
        .iter()
        .filter_map(|i| match i {
            CollectionItem::Summary(s) => Some(s.example_names.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(names, vec![vec!["200 OK".to_string(), "401".to_string()]]);
}

/// Saves request `A`, appends `tail` to its file and returns the example names of every
/// summary in the root, so a corrupt `examples` block can be checked not to hide the request.
fn example_names_after_appending(tail: &str) -> Vec<Vec<String>> {
    let (dir, repo) = setup();
    repo.create("col").expect("create collection");
    save(&repo, "a.yml", "A");
    let path = dir.path().join("col").join("a.yml");
    let mut text = fs::read_to_string(&path).expect("read request");
    text.push_str(tail);
    fs::write(&path, text).expect("write examples");
    let summaries = repo.get_summaries("col").expect("summaries load");
    summaries
        .root
        .items
        .iter()
        .filter_map(|i| match i {
            CollectionItem::Summary(s) => Some(s.example_names.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn summaries_skip_an_example_without_a_name() {
    let names = example_names_after_appending(
        "examples:\n  - name: first\n  - response:\n      status: 200\n  - name: last\n",
    );
    assert_eq!(names, vec![vec!["first".to_string(), "last".to_string()]]);
}

#[test]
fn summaries_skip_an_example_with_a_non_string_name() {
    let names = example_names_after_appending("examples:\n  - name: 200\n  - name: ok\n");
    assert_eq!(names, vec![vec!["ok".to_string()]]);
}

#[test]
fn summaries_keep_the_request_when_examples_is_not_a_list() {
    let names = example_names_after_appending("examples: nope\n");
    assert_eq!(names, vec![Vec::<String>::new()]);
}

#[test]
fn summaries_keep_the_request_when_examples_is_empty() {
    let names = example_names_after_appending("examples: []\n");
    assert_eq!(names, vec![Vec::<String>::new()]);
}
