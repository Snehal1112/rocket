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

/// Collection headers, then folder headers outermost first, merged into one list.
///
/// A more specific level replaces a header with the same key and keeps its
/// position. Keys match exactly, like `merge_headers` in `rocket-app`.
/// Disabled headers never shadow an outer header and are left out of the result.
/// The request's own headers are merged afterwards by the caller.
pub fn inherited_headers(collection: &[Header], folders: &[FolderSettings]) -> Vec<Header> {
    let levels = std::iter::once(collection).chain(folders.iter().map(|f| f.headers.as_slice()));
    let mut merged: Vec<Header> = Vec::new();
    for level in levels {
        for header in level.iter().filter(|h| h.enabled) {
            match merged.iter_mut().find(|m| m.key == header.key) {
                Some(existing) => *existing = header.clone(),
                None => merged.push(header.clone()),
            }
        }
    }
    merged
}

/// The innermost folder auth that is set and is not `Auth::None` or `Auth::Inherit`.
///
/// `folders` is ordered outermost first. Returns `None` when no folder sets auth,
/// so the caller can fall back to the collection auth.
pub fn resolve_folder_auth(folders: &[FolderSettings]) -> Option<Auth> {
    folders.iter().rev().find_map(|folder| match &folder.auth {
        None | Some(Auth::None) | Some(Auth::Inherit) => None,
        Some(auth) => Some(auth.clone()),
    })
}

/// Scripts for one phase in run order, including the request's own script.
///
/// `folders` is ordered outermost first. Sandwich runs pre-request scripts outer
/// to inner and then the request, and runs post-response and tests scripts from
/// the request out to the outermost folder. Sequential runs every phase outer to
/// inner and then the request. Blank scripts are skipped. Script text is kept as is.
pub fn chain_scripts(
    phase: ScriptPhase,
    flow: ScriptFlow,
    folders: &[FolderSettings],
    request_script: Option<&str>,
) -> Vec<String> {
    let folder_scripts: Vec<String> = folders
        .iter()
        .filter_map(|folder| non_blank(phase_script(folder, phase)))
        .collect();
    let request = non_blank(request_script);
    let request_first = flow == ScriptFlow::Sandwich && phase != ScriptPhase::PreRequest;

    let mut ordered = Vec::with_capacity(folder_scripts.len() + 1);
    if request_first {
        ordered.extend(request);
        ordered.extend(folder_scripts.into_iter().rev());
    } else {
        ordered.extend(folder_scripts);
        ordered.extend(request);
    }
    ordered
}

/// The folder's script for one phase.
fn phase_script(settings: &FolderSettings, phase: ScriptPhase) -> Option<&str> {
    match phase {
        ScriptPhase::PreRequest => settings.pre_request_script.as_deref(),
        ScriptPhase::PostResponse => settings.post_response_script.as_deref(),
        ScriptPhase::Tests => settings.tests_script.as_deref(),
    }
}

/// An owned copy of the script, or `None` when it is missing or only whitespace.
fn non_blank(script: Option<&str>) -> Option<String> {
    script.filter(|s| !s.trim().is_empty()).map(str::to_owned)
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

    fn header(key: &str, value: &str) -> Header {
        Header::new(key, value)
    }

    fn disabled(key: &str, value: &str) -> Header {
        Header {
            enabled: false,
            ..Header::new(key, value)
        }
    }

    fn with_headers(headers: Vec<Header>) -> FolderSettings {
        FolderSettings {
            headers,
            ..Default::default()
        }
    }

    fn with_auth(auth: Option<Auth>) -> FolderSettings {
        FolderSettings {
            auth,
            ..Default::default()
        }
    }

    fn bearer(token: &str) -> Auth {
        Auth::Bearer {
            token: token.to_string(),
        }
    }

    fn with_scripts(name: &str) -> FolderSettings {
        FolderSettings {
            pre_request_script: Some(format!("{name}-pre")),
            post_response_script: Some(format!("{name}-post")),
            tests_script: Some(format!("{name}-tests")),
            ..Default::default()
        }
    }

    fn pairs(headers: &[Header]) -> Vec<(String, String)> {
        headers
            .iter()
            .map(|h| (h.key.clone(), h.value.clone()))
            .collect()
    }

    fn strs(scripts: &[&str]) -> Vec<String> {
        scripts.iter().map(|s| s.to_string()).collect()
    }

    // inherited_headers

    #[test]
    fn inherited_headers_empty_inputs_return_empty() {
        assert!(inherited_headers(&[], &[]).is_empty());
    }

    #[test]
    fn inherited_headers_no_folders_returns_enabled_collection_headers() {
        let collection = vec![header("A", "1"), header("B", "2")];
        assert_eq!(
            pairs(&inherited_headers(&collection, &[])),
            vec![("A".into(), "1".into()), ("B".into(), "2".into())]
        );
    }

    #[test]
    fn inherited_headers_folder_adds_new_key_after_collection() {
        let collection = vec![header("A", "1")];
        let folders = vec![with_headers(vec![header("B", "2")])];
        assert_eq!(
            pairs(&inherited_headers(&collection, &folders)),
            vec![("A".into(), "1".into()), ("B".into(), "2".into())]
        );
    }

    #[test]
    fn inherited_headers_inner_folder_beats_outer_and_collection() {
        let collection = vec![header("X", "collection"), header("Y", "collection")];
        let folders = vec![
            with_headers(vec![header("X", "outer")]),
            with_headers(vec![header("X", "inner")]),
        ];
        // The replaced header keeps the position of the outermost occurrence.
        assert_eq!(
            pairs(&inherited_headers(&collection, &folders)),
            vec![
                ("X".into(), "inner".into()),
                ("Y".into(), "collection".into())
            ]
        );
    }

    #[test]
    fn inherited_headers_disabled_folder_header_does_not_shadow() {
        let collection = vec![header("X", "collection")];
        let folders = vec![
            with_headers(vec![header("X", "outer")]),
            with_headers(vec![disabled("X", "inner")]),
        ];
        assert_eq!(
            pairs(&inherited_headers(&collection, &folders)),
            vec![("X".into(), "outer".into())]
        );
    }

    #[test]
    fn inherited_headers_disabled_collection_header_is_dropped() {
        let collection = vec![disabled("X", "collection"), header("Y", "1")];
        let result = inherited_headers(&collection, &[]);
        assert_eq!(pairs(&result), vec![("Y".into(), "1".into())]);
        assert!(result.iter().all(|h| h.enabled));
    }

    #[test]
    fn inherited_headers_key_match_is_exact_like_merge_headers() {
        let collection = vec![header("X-Token", "a")];
        let folders = vec![with_headers(vec![header("x-token", "b")])];
        assert_eq!(
            pairs(&inherited_headers(&collection, &folders)),
            vec![
                ("X-Token".into(), "a".into()),
                ("x-token".into(), "b".into())
            ]
        );
    }

    // resolve_folder_auth

    #[test]
    fn resolve_folder_auth_empty_list_is_none() {
        assert_eq!(resolve_folder_auth(&[]), None);
    }

    #[test]
    fn resolve_folder_auth_innermost_wins() {
        let folders = vec![
            with_auth(Some(bearer("outer"))),
            with_auth(Some(bearer("inner"))),
        ];
        assert_eq!(resolve_folder_auth(&folders), Some(bearer("inner")));
    }

    #[test]
    fn resolve_folder_auth_skips_inherit_none_and_missing() {
        let folders = vec![
            with_auth(Some(bearer("outer"))),
            with_auth(Some(Auth::Inherit)),
            with_auth(Some(Auth::None)),
            with_auth(None),
        ];
        assert_eq!(resolve_folder_auth(&folders), Some(bearer("outer")));
    }

    #[test]
    fn resolve_folder_auth_all_inherit_or_none_is_none() {
        let folders = vec![
            with_auth(Some(Auth::Inherit)),
            with_auth(Some(Auth::None)),
            with_auth(None),
        ];
        assert_eq!(resolve_folder_auth(&folders), None);
    }

    #[test]
    fn resolve_folder_auth_keeps_any_auth_type() {
        let basic = Auth::Basic {
            username: "u".into(),
            password: "p".into(),
        };
        let folders = vec![with_auth(Some(basic.clone())), with_auth(None)];
        assert_eq!(resolve_folder_auth(&folders), Some(basic));
    }

    // chain_scripts

    fn two_folders() -> Vec<FolderSettings> {
        vec![with_scripts("outer"), with_scripts("inner")]
    }

    #[test]
    fn chain_scripts_sandwich_pre_request_is_outer_inner_request() {
        let out = chain_scripts(
            ScriptPhase::PreRequest,
            ScriptFlow::Sandwich,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["outer-pre", "inner-pre", "req"]));
    }

    #[test]
    fn chain_scripts_sandwich_post_response_is_request_inner_outer() {
        let out = chain_scripts(
            ScriptPhase::PostResponse,
            ScriptFlow::Sandwich,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["req", "inner-post", "outer-post"]));
    }

    #[test]
    fn chain_scripts_sandwich_tests_is_request_inner_outer() {
        let out = chain_scripts(
            ScriptPhase::Tests,
            ScriptFlow::Sandwich,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["req", "inner-tests", "outer-tests"]));
    }

    #[test]
    fn chain_scripts_sequential_pre_request_is_outer_inner_request() {
        let out = chain_scripts(
            ScriptPhase::PreRequest,
            ScriptFlow::Sequential,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["outer-pre", "inner-pre", "req"]));
    }

    #[test]
    fn chain_scripts_sequential_post_response_is_outer_inner_request() {
        let out = chain_scripts(
            ScriptPhase::PostResponse,
            ScriptFlow::Sequential,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["outer-post", "inner-post", "req"]));
    }

    #[test]
    fn chain_scripts_sequential_tests_is_outer_inner_request() {
        let out = chain_scripts(
            ScriptPhase::Tests,
            ScriptFlow::Sequential,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["outer-tests", "inner-tests", "req"]));
    }

    #[test]
    fn chain_scripts_empty_folder_list_returns_request_only_for_every_phase_and_flow() {
        for flow in [ScriptFlow::Sandwich, ScriptFlow::Sequential] {
            for phase in [
                ScriptPhase::PreRequest,
                ScriptPhase::PostResponse,
                ScriptPhase::Tests,
            ] {
                assert_eq!(chain_scripts(phase, flow, &[], Some("req")), strs(&["req"]));
                assert!(chain_scripts(phase, flow, &[], None).is_empty());
            }
        }
    }

    #[test]
    fn chain_scripts_skips_blank_scripts() {
        let folders = vec![
            FolderSettings {
                pre_request_script: Some("   \n\t".into()),
                ..Default::default()
            },
            FolderSettings {
                pre_request_script: Some(String::new()),
                ..Default::default()
            },
            FolderSettings::default(),
            FolderSettings {
                pre_request_script: Some("kept".into()),
                ..Default::default()
            },
        ];
        assert_eq!(
            chain_scripts(
                ScriptPhase::PreRequest,
                ScriptFlow::Sandwich,
                &folders,
                Some("  ")
            ),
            strs(&["kept"])
        );
        assert_eq!(
            chain_scripts(
                ScriptPhase::PreRequest,
                ScriptFlow::Sequential,
                &folders,
                Some("")
            ),
            strs(&["kept"])
        );
    }

    #[test]
    fn chain_scripts_only_uses_the_requested_phase() {
        let folders = vec![FolderSettings {
            pre_request_script: Some("pre".into()),
            ..Default::default()
        }];
        assert!(chain_scripts(ScriptPhase::Tests, ScriptFlow::Sandwich, &folders, None).is_empty());
        assert!(chain_scripts(
            ScriptPhase::PostResponse,
            ScriptFlow::Sequential,
            &folders,
            None
        )
        .is_empty());
    }

    #[test]
    fn chain_scripts_keeps_script_text_unchanged() {
        let folders = vec![FolderSettings {
            tests_script: Some("  test('a');\n".into()),
            ..Default::default()
        }];
        assert_eq!(
            chain_scripts(ScriptPhase::Tests, ScriptFlow::Sandwich, &folders, None),
            strs(&["  test('a');\n"])
        );
    }
}
