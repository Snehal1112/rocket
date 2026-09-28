use std::fs;

use rocket_collection::settings::SandboxMode;
use rocket_collection::{Collection, CollectionSettings, CollectionVariable, ScriptFlow};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::conversions::persisted_oc_auth;
use crate::oc::{OcCollection, OcHttpRequestHeader, OcInfo, OcRequestDefaults, OcVariable};
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

/// Reads Bruno's script order from `extensions.bruno.scripts.flow`. A missing key, an
/// unknown string or a value of the wrong type all mean `Sandwich`, Bruno's default.
fn script_flow_from_extensions(extensions: &Option<serde_yaml::Value>) -> ScriptFlow {
    match extensions
        .as_ref()
        .and_then(|v| v.get("bruno"))
        .and_then(|v| v.get("scripts"))
        .and_then(|v| v.get("flow"))
        .and_then(|v| v.as_str())
    {
        Some("sequential") => ScriptFlow::Sequential,
        _ => ScriptFlow::Sandwich,
    }
}

/// Writes the script order to `extensions.bruno.scripts.flow`, keeping every other key.
/// A value that already reads as `flow` is left as it is, so a save never rewrites it.
/// `Sequential` writes `flow: sequential`. `Sandwich` is the default, so it removes the
/// key and prunes the `scripts` and `bruno` mappings that this leaves empty.
fn set_script_flow_in_extensions(
    extensions: Option<serde_yaml::Value>,
    flow: &ScriptFlow,
) -> Option<serde_yaml::Value> {
    if script_flow_from_extensions(&extensions) == *flow {
        return extensions;
    }
    let mut root = match extensions {
        Some(serde_yaml::Value::Mapping(map)) => map,
        _ => serde_yaml::Mapping::new(),
    };
    let bruno_key = serde_yaml::Value::String("bruno".into());
    let scripts_key = serde_yaml::Value::String("scripts".into());
    let flow_key = serde_yaml::Value::String("flow".into());
    let mut bruno = match root.get(&bruno_key) {
        Some(serde_yaml::Value::Mapping(map)) => map.clone(),
        _ => serde_yaml::Mapping::new(),
    };
    let mut scripts = match bruno.get(&scripts_key) {
        Some(serde_yaml::Value::Mapping(map)) => map.clone(),
        _ => serde_yaml::Mapping::new(),
    };
    match flow {
        ScriptFlow::Sequential => {
            scripts.insert(flow_key, serde_yaml::Value::String("sequential".into()));
        }
        ScriptFlow::Sandwich => {
            scripts.remove(&flow_key);
        }
    }
    if scripts.is_empty() {
        bruno.remove(&scripts_key);
    } else {
        bruno.insert(scripts_key, serde_yaml::Value::Mapping(scripts));
    }
    if bruno.is_empty() {
        root.remove(&bruno_key);
    } else {
        root.insert(bruno_key, serde_yaml::Value::Mapping(bruno));
    }
    if root.is_empty() {
        None
    } else {
        Some(serde_yaml::Value::Mapping(root))
    }
}

/// Reads `agent_autonomy_enabled` out of `opencollection.yml`'s free-form `extensions` field
/// (`extensions.rocketapi.agentAutonomyEnabled`), mirroring `sandbox_mode_from_extensions`
/// above exactly. Missing or non-boolean values default to `false` -- a collection must opt
/// in explicitly to agent write access; it is never autonomous by default.
fn agent_autonomy_enabled_from_extensions(extensions: &Option<serde_yaml::Value>) -> bool {
    extensions
        .as_ref()
        .and_then(|v| v.get("rocketapi"))
        .and_then(|v| v.get("agentAutonomyEnabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Writes `agent_autonomy_enabled` into `extensions.rocketapi.agentAutonomyEnabled`, preserving
/// any other keys already present under `extensions` or under `extensions.rocketapi` -- mirrors
/// `set_sandbox_mode_in_extensions` exactly, so the two settings can be written in either order
/// (or the same call) without clobbering each other or unrelated tooling's data.
fn set_agent_autonomy_enabled_in_extensions(
    extensions: Option<serde_yaml::Value>,
    enabled: bool,
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
    rocketapi.insert(
        serde_yaml::Value::String("agentAutonomyEnabled".into()),
        serde_yaml::Value::Bool(enabled),
    );
    root.insert(rocketapi_key, serde_yaml::Value::Mapping(rocketapi));

    Some(serde_yaml::Value::Mapping(root))
}

pub(super) fn get_settings(
    repo: &FsCollectionRepo,
    name: &str,
) -> DomainResult<CollectionSettings> {
    Collection::validate_name(name)?;
    let path = repo.settings_path(name);
    if !path.exists() {
        return Ok(CollectionSettings::default());
    }
    let content = fs::read_to_string(&path)?;
    let oc: OcCollection = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse opencollection.yml: {e}")))?;

    let sandbox_mode = sandbox_mode_from_extensions(&oc.extensions);
    let script_context_roots = script_roots_from_extensions(&oc.extensions);
    let script_flow = script_flow_from_extensions(&oc.extensions);
    let agent_autonomy_enabled = agent_autonomy_enabled_from_extensions(&oc.extensions);

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
            script_context_roots,
            script_flow,
            agent_autonomy_enabled,
        })
    } else {
        Ok(CollectionSettings {
            docs: oc.docs,
            sandbox_mode,
            script_context_roots,
            script_flow,
            agent_autonomy_enabled,
            ..CollectionSettings::default()
        })
    }
}

pub(super) fn save_settings(
    repo: &FsCollectionRepo,
    name: &str,
    settings: &CollectionSettings,
) -> DomainResult<()> {
    Collection::validate_name(name)?;
    let mutex = repo.collection_mutex(name);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let path = repo.settings_path(name);

    let mut oc: OcCollection = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_yaml::from_str(&content).map_err(|e| {
            DomainError::Internal(format!("Failed to parse opencollection.yml: {e}"))
        })?
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

    // Only headers, auth and variables are edited through CollectionSettings. Start from what
    // the file already has, so scripts, metadata and request settings written by Bruno or by
    // hand are kept. "No auth" is written by omission.
    let auth = settings.auth.clone().and_then(persisted_oc_auth);
    let mut defaults = oc.request.take().unwrap_or_default();
    defaults.headers = if settings.headers.is_empty() {
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
    };
    defaults.auth = auth;
    defaults.variables = if settings.variables.is_empty() {
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
    };
    oc.request = if defaults == OcRequestDefaults::default() {
        None
    } else {
        Some(defaults)
    };
    oc.docs = settings.docs.clone();
    oc.extensions = set_sandbox_mode_in_extensions(oc.extensions.take(), settings.sandbox_mode);
    oc.extensions = set_agent_autonomy_enabled_in_extensions(
        oc.extensions.take(),
        settings.agent_autonomy_enabled,
    );
    oc.extensions =
        set_script_roots_in_extensions(oc.extensions.take(), &settings.script_context_roots);
    oc.extensions = set_script_flow_in_extensions(oc.extensions.take(), &settings.script_flow);

    let yaml = serde_yaml::to_string(&oc).map_err(|e| {
        DomainError::Internal(format!("Failed to serialize opencollection.yml: {e}"))
    })?;
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
        assert_eq!(
            sandbox_mode_from_extensions(&Some(value)),
            SandboxMode::Safe
        );
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

    fn ext(yaml: &str) -> Option<serde_yaml::Value> {
        Some(serde_yaml::from_str(yaml).expect("parse fixture yaml"))
    }

    #[test]
    fn script_flow_from_extensions_defaults_to_sandwich_for_absent_unknown_or_wrong_type() {
        assert_eq!(script_flow_from_extensions(&None), ScriptFlow::Sandwich);
        for yaml in [
            "rocketapi:\n  sandboxMode: safe\n",
            "bruno:\n  scripts:\n    flow: yolo\n",
            "bruno:\n  scripts:\n    flow: 1\n",
            "bruno:\n  scripts:\n    - flow\n",
            "bruno: 7\n",
            "- bruno\n",
        ] {
            assert_eq!(
                script_flow_from_extensions(&ext(yaml)),
                ScriptFlow::Sandwich,
                "{yaml}"
            );
        }
        assert_eq!(
            script_flow_from_extensions(&ext("bruno:\n  scripts:\n    flow: sequential\n")),
            ScriptFlow::Sequential
        );
        assert_eq!(
            script_flow_from_extensions(&ext("bruno:\n  scripts:\n    flow: sandwich\n")),
            ScriptFlow::Sandwich
        );
    }

    #[test]
    fn set_script_flow_sequential_keeps_sibling_keys() {
        let input = ext(
            "rocketapi:\n  sandboxMode: developer\n  keep: me\n  scripts:\n    additionalContextRoots:\n      - ../shared\nbruno:\n  other: 1\n  scripts:\n    keep: true\nsomeOtherTool:\n  foo: bar\n",
        );
        let out = set_script_flow_in_extensions(input, &ScriptFlow::Sequential)
            .expect("extensions value");
        let expected: serde_yaml::Value = serde_yaml::from_str(
            "rocketapi:\n  sandboxMode: developer\n  keep: me\n  scripts:\n    additionalContextRoots:\n      - ../shared\nbruno:\n  other: 1\n  scripts:\n    keep: true\n    flow: sequential\nsomeOtherTool:\n  foo: bar\n",
        )
        .expect("parse expected yaml");
        assert_eq!(out, expected);
        assert_eq!(
            script_flow_from_extensions(&Some(out)),
            ScriptFlow::Sequential
        );
    }

    #[test]
    fn set_script_flow_sequential_on_empty_extensions_creates_only_the_flow_key() {
        let out =
            set_script_flow_in_extensions(None, &ScriptFlow::Sequential).expect("extensions value");
        let expected: serde_yaml::Value =
            serde_yaml::from_str("bruno:\n  scripts:\n    flow: sequential\n")
                .expect("parse expected yaml");
        assert_eq!(out, expected);
    }

    #[test]
    fn set_script_flow_sandwich_removes_only_the_flow_key() {
        let out = set_script_flow_in_extensions(
            ext("bruno:\n  other: 1\n  scripts:\n    flow: sequential\n    keep: true\n"),
            &ScriptFlow::Sandwich,
        )
        .expect("extensions value");
        let expected: serde_yaml::Value =
            serde_yaml::from_str("bruno:\n  other: 1\n  scripts:\n    keep: true\n")
                .expect("parse expected yaml");
        assert_eq!(out, expected);

        // Emptied `scripts` and `bruno` stubs are pruned, siblings stay.
        let out = set_script_flow_in_extensions(
            ext("rocketapi:\n  sandboxMode: safe\nbruno:\n  scripts:\n    flow: sequential\n"),
            &ScriptFlow::Sandwich,
        )
        .expect("extensions value");
        let expected: serde_yaml::Value =
            serde_yaml::from_str("rocketapi:\n  sandboxMode: safe\n").expect("parse expected yaml");
        assert_eq!(out, expected);

        // Nothing left at all gives no `extensions` key.
        assert_eq!(
            set_script_flow_in_extensions(
                ext("bruno:\n  scripts:\n    flow: sequential\n"),
                &ScriptFlow::Sandwich
            ),
            None
        );
    }

    #[test]
    fn set_script_flow_sandwich_leaves_matching_values_untouched() {
        assert_eq!(
            set_script_flow_in_extensions(None, &ScriptFlow::Sandwich),
            None
        );
        for yaml in [
            "rocketapi:\n  sandboxMode: safe\n",
            "bruno:\n  scripts:\n    flow: sandwich\n",
            "bruno:\n  scripts:\n    flow: yolo\n",
            "bruno:\n  scripts:\n    flow: 1\n",
        ] {
            let input = ext(yaml);
            assert_eq!(
                set_script_flow_in_extensions(input.clone(), &ScriptFlow::Sandwich),
                input,
                "{yaml}"
            );
        }
        let sequential = ext("bruno:\n  scripts:\n    flow: sequential\n  other: 1\n");
        assert_eq!(
            set_script_flow_in_extensions(sequential.clone(), &ScriptFlow::Sequential),
            sequential
        );
    }

    #[test]
    fn agent_autonomy_enabled_from_extensions_none_defaults_to_false() {
        assert!(!agent_autonomy_enabled_from_extensions(&None));
    }

    #[test]
    fn agent_autonomy_enabled_from_extensions_reads_existing_rocketapi_mapping_with_other_keys() {
        let yaml = "rocketapi:\n  agentAutonomyEnabled: true\n  someOtherField: 1\nunrelatedTool:\n  foo: bar\n";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse fixture yaml");
        assert!(agent_autonomy_enabled_from_extensions(&Some(value)));
    }

    #[test]
    fn agent_autonomy_enabled_from_extensions_non_bool_value_falls_back_to_false() {
        let yaml = "rocketapi:\n  agentAutonomyEnabled: \"yes\"\n";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse fixture yaml");
        assert!(!agent_autonomy_enabled_from_extensions(&Some(value)));
    }

    #[test]
    fn set_agent_autonomy_enabled_in_extensions_preserves_sibling_keys() {
        let yaml = "someOtherTool:\n  foo: bar\nrocketapi:\n  unrelatedFlag: true\n";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse fixture yaml");
        let result = set_agent_autonomy_enabled_in_extensions(Some(value), true)
            .expect("extensions value");

        assert!(agent_autonomy_enabled_from_extensions(&Some(result.clone())));
        let serialized = serde_yaml::to_string(&result).expect("serialize extensions");
        assert!(serialized.contains("someOtherTool"));
        assert!(serialized.contains("foo: bar"));
        assert!(serialized.contains("unrelatedFlag: true"));
    }

    #[test]
    fn set_sandbox_mode_then_agent_autonomy_enabled_both_persist_together() {
        let extensions = set_sandbox_mode_in_extensions(None, SandboxMode::Developer);
        let extensions = set_agent_autonomy_enabled_in_extensions(extensions, true);
        assert_eq!(
            sandbox_mode_from_extensions(&extensions),
            SandboxMode::Developer
        );
        assert!(agent_autonomy_enabled_from_extensions(&extensions));
    }
}
