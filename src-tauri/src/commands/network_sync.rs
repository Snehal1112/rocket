//! Team network sync commands. They stay thin: parse text, call `SyncService`, map the result.
//! DTOs are camelCase and live here only; the domain types are not renamed.

use std::sync::Arc;

use rocket_app::{ConflictFiles, LinkView, SyncService};
use rocket_network::{Decision, LinkId, LinkState, Resolution, SyncId};
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

// ----- DTOs -----

/// What the UI gets after a sync started.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStartedDto {
    pub sync_id: String,
}

/// One conflict outcome sent by the review dialog. `resolution` is `keep_mine`, `keep_theirs`
/// or `keep_both`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionDto {
    pub path: String,
    pub resolution: String,
}

/// One link for the menu status line and the link list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkDto {
    pub link_id: String,
    pub device_id: String,
    pub display_name: String,
    pub short_id: String,
    pub collection_name: String,
    /// `in_sync`, `changes_waiting`, `out_of_date` or `broken`.
    pub state: &'static str,
    /// RFC 3339.
    pub updated_at: String,
}

/// Both versions of one conflicted file. A side without the file is null.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFilesDto {
    pub mine: Option<String>,
    pub theirs: Option<String>,
}

fn conflict_files_dto_from(files: ConflictFiles) -> ConflictFilesDto {
    ConflictFilesDto {
        mine: files.mine,
        theirs: files.theirs,
    }
}

fn link_state_name(state: &LinkState) -> &'static str {
    match state {
        LinkState::InSync => "in_sync",
        LinkState::ChangesWaiting => "changes_waiting",
        LinkState::OutOfDate => "out_of_date",
        LinkState::Broken => "broken",
    }
}

fn link_dto_from(view: LinkView) -> LinkDto {
    LinkDto {
        link_id: view.link_id,
        device_id: view.device_id,
        display_name: view.display_name,
        short_id: view.short_id,
        collection_name: view.collection_name,
        state: link_state_name(&view.state),
        updated_at: view.updated_at,
    }
}

// ----- parsing -----

fn parse_link_id(raw: &str) -> Result<LinkId, DomainError> {
    LinkId::parse(raw)
        .map_err(|_| DomainError::InvalidInput("That link id is not valid.".to_string()))
}

fn parse_sync_id(raw: &str) -> Result<SyncId, DomainError> {
    SyncId::parse(raw)
        .map_err(|_| DomainError::InvalidInput("That sync id is not valid.".to_string()))
}

fn parse_resolution(raw: &str) -> Result<Resolution, DomainError> {
    match raw {
        "keep_mine" => Ok(Resolution::KeepMine),
        "keep_theirs" => Ok(Resolution::KeepTheirs),
        "keep_both" => Ok(Resolution::KeepBoth),
        _ => Err(DomainError::InvalidInput(
            "The decision must be keep_mine, keep_theirs or keep_both.".to_string(),
        )),
    }
}

fn to_decisions(dtos: Vec<DecisionDto>) -> Result<Vec<Decision>, DomainError> {
    dtos.into_iter()
        .map(|dto| {
            Ok(Decision {
                resolution: parse_resolution(&dto.resolution)?,
                path: dto.path,
            })
        })
        .collect()
}

// ----- commands -----

/// Starts a sync of one link. The peer is asked; nothing is written here yet.
#[tauri::command]
pub async fn network_sync_start(
    link_id: String,
    svc: State<'_, Arc<SyncService>>,
) -> Result<SyncStartedDto, DomainError> {
    let id = parse_link_id(&link_id)?;
    let sync_id = svc.start(&id).await?;
    Ok(SyncStartedDto {
        sync_id: sync_id.as_str().to_string(),
    })
}

/// The planner's answer to the review dialog: accept with one decision per conflict, or decline.
#[tauri::command]
pub async fn network_sync_review(
    sync_id: String,
    accept: bool,
    decisions: Vec<DecisionDto>,
    svc: State<'_, Arc<SyncService>>,
) -> Result<(), DomainError> {
    let id = parse_sync_id(&sync_id)?;
    let decisions = to_decisions(decisions)?;
    svc.review(&id, accept, decisions).await
}

/// The starter's answer to the confirm dialog.
#[tauri::command]
pub async fn network_sync_confirm(
    sync_id: String,
    accept: bool,
    svc: State<'_, Arc<SyncService>>,
) -> Result<(), DomainError> {
    let id = parse_sync_id(&sync_id)?;
    svc.confirm(&id, accept).await
}

/// Cancels a sync from either side. Nothing is applied.
#[tauri::command]
pub async fn network_sync_cancel(
    sync_id: String,
    svc: State<'_, Arc<SyncService>>,
) -> Result<(), DomainError> {
    let id = parse_sync_id(&sync_id)?;
    svc.cancel(&id).await
}

/// The two versions of one conflicted file, for the review dialog.
#[tauri::command]
pub async fn network_sync_conflict_files(
    sync_id: String,
    path: String,
    svc: State<'_, Arc<SyncService>>,
) -> Result<ConflictFilesDto, DomainError> {
    let id = parse_sync_id(&sync_id)?;
    let svc = Arc::clone(&*svc);
    let files = tauri::async_runtime::spawn_blocking(move || svc.conflict_files(&id, &path))
        .await
        .map_err(|_| {
            DomainError::Internal("The conflict files could not be read.".to_string())
        })??;
    Ok(conflict_files_dto_from(files))
}

/// Every link of this node. The export runs once per link, so it goes to a worker thread.
#[tauri::command]
pub async fn network_list_links(
    svc: State<'_, Arc<SyncService>>,
) -> Result<Vec<LinkDto>, DomainError> {
    let svc = Arc::clone(&*svc);
    let views = tauri::async_runtime::spawn_blocking(move || svc.links())
        .await
        .map_err(|_| DomainError::Internal("The link list could not be read.".to_string()))??;
    Ok(views.into_iter().map(link_dto_from).collect())
}

/// Deletes the link and its base on this side. The collection is not touched.
#[tauri::command]
pub async fn network_unlink(
    link_id: String,
    svc: State<'_, Arc<SyncService>>,
) -> Result<(), DomainError> {
    let id = parse_link_id(&link_id)?;
    svc.unlink(&id).await
}

#[cfg(test)]
mod tests {
    use rocket_app::{ConflictFiles, LinkView};
    use rocket_network::{LinkId, LinkState, Resolution, SyncId};
    use rocket_shared::error::DomainError;

    use super::*;

    fn keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn sample_view(state: LinkState) -> LinkView {
        LinkView {
            link_id: LinkId::random().as_str().to_string(),
            device_id: "ABCDEFGH".to_string(),
            short_id: "K7QF-2M9X".to_string(),
            display_name: "Ana".to_string(),
            collection_name: "Orders".to_string(),
            state,
            updated_at: "2026-11-08T10:00:00+00:00".to_string(),
        }
    }

    #[test]
    fn wire_contract_sync_started_dto_uses_camel_case_keys() {
        let dto = SyncStartedDto {
            sync_id: "0123456789abcdef0123456789abcdef".to_string(),
        };
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(keys(&value), vec!["syncId"]);
        assert_eq!(value["syncId"], "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn wire_contract_link_dto_uses_camel_case_keys() {
        let dto = link_dto_from(sample_view(LinkState::InSync));
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(
            keys(&value),
            vec![
                "collectionName",
                "deviceId",
                "displayName",
                "linkId",
                "shortId",
                "state",
                "updatedAt"
            ]
        );
        assert_eq!(value["collectionName"], "Orders");
        assert_eq!(value["state"], "in_sync");
    }

    #[test]
    fn wire_contract_conflict_files_dto_uses_camel_case_keys() {
        let dto = conflict_files_dto_from(ConflictFiles {
            mine: Some("a: 1".to_string()),
            theirs: None,
        });
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(keys(&value), vec!["mine", "theirs"]);
        assert_eq!(value["mine"], "a: 1");
        assert!(value["theirs"].is_null());
    }

    #[test]
    fn link_states_map_to_the_four_wire_names() {
        let names: Vec<&str> = [
            LinkState::InSync,
            LinkState::ChangesWaiting,
            LinkState::OutOfDate,
            LinkState::Broken,
        ]
        .iter()
        .map(link_state_name)
        .collect();
        assert_eq!(
            names,
            vec!["in_sync", "changes_waiting", "out_of_date", "broken"]
        );
    }

    #[test]
    fn wire_contract_decision_dto_reads_camel_case_keys() {
        let dto: DecisionDto = serde_json::from_value(serde_json::json!({
            "path": "users/get.yml",
            "resolution": "keep_both"
        }))
        .expect("a decision parses");
        assert_eq!(dto.path, "users/get.yml");
        assert_eq!(dto.resolution, "keep_both");
    }

    #[test]
    fn decisions_parse_the_three_resolutions_and_refuse_others() {
        let dtos = vec![
            DecisionDto {
                path: "a.yml".into(),
                resolution: "keep_mine".into(),
            },
            DecisionDto {
                path: "b.yml".into(),
                resolution: "keep_theirs".into(),
            },
            DecisionDto {
                path: "c.yml".into(),
                resolution: "keep_both".into(),
            },
        ];
        let decisions = to_decisions(dtos).expect("all three parse");
        assert_eq!(decisions[0].resolution, Resolution::KeepMine);
        assert_eq!(decisions[1].resolution, Resolution::KeepTheirs);
        assert_eq!(decisions[2].resolution, Resolution::KeepBoth);
        assert_eq!(decisions[2].path, "c.yml");

        let err = to_decisions(vec![DecisionDto {
            path: "a.yml".into(),
            resolution: "secret-marker-text".into(),
        }])
        .expect_err("an unknown resolution is refused");
        assert_eq!(
            err,
            DomainError::InvalidInput(
                "The decision must be keep_mine, keep_theirs or keep_both.".to_string()
            )
        );
    }

    #[test]
    fn ids_parse_or_are_invalid_input() {
        let link = LinkId::random();
        assert_eq!(
            parse_link_id(link.as_str())
                .expect("a fresh id parses")
                .as_str(),
            link.as_str()
        );
        let sync = SyncId::random();
        assert_eq!(
            parse_sync_id(sync.as_str())
                .expect("a fresh id parses")
                .as_str(),
            sync.as_str()
        );
        assert_eq!(
            parse_link_id("not-an-id").expect_err("refused"),
            DomainError::InvalidInput("That link id is not valid.".to_string())
        );
        assert_eq!(
            parse_sync_id("not-an-id").expect_err("refused"),
            DomainError::InvalidInput("That sync id is not valid.".to_string())
        );
    }

    #[test]
    fn the_sync_commands_are_in_the_acl_list() {
        let list = include_str!("../../app-commands.txt");
        for name in [
            "network_send_collection",
            "network_sync_start",
            "network_sync_review",
            "network_sync_confirm",
            "network_sync_cancel",
            "network_sync_conflict_files",
            "network_list_links",
            "network_unlink",
        ] {
            assert!(
                list.lines().any(|line| line.trim() == name),
                "{name} is missing from app-commands.txt"
            );
        }
    }
}
