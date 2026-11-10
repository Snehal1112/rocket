//! Team network contract check commands. They stay thin: parse text, call
//! `ContractCheckService`, map the result. DTOs are camelCase and live here only; the domain
//! and wire types are not renamed.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rocket_app::{ContractCheckReview, ContractCheckService, ContractCheckStarted, ReviewedChange};
use rocket_network::{
    ContractChange, ContractChangeType, ContractCheckId, ContractPolicyWire, ContractVerdict,
    DeviceId,
};
use rocket_shared::error::{DomainError, DomainResult};
use serde::Serialize;
use tauri::State;

/// How often due checks are swept.
pub const CONTRACT_CHECK_SWEEP_EVERY: Duration = Duration::from_secs(30);

// ----- DTOs -----

/// One offered change as the UI shows it. Values are already redacted by the provider.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractChangeDto {
    pub method: String,
    pub path_template: String,
    pub field: String,
    /// `changed`, `added` or `removed`.
    pub change_type: ContractChangeType,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub breaking: bool,
}

/// The check id sent to one node.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedCheckDto {
    pub device_id: String,
    pub check_id: String,
}

/// What the provider gets after a check started: one check per node and the sent changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractCheckStartedDto {
    pub checks: Vec<StartedCheckDto>,
    pub changes: Vec<ContractChangeDto>,
}

/// One local request that matched a change. It never leaves this node.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMatchDto {
    pub collection: String,
    pub name: String,
}

/// One offered change with its local verdict and matches, in offer order.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewedChangeDto {
    pub index: u32,
    pub method: String,
    pub path_template: String,
    pub field: String,
    pub change_type: ContractChangeType,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub breaking: bool,
    /// `pass`, `break` or `not_used`.
    pub verdict: ContractVerdict,
    pub matches: Vec<LocalMatchDto>,
}

/// The consumer's review of an incoming check.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractCheckReviewDto {
    pub check_id: String,
    pub device_id: String,
    pub contract: String,
    pub version: String,
    /// `strict`, `lenient` or `additive_ok`.
    pub policy: ContractPolicyWire,
    pub changes: Vec<ReviewedChangeDto>,
}

fn change_dto_from(change: ContractChange) -> ContractChangeDto {
    ContractChangeDto {
        method: change.method,
        path_template: change.path_template,
        field: change.field,
        change_type: change.change_type,
        old_value: change.old_value,
        new_value: change.new_value,
        breaking: change.breaking,
    }
}

fn started_dto_from(started: ContractCheckStarted) -> ContractCheckStartedDto {
    ContractCheckStartedDto {
        checks: started
            .check_ids
            .into_iter()
            .map(|(device, id)| StartedCheckDto {
                device_id: device.to_base32(),
                check_id: id.as_str().to_string(),
            })
            .collect(),
        changes: started.changes.into_iter().map(change_dto_from).collect(),
    }
}

fn reviewed_dto_from(index: usize, reviewed: ReviewedChange) -> ReviewedChangeDto {
    let change = reviewed.change;
    ReviewedChangeDto {
        // At most 200 changes reach a review, so the index always fits.
        index: u32::try_from(index).unwrap_or(u32::MAX),
        method: change.method,
        path_template: change.path_template,
        field: change.field,
        change_type: change.change_type,
        old_value: change.old_value,
        new_value: change.new_value,
        breaking: change.breaking,
        verdict: reviewed.verdict,
        matches: reviewed
            .matches
            .into_iter()
            .map(|m| LocalMatchDto {
                collection: m.collection,
                name: m.name,
            })
            .collect(),
    }
}

fn review_dto_from(review: ContractCheckReview) -> ContractCheckReviewDto {
    ContractCheckReviewDto {
        check_id: review.check_id,
        device_id: review.device_id,
        contract: review.contract,
        version: review.version,
        policy: review.policy,
        changes: review
            .changes
            .into_iter()
            .enumerate()
            .map(|(i, c)| reviewed_dto_from(i, c))
            .collect(),
    }
}

// ----- parsing -----

fn parse_check_id(raw: &str) -> DomainResult<ContractCheckId> {
    ContractCheckId::parse(raw)
        .map_err(|_| DomainError::InvalidInput("That check id is not valid.".to_string()))
}

fn parse_device_ids(raw: &[String]) -> DomainResult<Vec<DeviceId>> {
    raw.iter()
        .map(|id| {
            DeviceId::parse_base32(id)
                .map_err(|_| DomainError::InvalidInput("That device id is not valid.".to_string()))
        })
        .collect()
}

/// Runs a blocking service call on a worker thread of the app runtime. The service spawns its
/// frames from there, so it must stay inside the runtime.
async fn blocking<T, F>(svc: &Arc<ContractCheckService>, failed: &str, f: F) -> DomainResult<T>
where
    T: Send + 'static,
    F: FnOnce(&ContractCheckService) -> DomainResult<T> + Send + 'static,
{
    let svc = Arc::clone(svc);
    tauri::async_runtime::spawn_blocking(move || f(&svc))
        .await
        .map_err(|_| DomainError::Internal(failed.to_string()))?
}

// ----- commands -----

/// Sends the pending changes of a contract to the chosen nodes.
#[tauri::command]
pub async fn network_contract_check_start(
    collection_root: String,
    contract_id: String,
    device_ids: Vec<String>,
    svc: State<'_, Arc<ContractCheckService>>,
) -> Result<ContractCheckStartedDto, DomainError> {
    let devices = parse_device_ids(&device_ids)?;
    let started = blocking(&svc, "The check could not be started.", move |s| {
        s.start(&collection_root, &contract_id, &devices)
    })
    .await?;
    Ok(started_dto_from(started))
}

/// Matches an incoming check against the requests of this workspace. Nothing is sent.
#[tauri::command]
pub async fn network_contract_check_review(
    check_id: String,
    svc: State<'_, Arc<ContractCheckService>>,
) -> Result<ContractCheckReviewDto, DomainError> {
    let id = parse_check_id(&check_id)?;
    let review = blocking(&svc, "The check could not be reviewed.", move |s| {
        s.review(&id)
    })
    .await?;
    Ok(review_dto_from(review))
}

/// Sends the reviewed verdicts to the provider.
#[tauri::command]
pub async fn network_contract_check_reply(
    check_id: String,
    svc: State<'_, Arc<ContractCheckService>>,
) -> Result<(), DomainError> {
    let id = parse_check_id(&check_id)?;
    blocking(&svc, "The result could not be sent.", move |s| s.reply(&id)).await
}

/// Declines an incoming check.
#[tauri::command]
pub async fn network_contract_check_decline(
    check_id: String,
    svc: State<'_, Arc<ContractCheckService>>,
) -> Result<(), DomainError> {
    let id = parse_check_id(&check_id)?;
    blocking(&svc, "The check could not be declined.", move |s| {
        s.decline(&id)
    })
    .await
}

/// Ends due checks every 30 s for the life of the app, on the app runtime.
pub(crate) fn start_contract_check_sweeper(svc: Arc<ContractCheckService>) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(CONTRACT_CHECK_SWEEP_EVERY);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The first tick fires at once and there is nothing to sweep yet.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            svc.sweep(Instant::now());
        }
    });
}

#[cfg(test)]
mod tests {
    use rocket_app::LocalMatch;

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

    fn change() -> ContractChange {
        ContractChange {
            method: "GET".into(),
            path_template: "/users/{id}".into(),
            field: "header:X-Trace".into(),
            change_type: ContractChangeType::Added,
            old_value: None,
            new_value: Some("on".into()),
            breaking: true,
        }
    }

    #[test]
    fn bad_check_ids_fail_with_the_fixed_sentence() {
        for bad in ["", "nope", "0123", "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"] {
            assert_eq!(
                parse_check_id(bad).map(|_| ()),
                Err(DomainError::InvalidInput(
                    "That check id is not valid.".to_string()
                )),
                "{bad}"
            );
        }
        let good = ContractCheckId::random();
        assert_eq!(parse_check_id(good.as_str()), Ok(good));
    }

    #[test]
    fn bad_device_ids_fail_with_the_fixed_sentence() {
        let good = DeviceId::from_bytes([3; 32]).to_base32();
        assert_eq!(
            parse_device_ids(&[good.clone(), "not a device".into()]).map(|_| ()),
            Err(DomainError::InvalidInput(
                "That device id is not valid.".to_string()
            ))
        );
        assert_eq!(
            parse_device_ids(&[good]),
            Ok(vec![DeviceId::from_bytes([3; 32])])
        );
    }

    #[test]
    fn wire_contract_started_dto_uses_camel_case_keys() {
        let id = ContractCheckId::random();
        let dto = started_dto_from(ContractCheckStarted {
            check_ids: vec![(DeviceId::from_bytes([3; 32]), id.clone())],
            changes: vec![change()],
        });
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(keys(&value), vec!["changes", "checks"]);
        assert_eq!(keys(&value["checks"][0]), vec!["checkId", "deviceId"]);
        assert_eq!(value["checks"][0]["checkId"], id.as_str());
        assert_eq!(
            value["checks"][0]["deviceId"],
            DeviceId::from_bytes([3; 32]).to_base32()
        );
        let change = &value["changes"][0];
        assert_eq!(
            keys(change),
            vec![
                "breaking",
                "changeType",
                "field",
                "method",
                "newValue",
                "oldValue",
                "pathTemplate"
            ]
        );
        assert_eq!(change["changeType"], "added");
        assert_eq!(change["pathTemplate"], "/users/{id}");
        assert_eq!(change["oldValue"], serde_json::Value::Null);
    }

    #[test]
    fn wire_contract_review_dto_uses_camel_case_keys() {
        let dto = review_dto_from(ContractCheckReview {
            check_id: "c".into(),
            device_id: "d".into(),
            contract: "Orders API".into(),
            version: "1.2.0".into(),
            policy: ContractPolicyWire::AdditiveOk,
            changes: vec![
                ReviewedChange {
                    change: change(),
                    verdict: ContractVerdict::NotUsed,
                    matches: Vec::new(),
                },
                ReviewedChange {
                    change: change(),
                    verdict: ContractVerdict::Break,
                    matches: vec![LocalMatch {
                        collection: "Shop".into(),
                        name: "Get user".into(),
                    }],
                },
            ],
        });
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(
            keys(&value),
            vec!["changes", "checkId", "contract", "deviceId", "policy", "version"]
        );
        assert_eq!(value["policy"], "additive_ok");
        let second = &value["changes"][1];
        assert_eq!(
            keys(second),
            vec![
                "breaking",
                "changeType",
                "field",
                "index",
                "matches",
                "method",
                "newValue",
                "oldValue",
                "pathTemplate",
                "verdict"
            ]
        );
        assert_eq!(second["index"], 1);
        assert_eq!(second["verdict"], "break");
        assert_eq!(value["changes"][0]["verdict"], "not_used");
        assert_eq!(keys(&second["matches"][0]), vec!["collection", "name"]);
        assert_eq!(second["matches"][0]["name"], "Get user");
    }
}
