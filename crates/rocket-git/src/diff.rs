use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub hunks: Vec<DiffHunk>,
}

impl FileDiff {
    /// Count lines added across all hunks.
    pub fn additions(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| matches!(l.line_type, LineType::Add))
            .count()
    }

    /// Count lines removed across all hunks.
    pub fn deletions(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| matches!(l.line_type, LineType::Remove))
            .count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub content: String,
    pub line_type: LineType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineType {
    Context,
    Add,
    Remove,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_additions_and_deletions() {
        let diff = FileDiff {
            path: "test.bru".into(),
            old_content: Some("old".into()),
            new_content: Some("new".into()),
            hunks: vec![DiffHunk {
                old_start: 1,
                old_lines: 1,
                new_start: 1,
                new_lines: 2,
                lines: vec![
                    DiffLine { content: "- old".into(), line_type: LineType::Remove },
                    DiffLine { content: "+ new".into(), line_type: LineType::Add },
                    DiffLine { content: "+ extra".into(), line_type: LineType::Add },
                ],
            }],
        };
        assert_eq!(diff.additions(), 2);
        assert_eq!(diff.deletions(), 1);
    }

    #[test]
    fn diff_serialization() {
        let line = DiffLine { content: "hello".into(), line_type: LineType::Add };
        let json = serde_json::to_string(&line).unwrap();
        assert!(json.contains("\"lineType\":\"add\""));
    }
}
