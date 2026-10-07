//! Converts between the domain `FolderSettings` and the OpenCollection `Folder`
//! shape of `folder.yml`. Only the sections the folder tab edits are touched.

use rocket_collection::{CollectionVariable, FolderSettings};
use rocket_shared::description::Description;
use rocket_shared::types::{Auth, Header};

use crate::oc::{OcFolder, OcHttpRequestHeader, OcRequestDefaults, OcVariable};

use super::auth::persisted_oc_auth;
use super::request::{scripts_from_oc, scripts_to_oc};

/// Script types the folder tab owns. Other entries, like `hooks`, are kept as they are.
const OWNED_SCRIPT_TYPES: [&str; 3] = ["before-request", "after-response", "tests"];

/// Reads the folder tab's sections out of a parsed `folder.yml`.
pub fn oc_folder_to_folder_settings(folder: &OcFolder) -> FolderSettings {
    let defaults = folder.request.clone().unwrap_or_default();
    let (pre_request_script, post_response_script, tests_script) =
        scripts_from_oc(defaults.scripts.as_deref().unwrap_or_default());
    FolderSettings {
        headers: defaults
            .headers
            .unwrap_or_default()
            .into_iter()
            .map(Header::from)
            .collect(),
        auth: defaults.auth.map(Auth::from),
        variables: defaults
            .variables
            .unwrap_or_default()
            .into_iter()
            .map(CollectionVariable::from)
            .collect(),
        pre_request_script,
        post_response_script,
        tests_script,
        docs: folder
            .docs
            .as_ref()
            .and_then(|d| d.content())
            .map(str::to_string),
    }
}

/// Writes the folder tab's sections into `folder`. `info`, `request.metadata`,
/// `request.settings` and script entries the tab does not own are kept. Empty
/// sections are left out, and `request` is dropped when nothing is left in it.
pub fn apply_folder_settings(folder: &mut OcFolder, settings: &FolderSettings) {
    let mut defaults = folder.request.take().unwrap_or_default();
    defaults.headers = non_empty(
        settings
            .headers
            .iter()
            .cloned()
            .map(OcHttpRequestHeader::from)
            .collect(),
    );
    defaults.auth = settings.auth.clone().and_then(persisted_oc_auth);
    defaults.variables = non_empty(
        settings
            .variables
            .iter()
            .cloned()
            .map(OcVariable::from)
            .collect(),
    );
    let mut scripts = scripts_to_oc(
        &non_blank(&settings.pre_request_script),
        &non_blank(&settings.post_response_script),
        &non_blank(&settings.tests_script),
    );
    scripts.extend(
        defaults
            .scripts
            .take()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !OWNED_SCRIPT_TYPES.contains(&s.script_type.as_str())),
    );
    defaults.scripts = non_empty(scripts);
    folder.request = if defaults == OcRequestDefaults::default() {
        None
    } else {
        Some(defaults)
    };
    folder.docs = docs_to_oc(folder.docs.take(), settings.docs.as_deref());
}

fn non_empty<T>(items: Vec<T>) -> Option<Vec<T>> {
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

/// A script with only whitespace counts as no script.
fn non_blank(script: &Option<String>) -> Option<String> {
    script.clone().filter(|code| !code.trim().is_empty())
}

/// Keeps the existing docs, including the object form with a type, when the
/// content did not change. New content is written as a plain string.
fn docs_to_oc(existing: Option<Description>, docs: Option<&str>) -> Option<Description> {
    let docs = docs.filter(|d| !d.trim().is_empty())?;
    match existing {
        Some(current) if current.content() == Some(docs) => Some(current),
        _ => Some(Description::text(docs)),
    }
}

#[cfg(test)]
mod tests {
    use rocket_collection::{CollectionVariable, FolderSettings};
    use rocket_shared::description::Description;
    use rocket_shared::types::{Auth, Header};

    use super::{apply_folder_settings, oc_folder_to_folder_settings};
    use crate::oc::{OcFolder, OcFolderInfo};

    fn bare_folder() -> OcFolder {
        OcFolder {
            info: OcFolderInfo {
                name: "auth".into(),
                uid: Some("f-1".into()),
                ..OcFolderInfo::default()
            },
            items: None,
            request: None,
            docs: None,
        }
    }

    fn parse(yaml: &str) -> OcFolder {
        serde_yaml::from_str(yaml).expect("fixture folder.yml")
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
            docs: Some("# Auth folder".into()),
        }
    }

    #[test]
    fn every_section_round_trips_through_the_oc_folder() {
        let mut folder = bare_folder();
        apply_folder_settings(&mut folder, &full_settings());
        assert_eq!(oc_folder_to_folder_settings(&folder), full_settings());
        assert_eq!(folder.info, bare_folder().info, "info must not change");
        assert!(folder.items.is_none());
    }

    #[test]
    fn inherit_auth_round_trips_and_none_auth_is_left_out() {
        let mut folder = bare_folder();
        let inherit = FolderSettings {
            auth: Some(Auth::Inherit),
            ..FolderSettings::default()
        };
        apply_folder_settings(&mut folder, &inherit);
        assert_eq!(
            oc_folder_to_folder_settings(&folder).auth,
            Some(Auth::Inherit)
        );

        let none = FolderSettings {
            auth: Some(Auth::None),
            ..FolderSettings::default()
        };
        apply_folder_settings(&mut folder, &none);
        assert!(folder.request.is_none(), "{:?}", folder.request);
    }

    #[test]
    fn empty_settings_write_no_request_and_no_docs() {
        let mut folder = parse(
            "info:\n  name: auth\n  type: folder\nrequest:\n  variables:\n  - name: token\n    value: abc\ndocs: old docs\n",
        );
        apply_folder_settings(&mut folder, &FolderSettings::default());
        assert!(folder.request.is_none(), "{:?}", folder.request);
        assert!(folder.docs.is_none(), "{:?}", folder.docs);
    }

    #[test]
    fn blank_scripts_are_left_out() {
        let mut folder = bare_folder();
        let settings = FolderSettings {
            pre_request_script: Some("  \n".into()),
            tests_script: Some(String::new()),
            ..FolderSettings::default()
        };
        apply_folder_settings(&mut folder, &settings);
        assert!(folder.request.is_none(), "{:?}", folder.request);
    }

    #[test]
    fn apply_keeps_metadata_settings_and_hooks() {
        let fixture = "info:\n  name: auth\n  type: folder\nrequest:\n  metadata:\n  - name: x-trace\n    value: '1'\n  settings:\n    timeout: 5000\n  scripts:\n  - type: hooks\n    code: onStart()\n  - type: before-request\n    code: old()\n";
        let before = parse(fixture).request.expect("fixture request");
        let mut folder = parse(fixture);
        apply_folder_settings(&mut folder, &full_settings());
        let after = folder.request.expect("request kept");

        assert_eq!(after.metadata, before.metadata);
        assert_eq!(after.settings, before.settings);
        let scripts = after.scripts.expect("scripts");
        let codes: Vec<(&str, &str)> = scripts
            .iter()
            .map(|s| (s.script_type.as_str(), s.code.as_str()))
            .collect();
        assert_eq!(
            codes,
            vec![
                ("before-request", "console.log('pre');"),
                ("after-response", "console.log('post');"),
                ("tests", "test('ok', () => {});"),
                ("hooks", "onStart()"),
            ]
        );
    }

    #[test]
    fn docs_object_form_is_read_and_kept_when_unchanged() {
        let mut folder = parse(
            "info:\n  name: auth\n  type: folder\ndocs:\n  content: '# Auth'\n  type: text/markdown\n",
        );
        let mut settings = oc_folder_to_folder_settings(&folder);
        assert_eq!(settings.docs.as_deref(), Some("# Auth"));

        apply_folder_settings(&mut folder, &settings);
        assert_eq!(
            folder.docs,
            Some(Description::typed("# Auth", "text/markdown"))
        );

        settings.docs = Some("# Changed".into());
        apply_folder_settings(&mut folder, &settings);
        assert_eq!(folder.docs, Some(Description::text("# Changed")));
    }
}
