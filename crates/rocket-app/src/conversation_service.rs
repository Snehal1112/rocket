//! Stores the workspace assistant's conversations (spec
//! `2026-10-11-assistant-conversation-history-design.md`). Masks secret
//! variable values of the session's workspace, applies the caps, writes into
//! the workspace that was active when the session started and publishes
//! id-only events. No I/O of its own.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use rocket_acp::conversation::{
    conversation_title, has_agent_reply, validate_conversation_id, ConversationCollection,
    ConversationMode, ConversationRecord, ConversationRepository, ConversationSummary,
    StoredMessage, CONVERSATION_FORMAT_VERSION,
};
use rocket_collection::CollectionRepository;
use rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

use crate::conversation_secrets::workspace_secret_values;
use crate::mcp_read_views::mask_secret_text;

/// Sessions whose workspace pin is kept. The oldest pin is dropped first.
pub const MAX_SESSION_PINS: usize = 16;

/// What the panel sends to save. `collections` are names; the record keeps
/// their `opencollection.yml` uid.
#[derive(Debug, Clone, PartialEq)]
pub struct ConversationInput {
    pub id: String,
    pub agent_config_id: String,
    pub mode: ConversationMode,
    pub workspace_id: String,
    pub collections: Vec<String>,
    pub messages: Vec<StoredMessage>,
    pub continued_from: Option<String>,
}

/// The repositories of the workspace a session started in.
struct WorkspacePin {
    conversations: Arc<dyn ConversationRepository>,
    collections: Arc<dyn CollectionRepository>,
    collection_envs: Arc<dyn EnvironmentRepositoryFactory>,
    global_envs: Arc<dyn EnvironmentRepository>,
}

type Clock = Arc<dyn Fn() -> String + Send + Sync>;

pub struct ConversationService {
    conversations: Arc<dyn ConversationRepository>,
    collections: Arc<dyn CollectionRepository>,
    collection_envs: Arc<dyn EnvironmentRepositoryFactory>,
    global_envs: Arc<dyn EnvironmentRepository>,
    events: Arc<dyn EventPublisher>,
    now: Clock,
    /// Workspace pins by ACP session id, oldest first.
    pins: Mutex<VecDeque<(String, Arc<WorkspacePin>)>>,
}

fn utc_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// The `pinned()` repository, or the shared one when it has a fixed location.
fn pin_of<T: ?Sized>(shared: &Arc<T>, pinned: Option<Box<T>>) -> Arc<T> {
    pinned.map(Arc::from).unwrap_or_else(|| Arc::clone(shared))
}

impl ConversationService {
    pub fn new(
        conversations: Arc<dyn ConversationRepository>,
        collections: Arc<dyn CollectionRepository>,
        collection_envs: Arc<dyn EnvironmentRepositoryFactory>,
        global_envs: Arc<dyn EnvironmentRepository>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            conversations,
            collections,
            collection_envs,
            global_envs,
            events,
            now: Arc::new(utc_now),
            pins: Mutex::new(VecDeque::new()),
        }
    }

    /// Replaces the clock, which returns an RFC 3339 UTC time. For tests.
    pub fn with_clock(mut self, now: Clock) -> Self {
        self.now = now;
        self
    }

    /// Fixes the active workspace for a session's saves. Called by
    /// `start_workspace_assistant` right after the handshake.
    pub fn pin_session(&self, session_id: &str) {
        let pin = Arc::new(WorkspacePin {
            conversations: pin_of(&self.conversations, self.conversations.pinned()),
            collections: pin_of(&self.collections, self.collections.pinned()),
            collection_envs: pin_of(&self.collection_envs, self.collection_envs.pinned()),
            global_envs: pin_of(&self.global_envs, self.global_envs.pinned()),
        });
        let mut pins = self.pins.lock().unwrap_or_else(|e| e.into_inner());
        pins.retain(|(id, _)| id != session_id);
        pins.push_back((session_id.to_string(), pin));
        while pins.len() > MAX_SESSION_PINS {
            pins.pop_front();
        }
    }

    fn pin(&self, session_id: &str) -> DomainResult<Arc<WorkspacePin>> {
        self.pins
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|(id, _)| id == session_id)
            .map(|(_, pin)| Arc::clone(pin))
            .ok_or_else(|| {
                DomainError::InvalidInput(
                    "This assistant session is not known, so the conversation was not saved."
                        .to_string(),
                )
            })
    }

    /// The active workspace's conversations, newest first.
    pub fn list(&self) -> DomainResult<Vec<ConversationSummary>> {
        self.conversations.list()
    }

    pub fn get(&self, id: &str) -> DomainResult<ConversationRecord> {
        validate_conversation_id(id)?;
        self.conversations.get(id)
    }

    /// Masks, caps and writes one conversation into the session's pinned
    /// workspace, then publishes `AssistantConversationSaved`.
    pub fn save(
        &self,
        session_id: &str,
        input: ConversationInput,
    ) -> DomainResult<ConversationSummary> {
        validate_conversation_id(&input.id)?;
        if let Some(from) = &input.continued_from {
            validate_conversation_id(from)?;
        }
        if input.agent_config_id.trim().is_empty() {
            return Err(DomainError::InvalidInput(
                "the agent config id is missing".to_string(),
            ));
        }
        if !has_agent_reply(&input.messages) {
            return Err(DomainError::InvalidInput(
                "a conversation is saved only after the assistant replied".to_string(),
            ));
        }
        let pin = self.pin(session_id)?;
        let secrets = workspace_secret_values(
            pin.collections.as_ref(),
            pin.collection_envs.as_ref(),
            pin.global_envs.as_ref(),
        )
        .map_err(|_| {
            DomainError::InvalidInput(
                "could not read the workspace variables to mask this conversation, so it was not saved"
                    .to_string(),
            )
        })?;
        let messages: Vec<StoredMessage> = input
            .messages
            .into_iter()
            .map(|message| message.map_text(|text| mask_secret_text(text, &secrets)))
            .collect();
        let title = conversation_title(&messages);
        let now = (self.now)();
        let created_at = match pin.conversations.get(&input.id) {
            Ok(existing) if !existing.created_at.is_empty() => existing.created_at,
            _ => now.clone(),
        };
        let mut record = ConversationRecord {
            version: CONVERSATION_FORMAT_VERSION,
            id: input.id,
            title,
            agent_config_id: input.agent_config_id,
            mode: input.mode,
            workspace_id: input.workspace_id,
            created_at,
            updated_at: now,
            collections: collection_refs(pin.collections.as_ref(), &input.collections),
            continued_from: input.continued_from,
            messages,
        };
        // Masking ran first, so a cut can never keep part of a secret.
        record.apply_caps();
        pin.conversations.save(&record)?;
        self.events
            .publish(DomainEvent::AssistantConversationSaved {
                conversation_id: record.id.clone(),
            });
        Ok(record.summary())
    }

    pub fn delete(&self, id: &str) -> DomainResult<()> {
        validate_conversation_id(id)?;
        self.conversations.delete(id)?;
        self.events
            .publish(DomainEvent::AssistantConversationDeleted {
                conversation_id: id.to_string(),
            });
        Ok(())
    }

    pub fn clear(&self) -> DomainResult<()> {
        self.conversations.clear()?;
        self.events
            .publish(DomainEvent::AssistantConversationsCleared);
        Ok(())
    }
}

/// Collections by their `opencollection.yml` uid. A name without a uid is left out.
fn collection_refs(
    collections: &dyn CollectionRepository,
    names: &[String],
) -> Vec<ConversationCollection> {
    let mut seen = HashSet::new();
    names
        .iter()
        .filter(|name| seen.insert(name.as_str()))
        .filter_map(|name| {
            let uid = collections.collection_identity(name).ok()?.uid?;
            Some(ConversationCollection {
                uid,
                name: name.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;

    use rocket_acp::conversation::{
        StoredProposalStatus, StoredToolStatus, MAX_MESSAGE_TEXT_BYTES, TEXT_CUT_MARKER,
    };
    use rocket_collection::{
        Collection, CollectionItem, CollectionSettings, CollectionVariable, Folder, FolderSettings,
        Request,
    };
    use rocket_environment::{Environment, Variable};
    use rocket_shared::types::HttpMethod;

    use crate::redaction::REDACTED;
    use crate::test_doubles::{ConfigurableCollectionRepo, RecordingPublisher};

    const COLLECTION_SECRET: &str = "col-secret-111111";
    const INITIAL_SECRET: &str = "col-initial-222222";
    const FOLDER_SECRET: &str = "folder-secret-333333";
    const REQUEST_SECRET: &str = "request-secret-444444";
    const ENV_SECRET: &str = "env-secret-555555";
    const GLOBAL_SECRET: &str = "global-secret-666666";
    const PLAIN_VALUE: &str = "plain-value-777777";
    const ALL_SECRETS: [&str; 6] = [
        COLLECTION_SECRET,
        INITIAL_SECRET,
        FOLDER_SECRET,
        REQUEST_SECRET,
        ENV_SECRET,
        GLOBAL_SECRET,
    ];
    const ID: &str = "0b6f3f4e-0000-4000-8000-000000000001";

    fn var(key: &str, value: &str, initial: &str, secret: bool) -> CollectionVariable {
        CollectionVariable {
            key: key.into(),
            value: value.into(),
            initial_value: initial.into(),
            enabled: true,
            secret,
        }
    }

    struct MapEnvRepo(Vec<Environment>);

    impl EnvironmentRepository for MapEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.0.clone())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.0
                .iter()
                .find(|env| env.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, _: &Environment) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    struct MapEnvFactory(Vec<Environment>);

    impl EnvironmentRepositoryFactory for MapEnvFactory {
        fn for_collection(&self, _: &str) -> Box<dyn EnvironmentRepository> {
            Box::new(MapEnvRepo(self.0.clone()))
        }
    }

    /// A "demo" collection with a secret in the collection, a folder and a
    /// request, a plain variable, and a folder with no folder.yml.
    fn collections() -> Arc<ConfigurableCollectionRepo> {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings(
            "demo",
            CollectionSettings {
                variables: vec![
                    var("token", COLLECTION_SECRET, "", true),
                    var("seed", "", INITIAL_SECRET, true),
                    var("plain", PLAIN_VALUE, "", false),
                ],
                ..Default::default()
            },
        );
        let mut request = Request::new("login", HttpMethod::Get, "https://api.test/login");
        request.variables = vec![var("pw", REQUEST_SECRET, "", true)];
        let mut auth = Folder::new("auth");
        auth.items.push(CollectionItem::Request(Box::new(request)));
        let mut tree = Collection::new("demo");
        tree.root.items.push(CollectionItem::Folder(auth));
        tree.root
            .items
            .push(CollectionItem::Folder(Folder::new("empty")));
        repo.with_summaries("demo", tree);
        repo.with_folder_settings(
            "demo",
            "auth",
            FolderSettings {
                variables: vec![var("fk", FOLDER_SECRET, "", true)],
                ..Default::default()
            },
        );
        repo
    }

    struct Fixture {
        _ws_a: tempfile::TempDir,
        ws_b: tempfile::TempDir,
        ws_a_path: PathBuf,
        active: Arc<StdMutex<PathBuf>>,
        events: Arc<RecordingPublisher>,
        svc: ConversationService,
    }

    fn fixture_with(collections: Arc<ConfigurableCollectionRepo>) -> Fixture {
        let ws_a = tempfile::tempdir().expect("tempdir");
        let ws_b = tempfile::tempdir().expect("tempdir");
        let active = Arc::new(StdMutex::new(ws_a.path().to_path_buf()));
        let events = RecordingPublisher::new();
        let mut global = Environment::new("globals");
        global.variables.push(Variable::secret("g", GLOBAL_SECRET));
        let mut env = Environment::new("dev");
        env.variables.push(Variable::secret("e", ENV_SECRET));
        let svc = ConversationService::new(
            Arc::new(rocket_infra::SharedPathConversationRepo::new(Arc::clone(
                &active,
            ))),
            collections,
            Arc::new(MapEnvFactory(vec![env])),
            Arc::new(MapEnvRepo(vec![global])),
            events.clone(),
        )
        .with_clock(Arc::new(|| "2026-10-11T09:20:41Z".to_string()));
        Fixture {
            ws_a_path: ws_a.path().to_path_buf(),
            _ws_a: ws_a,
            ws_b,
            active,
            events,
            svc,
        }
    }

    fn user(text: &str) -> StoredMessage {
        StoredMessage::User { text: text.into() }
    }

    fn agent(text: &str) -> StoredMessage {
        StoredMessage::Agent { text: text.into() }
    }

    fn input(messages: Vec<StoredMessage>) -> ConversationInput {
        ConversationInput {
            id: ID.into(),
            agent_config_id: "claude-code".into(),
            mode: ConversationMode::Edit,
            workspace_id: "default".into(),
            collections: vec!["demo".into()],
            messages,
            continued_from: None,
        }
    }

    fn stored_file(fx: &Fixture) -> PathBuf {
        fx.ws_a_path
            .join("assistant")
            .join("conversations")
            .join(format!("{ID}.yml"))
    }

    #[test]
    fn save_masks_secret_values_of_every_scope_in_every_text_field() {
        let fx = fixture_with(collections());
        fx.svc.pin_session("s1");
        let all = ALL_SECRETS.join(" ");
        fx.svc
            .save(
                "s1",
                input(vec![
                    user(&format!("use {all}")),
                    agent(&format!("ok {ENV_SECRET} and {PLAIN_VALUE}")),
                    StoredMessage::Tool {
                        title: format!("get_request {REQUEST_SECRET}"),
                        status: StoredToolStatus::Completed,
                    },
                    StoredMessage::Notice {
                        text: GLOBAL_SECRET.into(),
                    },
                    StoredMessage::Proposal {
                        summary: format!("Set token to {COLLECTION_SECRET}"),
                        status: StoredProposalStatus::Pending,
                    },
                ]),
            )
            .expect("save");
        let raw = std::fs::read_to_string(stored_file(&fx)).expect("read file");
        for secret in ALL_SECRETS {
            assert!(!raw.contains(secret), "{secret} reached disk");
        }
        let record = fx.svc.get(ID).expect("get");
        assert!(
            record.title.contains(REDACTED),
            "the title is built from masked text"
        );
        let StoredMessage::Agent { text } = &record.messages[1] else {
            panic!("agent second");
        };
        assert!(text.contains(REDACTED));
        assert!(text.contains(PLAIN_VALUE), "plain variables are not masked");
    }

    #[test]
    fn masking_runs_before_the_text_cap() {
        let fx = fixture_with(collections());
        fx.svc.pin_session("s1");
        // The secret starts just before the cut, so capping first would keep "col-".
        let cut_at = MAX_MESSAGE_TEXT_BYTES - TEXT_CUT_MARKER.len();
        let text = format!(
            "{}{COLLECTION_SECRET}{}",
            "a".repeat(cut_at - 4),
            "b".repeat(5_000)
        );
        fx.svc
            .save("s1", input(vec![user("q"), agent(&text)]))
            .expect("save");
        let record = fx.svc.get(ID).expect("get");
        let StoredMessage::Agent { text } = &record.messages[1] else {
            panic!("agent second");
        };
        assert!(text.ends_with(TEXT_CUT_MARKER));
        assert!(!text.contains("col-"), "no fragment of the secret is left");
    }

    #[test]
    fn save_after_a_workspace_switch_writes_into_the_pinned_workspace() {
        let fx = fixture_with(collections());
        fx.svc.pin_session("s1");
        *fx.active.lock().expect("lock") = fx.ws_b.path().to_path_buf();
        fx.svc
            .save("s1", input(vec![user("q"), agent("a")]))
            .expect("save");
        assert!(stored_file(&fx).is_file());
        assert!(!fx.ws_b.path().join("assistant").exists());
        assert!(
            fx.svc.list().expect("list").is_empty(),
            "list follows the live workspace"
        );
    }

    #[test]
    fn a_save_needs_a_known_session_and_an_agent_reply() {
        let fx = fixture_with(collections());
        let unknown = fx.svc.save("unknown", input(vec![user("q"), agent("a")]));
        assert!(matches!(unknown, Err(DomainError::InvalidInput(_))));
        fx.svc.pin_session("s1");
        let no_reply = fx.svc.save("s1", input(vec![user("q")]));
        assert!(matches!(no_reply, Err(DomainError::InvalidInput(_))));
        let mut bad_id = input(vec![user("q"), agent("a")]);
        bad_id.id = "../x".into();
        assert!(matches!(
            fx.svc.save("s1", bad_id),
            Err(DomainError::InvalidInput(_))
        ));
        let mut bad_from = input(vec![user("q"), agent("a")]);
        bad_from.continued_from = Some("../y".into());
        assert!(matches!(
            fx.svc.save("s1", bad_from),
            Err(DomainError::InvalidInput(_))
        ));
        assert!(
            !fx.ws_a_path.join("assistant").exists(),
            "nothing was written"
        );
        assert!(fx.events.events().is_empty());
    }

    #[test]
    fn an_unreadable_variable_scope_refuses_the_save() {
        // "broken" is listed (it has settings) but its tree cannot be read.
        let repo = collections();
        repo.set_settings("broken", CollectionSettings::default());
        let fx = fixture_with(repo);
        fx.svc.pin_session("s1");
        let err = fx
            .svc
            .save("s1", input(vec![user("q"), agent("a")]))
            .expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(!fx.ws_a_path.join("assistant").exists());
    }

    #[test]
    fn a_resave_keeps_created_at_and_events_carry_ids_only() {
        let fx = fixture_with(collections());
        let tick = Arc::new(AtomicUsize::new(0));
        let svc = fx.svc.with_clock(Arc::new(move || {
            format!("2026-10-11T09:0{}:00Z", tick.fetch_add(1, Ordering::SeqCst))
        }));
        svc.pin_session("s1");
        svc.save("s1", input(vec![user("q"), agent("a")]))
            .expect("first save");
        let summary = svc
            .save(
                "s1",
                input(vec![user("q"), agent("a"), user("q2"), agent("a2")]),
            )
            .expect("second save");
        assert_eq!(summary.created_at, "2026-10-11T09:00:00Z");
        assert_eq!(summary.updated_at, "2026-10-11T09:01:00Z");
        assert_eq!(summary.message_count, 4);
        assert_eq!(summary.mode, ConversationMode::Edit);
        svc.delete(ID).expect("delete");
        svc.clear().expect("clear");
        let events: Vec<String> = fx
            .events
            .events()
            .iter()
            .map(|e| serde_json::to_string(e).expect("json"))
            .collect();
        let saved = format!(r#"{{"type":"assistantConversationSaved","conversation_id":"{ID}"}}"#);
        assert_eq!(
            events,
            vec![
                saved.clone(),
                saved,
                format!(r#"{{"type":"assistantConversationDeleted","conversation_id":"{ID}"}}"#),
                r#"{"type":"assistantConversationsCleared"}"#.to_string(),
            ]
        );
    }

    #[test]
    fn only_the_newest_sixteen_sessions_stay_pinned() {
        let fx = fixture_with(collections());
        for n in 0..=MAX_SESSION_PINS {
            fx.svc.pin_session(&format!("s{n}"));
        }
        let oldest = fx.svc.save("s0", input(vec![user("q"), agent("a")]));
        assert!(matches!(oldest, Err(DomainError::InvalidInput(_))));
        fx.svc
            .save(
                &format!("s{MAX_SESSION_PINS}"),
                input(vec![user("q"), agent("a")]),
            )
            .expect("the newest pin saves");
    }
}
