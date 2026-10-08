//! Reads and writes the folder tab's sections of `folder.yml`.

use std::path::{Component, Path, PathBuf};

use rocket_collection::{Collection, FolderSettings};
use rocket_shared::error::{DomainError, DomainResult};

use crate::conversions::{apply_folder_settings, oc_folder_to_folder_settings};
use crate::oc::{OcFolder, OcFolderInfo};

use super::folder_file::{read_folder_yml, write_folder_yml};
use super::FsCollectionRepo;

/// Resolves a folder path relative to the collection root. `""` is the root.
fn folder_dir(
    repo: &FsCollectionRepo,
    collection_dir: &Path,
    folder_path: &str,
) -> DomainResult<PathBuf> {
    if folder_path.is_empty() {
        Ok(collection_dir.to_path_buf())
    } else {
        repo.validate_path(collection_dir, Path::new(folder_path))
    }
}

/// Reads `folder.yml` and puts the folder path into a parse error.
fn read_named(path: &Path, folder_path: &str) -> DomainResult<OcFolder> {
    read_folder_yml(path).map_err(|e| match e {
        DomainError::Internal(msg) => {
            DomainError::Internal(format!("Folder '{folder_path}': {msg}"))
        }
        other => other,
    })
}

/// A new `folder.yml` for a directory that has none, named after the directory.
fn blank_folder(dir: &Path) -> OcFolder {
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    OcFolder {
        info: OcFolderInfo {
            name,
            ..OcFolderInfo::default()
        },
        items: None,
        request: None,
        docs: None,
    }
}

/// Reads a folder's own settings. A folder without `folder.yml` has default settings.
pub(super) fn get_folder_settings(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
) -> DomainResult<FolderSettings> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let dir = folder_dir(repo, &collection_dir, folder_path)?;
    let path = dir.join("folder.yml");
    if !path.exists() {
        return Ok(FolderSettings::default());
    }
    Ok(oc_folder_to_folder_settings(&read_named(
        &path,
        folder_path,
    )?))
}

/// Reads `folder.yml`, or starts a new one when it is absent, applies `edit` and
/// writes the file back atomically, all under the collection lock. With
/// `require_existing_dir`, a folder directory that does not exist is `InvalidInput`.
pub(super) fn edit_folder_yml(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
    require_existing_dir: bool,
    edit: impl FnOnce(&mut OcFolder),
) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let collection_dir = repo.collection_path(collection);
    let dir = folder_dir(repo, &collection_dir, folder_path)?;
    if require_existing_dir && !dir.is_dir() {
        return Err(DomainError::InvalidInput(format!(
            "Folder '{folder_path}' does not exist in collection '{collection}'"
        )));
    }
    let path = dir.join("folder.yml");
    let mut folder = if path.exists() {
        read_named(&path, folder_path)?
    } else {
        blank_folder(&dir)
    };
    edit(&mut folder);
    write_folder_yml(&path, &folder)
}

/// Saves a folder's own settings into its `folder.yml`. The collection root has
/// no `folder.yml`; its settings are the collection settings.
pub(super) fn save_folder_settings(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
    settings: &FolderSettings,
) -> DomainResult<()> {
    if folder_path.is_empty() {
        return Err(DomainError::InvalidInput(
            "Folder settings need a folder path. Use the collection settings for the root.".into(),
        ));
    }
    edit_folder_yml(repo, collection, folder_path, true, |folder| {
        apply_folder_settings(folder, settings)
    })
}

/// Settings of every ancestor folder of a request, outermost first. A folder
/// without `folder.yml` gives `FolderSettings::default()`, so there is one entry
/// per folder level. A `folder.yml` that does not parse is an error naming that
/// folder; unlike `get_folder_chain_variables`, nothing is skipped.
pub(super) fn get_folder_chain_settings(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
) -> DomainResult<Vec<FolderSettings>> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let parent = Path::new(request_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let mut segments = Vec::new();
    for component in parent.components() {
        match component {
            Component::Normal(segment) => segments.push(segment),
            Component::CurDir => {}
            _ => {
                return Err(DomainError::InvalidInput(format!(
                    "Invalid request path '{request_path}'"
                )))
            }
        }
    }

    let mut chain = Vec::with_capacity(segments.len());
    let mut rel = PathBuf::new();
    for segment in segments {
        rel.push(segment);
        let dir = repo.validate_path(&collection_dir, &rel)?;
        let path = dir.join("folder.yml");
        if path.exists() {
            let folder = read_named(&path, &rel.to_string_lossy())?;
            chain.push(oc_folder_to_folder_settings(&folder));
        } else {
            chain.push(FolderSettings::default());
        }
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rocket_collection::{CollectionRepository, CollectionVariable, FolderSettings};
    use rocket_shared::error::DomainError;
    use rocket_shared::types::{Auth, Header};
    use serde_yaml::Value;
    use tempfile::TempDir;

    use crate::FsCollectionRepo;

    /// A collection `api` with one folder `users` created through the repo.
    fn setup() -> (TempDir, FsCollectionRepo) {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        repo.create_folder("api", "users").expect("create folder");
        (dir, repo)
    }

    fn read_raw(dir: &TempDir, rel: &str) -> Value {
        let content =
            fs::read_to_string(dir.path().join("api").join(rel)).expect("read folder.yml");
        serde_yaml::from_str(&content).expect("valid yaml")
    }

    fn full_settings() -> FolderSettings {
        FolderSettings {
            headers: vec![
                Header::new("X-Tenant", "acme"),
                Header::disabled("X-Debug", "1"),
            ],
            auth: Some(Auth::Bearer {
                token: "{{token}}".into(),
            }),
            variables: vec![CollectionVariable {
                key: "region".into(),
                value: "eu".into(),
                initial_value: "eu".into(),
                enabled: true,
                secret: false,
            }],
            pre_request_script: Some("console.log('pre');".into()),
            post_response_script: Some("console.log('post');".into()),
            tests_script: Some("test('ok', () => {});".into()),
            docs: Some("# Users".into()),
        }
    }

    #[test]
    fn save_and_get_folder_settings_round_trip() {
        let (dir, repo) = setup();
        let uid_before = read_raw(&dir, "users/folder.yml")["info"]["uid"]
            .as_str()
            .map(str::to_string);
        assert!(uid_before.is_some(), "create_folder writes a uid");

        repo.save_folder_settings("api", "users", &full_settings())
            .expect("save");

        assert_eq!(
            repo.get_folder_settings("api", "users").expect("get"),
            full_settings()
        );
        let raw = read_raw(&dir, "users/folder.yml");
        assert_eq!(raw["info"]["name"].as_str(), Some("users"), "{raw:?}");
        assert_eq!(
            raw["info"]["uid"].as_str().map(str::to_string),
            uid_before,
            "{raw:?}"
        );
        assert_eq!(raw["docs"].as_str(), Some("# Users"), "{raw:?}");
    }

    #[test]
    fn get_folder_settings_without_folder_yml_is_default() {
        let (dir, repo) = setup();
        fs::create_dir_all(dir.path().join("api/billing")).expect("mkdir");
        assert_eq!(
            repo.get_folder_settings("api", "billing").expect("get"),
            FolderSettings::default()
        );
    }

    #[test]
    fn save_folder_settings_creates_folder_yml_when_absent() {
        let (dir, repo) = setup();
        // A folder made outside Rocket, with no folder.yml yet.
        fs::create_dir_all(dir.path().join("api/billing")).expect("mkdir");

        repo.save_folder_settings("api", "billing", &full_settings())
            .expect("save");

        let raw = read_raw(&dir, "billing/folder.yml");
        assert_eq!(raw["info"]["name"].as_str(), Some("billing"), "{raw:?}");
        assert_eq!(raw["info"]["type"].as_str(), Some("folder"), "{raw:?}");
        assert_eq!(
            repo.get_folder_settings("api", "billing").expect("get"),
            full_settings()
        );
    }

    #[test]
    fn save_folder_settings_to_a_missing_folder_is_invalid_input() {
        let (dir, repo) = setup();
        let err = repo
            .save_folder_settings("api", "ghost", &full_settings())
            .expect_err("missing folder");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
        assert!(
            !dir.path().join("api/ghost").exists(),
            "no directory may be created"
        );
    }

    #[test]
    fn save_folder_settings_rejects_the_collection_root() {
        let (dir, repo) = setup();
        let err = repo
            .save_folder_settings("api", "", &full_settings())
            .expect_err("root");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
        assert!(!dir.path().join("api/folder.yml").exists());
    }

    #[test]
    fn legacy_shape_folder_yml_still_reads() {
        let (dir, repo) = setup();
        fs::write(
            dir.path().join("api/users/folder.yml"),
            "name: users\nuid: legacy-uid\ntype: folder\nrequest:\n  headers:\n  - name: X-Legacy\n    value: '1'\n  variables:\n  - name: token\n    value: abc\n",
        )
        .expect("write legacy folder.yml");

        let settings = repo.get_folder_settings("api", "users").expect("get");
        assert_eq!(settings.headers, vec![Header::new("X-Legacy", "1")]);
        assert_eq!(settings.variables.len(), 1);
        assert_eq!(settings.variables[0].key, "token");
    }

    #[test]
    fn corrupt_folder_yml_error_names_the_folder() {
        let (dir, repo) = setup();
        let path = dir.path().join("api/users/folder.yml");
        fs::write(&path, "{{{{not valid yaml: [[[").expect("write");

        let read_err = repo
            .get_folder_settings("api", "users")
            .expect_err("corrupt read");
        assert!(read_err.to_string().contains("'users'"), "{read_err}");

        let save_err = repo
            .save_folder_settings("api", "users", &full_settings())
            .expect_err("corrupt save");
        assert!(save_err.to_string().contains("'users'"), "{save_err}");
        assert!(
            fs::read_to_string(&path)
                .expect("read")
                .contains("not valid yaml"),
            "the broken file must not be overwritten"
        );
    }

    #[test]
    fn folder_settings_path_traversal_is_rejected() {
        let (_dir, repo) = setup();
        let get = repo
            .get_folder_settings("api", "../../evil")
            .expect_err("traversal");
        assert!(
            matches!(get, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
            "{get:?}"
        );
        let save = repo
            .save_folder_settings("api", "../../evil", &full_settings())
            .expect_err("traversal");
        assert!(
            matches!(
                save,
                DomainError::InvalidInput(_) | DomainError::NotFound(_)
            ),
            "{save:?}"
        );
    }

    #[test]
    fn save_folder_settings_keeps_unknown_request_fields_on_disk() {
        let (dir, repo) = setup();
        fs::write(
            dir.path().join("api/users/folder.yml"),
            "info:\n  name: users\n  type: folder\nrequest:\n  metadata:\n  - name: x-trace\n    value: '1'\n  settings:\n    timeout: 5000\n  scripts:\n  - type: hooks\n    code: onStart()\n",
        )
        .expect("write fixture");

        repo.save_folder_settings("api", "users", &full_settings())
            .expect("save");

        let raw = read_raw(&dir, "users/folder.yml");
        assert_eq!(
            raw["request"]["metadata"][0]["name"].as_str(),
            Some("x-trace"),
            "{raw:?}"
        );
        assert!(raw["request"]["settings"]["timeout"].is_number(), "{raw:?}");
        let hooks: Vec<&Value> = raw["request"]["scripts"]
            .as_sequence()
            .expect("scripts")
            .iter()
            .filter(|s| s["type"].as_str() == Some("hooks"))
            .collect();
        assert_eq!(hooks.len(), 1, "{raw:?}");
        assert_eq!(hooks[0]["code"].as_str(), Some("onStart()"));
    }

    #[test]
    fn empty_settings_leave_no_empty_sections() {
        let (dir, repo) = setup();
        repo.save_folder_settings("api", "users", &full_settings())
            .expect("save full");
        repo.save_folder_settings("api", "users", &FolderSettings::default())
            .expect("save empty");
        let raw = read_raw(&dir, "users/folder.yml");
        assert!(raw.get("request").is_none(), "{raw:?}");
        assert!(raw.get("docs").is_none(), "{raw:?}");
        assert_eq!(raw["info"]["name"].as_str(), Some("users"), "{raw:?}");
    }

    #[test]
    fn save_folder_variables_keeps_the_other_sections() {
        let (_dir, repo) = setup();
        repo.save_folder_settings("api", "users", &full_settings())
            .expect("save settings");
        let vars = vec![CollectionVariable {
            key: "page".into(),
            value: "2".into(),
            initial_value: "2".into(),
            enabled: true,
            secret: false,
        }];

        repo.save_folder_variables("api", "users", vars.clone())
            .expect("save vars");

        assert_eq!(
            repo.get_folder_settings("api", "users").expect("get"),
            FolderSettings {
                variables: vars.clone(),
                ..full_settings()
            }
        );
        assert_eq!(
            repo.get_folder_variables("api", "users").expect("vars"),
            vars
        );
    }

    #[test]
    fn chain_has_one_entry_per_folder_outermost_first() {
        let (_dir, repo) = setup();
        // `users/bare` gets no folder.yml; create_folder only writes one for `inner`.
        repo.create_folder("api", "users/bare/inner")
            .expect("create inner");
        let outer = FolderSettings {
            headers: vec![Header::new("X-Outer", "1")],
            ..FolderSettings::default()
        };
        let inner = FolderSettings {
            headers: vec![Header::new("X-Inner", "2")],
            ..FolderSettings::default()
        };
        repo.save_folder_settings("api", "users", &outer)
            .expect("save outer");
        repo.save_folder_settings("api", "users/bare/inner", &inner)
            .expect("save inner");

        let chain = repo
            .get_folder_chain_settings("api", "users/bare/inner/list.yml")
            .expect("chain");

        assert_eq!(chain, vec![outer, FolderSettings::default(), inner]);
    }

    #[test]
    fn chain_of_a_root_level_request_is_empty() {
        let (_dir, repo) = setup();
        assert!(repo
            .get_folder_chain_settings("api", "list.yml")
            .expect("chain")
            .is_empty());
    }

    #[test]
    fn chain_reports_a_corrupt_folder_by_name() {
        let (dir, repo) = setup();
        repo.create_folder("api", "users/admin")
            .expect("create admin");
        fs::write(
            dir.path().join("api/users/admin/folder.yml"),
            "{{{{not valid yaml: [[[",
        )
        .expect("write");

        let err = repo
            .get_folder_chain_settings("api", "users/admin/list.yml")
            .expect_err("corrupt folder.yml");
        assert!(err.to_string().contains("'users/admin'"), "{err}");

        // The variables chain stays lenient and skips the broken file, as before.
        assert!(repo
            .get_folder_chain_variables("api", "users/admin/list.yml")
            .is_ok());
    }

    #[test]
    fn chain_rejects_parent_dir_components() {
        let (_dir, repo) = setup();
        let err = repo
            .get_folder_chain_settings("api", "users/../../evil/list.yml")
            .expect_err("traversal");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    }
}
