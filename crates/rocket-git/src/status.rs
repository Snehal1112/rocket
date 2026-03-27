use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
    Unchanged,
}

impl GitStatus {
    pub fn short_label(&self) -> &'static str {
        match self {
            GitStatus::Modified => "M",
            GitStatus::Added => "A",
            GitStatus::Deleted => "D",
            GitStatus::Renamed => "R",
            GitStatus::Untracked => "?",
            GitStatus::Conflicted => "C",
            GitStatus::Unchanged => "",
        }
    }

    pub fn is_changed(&self) -> bool {
        !matches!(self, GitStatus::Unchanged)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    pub status: GitStatus,
    pub staged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub branch: String,
    pub files: Vec<FileStatus>,
    pub ahead: usize,
    pub behind: usize,
    pub is_clean: bool,
}

impl RepoStatus {
    pub fn changed_count(&self) -> usize {
        self.files.iter().filter(|f| f.status.is_changed()).count()
    }

    pub fn staged_count(&self) -> usize {
        self.files.iter().filter(|f| f.staged && f.status.is_changed()).count()
    }

    pub fn unstaged_count(&self) -> usize {
        self.files.iter().filter(|f| !f.staged && f.status.is_changed()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_status_short_labels() {
        assert_eq!(GitStatus::Modified.short_label(), "M");
        assert_eq!(GitStatus::Added.short_label(), "A");
        assert_eq!(GitStatus::Deleted.short_label(), "D");
        assert_eq!(GitStatus::Untracked.short_label(), "?");
        assert_eq!(GitStatus::Conflicted.short_label(), "C");
        assert_eq!(GitStatus::Renamed.short_label(), "R");
    }

    #[test]
    fn git_status_is_changed() {
        assert!(GitStatus::Modified.is_changed());
        assert!(GitStatus::Added.is_changed());
        assert!(!GitStatus::Unchanged.is_changed());
    }

    #[test]
    fn repo_status_changed_count() {
        let status = RepoStatus {
            branch: "main".into(),
            files: vec![
                FileStatus { path: "a.bru".into(), status: GitStatus::Modified, staged: false },
                FileStatus { path: "b.bru".into(), status: GitStatus::Added, staged: true },
                FileStatus { path: "c.bru".into(), status: GitStatus::Unchanged, staged: false },
            ],
            ahead: 1,
            behind: 0,
            is_clean: false,
        };
        assert_eq!(status.changed_count(), 2);
        assert_eq!(status.staged_count(), 1);
        assert_eq!(status.unstaged_count(), 1);
    }

    #[test]
    fn file_status_serialization_roundtrip() {
        let fs = FileStatus { path: "auth/login.bru".into(), status: GitStatus::Modified, staged: false };
        let json = serde_json::to_string(&fs).unwrap();
        assert!(json.contains("\"status\":\"modified\""));
        let deserialized: FileStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, GitStatus::Modified);
        assert!(!deserialized.staged);
    }
}
