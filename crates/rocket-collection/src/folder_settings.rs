//! Folder-level settings stored in `folder.yml`, and the pure helpers that
//! apply a folder chain to one request.

use rocket_shared::types::{Auth, Header};
use serde::{Deserialize, Serialize};

use crate::settings::CollectionVariable;

/// Settings one folder applies to every request below it.
///
/// This is a domain value object. It has no serde derives, because the on-disk
/// shape belongs to `rocket-infra` and the IPC shape belongs to `src-tauri`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FolderSettings {
    /// Default headers for every request below this folder.
    pub headers: Vec<Header>,
    /// Folder auth. `None` and `Some(Auth::Inherit)` both mean "no folder auth".
    pub auth: Option<Auth>,
    /// Folder variables. They apply before the request runs only.
    pub variables: Vec<CollectionVariable>,
    /// Script of OpenCollection type `before-request`.
    pub pre_request_script: Option<String>,
    /// Script of OpenCollection type `after-response`.
    pub post_response_script: Option<String>,
    /// Script of OpenCollection type `tests`.
    pub tests_script: Option<String>,
    /// Markdown docs content.
    pub docs: Option<String>,
}

/// Order in which collection, folder and request scripts run.
/// Persisted at `extensions.bruno.scripts.flow` in `opencollection.yml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptFlow {
    /// Pre-request runs outer to inner. Post-response and tests run inner to outer.
    #[default]
    Sandwich,
    /// Every phase runs outer to inner.
    Sequential,
}

/// One script phase of a request run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptPhase {
    PreRequest,
    PostResponse,
    Tests,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_settings_default_is_empty() {
        let settings = FolderSettings::default();
        assert!(settings.headers.is_empty());
        assert_eq!(settings.auth, None);
        assert!(settings.variables.is_empty());
        assert_eq!(settings.pre_request_script, None);
        assert_eq!(settings.post_response_script, None);
        assert_eq!(settings.tests_script, None);
        assert_eq!(settings.docs, None);
    }

    #[test]
    fn script_flow_defaults_to_sandwich() {
        assert_eq!(ScriptFlow::default(), ScriptFlow::Sandwich);
    }

    #[test]
    fn script_flow_serializes_lowercase() {
        let json = |f: ScriptFlow| serde_json::to_string(&f).expect("serialize");
        assert_eq!(json(ScriptFlow::Sandwich), "\"sandwich\"");
        assert_eq!(json(ScriptFlow::Sequential), "\"sequential\"");
        let parsed: ScriptFlow = serde_json::from_str("\"sequential\"").expect("deserialize");
        assert_eq!(parsed, ScriptFlow::Sequential);
    }

    #[test]
    fn script_flow_rejects_unknown_value() {
        assert!(serde_json::from_str::<ScriptFlow>("\"bogus\"").is_err());
    }

    #[test]
    fn script_phase_is_copy_and_comparable() {
        let phase = ScriptPhase::Tests;
        let copy = phase;
        assert_eq!(phase, copy);
        assert_ne!(ScriptPhase::PreRequest, ScriptPhase::PostResponse);
    }
}
