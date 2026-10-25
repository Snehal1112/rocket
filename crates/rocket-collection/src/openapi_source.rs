//! Where a collection created by an OpenAPI import came from.

use serde::{Deserialize, Serialize};

/// The place a spec is fetched from. Persisted, so keys are snake_case with no renaming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpecLocation {
    /// A file in a GitHub repository. `git_ref` is a branch, tag or commit.
    Github {
        owner: String,
        repo: String,
        git_ref: String,
        path: String,
    },
    /// Any other `https` URL.
    Https { url: String },
}

impl SpecLocation {
    /// The canonical link for this location, as a user would paste it.
    pub fn display_url(&self) -> String {
        match self {
            SpecLocation::Github {
                owner,
                repo,
                git_ref,
                path,
            } => format!("https://github.com/{owner}/{repo}/blob/{git_ref}/{path}"),
            SpecLocation::Https { url } => url.clone(),
        }
    }

    /// True when the ref is a full commit id, so the file can never change.
    pub fn ref_is_immutable(&self) -> bool {
        match self {
            SpecLocation::Github { git_ref, .. } => {
                git_ref.len() == 40 && git_ref.chars().all(|c| c.is_ascii_hexdigit())
            }
            SpecLocation::Https { .. } => false,
        }
    }
}
