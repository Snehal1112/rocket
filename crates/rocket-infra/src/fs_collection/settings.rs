use std::fs;

use rocket_collection::{Collection, CollectionSettings, CollectionVariable};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::opencollection::{OcAuth, OcCollection, OcHttpRequestHeader, OcInfo, OcRequestDefaults, OcVariable};
use rocket_collection::generate_uid;

use super::FsCollectionRepo;

pub(super) fn get_settings(repo: &FsCollectionRepo, name: &str) -> DomainResult<CollectionSettings> {
    Collection::validate_name(name)?;
    let path = repo.settings_path(name);
    if !path.exists() {
        return Ok(CollectionSettings::default());
    }
    let content = fs::read_to_string(&path)?;
    let oc: OcCollection = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse opencollection.yml: {e}")))?;

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
        })
    } else {
        Ok(CollectionSettings {
            docs: oc.docs,
            ..CollectionSettings::default()
        })
    }
}

pub(super) fn save_settings(repo: &FsCollectionRepo, name: &str, settings: &CollectionSettings) -> DomainResult<()> {
    Collection::validate_name(name)?;
    let mutex = repo.collection_mutex(name);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let path = repo.settings_path(name);

    let mut oc: OcCollection = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_yaml::from_str(&content)
            .map_err(|e| DomainError::Internal(format!("Failed to parse opencollection.yml: {e}")))?
    } else {
        OcCollection {
            opencollection: Some("1.0.0".into()),
            uid: Some(generate_uid()),
            info: Some(OcInfo {
                name: name.into(),
                summary: None,
                version: None,
                authors: None,
            }),
            config: None,
            items: None,
            request: None,
            docs: None,
            bundled: None,
            extensions: None,
        }
    };

    // Build OcRequestDefaults from settings.
    let has_defaults =
        !settings.headers.is_empty() || settings.auth.is_some() || !settings.variables.is_empty();

    oc.request = if has_defaults {
        Some(OcRequestDefaults {
            headers: if settings.headers.is_empty() {
                None
            } else {
                Some(
                    settings
                        .headers
                        .iter()
                        .cloned()
                        .map(OcHttpRequestHeader::from)
                        .collect(),
                )
            },
            metadata: None,
            auth: settings.auth.clone().map(OcAuth::from),
            variables: if settings.variables.is_empty() {
                None
            } else {
                Some(
                    settings
                        .variables
                        .iter()
                        .cloned()
                        .map(OcVariable::from)
                        .collect(),
                )
            },
            scripts: None,
            settings: None,
        })
    } else {
        None
    };
    oc.docs = settings.docs.clone();

    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize opencollection.yml: {e}")))?;
    atomic_write(&path, yaml.as_bytes())?;

    // Clean up legacy collection.json.
    let legacy = repo.collection_path(name).join("collection.json");
    if legacy.exists() {
        let _ = fs::remove_file(&legacy);
    }

    Ok(())
}
