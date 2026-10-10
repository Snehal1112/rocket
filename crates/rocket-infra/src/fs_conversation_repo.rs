//! Assistant conversations of one workspace, one YAML file per conversation
//! at `<workspace>/assistant/conversations/<id>.yml`. A Rocket-private store
//! next to `collections/`, so no OpenCollection file is read or written.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use rocket_acp::conversation::{
    validate_conversation_id, ConversationRecord, ConversationRepository, ConversationSummary,
    CONVERSATION_FORMAT_VERSION, MAX_CONVERSATIONS,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::yaml_io::delete_if_exists;

pub struct FsConversationRepo {
    assistant_dir: PathBuf,
}

impl FsConversationRepo {
    /// `assistant_dir` is `<workspace>/assistant`. Nothing is created until a save.
    pub fn new(assistant_dir: PathBuf) -> Self {
        Self { assistant_dir }
    }

    fn conversations_dir(&self) -> PathBuf {
        self.assistant_dir.join("conversations")
    }

    /// The file of a valid id. Any other id is refused, so no path can escape.
    fn file_path(&self, id: &str) -> DomainResult<PathBuf> {
        validate_conversation_id(id)?;
        Ok(self.conversations_dir().join(format!("{id}.yml")))
    }

    /// Keeps transcripts out of git when the workspace is a repository.
    /// Creates `<workspace>/assistant/.gitignore` only if it is missing and
    /// never fails the save, like `FsCookieRepo::ensure_gitignore`.
    fn ensure_gitignore(&self) {
        let result = fs::create_dir_all(&self.assistant_dir).and_then(|()| {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.assistant_dir.join(".gitignore"))
            {
                // An empty file left by a failed write is kept: never overwrite.
                Ok(mut file) => file.write_all(b"*"),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
                Err(e) => Err(e),
            }
        });
        if let Err(e) = result {
            tracing::warn!("Could not write the assistant folder .gitignore: {e}");
        }
    }

    /// A regular file named `<valid id>.yml`. Symlinks and directories are not.
    fn is_conversation_file(path: &Path) -> bool {
        let regular = fs::symlink_metadata(path).is_ok_and(|m| m.is_file());
        regular
            && path.extension().is_some_and(|e| e == "yml")
            && path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| validate_conversation_id(s).is_ok())
    }

    /// Reads one file. The error text never quotes file content.
    fn parse(path: &Path) -> DomainResult<ConversationRecord> {
        let content = fs::read_to_string(path)?;
        serde_yaml::from_str(&content).map_err(|_| {
            DomainError::InvalidInput("the conversation file could not be read".to_string())
        })
    }

    /// Every readable file of this version whose `id` matches its name.
    /// Others are skipped with a warning.
    fn read_all(&self) -> DomainResult<Vec<(PathBuf, ConversationRecord)>> {
        let dir = self.conversations_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file())
                || path.extension().is_none_or(|e| e != "yml")
            {
                continue;
            }
            match Self::parse(&path) {
                Ok(record)
                    if record.version == CONVERSATION_FORMAT_VERSION
                        && path.file_stem().and_then(|s| s.to_str())
                            == Some(record.id.as_str()) =>
                {
                    out.push((path, record));
                }
                Ok(_) => {
                    tracing::warn!(path = %path.display(), "skipping a conversation file of another version or id");
                }
                Err(_) => {
                    tracing::warn!(path = %path.display(), "skipping a conversation file that could not be read");
                }
            }
        }
        Ok(out)
    }

    /// Removes the oldest conversations beyond `MAX_CONVERSATIONS`, never `keep_id`.
    fn evict_beyond_cap(&self, keep_id: &str) -> DomainResult<()> {
        let mut all = self.read_all()?;
        if all.len() <= MAX_CONVERSATIONS {
            return Ok(());
        }
        all.sort_by(|a, b| {
            a.1.updated_at
                .cmp(&b.1.updated_at)
                .then_with(|| a.1.id.cmp(&b.1.id))
        });
        let excess = all.len() - MAX_CONVERSATIONS;
        for (path, _) in all
            .into_iter()
            .filter(|(_, r)| r.id != keep_id)
            .take(excess)
        {
            fs::remove_file(&path)?;
        }
        Ok(())
    }
}

impl ConversationRepository for FsConversationRepo {
    fn list(&self) -> DomainResult<Vec<ConversationSummary>> {
        let mut summaries: Vec<ConversationSummary> =
            self.read_all()?.iter().map(|(_, r)| r.summary()).collect();
        summaries.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(summaries)
    }

    fn get(&self, id: &str) -> DomainResult<ConversationRecord> {
        let path = self.file_path(id)?;
        if !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
            return Err(DomainError::NotFound(format!("conversation '{id}'")));
        }
        let record = Self::parse(&path)?;
        if record.version != CONVERSATION_FORMAT_VERSION {
            return Err(DomainError::InvalidInput(
                "this conversation was saved by another version of Rocket".to_string(),
            ));
        }
        Ok(record)
    }

    fn save(&self, record: &ConversationRecord) -> DomainResult<()> {
        let path = self.file_path(&record.id)?;
        let mut capped = record.clone();
        capped.version = CONVERSATION_FORMAT_VERSION;
        capped.apply_caps();
        let yaml = serde_yaml::to_string(&capped).map_err(|e| {
            DomainError::Internal(format!("Failed to serialize the conversation: {e}"))
        })?;
        self.ensure_gitignore();
        atomic_write(&path, yaml.as_bytes())?;
        // The conversation is saved. A failed eviction only leaves one file too many.
        if let Err(e) = self.evict_beyond_cap(&record.id) {
            tracing::warn!("Could not remove old conversations: {e}");
        }
        Ok(())
    }

    fn delete(&self, id: &str) -> DomainResult<()> {
        let path = self.file_path(id)?;
        delete_if_exists(&path, &format!("conversation '{id}'"))
    }

    fn clear(&self) -> DomainResult<()> {
        let dir = self.conversations_dir();
        if !dir.exists() {
            return Ok(());
        }
        // Only conversation files go: foreign files and directories stay.
        let mut first_error: Option<DomainError> = None;
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if !Self::is_conversation_file(&path) {
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!(path = %path.display(), "could not remove a conversation file: {e}");
                    first_error.get_or_insert(e.into());
                }
            }
        }
        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::conversation::{
        ConversationMode, StoredMessage, MAX_MESSAGES, MAX_MESSAGE_TEXT_BYTES, TEXT_CUT_MARKER,
    };
    use tempfile::TempDir;

    fn id(n: u32) -> String {
        format!("00000000-0000-4000-8000-{n:012x}")
    }

    fn record(n: u32, updated_at: &str) -> ConversationRecord {
        ConversationRecord {
            version: CONVERSATION_FORMAT_VERSION,
            id: id(n),
            title: format!("chat {n}"),
            agent_config_id: "claude-code".into(),
            mode: ConversationMode::Ask,
            workspace_id: "default".into(),
            created_at: updated_at.into(),
            updated_at: updated_at.into(),
            collections: Vec::new(),
            continued_from: None,
            messages: vec![
                StoredMessage::User { text: "hi".into() },
                StoredMessage::Agent {
                    text: "hello".into(),
                },
            ],
        }
    }

    fn setup() -> (TempDir, PathBuf, FsConversationRepo) {
        let dir = TempDir::new().expect("tempdir");
        let assistant = dir.path().join("assistant");
        let repo = FsConversationRepo::new(assistant.clone());
        (dir, assistant, repo)
    }

    fn file(assistant: &Path, n: u32) -> PathBuf {
        assistant
            .join("conversations")
            .join(format!("{}.yml", id(n)))
    }

    fn ids(repo: &FsConversationRepo) -> Vec<String> {
        repo.list()
            .expect("list")
            .into_iter()
            .map(|s| s.id)
            .collect()
    }

    #[test]
    fn list_of_a_missing_directory_is_empty() {
        let (_dir, assistant, repo) = setup();
        assert!(repo.list().expect("list").is_empty());
        assert!(!assistant.exists(), "reading creates nothing");
    }

    #[test]
    fn save_get_list_round_trip() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z"))
            .expect("save 1");
        repo.save(&record(2, "2026-10-11T10:00:00Z"))
            .expect("save 2");
        assert_eq!(
            repo.get(&id(1)).expect("get"),
            record(1, "2026-10-11T09:00:00Z")
        );
        assert_eq!(ids(&repo), vec![id(2), id(1)], "newest first");
        assert!(file(&assistant, 1).is_file(), "the file name is the id");
    }

    #[test]
    fn the_file_uses_snake_case_keys_only() {
        let (_dir, assistant, repo) = setup();
        let mut r = record(1, "2026-10-11T09:00:00Z");
        r.continued_from = Some(id(9));
        repo.save(&r).expect("save");
        let raw = fs::read_to_string(file(&assistant, 1)).expect("read");
        let parsed: serde_yaml::Mapping = serde_yaml::from_str(&raw).expect("parse");
        let mut keys: Vec<String> = parsed
            .keys()
            .map(|k| k.as_str().expect("string key").to_string())
            .collect();
        keys.sort();
        let mut expected = vec![
            "version",
            "id",
            "title",
            "agent_config_id",
            "mode",
            "workspace_id",
            "created_at",
            "updated_at",
            "collections",
            "continued_from",
            "messages",
        ];
        expected.sort();
        assert_eq!(
            keys, expected,
            "a new persisted key must be reviewed: snake_case, no secrets"
        );
    }

    #[test]
    fn save_writes_gitignore_with_star_once() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z"))
            .expect("save 1");
        repo.save(&record(2, "2026-10-11T09:01:00Z"))
            .expect("save 2");
        let content = fs::read_to_string(assistant.join(".gitignore")).expect("gitignore");
        assert_eq!(content, "*");
    }

    #[test]
    fn save_leaves_a_user_edited_gitignore_untouched() {
        let (_dir, assistant, repo) = setup();
        fs::create_dir_all(&assistant).expect("mkdir");
        fs::write(assistant.join(".gitignore"), "custom\n").expect("write");
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save");
        let content = fs::read_to_string(assistant.join(".gitignore")).expect("gitignore");
        assert_eq!(content, "custom\n");
    }

    #[cfg(unix)]
    #[test]
    fn save_succeeds_when_gitignore_cannot_be_written() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, assistant, repo) = setup();
        fs::create_dir_all(assistant.join("conversations")).expect("mkdir");
        // A read-only folder refuses the new .gitignore but not the conversations inside.
        fs::set_permissions(&assistant, fs::Permissions::from_mode(0o555)).expect("chmod");
        let blocked = fs::File::create(assistant.join("probe")).is_err();
        let saved = repo.save(&record(1, "2026-10-11T09:00:00Z"));
        let has_gitignore = assistant.join(".gitignore").exists();
        fs::set_permissions(&assistant, fs::Permissions::from_mode(0o755)).expect("chmod back");
        if !blocked {
            return; // Running with rights that ignore permissions, such as root.
        }
        assert!(!has_gitignore, "the failure path was exercised");
        saved.expect("save still succeeds");
        assert!(repo.get(&id(1)).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_yml_files_are_ignored() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save");
        std::os::unix::fs::symlink(file(&assistant, 1), file(&assistant, 2)).expect("symlink");
        assert_eq!(ids(&repo), vec![id(1)]);
        assert!(matches!(repo.get(&id(2)), Err(DomainError::NotFound(_))));
        repo.clear().expect("clear");
        assert!(
            fs::symlink_metadata(file(&assistant, 2)).is_ok(),
            "clear leaves the link"
        );
    }

    #[test]
    fn the_51st_save_evicts_the_oldest() {
        let (_dir, _assistant, repo) = setup();
        for n in 0..MAX_CONVERSATIONS as u32 {
            repo.save(&record(n + 1, &format!("2026-10-11T09:{n:02}:00Z")))
                .expect("save");
        }
        repo.save(&record(100, "2026-10-12T00:00:00Z"))
            .expect("save 51st");
        let ids = ids(&repo);
        assert_eq!(ids.len(), MAX_CONVERSATIONS);
        assert!(!ids.contains(&id(1)), "the oldest is gone");
        assert!(ids.contains(&id(100)));
    }

    #[test]
    fn eviction_never_removes_the_conversation_just_saved() {
        let (_dir, _assistant, repo) = setup();
        for n in 0..MAX_CONVERSATIONS as u32 {
            repo.save(&record(n + 1, &format!("2026-10-12T09:{n:02}:00Z")))
                .expect("save");
        }
        // Older than every other one, for example a clock that went back.
        repo.save(&record(100, "2026-10-01T00:00:00Z"))
            .expect("save");
        let ids = ids(&repo);
        assert_eq!(ids.len(), MAX_CONVERSATIONS);
        assert!(ids.contains(&id(100)));
        assert!(!ids.contains(&id(1)), "the next oldest went instead");
    }

    #[test]
    fn save_caps_messages_and_text() {
        let (_dir, _assistant, repo) = setup();
        let mut r = record(1, "2026-10-11T09:00:00Z");
        r.messages = (0..MAX_MESSAGES + 1)
            .map(|i| StoredMessage::User {
                text: format!("m{i}"),
            })
            .collect();
        r.messages.push(StoredMessage::Agent {
            text: "a".repeat(MAX_MESSAGE_TEXT_BYTES * 2),
        });
        repo.save(&r).expect("save");
        let loaded = repo.get(&id(1)).expect("get");
        assert_eq!(loaded.messages.len(), MAX_MESSAGES);
        let Some(StoredMessage::Agent { text }) = loaded.messages.last() else {
            panic!("the agent message stays last");
        };
        assert!(text.len() <= MAX_MESSAGE_TEXT_BYTES);
        assert!(text.ends_with(TEXT_CUT_MARKER));
    }

    #[test]
    fn list_skips_corrupt_and_other_version_files_and_get_reports_them() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save");
        fs::write(file(&assistant, 2), "version: [not yaml").expect("write corrupt");
        let mut future = record(3, "2026-10-11T09:00:00Z");
        future.version = 2;
        fs::write(
            file(&assistant, 3),
            serde_yaml::to_string(&future).expect("yaml"),
        )
        .expect("write v2");
        assert_eq!(ids(&repo), vec![id(1)]);
        assert!(matches!(
            repo.get(&id(2)),
            Err(DomainError::InvalidInput(_))
        ));
        assert!(matches!(
            repo.get(&id(3)),
            Err(DomainError::InvalidInput(_))
        ));
        assert!(matches!(repo.get(&id(4)), Err(DomainError::NotFound(_))));
    }

    #[test]
    fn a_file_whose_id_differs_from_its_name_is_skipped() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save");
        fs::copy(file(&assistant, 1), file(&assistant, 5)).expect("copy");
        assert_eq!(ids(&repo), vec![id(1)]);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save");
        let raw = fs::read_to_string(file(&assistant, 1)).expect("read");
        fs::write(file(&assistant, 1), format!("{raw}future_key: 1\n")).expect("write");
        assert_eq!(repo.get(&id(1)).expect("get").title, "chat 1");
    }

    #[test]
    fn an_invalid_id_is_refused_without_writing() {
        let (_dir, assistant, repo) = setup();
        let mut r = record(1, "2026-10-11T09:00:00Z");
        r.id = "../escape".into();
        assert!(matches!(repo.save(&r), Err(DomainError::InvalidInput(_))));
        assert!(!assistant.exists(), "nothing was written");
        assert!(matches!(
            repo.get("../escape"),
            Err(DomainError::InvalidInput(_))
        ));
        assert!(matches!(
            repo.delete("../escape"),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[test]
    fn delete_and_clear() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z"))
            .expect("save 1");
        repo.save(&record(2, "2026-10-11T09:01:00Z"))
            .expect("save 2");
        repo.delete(&id(1)).expect("delete");
        assert_eq!(ids(&repo), vec![id(2)]);
        assert!(matches!(repo.delete(&id(1)), Err(DomainError::NotFound(_))));
        fs::write(file(&assistant, 3), "version: [not yaml").expect("write corrupt");
        let conversations = assistant.join("conversations");
        fs::write(conversations.join("notes.yml"), "keep").expect("foreign");
        fs::create_dir(conversations.join("x.yml")).expect("dir");
        repo.clear().expect("clear");
        assert!(
            conversations.join("notes.yml").is_file(),
            "foreign file stays"
        );
        assert!(conversations.join("x.yml").is_dir(), "directory stays");
        assert!(repo.list().expect("list").is_empty());
        assert!(
            !file(&assistant, 3).exists(),
            "clear removes unreadable files too"
        );
        assert!(
            assistant.join(".gitignore").is_file(),
            "clear keeps the .gitignore"
        );
    }

    #[test]
    fn stored_message_kinds_round_trip_through_the_file() {
        let (_dir, assistant, repo) = setup();
        let r = record(1, "2026-10-11T09:00:00Z");
        repo.save(&r).expect("save");
        let raw = fs::read_to_string(file(&assistant, 1)).expect("read");
        assert!(
            raw.contains("kind: user"),
            "the enum is tagged by kind: {raw}"
        );
        assert!(
            raw.contains("kind: agent"),
            "the enum is tagged by kind: {raw}"
        );
        assert_eq!(repo.get(&id(1)).expect("get").messages, r.messages);
    }

    #[test]
    fn a_file_with_an_unknown_message_kind_is_skipped_by_list() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z"))
            .expect("save 1");
        repo.save(&record(2, "2026-10-11T09:01:00Z"))
            .expect("save 2");
        let raw = fs::read_to_string(file(&assistant, 2)).expect("read");
        let edited = raw.replacen("kind: agent", "kind: from_the_future", 1);
        assert_ne!(raw, edited, "the edit applied");
        fs::write(file(&assistant, 2), edited).expect("write");
        assert_eq!(ids(&repo), vec![id(1)]);
        assert!(matches!(
            repo.get(&id(2)),
            Err(DomainError::InvalidInput(_))
        ));
    }
}
