//! Bruno collection ZIP builder.

use std::collections::BTreeSet;
use std::io::{Cursor, Write};

use rocket_shared::error::{DomainError, DomainResult};
use serde_yaml::Value;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::ports::SourceFile;
use crate::report::{ExportAction, ExportNote, ExportReport};
use crate::scrub::{scrub_yaml, strip_rocket_namespace};

fn is_noise(rel_path: &str) -> bool {
    if rel_path == "workspace.yml" {
        return true;
    }
    rel_path
        .split('/')
        .any(|part| part == ".git" || part == "node_modules")
}

/// The top-level directory of a Rocket-only file, which Bruno cannot read: `flows/` and
/// any hidden directory such as `.rocket/` (contract snapshots).
fn rocket_only_root(rel_path: &str) -> Option<&str> {
    let (first, _) = rel_path.split_once('/')?;
    (first == "flows" || first.starts_with('.')).then_some(first)
}

/// Removes saved request examples, which hold captured responses that may contain
/// tokens. Returns true when something was removed.
fn drop_examples(value: &mut Value) -> bool {
    match value {
        Value::Mapping(map) => {
            let mut removed = false;
            if matches!(map.get("examples"), Some(Value::Sequence(_))) {
                map.remove("examples");
                removed = true;
            }
            for (_, child) in map.iter_mut() {
                removed |= drop_examples(child);
            }
            removed
        }
        Value::Sequence(items) => items.iter_mut().fold(false, |acc, i| drop_examples(i) | acc),
        _ => false,
    }
}

fn extension(rel_path: &str) -> &str {
    rel_path.rsplit_once('.').map(|(_, e)| e).unwrap_or("")
}

fn is_yaml(rel_path: &str) -> bool {
    matches!(extension(rel_path), "yml" | "yaml")
}

/// Records one `Preserved` note per file that has a `rok.` script.
fn note_rok_scripts(rel_path: &str, value: &Value, notes: &mut Vec<ExportNote>) {
    if has_rok_script(value) {
        notes.push(ExportNote {
            path: rel_path.to_string(),
            feature: "Scripts".to_string(),
            action: ExportAction::Preserved,
            detail: Some(
                "Scripts use rok.* calls. They are kept as written and need bru.* calls to run in Bruno."
                    .to_string(),
            ),
        });
    }
}

fn has_rok_script(value: &Value) -> bool {
    match value {
        Value::Mapping(map) => map.iter().any(|(key, child)| {
            if key.as_str() == Some("scripts") {
                if let Value::Sequence(items) = child {
                    let found = items.iter().any(|item| {
                        item.get("code")
                            .and_then(Value::as_str)
                            .is_some_and(|code| code.contains("rok."))
                    });
                    if found {
                        return true;
                    }
                }
            }
            has_rok_script(child)
        }),
        Value::Sequence(items) => items.iter().any(has_rok_script),
        _ => false,
    }
}

/// Rebuilds a legacy flat `folder.yml` into the OpenCollection `info` shape, which is the
/// only place Bruno reads the folder name. Returns true when the file was converted.
fn normalize_folder_yml(rel_path: &str, fallback_name: &str, value: &mut Value) -> bool {
    if rel_path.rsplit('/').next() != Some("folder.yml") {
        return false;
    }
    let Value::Mapping(map) = value else {
        return false;
    };
    if matches!(map.get("info"), Some(Value::Mapping(_))) {
        return false;
    }
    let dir_name = rel_path
        .rsplit('/')
        .nth(1)
        .filter(|d| !d.is_empty())
        .unwrap_or(fallback_name);
    let name = match map.get("name").and_then(Value::as_str) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => dir_name.to_string(),
    };
    let mut info = serde_yaml::Mapping::new();
    info.insert("name".into(), Value::String(name));
    info.insert("type".into(), Value::String("folder".to_string()));
    for key in ["seq", "tags"] {
        if let Some(v) = map.remove(key) {
            info.insert(key.into(), v);
        }
    }
    map.remove("name");
    map.remove("type");
    map.remove("uid");
    let mut rebuilt = serde_yaml::Mapping::new();
    rebuilt.insert("info".into(), Value::Mapping(info));
    for (k, v) in std::mem::take(map) {
        rebuilt.insert(k, v);
    }
    *map = rebuilt;
    true
}

/// Removes the top-level `uid` and `info.uid`, which are not part of the OpenCollection
/// schema. Returns true when a key was removed.
fn strip_uids(value: &mut Value) -> bool {
    let Value::Mapping(map) = value else {
        return false;
    };
    let mut removed = map.remove("uid").is_some();
    if let Some(Value::Mapping(info)) = map.get_mut("info") {
        removed |= info.remove("uid").is_some();
    }
    removed
}

/// Builds a Bruno-readable ZIP with one root folder named after the collection.
pub fn bruno_zip(
    files: Vec<SourceFile>,
    collection_name: &str,
    include_environments: bool,
) -> DomainResult<(Vec<u8>, ExportReport)> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut report = ExportReport::default();
    let mut rocket_only: BTreeSet<String> = BTreeSet::new();
    let mut uid_files = 0usize;

    for file in files {
        let rel = file.rel_path.as_str();
        if is_noise(rel) {
            continue;
        }
        if let Some(root) = rocket_only_root(rel) {
            rocket_only.insert(root.to_string());
            continue;
        }
        let ext = extension(rel);
        if !matches!(ext, "yml" | "yaml" | "proto" | "js") {
            continue;
        }
        let in_env = rel.starts_with("environments/");
        if in_env && !include_environments {
            continue;
        }
        if is_yaml(rel) {
            let mut value: Value = serde_yaml::from_slice(&file.bytes)
                .map_err(|e| DomainError::InvalidInput(format!("{rel}: invalid YAML: {e}")))?;
            if normalize_folder_yml(rel, collection_name, &mut value) {
                report.items.push(ExportNote {
                    path: rel.to_string(),
                    feature: "Legacy folder.yml".to_string(),
                    action: ExportAction::Translated,
                    detail: Some("converted to the info shape".to_string()),
                });
            }
            if strip_uids(&mut value) {
                uid_files += 1;
            }
            strip_rocket_namespace(&mut value);
            if drop_examples(&mut value) {
                report.items.push(ExportNote {
                    path: rel.to_string(),
                    feature: "Examples".to_string(),
                    action: ExportAction::Skipped,
                    detail: Some(
                        "Saved responses can hold tokens, so they are not exported.".to_string(),
                    ),
                });
            }
            scrub_yaml(rel, &mut value, &mut report.items);
            note_rok_scripts(rel, &value, &mut report.items);
            let text = serde_yaml::to_string(&value)
                .map_err(|e| DomainError::Internal(format!("{rel}: cannot write YAML: {e}")))?;
            entries.push((rel.to_string(), text.into_bytes()));
        } else {
            entries.push((rel.to_string(), file.bytes));
        }
    }
    if uid_files > 0 {
        report.items.push(ExportNote {
            path: "(collection)".to_string(),
            feature: "uid".to_string(),
            action: ExportAction::Translated,
            detail: Some(format!(
                "{uid_files} files had a uid key removed (not part of the OpenCollection schema)"
            )),
        });
    }
    for root in rocket_only {
        report.items.push(ExportNote {
            path: root,
            feature: "Rocket-only files".to_string(),
            action: ExportAction::Skipped,
            detail: Some("Bruno cannot read these files, so they are not exported.".to_string()),
        });
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    // Every file in the ZIP counts: requests, folders, scripts, protos and environments.
    report.exported = entries.len();

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    for (rel, bytes) in &entries {
        writer
            .start_file(format!("{collection_name}/{rel}"), options)
            .map_err(|e| DomainError::Internal(format!("zip: {e}")))?;
        writer
            .write_all(bytes)
            .map_err(|e| DomainError::Internal(format!("zip: {e}")))?;
    }
    let cursor = writer
        .finish()
        .map_err(|e| DomainError::Internal(format!("zip: {e}")))?;
    Ok((cursor.into_inner(), report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile {
            rel_path: path.to_string(),
            bytes: text.as_bytes().to_vec(),
        }
    }

    fn names(zip_bytes: &[u8]) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes)).expect("valid zip");
        (0..archive.len())
            .map(|i| archive.by_index(i).expect("entry").name().to_string())
            .collect()
    }

    fn read(zip_bytes: &[u8], name: &str) -> String {
        let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes)).expect("valid zip");
        let mut entry = archive.by_name(name).expect("entry exists");
        let mut text = String::new();
        entry.read_to_string(&mut text).expect("utf8");
        text
    }

    fn sample() -> Vec<SourceFile> {
        vec![
            file("opencollection.yml", "opencollection: \"1.0.0\"\ninfo:\n  name: my-api\n"),
            file("users/folder.yml", "info:\n  name: users\n"),
            file(
                "users/get.yml",
                "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x\n",
            ),
            file("environments/local.yml", "name: local\nvariables: []\n"),
            file(".git/config", "x"),
            file("node_modules/a/b.js", "x"),
            file("workspace.yml", "name: ws\n"),
            file("notes.txt", "x"),
        ]
    }

    #[test]
    fn bruno_zip_has_single_root_folder_and_skips_noise() {
        let (bytes, report) = bruno_zip(sample(), "my-api", true).expect("zip builds");
        assert_eq!(
            names(&bytes),
            vec![
                "my-api/environments/local.yml",
                "my-api/opencollection.yml",
                "my-api/users/folder.yml",
                "my-api/users/get.yml"
            ]
        );
        // Every file in the ZIP counts, environments included.
        assert_eq!(report.exported, 4);
    }

    #[test]
    fn bruno_zip_counts_proto_and_js_files_too() {
        let files = vec![
            file("opencollection.yml", "info:\n  name: a\n"),
            file("protos/a.proto", "syntax = \"proto3\";"),
            file("shared/util.js", "x"),
        ];
        let (bytes, report) = bruno_zip(files, "a", true).expect("zip builds");
        assert_eq!(report.exported, names(&bytes).len());
        assert_eq!(report.exported, 3);
    }

    #[test]
    fn bruno_zip_without_environments_leaves_folder_out() {
        let (bytes, _) = bruno_zip(sample(), "my-api", false).expect("zip builds");
        assert!(names(&bytes)
            .iter()
            .all(|n| !n.starts_with("my-api/environments/")));
    }

    #[test]
    fn bruno_zip_drops_saved_examples_and_reports_them() {
        let files = vec![file(
            "users/login.yml",
            "info:\n  name: login\nhttp:\n  method: POST\n  url: https://x\nexamples:\n  - name: ok\n    response:\n      body:\n        data: '{\"access_token\":\"LEAK8\"}'\n",
        )];
        let (bytes, report) = bruno_zip(files, "a", true).expect("zip builds");
        let text = read(&bytes, "a/users/login.yml");
        assert!(!text.contains("LEAK8"));
        assert!(!text.contains("examples"));
        let note = report
            .items
            .iter()
            .find(|n| n.feature == "Examples")
            .expect("examples note");
        assert_eq!(note.path, "users/login.yml");
        assert_eq!(note.action, ExportAction::Skipped);
    }

    #[test]
    fn bruno_zip_skips_rocket_only_directories_and_reports_them() {
        let files = vec![
            file("opencollection.yml", "info:\n  name: a\n"),
            file("users/get.yml", "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x\n"),
            file("flows/f.yml", "name: f\n"),
            file("flows/deep/g.yml", "name: g\n"),
            file(".rocket/contracts/a.yml", "name: c\n"),
        ];
        let (bytes, report) = bruno_zip(files, "a", true).expect("zip builds");
        assert_eq!(names(&bytes), vec!["a/opencollection.yml", "a/users/get.yml"]);
        assert_eq!(report.exported, 2);
        let skipped: Vec<&str> = report
            .items
            .iter()
            .filter(|n| n.feature == "Rocket-only files")
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(skipped, [".rocket", "flows"]);
    }

    #[test]
    fn bruno_zip_strips_rocketapi_namespace() {
        let files = vec![file(
            "opencollection.yml",
            "info:\n  name: a\nextensions:\n  rocketapi:\n    sandboxMode: developer\n  bruno:\n    other: 1\n",
        )];
        let (bytes, _) = bruno_zip(files, "a", true).expect("zip builds");
        let text = read(&bytes, "a/opencollection.yml");
        assert!(!text.contains("rocketapi"));
        assert!(text.contains("bruno"));
    }

    #[test]
    fn bruno_zip_blanks_credentials_and_reports() {
        let files = vec![file(
            "users/get.yml",
            "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x\n  auth:\n    type: bearer\n    token: abc\n",
        )];
        let (bytes, report) = bruno_zip(files, "a", true).expect("zip builds");
        assert!(!read(&bytes, "a/users/get.yml").contains("abc"));
        assert!(report
            .items
            .iter()
            .any(|n| n.action == ExportAction::Blanked && n.path == "users/get.yml"));
    }

    #[test]
    fn bruno_zip_notes_rok_scripts() {
        let files = vec![file(
            "users/get.yml",
            "info:\n  name: get\nruntime:\n  scripts:\n  - type: before-request\n    code: \"rok.setVar('a', 1)\"\n",
        )];
        let (_, report) = bruno_zip(files, "a", true).expect("zip builds");
        let preserved: Vec<_> = report
            .items
            .iter()
            .filter(|n| n.action == ExportAction::Preserved)
            .collect();
        assert_eq!(preserved.len(), 1);
        assert!(preserved[0]
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("bru.*")));
    }

    #[test]
    fn bruno_zip_keeps_proto_and_js_files() {
        let files = vec![file("protos/a.proto", "syntax"), file("shared/util.js", "x")];
        let (bytes, _) = bruno_zip(files, "a", true).expect("zip builds");
        assert_eq!(names(&bytes), vec!["a/protos/a.proto", "a/shared/util.js"]);
    }

    #[test]
    fn bruno_zip_rejects_unparsable_yaml() {
        let files = vec![file("users/bad.yml", ": : :")];
        assert!(bruno_zip(files, "a", true).is_err());
    }

    fn yaml(zip_bytes: &[u8], name: &str) -> Value {
        serde_yaml::from_str(&read(zip_bytes, name)).expect("valid yaml")
    }

    fn feature_notes<'a>(report: &'a ExportReport, feature: &str) -> Vec<&'a ExportNote> {
        report.items.iter().filter(|n| n.feature == feature).collect()
    }

    #[test]
    fn flat_folder_yml_is_converted_to_the_info_shape() {
        let files = vec![file("users/folder.yml", "name: users\nuid: u1\ntype: folder\n")];
        let (bytes, report) = bruno_zip(files, "a", true).expect("zip builds");
        let v = yaml(&bytes, "a/users/folder.yml");
        assert_eq!(v["info"]["name"].as_str(), Some("users"));
        assert_eq!(v["info"]["type"].as_str(), Some("folder"));
        assert!(v.get("name").is_none());
        assert!(v.get("uid").is_none());
        assert!(v.get("type").is_none());
        let notes = feature_notes(&report, "Legacy folder.yml");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].action, ExportAction::Translated);
        assert_eq!(notes[0].path, "users/folder.yml");
        assert_eq!(notes[0].detail.as_deref(), Some("converted to the info shape"));
    }

    #[test]
    fn flat_folder_yml_keeps_seq_tags_request_and_docs() {
        let files = vec![file(
            "users/folder.yml",
            "name: users\ntype: folder\nseq: 3\ntags: [a]\nrequest:\n  auth:\n    type: bearer\n    token: '{{t}}'\ndocs: hello\n",
        )];
        let (bytes, _) = bruno_zip(files, "a", true).expect("zip builds");
        let v = yaml(&bytes, "a/users/folder.yml");
        assert_eq!(v["info"]["seq"].as_u64(), Some(3));
        assert_eq!(v["info"]["tags"][0].as_str(), Some("a"));
        assert!(v.get("seq").is_none());
        assert!(v.get("tags").is_none());
        assert_eq!(v["docs"].as_str(), Some("hello"));
        assert_eq!(v["request"]["auth"]["token"].as_str(), Some("{{t}}"));
    }

    #[test]
    fn flat_folder_yml_without_a_name_uses_the_directory_name() {
        let files = vec![file("reports/folder.yml", "uid: u2\ntype: folder\n")];
        let (bytes, _) = bruno_zip(files, "a", true).expect("zip builds");
        let v = yaml(&bytes, "a/reports/folder.yml");
        assert_eq!(v["info"]["name"].as_str(), Some("reports"));
    }

    #[test]
    fn info_form_folder_yml_is_kept_and_loses_its_uid() {
        let files = vec![file(
            "keys/folder.yml",
            "info:\n  name: keys\n  uid: u3\n  type: folder\ndocs: x\n",
        )];
        let (bytes, report) = bruno_zip(files, "a", true).expect("zip builds");
        let v = yaml(&bytes, "a/keys/folder.yml");
        assert_eq!(v["info"]["name"].as_str(), Some("keys"));
        assert_eq!(v["info"]["type"].as_str(), Some("folder"));
        assert!(v["info"].get("uid").is_none());
        assert_eq!(v["docs"].as_str(), Some("x"));
        assert!(feature_notes(&report, "Legacy folder.yml").is_empty());
    }

    #[test]
    fn request_and_collection_uid_are_dropped() {
        let files = vec![
            file(
                "get.yml",
                "uid: r1\ninfo:\n  name: get\n  type: http\nhttp:\n  method: GET\n  url: https://x\n",
            ),
            file("opencollection.yml", "uid: c1\ninfo:\n  name: a\n"),
        ];
        let (bytes, report) = bruno_zip(files, "a", true).expect("zip builds");
        let req = yaml(&bytes, "a/get.yml");
        assert!(req.get("uid").is_none());
        assert_eq!(req["info"]["name"].as_str(), Some("get"));
        assert_eq!(req["http"]["url"].as_str(), Some("https://x"));
        assert!(yaml(&bytes, "a/opencollection.yml").get("uid").is_none());
        let notes = feature_notes(&report, "uid");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].action, ExportAction::Translated);
        assert_eq!(notes[0].path, "(collection)");
        assert_eq!(
            notes[0].detail.as_deref(),
            Some("2 files had a uid key removed (not part of the OpenCollection schema)")
        );
    }

    #[test]
    fn files_without_uid_produce_no_uid_note() {
        let (_, report) = bruno_zip(sample(), "my-api", true).expect("zip builds");
        assert!(feature_notes(&report, "uid").is_empty());
    }
}
