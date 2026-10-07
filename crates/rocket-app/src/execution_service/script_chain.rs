//! The folder script chain of one request.
//!
//! The order rule lives in `rocket_collection::chain_scripts` only. This module
//! pairs each script with the folder it came from, so an error can name it.

use rocket_collection::{chain_scripts, FolderSettings, ScriptFlow, ScriptPhase as ChainPhase};

/// Where a chained script came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScriptSource {
    /// A folder's `folder.yml`, named by its path relative to the collection root.
    Folder(String),
    /// The request's own script.
    Request,
}

/// One script of a phase, in run order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChainedScript {
    pub source: ScriptSource,
    pub code: String,
}

impl ChainedScript {
    /// The error text for this script. A request script keeps the raw message,
    /// so the error text users see today does not change.
    pub(crate) fn attribute(&self, phase: &str, message: &str) -> String {
        match &self.source {
            ScriptSource::Request => message.to_string(),
            ScriptSource::Folder(label) => {
                format!("Folder \"{label}\" {phase} script: {message}")
            }
        }
    }
}

/// The scripts of every phase of one request, in run order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PhaseScripts {
    pub pre_request: Vec<ChainedScript>,
    pub post_response: Vec<ChainedScript>,
    pub tests: Vec<ChainedScript>,
}

impl PhaseScripts {
    /// Builds every phase from the folder chain (outermost first) and the
    /// request's own scripts. `labels[i]` names `folders[i]`.
    pub(crate) fn assemble(
        folders: &[FolderSettings],
        labels: &[String],
        flow: ScriptFlow,
        pre_request: Option<&str>,
        post_response: Option<&str>,
        tests: Option<&str>,
    ) -> Self {
        Self {
            pre_request: phase_scripts(ChainPhase::PreRequest, flow, folders, labels, pre_request),
            post_response: phase_scripts(
                ChainPhase::PostResponse,
                flow,
                folders,
                labels,
                post_response,
            ),
            tests: phase_scripts(ChainPhase::Tests, flow, folders, labels, tests),
        }
    }
}

/// Stands in for the request's script when `chain_scripts` orders the markers.
const REQUEST_MARKER: &str = "request";

/// One phase in `chain_scripts` order.
///
/// `chain_scripts` returns script text only, and two folders may hold the same
/// text. So each folder's script is replaced by its index before ordering, and
/// each returned index is mapped back to its script and folder name.
fn phase_scripts(
    phase: ChainPhase,
    flow: ScriptFlow,
    folders: &[FolderSettings],
    labels: &[String],
    request_script: Option<&str>,
) -> Vec<ChainedScript> {
    let codes: Vec<Option<&str>> = folders
        .iter()
        .map(|folder| folder_script(folder, phase))
        .collect();
    let markers: Vec<FolderSettings> = codes
        .iter()
        .enumerate()
        .map(|(index, code)| marker_folder(phase, code.map(|_| index.to_string())))
        .collect();
    let request_code = request_script.filter(|code| !code.trim().is_empty());

    chain_scripts(phase, flow, &markers, request_code.map(|_| REQUEST_MARKER))
        .into_iter()
        .filter_map(|marker| {
            if marker == REQUEST_MARKER {
                return request_code.map(|code| ChainedScript {
                    source: ScriptSource::Request,
                    code: code.to_string(),
                });
            }
            let index: usize = marker.parse().ok()?;
            let code = codes.get(index).copied().flatten()?;
            let label = labels
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("folder level {}", index + 1));
            Some(ChainedScript {
                source: ScriptSource::Folder(label),
                code: code.to_string(),
            })
        })
        .collect()
}

/// The folder's script for one phase, or `None` when it is missing or blank.
fn folder_script(folder: &FolderSettings, phase: ChainPhase) -> Option<&str> {
    let code = match phase {
        ChainPhase::PreRequest => folder.pre_request_script.as_deref(),
        ChainPhase::PostResponse => folder.post_response_script.as_deref(),
        ChainPhase::Tests => folder.tests_script.as_deref(),
    };
    code.filter(|code| !code.trim().is_empty())
}

/// A folder whose only content is `marker` as its script for `phase`.
fn marker_folder(phase: ChainPhase, marker: Option<String>) -> FolderSettings {
    let mut folder = FolderSettings::default();
    match phase {
        ChainPhase::PreRequest => folder.pre_request_script = marker,
        ChainPhase::PostResponse => folder.post_response_script = marker,
        ChainPhase::Tests => folder.tests_script = marker,
    }
    folder
}

/// Names for the `count` folders above `request_path`, outermost first, such as
/// `api` and `api/users` for `api/users/get.yml`. When the chain length does not
/// match the path, the folders are named by level instead.
pub(crate) fn folder_labels(request_path: &str, count: usize) -> Vec<String> {
    let segments: Vec<&str> = request_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let dirs = &segments[..segments.len().saturating_sub(1)];
    if dirs.len() == count {
        (1..=count).map(|n| dirs[..n].join("/")).collect()
    } else {
        (1..=count).map(|n| format!("folder level {n}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(pre: &str, post: &str, tests: &str) -> FolderSettings {
        FolderSettings {
            pre_request_script: Some(pre.to_string()),
            post_response_script: Some(post.to_string()),
            tests_script: Some(tests.to_string()),
            ..FolderSettings::default()
        }
    }

    fn from_folder(label: &str, code: &str) -> ChainedScript {
        ChainedScript {
            source: ScriptSource::Folder(label.to_string()),
            code: code.to_string(),
        }
    }

    fn from_request(code: &str) -> ChainedScript {
        ChainedScript {
            source: ScriptSource::Request,
            code: code.to_string(),
        }
    }

    fn labels() -> Vec<String> {
        vec!["api".to_string(), "api/users".to_string()]
    }

    fn two_folders() -> Vec<FolderSettings> {
        vec![
            folder("o-pre", "o-post", "o-test"),
            folder("i-pre", "i-post", "i-test"),
        ]
    }

    #[test]
    fn sandwich_wraps_the_request_in_its_folders() {
        let scripts = PhaseScripts::assemble(
            &two_folders(),
            &labels(),
            ScriptFlow::Sandwich,
            Some("r-pre"),
            Some("r-post"),
            Some("r-test"),
        );
        assert_eq!(
            scripts.pre_request,
            vec![
                from_folder("api", "o-pre"),
                from_folder("api/users", "i-pre"),
                from_request("r-pre"),
            ]
        );
        assert_eq!(
            scripts.post_response,
            vec![
                from_request("r-post"),
                from_folder("api/users", "i-post"),
                from_folder("api", "o-post"),
            ]
        );
        assert_eq!(
            scripts.tests,
            vec![
                from_request("r-test"),
                from_folder("api/users", "i-test"),
                from_folder("api", "o-test"),
            ]
        );
    }

    #[test]
    fn sequential_runs_folders_first_in_every_phase() {
        let scripts = PhaseScripts::assemble(
            &two_folders(),
            &labels(),
            ScriptFlow::Sequential,
            Some("r-pre"),
            Some("r-post"),
            Some("r-test"),
        );
        assert_eq!(
            scripts.pre_request,
            vec![
                from_folder("api", "o-pre"),
                from_folder("api/users", "i-pre"),
                from_request("r-pre"),
            ]
        );
        assert_eq!(
            scripts.post_response,
            vec![
                from_folder("api", "o-post"),
                from_folder("api/users", "i-post"),
                from_request("r-post"),
            ]
        );
        assert_eq!(
            scripts.tests,
            vec![
                from_folder("api", "o-test"),
                from_folder("api/users", "i-test"),
                from_request("r-test"),
            ]
        );
    }

    #[test]
    fn blank_and_missing_scripts_are_skipped() {
        let folders = vec![
            FolderSettings::default(),
            FolderSettings {
                pre_request_script: Some("  \n".to_string()),
                tests_script: Some("i-test".to_string()),
                ..FolderSettings::default()
            },
        ];
        let scripts = PhaseScripts::assemble(
            &folders,
            &labels(),
            ScriptFlow::Sandwich,
            Some(" "),
            None,
            Some("r-test"),
        );
        assert!(scripts.pre_request.is_empty());
        assert!(scripts.post_response.is_empty());
        assert_eq!(
            scripts.tests,
            vec![from_request("r-test"), from_folder("api/users", "i-test")]
        );
    }

    #[test]
    fn no_folders_gives_only_the_request_scripts() {
        let scripts =
            PhaseScripts::assemble(&[], &[], ScriptFlow::Sandwich, Some("r-pre"), None, None);
        assert_eq!(scripts.pre_request, vec![from_request("r-pre")]);
        assert!(scripts.post_response.is_empty());
        assert!(scripts.tests.is_empty());
        assert_eq!(
            PhaseScripts::assemble(&[], &[], ScriptFlow::Sequential, None, None, None),
            PhaseScripts::default()
        );
    }

    #[test]
    fn identical_scripts_in_two_folders_keep_their_own_labels() {
        let folders = vec![folder("same", "", ""), folder("same", "", "")];
        let scripts =
            PhaseScripts::assemble(&folders, &labels(), ScriptFlow::Sandwich, None, None, None);
        assert_eq!(
            scripts.pre_request,
            vec![from_folder("api", "same"), from_folder("api/users", "same")]
        );
    }

    #[test]
    fn script_text_is_kept_as_written() {
        let folders = vec![folder("  rok.setVar('a', 1);\n", "", "")];
        let scripts = PhaseScripts::assemble(
            &folders,
            &["api".to_string()],
            ScriptFlow::Sandwich,
            None,
            None,
            None,
        );
        assert_eq!(scripts.pre_request[0].code, "  rok.setVar('a', 1);\n");
    }

    #[test]
    fn folder_labels_name_each_ancestor_folder() {
        assert_eq!(
            folder_labels("api/users/get.yml", 2),
            vec!["api".to_string(), "api/users".to_string()]
        );
        assert!(folder_labels("get.yml", 0).is_empty());
        assert!(folder_labels("", 0).is_empty());
    }

    #[test]
    fn folder_labels_fall_back_when_the_chain_does_not_match_the_path() {
        assert_eq!(
            folder_labels("api/get.yml", 2),
            vec!["folder level 1".to_string(), "folder level 2".to_string()]
        );
    }

    #[test]
    fn a_missing_label_falls_back_to_the_folder_level() {
        let scripts =
            PhaseScripts::assemble(&two_folders(), &[], ScriptFlow::Sandwich, None, None, None);
        assert_eq!(
            scripts.pre_request,
            vec![
                from_folder("folder level 1", "o-pre"),
                from_folder("folder level 2", "i-pre"),
            ]
        );
    }

    #[test]
    fn errors_name_the_folder_and_leave_request_errors_as_they_were() {
        assert_eq!(
            from_request("x").attribute("before-request", "boom"),
            "boom"
        );
        assert_eq!(
            from_folder("api/users", "x").attribute("tests", "boom"),
            "Folder \"api/users\" tests script: boom"
        );
    }
}
