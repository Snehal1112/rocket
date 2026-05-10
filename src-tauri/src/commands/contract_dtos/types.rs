//! IPC DTOs for `Contract` and its sub-types.

use rocket_collection::contract::types::{
    BreakingChangePolicy, Contract, ContractEnforcementMode, ContractParty,
    ContractPolicy, ContractScope, ContractStatus, PartyKind,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractPartyDto {
    pub id: String,
    pub name: String,
    pub kind: PartyKindDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PartyKindDto {
    #[default]
    Team,
    Company,
    Service,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractPolicyDto {
    #[serde(default)]
    pub breaking_change_policy: BreakingChangePolicyDto,
    #[serde(default = "default_notice_days_dto")]
    pub notice_days: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_sla: Option<f32>,
}

impl Default for ContractPolicyDto {
    fn default() -> Self {
        Self {
            breaking_change_policy: BreakingChangePolicyDto::Lenient,
            notice_days: 30,
            uptime_sla: None,
        }
    }
}

fn default_notice_days_dto() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BreakingChangePolicyDto {
    Strict,
    #[default]
    Lenient,
    AdditiveOk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContractScopeDto {
    Collection,
    Folder { rel_path: PathBuf },
    Request { rel_path: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatusDto {
    Draft,
    #[default]
    Active,
    Drift,
    Breach,
    InReview,
    Paused,
    ExpiringIn30Days,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractEnforcementModeDto {
    #[default]
    Informational,
    Warn,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractDto {
    pub id: Ulid,
    pub title: String,
    pub provider: ContractPartyDto,
    pub consumers: Vec<ContractPartyDto>,
    pub project: String,
    pub version: String,
    pub status: ContractStatusDto,
    pub effective_date: chrono::NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<chrono::NaiveDate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_paths: Vec<PathBuf>,
    pub enforcement_mode: ContractEnforcementModeDto,
    pub scope: ContractScopeDto,
    pub policy: ContractPolicyDto,
    pub drift_count: u32,
    pub breach_count: u32,
    pub endpoint_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

// From impls ---------------------------------------------------------------

impl From<&PartyKind> for PartyKindDto {
    fn from(k: &PartyKind) -> Self {
        match k {
            PartyKind::Team => Self::Team,
            PartyKind::Company => Self::Company,
            PartyKind::Service => Self::Service,
        }
    }
}
impl From<PartyKindDto> for PartyKind {
    fn from(d: PartyKindDto) -> Self {
        match d {
            PartyKindDto::Team => Self::Team,
            PartyKindDto::Company => Self::Company,
            PartyKindDto::Service => Self::Service,
        }
    }
}

impl From<&ContractParty> for ContractPartyDto {
    fn from(p: &ContractParty) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            kind: (&p.kind).into(),
            avatar_seed: p.avatar_seed.clone(),
            avatar_color: p.avatar_color.clone(),
        }
    }
}
impl From<ContractPartyDto> for ContractParty {
    fn from(d: ContractPartyDto) -> Self {
        Self {
            id: d.id,
            name: d.name,
            kind: d.kind.into(),
            avatar_seed: d.avatar_seed,
            avatar_color: d.avatar_color,
        }
    }
}

impl From<&BreakingChangePolicy> for BreakingChangePolicyDto {
    fn from(p: &BreakingChangePolicy) -> Self {
        match p {
            BreakingChangePolicy::Strict => Self::Strict,
            BreakingChangePolicy::Lenient => Self::Lenient,
            BreakingChangePolicy::AdditiveOk => Self::AdditiveOk,
        }
    }
}
impl From<BreakingChangePolicyDto> for BreakingChangePolicy {
    fn from(d: BreakingChangePolicyDto) -> Self {
        match d {
            BreakingChangePolicyDto::Strict => Self::Strict,
            BreakingChangePolicyDto::Lenient => Self::Lenient,
            BreakingChangePolicyDto::AdditiveOk => Self::AdditiveOk,
        }
    }
}

impl From<&ContractPolicy> for ContractPolicyDto {
    fn from(p: &ContractPolicy) -> Self {
        Self {
            breaking_change_policy: (&p.breaking_change_policy).into(),
            notice_days: p.notice_days,
            uptime_sla: p.uptime_sla,
        }
    }
}
impl From<ContractPolicyDto> for ContractPolicy {
    fn from(d: ContractPolicyDto) -> Self {
        Self {
            breaking_change_policy: d.breaking_change_policy.into(),
            notice_days: d.notice_days,
            uptime_sla: d.uptime_sla,
        }
    }
}

impl From<&ContractScope> for ContractScopeDto {
    fn from(s: &ContractScope) -> Self {
        match s {
            ContractScope::Collection => Self::Collection,
            ContractScope::Folder { rel_path } => Self::Folder { rel_path: rel_path.clone() },
            ContractScope::Request { rel_path } => Self::Request { rel_path: rel_path.clone() },
        }
    }
}
impl From<ContractScopeDto> for ContractScope {
    fn from(d: ContractScopeDto) -> Self {
        match d {
            ContractScopeDto::Collection => Self::Collection,
            ContractScopeDto::Folder { rel_path } => Self::Folder { rel_path },
            ContractScopeDto::Request { rel_path } => Self::Request { rel_path },
        }
    }
}

impl From<&ContractStatus> for ContractStatusDto {
    fn from(s: &ContractStatus) -> Self {
        match s {
            ContractStatus::Draft => Self::Draft,
            ContractStatus::Active => Self::Active,
            ContractStatus::Drift => Self::Drift,
            ContractStatus::Breach => Self::Breach,
            ContractStatus::InReview => Self::InReview,
            ContractStatus::Paused => Self::Paused,
            ContractStatus::ExpiringIn30Days => Self::ExpiringIn30Days,
            ContractStatus::Expired => Self::Expired,
        }
    }
}
impl From<ContractStatusDto> for ContractStatus {
    fn from(d: ContractStatusDto) -> Self {
        match d {
            ContractStatusDto::Draft => Self::Draft,
            ContractStatusDto::Active => Self::Active,
            ContractStatusDto::Drift => Self::Drift,
            ContractStatusDto::Breach => Self::Breach,
            ContractStatusDto::InReview => Self::InReview,
            ContractStatusDto::Paused => Self::Paused,
            ContractStatusDto::ExpiringIn30Days => Self::ExpiringIn30Days,
            ContractStatusDto::Expired => Self::Expired,
        }
    }
}

impl From<&ContractEnforcementMode> for ContractEnforcementModeDto {
    fn from(e: &ContractEnforcementMode) -> Self {
        match e {
            ContractEnforcementMode::Informational => Self::Informational,
            ContractEnforcementMode::Warn => Self::Warn,
            ContractEnforcementMode::Block => Self::Block,
        }
    }
}
impl From<ContractEnforcementModeDto> for ContractEnforcementMode {
    fn from(d: ContractEnforcementModeDto) -> Self {
        match d {
            ContractEnforcementModeDto::Informational => Self::Informational,
            ContractEnforcementModeDto::Warn => Self::Warn,
            ContractEnforcementModeDto::Block => Self::Block,
        }
    }
}

impl From<&Contract> for ContractDto {
    fn from(c: &Contract) -> Self {
        Self {
            id: c.id,
            title: c.title.clone(),
            provider: (&c.provider).into(),
            consumers: c.consumers.iter().map(Into::into).collect(),
            project: c.project.clone(),
            version: c.version.clone(),
            status: (&c.status).into(),
            effective_date: c.effective_date,
            expiry_date: c.expiry_date,
            document_paths: c.document_paths.clone(),
            enforcement_mode: (&c.enforcement_mode).into(),
            scope: (&c.scope).into(),
            policy: (&c.policy).into(),
            drift_count: c.drift_count,
            breach_count: c.breach_count,
            endpoint_count: c.endpoint_count,
            created_by: c.created_by.clone(),
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

impl From<ContractDto> for Contract {
    fn from(d: ContractDto) -> Self {
        Self {
            id: d.id,
            title: d.title,
            provider: d.provider.into(),
            consumers: d.consumers.into_iter().map(Into::into).collect(),
            project: d.project,
            version: d.version,
            status: d.status.into(),
            effective_date: d.effective_date,
            expiry_date: d.expiry_date,
            document_paths: d.document_paths,
            enforcement_mode: d.enforcement_mode.into(),
            scope: d.scope.into(),
            policy: d.policy.into(),
            drift_count: d.drift_count,
            breach_count: d.breach_count,
            endpoint_count: d.endpoint_count,
            created_by: d.created_by,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn party_dto_json_roundtrip_camel_case() {
        let d = ContractPartyDto {
            id: "billing-team".into(),
            name: "Billing Team".into(),
            kind: PartyKindDto::Team,
            avatar_seed: Some("seed".into()),
            avatar_color: None,
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"avatarSeed\":"), "got {json}");
        let back: ContractPartyDto = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn scope_dto_json_keeps_rel_path_snake_case() {
        let d = ContractScopeDto::Folder { rel_path: PathBuf::from("a/b.yml") };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"rel_path\":"), "rel_path must stay snake_case in JSON: {json}");
    }

    #[test]
    fn contract_dto_domain_roundtrip() {
        let domain = Contract {
            id: Ulid::new(),
            title: "T".into(),
            provider: ContractParty::from_name("Prov"),
            consumers: vec![ContractParty::from_name("Cons")],
            project: "P".into(),
            version: "1.0.0".into(),
            status: ContractStatus::Active,
            effective_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            expiry_date: None,
            document_paths: vec![],
            enforcement_mode: ContractEnforcementMode::Informational,
            scope: ContractScope::Collection,
            policy: ContractPolicy::default(),
            drift_count: 0,
            breach_count: 0,
            endpoint_count: 0,
            created_by: None,
            created_at: None,
            updated_at: None,
        };
        let dto: ContractDto = (&domain).into();
        let back: Contract = dto.into();
        assert_eq!(domain, back);
    }
}
