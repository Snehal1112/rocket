//! IPC commands and DTOs for the assistant's proposals. The commands stay
//! thin: they call `ProposalService` and map its output to camelCase DTOs.
//! The base fingerprint stays in the backend.

use std::sync::Arc;

use rocket_acp::proposal::{
    AgentProposal, ProposedChange, ProposedRequest, RequestPatch, ScriptPhase,
};
use rocket_app::ProposalService;
use rocket_shared::error::DomainError;
use rocket_shared::types::{Body, Header, QueryParam};
use serde::Serialize;
use tauri::State;

/// One proposal as the panel reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProposalDto {
    pub id: String,
    pub session_id: String,
    pub change: ProposedChangeDto,
    pub summary: String,
    /// `pending`, `accepted`, `rejected`, `stale` or `failed`.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    pub created_at_ms: i64,
}

/// The proposed operation. The tag is `op`, for example `editScript`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ProposedChangeDto {
    CreateFolder {
        collection: String,
        parent_path: String,
        name: String,
    },
    CreateRequest {
        collection: String,
        folder_path: String,
        request: ProposedRequestDto,
    },
    UpdateRequest {
        collection: String,
        request_path: String,
        patch: RequestPatchDto,
    },
    EditScript {
        collection: String,
        request_path: String,
        /// `preRequest`, `postResponse` or `tests`.
        phase: &'static str,
        body: String,
    },
    MoveItem {
        collection: String,
        from_path: String,
        to_folder: String,
    },
    RenameItem {
        collection: String,
        path: String,
        new_name: String,
    },
    SetEnvVar {
        collection: String,
        environment: String,
        key: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedRequestDto {
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<Header>,
    pub query_params: Vec<QueryParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_request_script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_response_script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tests: Option<String>,
}

/// Only the fields the patch sets are present.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPatchDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Vec<Header>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_params: Option<Vec<QueryParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

fn phase_name(phase: ScriptPhase) -> &'static str {
    match phase {
        ScriptPhase::PreRequest => "preRequest",
        ScriptPhase::PostResponse => "postResponse",
        ScriptPhase::Tests => "tests",
    }
}

impl From<ProposedRequest> for ProposedRequestDto {
    fn from(request: ProposedRequest) -> Self {
        Self {
            method: request.method.to_string(),
            name: request.name,
            url: request.url,
            headers: request.headers,
            query_params: request.query_params,
            body: request.body,
            docs: request.docs,
            pre_request_script: request.pre_request_script,
            post_response_script: request.post_response_script,
            tests: request.tests,
        }
    }
}

impl From<RequestPatch> for RequestPatchDto {
    fn from(patch: RequestPatch) -> Self {
        Self {
            method: patch.method.map(|method| method.to_string()),
            url: patch.url,
            headers: patch.headers,
            query_params: patch.query_params,
            body: patch.body,
            docs: patch.docs,
        }
    }
}

impl From<ProposedChange> for ProposedChangeDto {
    fn from(change: ProposedChange) -> Self {
        match change {
            ProposedChange::CreateFolder {
                collection,
                parent_path,
                name,
            } => Self::CreateFolder {
                collection,
                parent_path,
                name,
            },
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => Self::CreateRequest {
                collection,
                folder_path,
                request: request.into(),
            },
            ProposedChange::UpdateRequest {
                collection,
                request_path,
                patch,
                ..
            } => Self::UpdateRequest {
                collection,
                request_path,
                patch: patch.into(),
            },
            ProposedChange::EditScript {
                collection,
                request_path,
                phase,
                body,
                ..
            } => Self::EditScript {
                collection,
                request_path,
                phase: phase_name(phase),
                body,
            },
            ProposedChange::MoveItem {
                collection,
                from_path,
                to_folder,
                ..
            } => Self::MoveItem {
                collection,
                from_path,
                to_folder,
            },
            ProposedChange::RenameItem {
                collection,
                path,
                new_name,
                ..
            } => Self::RenameItem {
                collection,
                path,
                new_name,
            },
            ProposedChange::SetEnvVar {
                collection,
                environment,
                key,
                value,
            } => Self::SetEnvVar {
                collection,
                environment,
                key,
                value,
            },
        }
    }
}

impl From<AgentProposal> for AgentProposalDto {
    fn from(proposal: AgentProposal) -> Self {
        Self {
            status_message: proposal.status.message().map(str::to_string),
            status: proposal.status.as_str().to_string(),
            id: proposal.id,
            session_id: proposal.session_id,
            change: proposal.change.into(),
            summary: proposal.summary,
            created_at_ms: proposal.created_at_ms,
        }
    }
}

/// Runs a blocking service call off the UI thread. The service holds a
/// std mutex and does disk I/O, so it must not run on the main thread.
async fn run_blocking<T, F>(svc: Arc<ProposalService>, call: F) -> Result<T, DomainError>
where
    T: Send + 'static,
    F: FnOnce(&ProposalService) -> Result<T, DomainError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || call(&svc))
        .await
        .map_err(|e| DomainError::Internal(format!("proposal task failed: {e}")))?
}

async fn list_proposals_inner(
    svc: Arc<ProposalService>,
    session_id: String,
) -> Result<Vec<AgentProposalDto>, DomainError> {
    run_blocking(svc, move |svc| {
        Ok(svc
            .list(&session_id)
            .into_iter()
            .map(AgentProposalDto::from)
            .collect())
    })
    .await
}

async fn accept_proposal_inner(
    svc: Arc<ProposalService>,
    session_id: String,
    proposal_id: String,
) -> Result<AgentProposalDto, DomainError> {
    run_blocking(svc, move |svc| {
        svc.accept(&session_id, &proposal_id)
            .map(AgentProposalDto::from)
    })
    .await
}

async fn reject_proposal_inner(
    svc: Arc<ProposalService>,
    session_id: String,
    proposal_id: String,
) -> Result<AgentProposalDto, DomainError> {
    run_blocking(svc, move |svc| {
        svc.reject(&session_id, &proposal_id)
            .map(AgentProposalDto::from)
    })
    .await
}

#[tauri::command]
pub async fn list_agent_proposals(
    session_id: String,
    svc: State<'_, Arc<ProposalService>>,
) -> Result<Vec<AgentProposalDto>, DomainError> {
    list_proposals_inner(Arc::clone(&svc), session_id).await
}

#[tauri::command]
pub async fn accept_agent_proposal(
    session_id: String,
    proposal_id: String,
    svc: State<'_, Arc<ProposalService>>,
) -> Result<AgentProposalDto, DomainError> {
    accept_proposal_inner(Arc::clone(&svc), session_id, proposal_id).await
}

#[tauri::command]
pub async fn reject_agent_proposal(
    session_id: String,
    proposal_id: String,
    svc: State<'_, Arc<ProposalService>>,
) -> Result<AgentProposalDto, DomainError> {
    reject_proposal_inner(Arc::clone(&svc), session_id, proposal_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::proposal::ProposalStatus;

    use rocket_app::CollectionService;
    use rocket_shared::events::NullEventPublisher;

    /// A service over a temp workspace that holds a "demo" collection.
    fn service() -> (tempfile::TempDir, Arc<ProposalService>) {
        use rocket_collection::CollectionRepository;
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("demo").expect("create collection");
        let svc = ProposalService::new(
            CollectionService::new(
                Box::new(rocket_infra::FsCollectionRepo::new_standalone(
                    dir.path().to_path_buf(),
                )),
                Box::new(NullEventPublisher),
            ),
            Arc::new(rocket_infra::SharedCollectionEnvironmentRepo::new(
                Arc::new(std::sync::Mutex::new(dir.path().to_path_buf())),
            )),
            Arc::new(NullEventPublisher),
        );
        (dir, Arc::new(svc))
    }

    fn new_folder(name: &str) -> ProposedChange {
        ProposedChange::CreateFolder {
            collection: "demo".into(),
            parent_path: String::new(),
            name: name.into(),
        }
    }

    #[tokio::test]
    async fn the_async_commands_list_accept_and_reject_like_the_service() {
        let (dir, svc) = service();
        let ids = svc
            .propose("s1", vec![new_folder("kept"), new_folder("dropped")])
            .expect("propose");

        let listed = list_proposals_inner(Arc::clone(&svc), "s1".into())
            .await
            .expect("list");
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|p| p.status == "pending"));

        let accepted = accept_proposal_inner(Arc::clone(&svc), "s1".into(), ids[0].clone())
            .await
            .expect("accept");
        assert_eq!(accepted.status, "accepted");
        assert!(dir.path().join("demo").join("kept").is_dir());

        let rejected = reject_proposal_inner(Arc::clone(&svc), "s1".into(), ids[1].clone())
            .await
            .expect("reject");
        assert_eq!(rejected.status, "rejected");
        assert!(!dir.path().join("demo").join("dropped").exists());

        let again = accept_proposal_inner(Arc::clone(&svc), "s1".into(), ids[0].clone())
            .await
            .expect_err("a second accept is refused");
        assert!(matches!(again, DomainError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn a_panicking_blocking_call_maps_to_an_internal_error() {
        let (_dir, svc) = service();
        let result: Result<(), DomainError> = run_blocking(svc, |_| panic!("boom")).await;
        assert!(matches!(result, Err(DomainError::Internal(_))));
    }

    #[test]
    fn a_failed_edit_script_proposal_serializes_in_camel_case() {
        let mut proposal = AgentProposal::new(
            "p1".into(),
            "s1".into(),
            ProposedChange::EditScript {
                collection: "demo".into(),
                request_path: "get-users.yml".into(),
                phase: ScriptPhase::PreRequest,
                body: "console.log(1);".into(),
                base_fingerprint: "abc".into(),
            },
            42,
        );
        proposal.status = ProposalStatus::Failed {
            message: "disk full".into(),
        };
        let json = serde_json::to_value(AgentProposalDto::from(proposal)).expect("serialize");
        assert_eq!(json["id"], "p1");
        assert_eq!(json["sessionId"], "s1");
        assert_eq!(json["createdAtMs"], 42);
        assert_eq!(json["status"], "failed");
        assert_eq!(json["statusMessage"], "disk full");
        assert_eq!(json["change"]["op"], "editScript");
        assert_eq!(json["change"]["requestPath"], "get-users.yml");
        assert_eq!(json["change"]["phase"], "preRequest");
        assert_eq!(json["change"]["body"], "console.log(1);");
        assert!(
            json["change"].get("baseFingerprint").is_none(),
            "the fingerprint stays in the backend"
        );
    }

    #[test]
    fn a_pending_create_request_has_no_status_message_and_camel_case_fields() {
        let proposal = AgentProposal::new(
            "p2".into(),
            "s1".into(),
            ProposedChange::CreateRequest {
                collection: "demo".into(),
                folder_path: "users".into(),
                request: ProposedRequest {
                    name: "List Users".into(),
                    method: rocket_shared::types::HttpMethod::Get,
                    url: "https://x/users".into(),
                    headers: vec![Header::new("Accept", "application/json")],
                    query_params: vec![],
                    body: None,
                    docs: None,
                    pre_request_script: None,
                    post_response_script: None,
                    tests: Some("rok.test('ok', () => {});".into()),
                },
            },
            7,
        );
        let json = serde_json::to_value(AgentProposalDto::from(proposal)).expect("serialize");
        assert!(json.get("statusMessage").is_none());
        assert_eq!(json["status"], "pending");
        assert_eq!(json["change"]["op"], "createRequest");
        assert_eq!(json["change"]["folderPath"], "users");
        assert_eq!(json["change"]["request"]["method"], "GET");
        assert_eq!(
            json["change"]["request"]["queryParams"],
            serde_json::json!([])
        );
        assert_eq!(
            json["change"]["request"]["tests"],
            "rok.test('ok', () => {});"
        );
        assert!(json["change"]["request"].get("body").is_none());
    }

    #[test]
    fn set_env_var_uses_the_set_env_var_tag() {
        let proposal = AgentProposal::new(
            "p3".into(),
            "s1".into(),
            ProposedChange::SetEnvVar {
                collection: "demo".into(),
                environment: "dev".into(),
                key: "HOST".into(),
                value: "api.example.com".into(),
            },
            1,
        );
        let json = serde_json::to_value(AgentProposalDto::from(proposal)).expect("serialize");
        assert_eq!(json["change"]["op"], "setEnvVar");
        assert_eq!(json["change"]["environment"], "dev");
    }
}
