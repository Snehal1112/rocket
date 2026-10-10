//! IPC commands for stored assistant conversations. Thin: each calls
//! `ConversationService` off the main thread and maps to camelCase DTOs.
//! `StoredMessage` is passed through as is: its tag values and field names
//! are single lowercase words and its statuses snake_case, the same on disk
//! and on the wire (pinned by a test below).

use std::sync::Arc;

use rocket_acp::conversation::{
    ConversationMode, ConversationRecord, ConversationSummary, StoredMessage,
};
use rocket_app::{ConversationInput, ConversationService};
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummaryDto {
    pub id: String,
    pub title: String,
    pub agent_config_id: String,
    pub mode: ConversationMode,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continued_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationCollectionDto {
    pub uid: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecordDto {
    pub id: String,
    pub title: String,
    pub agent_config_id: String,
    pub mode: ConversationMode,
    pub workspace_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub collections: Vec<ConversationCollectionDto>,
    pub messages: Vec<StoredMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continued_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationInputDto {
    pub id: String,
    pub agent_config_id: String,
    pub mode: ConversationMode,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub collections: Vec<String>,
    pub messages: Vec<StoredMessage>,
    #[serde(default)]
    pub continued_from: Option<String>,
}

impl From<ConversationSummary> for ConversationSummaryDto {
    fn from(s: ConversationSummary) -> Self {
        Self {
            id: s.id,
            title: s.title,
            agent_config_id: s.agent_config_id,
            mode: s.mode,
            created_at: s.created_at,
            updated_at: s.updated_at,
            message_count: s.message_count,
            continued_from: s.continued_from,
        }
    }
}

impl From<ConversationRecord> for ConversationRecordDto {
    fn from(r: ConversationRecord) -> Self {
        Self {
            id: r.id,
            title: r.title,
            agent_config_id: r.agent_config_id,
            mode: r.mode,
            workspace_id: r.workspace_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
            collections: r
                .collections
                .into_iter()
                .map(|c| ConversationCollectionDto {
                    uid: c.uid,
                    name: c.name,
                })
                .collect(),
            messages: r.messages,
            continued_from: r.continued_from,
        }
    }
}

impl From<ConversationInputDto> for ConversationInput {
    fn from(d: ConversationInputDto) -> Self {
        Self {
            id: d.id,
            agent_config_id: d.agent_config_id,
            mode: d.mode,
            workspace_id: d.workspace_id,
            collections: d.collections,
            messages: d.messages,
            continued_from: d.continued_from,
        }
    }
}

/// Runs a blocking service call off the UI thread. The service reads the
/// workspace's variables and writes a file.
async fn run_blocking<T, F>(svc: Arc<ConversationService>, call: F) -> Result<T, DomainError>
where
    T: Send + 'static,
    F: FnOnce(&ConversationService) -> Result<T, DomainError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || call(&svc))
        .await
        .map_err(|e| DomainError::Internal(format!("conversation task failed: {e}")))?
}

pub async fn list_conversations_inner(
    svc: Arc<ConversationService>,
) -> Result<Vec<ConversationSummaryDto>, DomainError> {
    run_blocking(svc, |svc| {
        Ok(svc
            .list()?
            .into_iter()
            .map(ConversationSummaryDto::from)
            .collect())
    })
    .await
}

pub async fn get_conversation_inner(
    svc: Arc<ConversationService>,
    id: String,
) -> Result<ConversationRecordDto, DomainError> {
    run_blocking(svc, move |svc| {
        svc.get(&id).map(ConversationRecordDto::from)
    })
    .await
}

pub async fn save_conversation_inner(
    svc: Arc<ConversationService>,
    session_id: String,
    conversation: ConversationInputDto,
) -> Result<ConversationSummaryDto, DomainError> {
    run_blocking(svc, move |svc| {
        svc.save(&session_id, conversation.into())
            .map(ConversationSummaryDto::from)
    })
    .await
}

pub async fn delete_conversation_inner(
    svc: Arc<ConversationService>,
    id: String,
) -> Result<(), DomainError> {
    run_blocking(svc, move |svc| svc.delete(&id)).await
}

pub async fn clear_conversations_inner(svc: Arc<ConversationService>) -> Result<(), DomainError> {
    run_blocking(svc, |svc| svc.clear()).await
}

#[tauri::command]
pub async fn list_conversations(
    svc: State<'_, Arc<ConversationService>>,
) -> Result<Vec<ConversationSummaryDto>, DomainError> {
    list_conversations_inner(Arc::clone(&svc)).await
}

#[tauri::command]
pub async fn get_conversation(
    id: String,
    svc: State<'_, Arc<ConversationService>>,
) -> Result<ConversationRecordDto, DomainError> {
    get_conversation_inner(Arc::clone(&svc), id).await
}

#[tauri::command]
pub async fn save_conversation(
    session_id: String,
    conversation: ConversationInputDto,
    svc: State<'_, Arc<ConversationService>>,
) -> Result<ConversationSummaryDto, DomainError> {
    save_conversation_inner(Arc::clone(&svc), session_id, conversation).await
}

#[tauri::command]
pub async fn delete_conversation(
    id: String,
    svc: State<'_, Arc<ConversationService>>,
) -> Result<(), DomainError> {
    delete_conversation_inner(Arc::clone(&svc), id).await
}

#[tauri::command]
pub async fn clear_conversations(
    svc: State<'_, Arc<ConversationService>>,
) -> Result<(), DomainError> {
    clear_conversations_inner(Arc::clone(&svc)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::conversation::StoredToolStatus;
    use rocket_shared::events::NullEventPublisher;

    const ID: &str = "0b6f3f4e-0000-4000-8000-000000000001";

    fn service() -> (tempfile::TempDir, Arc<ConversationService>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = Arc::new(std::sync::Mutex::new(dir.path().to_path_buf()));
        let svc = ConversationService::new(
            Arc::new(rocket_infra::SharedPathConversationRepo::new(Arc::clone(
                &ws,
            ))),
            Arc::new(rocket_infra::SharedPathCollectionRepo::new(Arc::clone(&ws))),
            Arc::new(rocket_infra::SharedCollectionEnvironmentRepo::new(
                Arc::clone(&ws),
            )),
            Arc::new(rocket_infra::SharedPathEnvironmentRepo::with_secret_store(
                Arc::clone(&ws),
                Arc::new(rocket_environment::NullSecretStore),
            )),
            Arc::new(NullEventPublisher),
        );
        (dir, Arc::new(svc))
    }

    fn input() -> ConversationInputDto {
        serde_json::from_value(serde_json::json!({
            "id": ID,
            "agentConfigId": "claude-code",
            "mode": "edit",
            "workspaceId": "default",
            "collections": [],
            "messages": [
                { "kind": "user", "text": "Why 401?" },
                { "kind": "agent", "text": "Because." },
                { "kind": "tool", "title": "get_request", "status": "in_progress" }
            ]
        }))
        .expect("the camelCase input parses")
    }

    #[tokio::test]
    async fn save_list_get_delete_and_clear_through_the_commands() {
        let (dir, svc) = service();
        svc.pin_session("s1");
        let saved = save_conversation_inner(Arc::clone(&svc), "s1".into(), input())
            .await
            .expect("save");
        assert_eq!(saved.title, "Why 401?");
        assert!(dir.path().join("assistant").join(".gitignore").is_file());
        let listed = list_conversations_inner(Arc::clone(&svc))
            .await
            .expect("list");
        assert_eq!(listed, vec![saved.clone()]);
        let record = get_conversation_inner(Arc::clone(&svc), ID.into())
            .await
            .expect("get");
        assert_eq!(record.messages.len(), 3);
        delete_conversation_inner(Arc::clone(&svc), ID.into())
            .await
            .expect("delete");
        clear_conversations_inner(Arc::clone(&svc))
            .await
            .expect("clear");
        assert!(list_conversations_inner(svc)
            .await
            .expect("list")
            .is_empty());
    }

    #[test]
    fn dtos_are_camel_case_and_messages_keep_their_wire_shape() {
        let summary = ConversationSummaryDto::from(ConversationSummary {
            id: ID.into(),
            title: "t".into(),
            agent_config_id: "a".into(),
            mode: ConversationMode::Ask,
            created_at: "c".into(),
            updated_at: "u".into(),
            message_count: 2,
            continued_from: Some("x".into()),
        });
        assert_eq!(
            serde_json::to_value(&summary).expect("json"),
            serde_json::json!({
                "id": ID, "title": "t", "agentConfigId": "a", "mode": "ask",
                "createdAt": "c", "updatedAt": "u", "messageCount": 2, "continuedFrom": "x"
            })
        );
        let message = StoredMessage::Tool {
            title: "t".into(),
            status: StoredToolStatus::InProgress,
        };
        assert_eq!(
            serde_json::to_value(&message).expect("json"),
            serde_json::json!({ "kind": "tool", "title": "t", "status": "in_progress" })
        );
    }

    #[tokio::test]
    async fn errors_keep_their_domain_kind() {
        let (_dir, svc) = service();
        let err = get_conversation_inner(Arc::clone(&svc), "../x".into())
            .await
            .expect_err("invalid id");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        let err = save_conversation_inner(svc, "never-started".into(), input())
            .await
            .expect_err("unknown session");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }
}
