//! IPC DTOs for service-layer summary types.

use rocket_app::contract_service::{ContractDriftSummary, ContractSummary};
use serde::{Deserialize, Serialize};

use super::types::ContractStatusDto;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractDriftSummaryDto {
    pub contract_id: String,
    pub status: ContractStatusDto,
    pub drift_count: u32,
    pub breach_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractSummaryDto {
    pub id: String,
    pub title: String,
    pub status: ContractStatusDto,
    pub drift_count: u32,
    pub breach_count: u32,
    pub endpoint_count: u32,
}

impl From<ContractDriftSummary> for ContractDriftSummaryDto {
    fn from(s: ContractDriftSummary) -> Self {
        Self {
            contract_id: s.contract_id,
            status: (&s.status).into(),
            drift_count: s.drift_count,
            breach_count: s.breach_count,
        }
    }
}

impl From<ContractSummary> for ContractSummaryDto {
    fn from(s: ContractSummary) -> Self {
        Self {
            id: s.id,
            title: s.title,
            status: (&s.status).into(),
            drift_count: s.drift_count,
            breach_count: s.breach_count,
            endpoint_count: s.endpoint_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::contract::types::ContractStatus;

    #[test]
    fn drift_summary_dto_camel_case_json() {
        let s = ContractDriftSummary {
            contract_id: "01ABC".into(),
            status: ContractStatus::Drift,
            drift_count: 3,
            breach_count: 1,
        };
        let dto: ContractDriftSummaryDto = s.into();
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"contractId\":"));
        assert!(json.contains("\"driftCount\":"));
    }

    #[test]
    fn summary_dto_camel_case_json() {
        let s = ContractSummary {
            id: "x".into(),
            title: "T".into(),
            status: ContractStatus::Active,
            drift_count: 0,
            breach_count: 0,
            endpoint_count: 5,
        };
        let dto: ContractSummaryDto = s.into();
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"endpointCount\":"));
    }
}
