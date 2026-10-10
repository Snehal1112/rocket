//! Stored assistant conversations. Pure data and rules: the files live in
//! `rocket-infra` (`FsConversationRepo`), and masking and workspace pins in
//! `rocket-app` (`ConversationService`). The YAML keys are the snake_case
//! field names, because this is a Rocket-private store, not an
//! OpenCollection file.

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

/// The `version` this build writes and reads. Other versions are skipped.
pub const CONVERSATION_FORMAT_VERSION: u32 = 1;
/// Conversations kept per workspace. Saving one more removes the oldest.
pub const MAX_CONVERSATIONS: usize = 50;
/// Messages kept per conversation. The newest ones are kept.
pub const MAX_MESSAGES: usize = 500;
/// Bytes of text kept per message field, marker included.
pub const MAX_MESSAGE_TEXT_BYTES: usize = 16 * 1024;
/// Characters kept for the title.
pub const MAX_TITLE_CHARS: usize = 80;
/// Ends a text that was cut.
pub const TEXT_CUT_MARKER: &str = "\n\n[cut: longer than 16 KB]";

/// The assistant mode when the conversation was saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationMode {
    #[default]
    Ask,
    Edit,
    Agent,
}

/// A tool line's last status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredToolStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// A proposal's status when the conversation was saved. History only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredProposalStatus {
    Pending,
    Accepted,
    Rejected,
    Stale,
    Failed,
}

/// One line of a stored conversation. Tool results, the ACP session id,
/// usage numbers and proposal payloads are never stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoredMessage {
    User {
        text: String,
    },
    Agent {
        text: String,
    },
    Tool {
        title: String,
        status: StoredToolStatus,
    },
    Notice {
        text: String,
    },
    Proposal {
        summary: String,
        status: StoredProposalStatus,
    },
}

impl StoredMessage {
    /// Applies `f` to every free-text field. Used for masking and capping.
    pub fn map_text(self, f: impl Fn(&str) -> String) -> Self {
        match self {
            Self::User { text } => Self::User { text: f(&text) },
            Self::Agent { text } => Self::Agent { text: f(&text) },
            Self::Tool { title, status } => Self::Tool {
                title: f(&title),
                status,
            },
            Self::Notice { text } => Self::Notice { text: f(&text) },
            Self::Proposal { summary, status } => Self::Proposal {
                summary: f(&summary),
                status,
            },
        }
    }
}

/// A collection the chat focused on, by its `opencollection.yml` uid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationCollection {
    pub uid: String,
    pub name: String,
}

/// One stored conversation, the whole file. Every field but `version` and
/// `id` has a default, so an older or hand-edited file still loads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationRecord {
    pub version: u32,
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub agent_config_id: String,
    #[serde(default)]
    pub mode: ConversationMode,
    /// Informational. The file's location decides the workspace.
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub collections: Vec<ConversationCollection>,
    /// The conversation this one continued (Plan 03).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continued_from: Option<String>,
    #[serde(default)]
    pub messages: Vec<StoredMessage>,
}

/// What the History list shows for one conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub agent_config_id: String,
    pub mode: ConversationMode,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub continued_from: Option<String>,
}

impl ConversationRecord {
    pub fn summary(&self) -> ConversationSummary {
        ConversationSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            agent_config_id: self.agent_config_id.clone(),
            mode: self.mode,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            message_count: self.messages.len(),
            continued_from: self.continued_from.clone(),
        }
    }

    /// Keeps the newest `MAX_MESSAGES` messages and cuts every text field to
    /// `MAX_MESSAGE_TEXT_BYTES`. Mask before calling this, so a cut never
    /// keeps part of a secret.
    pub fn apply_caps(&mut self) {
        if self.messages.len() > MAX_MESSAGES {
            let excess = self.messages.len() - MAX_MESSAGES;
            self.messages.drain(..excess);
        }
        let messages = std::mem::take(&mut self.messages);
        self.messages = messages
            .into_iter()
            .map(|message| message.map_text(cap_message_text))
            .collect();
    }
}

/// Refuses anything but a lowercase UUID, so an id is always a safe file name.
pub fn validate_conversation_id(id: &str) -> DomainResult<()> {
    let bytes = id.as_bytes();
    let shaped = bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => *b == b'-',
            _ => b.is_ascii_digit() || (b'a'..=b'f').contains(b),
        });
    if shaped {
        Ok(())
    } else {
        Err(DomainError::InvalidInput(
            "invalid conversation id".to_string(),
        ))
    }
}

/// Cuts `text` to `MAX_MESSAGE_TEXT_BYTES`, marker included, on a char boundary.
pub fn cap_message_text(text: &str) -> String {
    if text.len() <= MAX_MESSAGE_TEXT_BYTES {
        return text.to_string();
    }
    let mut end = MAX_MESSAGE_TEXT_BYTES - TEXT_CUT_MARKER.len();
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{TEXT_CUT_MARKER}", &text[..end])
}

/// The first user message on one line, at most `MAX_TITLE_CHARS` characters.
pub fn conversation_title(messages: &[StoredMessage]) -> String {
    let first = messages
        .iter()
        .find_map(|message| match message {
            StoredMessage::User { text } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("");
    first
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_TITLE_CHARS)
        .collect()
}

/// True when the agent has replied with some text.
pub fn has_agent_reply(messages: &[StoredMessage]) -> bool {
    messages
        .iter()
        .any(|message| matches!(message, StoredMessage::Agent { text } if !text.trim().is_empty()))
}

/// Stored conversations of one workspace. Implemented in `rocket-infra`.
pub trait ConversationRepository: Send + Sync {
    /// Every readable conversation, newest `updated_at` first. Unparseable
    /// and other-version files are skipped with a warning.
    fn list(&self) -> DomainResult<Vec<ConversationSummary>>;
    /// `NotFound` for a missing file, `InvalidInput` for one that cannot be read.
    fn get(&self, id: &str) -> DomainResult<ConversationRecord>;
    /// Writes the whole record, applies the caps and evicts beyond `MAX_CONVERSATIONS`.
    fn save(&self, record: &ConversationRecord) -> DomainResult<()>;
    fn delete(&self, id: &str) -> DomainResult<()>;
    /// Removes every conversation file of the workspace.
    fn clear(&self) -> DomainResult<()>;
    /// A repository fixed to the workspace this one points at now, or `None`
    /// for one with a fixed location.
    fn pinned(&self) -> Option<Box<dyn ConversationRepository>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0b6f3f4e-0000-4000-8000-000000000001";

    fn user(text: &str) -> StoredMessage {
        StoredMessage::User {
            text: text.to_string(),
        }
    }

    fn agent(text: &str) -> StoredMessage {
        StoredMessage::Agent {
            text: text.to_string(),
        }
    }

    fn record(messages: Vec<StoredMessage>) -> ConversationRecord {
        ConversationRecord {
            version: CONVERSATION_FORMAT_VERSION,
            id: ID.to_string(),
            title: String::new(),
            agent_config_id: "claude-code".to_string(),
            mode: ConversationMode::Ask,
            workspace_id: "default".to_string(),
            created_at: "2026-10-11T09:12:00Z".to_string(),
            updated_at: "2026-10-11T09:20:41Z".to_string(),
            collections: Vec::new(),
            continued_from: None,
            messages,
        }
    }

    #[test]
    fn messages_serialize_with_a_kind_tag_and_snake_case_values() {
        let tool = StoredMessage::Tool {
            title: "run_request: login".into(),
            status: StoredToolStatus::InProgress,
        };
        assert_eq!(
            serde_json::to_string(&tool).expect("serialize"),
            r#"{"kind":"tool","title":"run_request: login","status":"in_progress"}"#
        );
        let proposal = StoredMessage::Proposal {
            summary: "Change header X".into(),
            status: StoredProposalStatus::Accepted,
        };
        assert_eq!(
            serde_json::to_string(&proposal).expect("serialize"),
            r#"{"kind":"proposal","summary":"Change header X","status":"accepted"}"#
        );
        assert_eq!(
            serde_json::to_string(&ConversationMode::Agent).expect("serialize"),
            r#""agent""#
        );
    }

    #[test]
    fn a_record_with_unknown_and_missing_keys_still_loads() {
        let json = format!(
            r#"{{"version":1,"id":"{ID}","future_key":true,"messages":[{{"kind":"user","text":"hi"}}]}}"#
        );
        let loaded: ConversationRecord = serde_json::from_str(&json).expect("parse");
        assert_eq!(loaded.mode, ConversationMode::Ask);
        assert!(loaded.collections.is_empty());
        assert_eq!(loaded.continued_from, None);
        assert_eq!(loaded.messages, vec![user("hi")]);
    }

    #[test]
    fn ids_must_be_lowercase_uuids() {
        assert!(validate_conversation_id(ID).is_ok());
        for bad in [
            "",
            "../etc/passwd",
            "0B6F3F4E-0000-4000-8000-000000000001",
            "0b6f3f4e00004000800000000000000001xx",
            "0b6f3f4e-0000-4000-8000-00000000000g",
            "0b6f3f4e-0000-4000-8000-000000000001.yml",
        ] {
            assert!(
                matches!(
                    validate_conversation_id(bad),
                    Err(DomainError::InvalidInput(_))
                ),
                "{bad} must be refused"
            );
        }
    }

    #[test]
    fn long_text_is_cut_on_a_char_boundary_with_the_marker() {
        let text = "é".repeat(MAX_MESSAGE_TEXT_BYTES);
        let cut = cap_message_text(&text);
        assert!(cut.len() <= MAX_MESSAGE_TEXT_BYTES);
        assert!(cut.ends_with(TEXT_CUT_MARKER));
        let exact = "a".repeat(MAX_MESSAGE_TEXT_BYTES);
        assert_eq!(cap_message_text(&exact), exact);
    }

    #[test]
    fn apply_caps_keeps_the_newest_messages_and_cuts_every_text_field() {
        let mut messages: Vec<StoredMessage> = (0..MAX_MESSAGES + 3)
            .map(|i| user(&format!("m{i}")))
            .collect();
        messages.push(StoredMessage::Tool {
            title: "t".repeat(MAX_MESSAGE_TEXT_BYTES + 1),
            status: StoredToolStatus::Completed,
        });
        let mut r = record(messages);
        r.apply_caps();
        assert_eq!(r.messages.len(), MAX_MESSAGES);
        assert_eq!(r.messages[0], user("m4"));
        let StoredMessage::Tool { title, .. } = &r.messages[MAX_MESSAGES - 1] else {
            panic!("the tool line stays last");
        };
        assert!(title.ends_with(TEXT_CUT_MARKER));
    }

    #[test]
    fn the_title_is_the_first_user_message_on_one_line_and_at_most_80_chars() {
        let messages = vec![
            agent("hello"),
            user("  Why does\nlogin   return 401?  "),
            user("second"),
        ];
        assert_eq!(conversation_title(&messages), "Why does login return 401?");
        let long = vec![user(&"x".repeat(200))];
        assert_eq!(conversation_title(&long).chars().count(), MAX_TITLE_CHARS);
        assert_eq!(conversation_title(&[agent("only")]), "");
    }

    #[test]
    fn an_agent_reply_needs_text() {
        assert!(!has_agent_reply(&[user("hi"), agent("  ")]));
        assert!(has_agent_reply(&[user("hi"), agent("Hello")]));
    }

    #[test]
    fn map_text_reaches_every_text_field() {
        let upper = |t: &str| t.to_uppercase();
        assert_eq!(user("a").map_text(upper), user("A"));
        let tool = StoredMessage::Tool {
            title: "t".into(),
            status: StoredToolStatus::Failed,
        };
        assert_eq!(
            tool.map_text(upper),
            StoredMessage::Tool {
                title: "T".into(),
                status: StoredToolStatus::Failed
            }
        );
        let proposal = StoredMessage::Proposal {
            summary: "s".into(),
            status: StoredProposalStatus::Stale,
        };
        assert_eq!(
            proposal.map_text(upper),
            StoredMessage::Proposal {
                summary: "S".into(),
                status: StoredProposalStatus::Stale
            }
        );
    }

    #[test]
    fn the_summary_counts_messages() {
        let r = record(vec![user("a"), agent("b")]);
        let s = r.summary();
        assert_eq!(s.message_count, 2);
        assert_eq!(s.id, r.id);
        assert_eq!(s.updated_at, r.updated_at);
    }

    #[test]
    fn the_repository_trait_is_object_safe() {
        fn _assert(_: Box<dyn ConversationRepository>) {}
    }
}
