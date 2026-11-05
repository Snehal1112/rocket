//! Events the git context publishes.

use rocket_shared::published_event::PublishedEvent;
use serde::Serialize;

/// A git operation changed a repository. All of them use the `git-changed`
/// channel. Fields stay snake_case on the wire, as before the split.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(serde::Deserialize))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GitEvent {
    GitStatusChanged {
        collection: String,
    },
    GitCommit {
        collection: String,
        message: String,
        sha: String,
    },
    GitPush {
        collection: String,
        remote: String,
    },
    GitPull {
        collection: String,
        remote: String,
    },
    BranchSwitched {
        collection: String,
        branch: String,
    },
    BranchMerged {
        collection: String,
        branch: String,
    },
    GitStashChanged {
        collection: String,
    },
    GitConflictDetected {
        collection: String,
        files: Vec<String>,
    },
    GitCloned {
        url: String,
        dest: String,
    },
    GitRemoteAdded {
        collection: String,
        name: String,
        url: String,
    },
    GitRemoteRemoved {
        collection: String,
        name: String,
    },
}

impl PublishedEvent for GitEvent {
    fn channel(&self) -> &'static str {
        "git-changed"
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// A branch switch or merge replaces the working-tree content the
    /// collection sidebar shows, so it also fires `collection-changed`.
    fn also_emit_on(&self) -> &'static [&'static str] {
        match self {
            GitEvent::BranchSwitched { .. } | GitEvent::BranchMerged { .. } => {
                &["collection-changed"]
            }
            _ => &[],
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::published_event::PublishedEvent;
    use serde_json::json;

    fn s(v: &str) -> String {
        v.to_string()
    }

    fn cases() -> Vec<(GitEvent, serde_json::Value)> {
        vec![
            (
                GitEvent::GitStatusChanged { collection: s("c") },
                json!({"type":"gitStatusChanged","collection":"c"}),
            ),
            (
                GitEvent::GitCommit {
                    collection: s("c"),
                    message: s("m"),
                    sha: s("abc123"),
                },
                json!({"type":"gitCommit","collection":"c","message":"m","sha":"abc123"}),
            ),
            (
                GitEvent::GitPush {
                    collection: s("c"),
                    remote: s("origin"),
                },
                json!({"type":"gitPush","collection":"c","remote":"origin"}),
            ),
            (
                GitEvent::GitPull {
                    collection: s("c"),
                    remote: s("origin"),
                },
                json!({"type":"gitPull","collection":"c","remote":"origin"}),
            ),
            (
                GitEvent::BranchSwitched {
                    collection: s("c"),
                    branch: s("dev"),
                },
                json!({"type":"branchSwitched","collection":"c","branch":"dev"}),
            ),
            (
                GitEvent::BranchMerged {
                    collection: s("c"),
                    branch: s("dev"),
                },
                json!({"type":"branchMerged","collection":"c","branch":"dev"}),
            ),
            (
                GitEvent::GitStashChanged { collection: s("c") },
                json!({"type":"gitStashChanged","collection":"c"}),
            ),
            (
                GitEvent::GitConflictDetected {
                    collection: s("c"),
                    files: vec![s("a.yml"), s("b.yml")],
                },
                json!({"type":"gitConflictDetected","collection":"c","files":["a.yml","b.yml"]}),
            ),
            (
                GitEvent::GitCloned {
                    url: s("https://h/r.git"),
                    dest: s("/d"),
                },
                json!({"type":"gitCloned","url":"https://h/r.git","dest":"/d"}),
            ),
            (
                GitEvent::GitRemoteAdded {
                    collection: s("c"),
                    name: s("origin"),
                    url: s("https://h/r.git"),
                },
                json!({"type":"gitRemoteAdded","collection":"c","name":"origin","url":"https://h/r.git"}),
            ),
            (
                GitEvent::GitRemoteRemoved {
                    collection: s("c"),
                    name: s("origin"),
                },
                json!({"type":"gitRemoteRemoved","collection":"c","name":"origin"}),
            ),
        ]
    }

    #[test]
    fn git_events_json_are_unchanged() {
        for (event, want) in cases() {
            assert_eq!(event.to_json().expect("serializes"), want);
        }
    }

    #[test]
    fn git_events_use_one_channel() {
        for (event, _) in cases() {
            assert_eq!(event.channel(), "git-changed");
        }
    }

    #[test]
    fn only_branch_events_also_emit_on_collection_changed() {
        for (event, _) in cases() {
            let want: &[&str] = match event {
                GitEvent::BranchSwitched { .. } | GitEvent::BranchMerged { .. } => {
                    &["collection-changed"]
                }
                _ => &[],
            };
            assert_eq!(event.also_emit_on(), want);
        }
    }

    #[test]
    fn git_events_have_no_written_collections() {
        for (event, _) in cases() {
            assert!(event.written_collections().is_empty());
        }
    }

    #[test]
    fn wire_contract_keys_are_snake_case_with_a_camel_tag() {
        let json = GitEvent::GitCommit {
            collection: s("c"),
            message: s("m"),
            sha: s("x"),
        }
        .to_json()
        .expect("serializes");
        let mut keys: Vec<&str> = json
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["collection", "message", "sha", "type"]);
    }
}
