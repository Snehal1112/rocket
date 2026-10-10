use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_acp::conversation::{ConversationRecord, ConversationRepository, ConversationSummary};
use rocket_shared::error::DomainResult;

use crate::FsConversationRepo;

/// The assistant conversations of `<active workspace>/assistant`.
///
/// Every call builds a short-lived `FsConversationRepo` from the shared active
/// workspace path, so a workspace switch takes effect on the next call.
/// `pinned()` fixes one workspace, which `ConversationService` does when a
/// session starts, so a save after a switch still lands where the chat began.
pub struct SharedPathConversationRepo {
    active_workspace_path: Arc<Mutex<PathBuf>>,
}

impl SharedPathConversationRepo {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
        }
    }

    /// The repository for the workspace that is active right now.
    fn current(&self) -> FsConversationRepo {
        let dir = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .join("assistant");
        FsConversationRepo::new(dir)
    }
}

impl ConversationRepository for SharedPathConversationRepo {
    fn list(&self) -> DomainResult<Vec<ConversationSummary>> {
        self.current().list()
    }

    fn get(&self, id: &str) -> DomainResult<ConversationRecord> {
        self.current().get(id)
    }

    fn save(&self, record: &ConversationRecord) -> DomainResult<()> {
        self.current().save(record)
    }

    fn delete(&self, id: &str) -> DomainResult<()> {
        self.current().delete(id)
    }

    fn clear(&self) -> DomainResult<()> {
        self.current().clear()
    }

    fn pinned(&self) -> Option<Box<dyn ConversationRepository>> {
        Some(Box::new(self.current()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::conversation::{ConversationMode, StoredMessage, CONVERSATION_FORMAT_VERSION};

    const ID: &str = "0b6f3f4e-0000-4000-8000-000000000001";

    fn record() -> ConversationRecord {
        ConversationRecord {
            version: CONVERSATION_FORMAT_VERSION,
            id: ID.into(),
            title: "t".into(),
            agent_config_id: "a".into(),
            mode: ConversationMode::Ask,
            workspace_id: "default".into(),
            created_at: "2026-10-11T09:00:00Z".into(),
            updated_at: "2026-10-11T09:00:00Z".into(),
            collections: Vec::new(),
            continued_from: None,
            messages: vec![StoredMessage::Agent {
                text: "hello".into(),
            }],
        }
    }

    #[test]
    fn calls_follow_the_active_workspace_and_pinned_stays() {
        let first = tempfile::tempdir().expect("tempdir");
        let second = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(first.path().to_path_buf()));
        let repo = SharedPathConversationRepo::new(Arc::clone(&path));
        let pinned = repo.pinned().expect("a shared repo can pin");

        *path.lock().expect("lock") = second.path().to_path_buf();
        pinned.save(&record()).expect("pinned save");
        let in_first = first
            .path()
            .join("assistant")
            .join("conversations")
            .join(format!("{ID}.yml"));
        assert!(
            in_first.is_file(),
            "the pinned repo writes into the first workspace"
        );
        assert!(
            repo.list().expect("list").is_empty(),
            "the live repo reads the second one"
        );

        repo.save(&record()).expect("live save");
        let in_second = second
            .path()
            .join("assistant")
            .join("conversations")
            .join(format!("{ID}.yml"));
        assert!(in_second.is_file());
    }
}
