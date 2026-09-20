use std::fs;

use rocket_collection::settings::SandboxMode;
use rocket_collection::{Collection, CollectionSettings, CollectionVariable};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::oc::{OcAuth, OcCollection, OcHttpRequestHeader, OcInfo, OcRequestDefaults, OcVariable};
use rocket_collection::generate_uid;

use super::FsCollectionRepo;

/// Reads `sandbox_mode` out of `opencollection.yml`'s free-form `extensions` field
/// (`extensions.rocketapi.sandboxMode`), since `sandboxMode` is a RocketAPI-specific
/// setting, not part of the OpenCollection spec's `additionalProperties: false` schema —
/// `extensions` is the spec's own designated escape hatch for exactly this. Missing or
/// unrecognized values default to `Safe`, matching `SandboxMode`'s own `#[default]`.
fn sandbox_mode_from_extensions(extensions: &Option<serde_yaml::Value>) -> SandboxMode {
    extensions
        .as_ref()
        .and_then(|v| v.get("rocketapi"))
        .and_then(|v| v.get("sandboxMode"))
        .and_then(|v| v.as_str())
        .and_then(|s| match s {
            "developer" => Some(SandboxMode::Developer),
            "safe" => Some(SandboxMode::Safe),
            _ => None,
        })
        .unwrap_or_default()
}

/// Writes `sandbox_mode` into `extensions.rocketapi.sandboxMode`, preserving any other
/// keys already present under `extensions` or under `extensions.rocketapi` (the field is
/// free-form and may carry data this app doesn't own).
fn set_sandbox_mode_in_extensions(
    extensions: Option<serde_yaml::Value>,
    mode: SandboxMode,
) -> Option<serde_yaml::Value> {
    let mode_str = match mode {
        SandboxMode::Safe => "safe",
        SandboxMode::Developer => "developer",
    };

    let mut root = match extensions {
        Some(serde_yaml::Value::Mapping(map)) => map,
        _ => serde_yaml::Mapping::new(),
    };

    let rocketapi_key = serde_yaml::Value::String("rocketapi".into());
    let mut rocketapi = match root.get(&rocketapi_key) {
        Some(serde_yaml::Value::Mapping(map)) => map.clone(),
        _ => serde_yaml::Mapping::new(),
    };
    rocketapi.insert(
        serde_yaml::Value::String("sandboxMode".into()),
        serde_yaml::Value::String(mode_str.into()),
    );
    root.insert(rocketapi_key, serde_yaml::Value::Mapping(rocketapi));

    Some(serde_yaml::Value::Mapping(root))
}

pub(super) fn get_settings(repo: &FsCollectionRepo, name: &str) -> DomainResult<CollectionSettings> {
    Collection::validate_name(name)?;
    let path = repo.settings_path(name);
    if !path.exists() {
        return Ok(CollectionSettings::default());
    }
    let content = fs::read_to_string(&path)?;
    let oc: OcCollection = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse opencollection.yml: {e}")))?;

    let sandbox_mode = sandbox_mode_from_extensions(&oc.extensions);

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
            sandbox_mode,
        })
    } else {
        Ok(CollectionSettings {
            docs: oc.docs,
            sandbox_mode,
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
    oc.extensions = set_sandbox_mode_in_extensions(oc.extensions.take(), settings.sandbox_mode);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_mode_from_extensions_none_defaults_to_safe() {
        assert_eq!(sandbox_mode_from_extensions(&None), SandboxMode::Safe);
    }

    #[test]
    fn sandbox_mode_from_extensions_reads_existing_rocketapi_mapping_with_other_keys() {
        let yaml = "rocketapi:\n  sandboxMode: developer\n  someOtherField: true\nunrelatedTool:\n  foo: bar\n";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse fixture yaml");
        assert_eq!(
            sandbox_mode_from_extensions(&Some(value)),
            SandboxMode::Developer
        );
    }

    #[test]
    fn sandbox_mode_from_extensions_unrecognized_value_falls_back_to_safe() {
        let yaml = "rocketapi:\n  sandboxMode: yolo\n";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse fixture yaml");
        assert_eq!(sandbox_mode_from_extensions(&Some(value)), SandboxMode::Safe);
    }

    #[test]
    fn set_sandbox_mode_in_extensions_preserves_sibling_keys() {
        let yaml = "someOtherTool:\n  foo: bar\nrocketapi:\n  unrelatedFlag: true\n";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse fixture yaml");
        let result = set_sandbox_mode_in_extensions(Some(value), SandboxMode::Developer)
            .expect("extensions value");

        assert_eq!(
            sandbox_mode_from_extensions(&Some(result.clone())),
            SandboxMode::Developer
        );
        let serialized = serde_yaml::to_string(&result).expect("serialize extensions");
        assert!(serialized.contains("someOtherTool"));
        assert!(serialized.contains("foo: bar"));
        assert!(serialized.contains("unrelatedFlag: true"));
    }
}
