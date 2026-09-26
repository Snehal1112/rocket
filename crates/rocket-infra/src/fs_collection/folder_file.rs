//! Reads and writes `folder.yml` in the OpenCollection `Folder` shape
//! (`info` + `request` + `docs`). Files written before this change use a bare
//! `FolderInfo` shape with request defaults nested inside it, and are still read.

use std::fs;
use std::path::Path;

use rocket_shared::description::Description;
use rocket_shared::error::{DomainError, DomainResult};
use serde::Deserialize;

use crate::atomic_write;
use crate::oc::{OcFolder, OcFolderInfo, OcRequestDefaults};

/// Legacy `folder.yml` shape: folder info at the top level, request defaults inside it.
#[derive(Deserialize)]
struct LegacyFolderYml {
    name: String,
    #[serde(default)]
    uid: Option<String>,
    #[serde(default)]
    description: Option<Description>,
    #[serde(default, rename = "type")]
    folder_type: Option<String>,
    #[serde(default)]
    seq: Option<u32>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    request: Option<OcRequestDefaults>,
}

impl From<LegacyFolderYml> for OcFolder {
    fn from(old: LegacyFolderYml) -> Self {
        OcFolder {
            info: OcFolderInfo {
                name: old.name,
                uid: old.uid,
                description: old.description,
                folder_type: old.folder_type,
                seq: old.seq,
                tags: old.tags,
            },
            items: None,
            request: old.request,
            docs: None,
        }
    }
}

/// Parses `folder.yml` content. The spec shape is tried first. Its required
/// `info` key never appears in the legacy shape, so a legacy file fails that
/// parse and falls back to `LegacyFolderYml`. When both fail, the spec-shape
/// error is returned.
pub(crate) fn parse_folder_yml(content: &str) -> Result<OcFolder, serde_yaml::Error> {
    match serde_yaml::from_str::<OcFolder>(content) {
        Ok(folder) => Ok(folder),
        Err(spec_err) => serde_yaml::from_str::<LegacyFolderYml>(content)
            .map(OcFolder::from)
            .map_err(|_| spec_err),
    }
}

/// Reads and parses a `folder.yml` file.
pub(crate) fn read_folder_yml(path: &Path) -> DomainResult<OcFolder> {
    let content = fs::read_to_string(path)?;
    parse_folder_yml(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse folder.yml: {e}")))
}

/// Writes `folder.yml` in the spec shape. Items live as separate files in the
/// unbundled layout, so `items` is always dropped before writing.
pub(crate) fn write_folder_yml(path: &Path, folder: &OcFolder) -> DomainResult<()> {
    let on_disk = OcFolder {
        items: None,
        ..folder.clone()
    };
    let yaml = serde_yaml::to_string(&on_disk)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize folder.yml: {e}")))?;
    atomic_write(path, yaml.as_bytes())?;
    Ok(())
}

/// Builds the `folder.yml` content for a brand-new folder.
pub(crate) fn new_folder(name: String, uid: String) -> OcFolder {
    OcFolder {
        info: OcFolderInfo {
            name,
            uid: Some(uid),
            ..OcFolderInfo::default()
        },
        items: None,
        request: None,
        docs: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_folder_yml_reads_spec_shape() {
        let yaml = "info:\n  name: auth\n  uid: f-1\n  type: folder\nrequest:\n  variables:\n  - name: token\n    value: abc\n";
        let folder = parse_folder_yml(yaml).expect("spec shape parses");
        assert_eq!(folder.info.name, "auth");
        assert_eq!(folder.info.uid.as_deref(), Some("f-1"));
        let vars = folder.request.and_then(|r| r.variables).expect("vars");
        assert_eq!(vars[0].name, "token");
    }

    #[test]
    fn parse_folder_yml_falls_back_to_legacy_shape_and_lifts_request() {
        let yaml = "name: auth\nuid: f-1\ntype: folder\nrequest:\n  variables:\n  - name: token\n    value: abc\n";
        let folder = parse_folder_yml(yaml).expect("legacy shape parses");
        assert_eq!(folder.info.name, "auth");
        assert_eq!(folder.info.uid.as_deref(), Some("f-1"));
        let vars = folder
            .request
            .and_then(|r| r.variables)
            .expect("vars lifted to Folder.request");
        assert_eq!(vars[0].name, "token");
    }

    #[test]
    fn parse_folder_yml_rejects_garbage() {
        assert!(parse_folder_yml("{{{{not valid yaml: [[[").is_err());
    }

    #[test]
    fn write_folder_yml_emits_spec_shape_without_items() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("folder.yml");
        let mut folder = new_folder("auth".into(), "f-1".into());
        folder.items = Some(Vec::new());
        write_folder_yml(&path, &folder).expect("write");

        let raw: serde_yaml::Value =
            serde_yaml::from_str(&fs::read_to_string(&path).expect("read")).expect("yaml");
        assert!(raw.get("info").is_some(), "must be wrapped in info: {raw:?}");
        assert!(raw.get("name").is_none(), "no bare top-level name: {raw:?}");
        assert!(raw.get("items").is_none(), "items must never be written: {raw:?}");
        assert_eq!(raw["info"]["name"].as_str(), Some("auth"));
    }
}
