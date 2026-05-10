use chrono::NaiveDate;
use std::path::PathBuf;
use ulid::Ulid;

/// A party (provider or consumer) in a contract.
#[derive(Debug, Clone, PartialEq)]
pub struct ContractParty {
    pub id: String,
    pub name: String,
    pub kind: PartyKind,
    pub avatar_seed: Option<String>,
    pub avatar_color: Option<String>,
}

impl ContractParty {
    pub fn from_name(name: &str) -> Self {
        Self {
            id: name.to_lowercase().replace(' ', "-"),
            name: name.to_string(),
            kind: PartyKind::Team,
            avatar_seed: None,
            avatar_color: None,
        }
    }
}

/// What kind of entity a party represents.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PartyKind {
    #[default]
    Team,
    Company,
    Service,
}

/// Policy governing how contract drift is evaluated.
#[derive(Debug, Clone, PartialEq)]
pub struct ContractPolicy {
    pub breaking_change_policy: BreakingChangePolicy,
    pub notice_days: u32,
    pub uptime_sla: Option<f32>,
}

impl Default for ContractPolicy {
    fn default() -> Self {
        Self {
            breaking_change_policy: BreakingChangePolicy::Lenient,
            notice_days: 30,
            uptime_sla: None,
        }
    }
}

/// How strictly drift is classified as breaking.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum BreakingChangePolicy {
    Strict,
    #[default]
    Lenient,
    AdditiveOk,
}

/// A contract between a provider and one or more consumers.
#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    pub id: Ulid,
    pub title: String,

    pub provider: ContractParty,
    pub consumers: Vec<ContractParty>,

    pub project: String,

    pub version: String,

    pub status: ContractStatus,

    pub effective_date: NaiveDate,
    pub expiry_date: Option<NaiveDate>,

    pub document_paths: Vec<PathBuf>,

    pub enforcement_mode: ContractEnforcementMode,
    pub scope: ContractScope,

    pub policy: ContractPolicy,

    pub drift_count: u32,
    pub breach_count: u32,
    pub endpoint_count: u32,

    pub created_by: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Lifecycle status of a contract.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ContractStatus {
    /// Not yet published — snapshot not taken.
    Draft,
    /// Healthy, in compliance.
    #[default]
    Active,
    /// Non-breaking changes detected since signing.
    Drift,
    /// Breaking changes detected — consumer build at risk.
    Breach,
    /// Sent for consumer sign-off (not yet approved).
    InReview,
    /// Monitoring suspended by the provider.
    Paused,
    /// Expiry date is within 30 days.
    ExpiringIn30Days,
    /// Past expiry date.
    Expired,
}

/// Model B extension seam.
/// Only `Informational` is reachable from UI in this sprint.
/// `Warn` and `Block` are defined now so Model B requires zero domain changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ContractEnforcementMode {
    #[default]
    Informational,
    Warn,
    Block,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContractScope {
    Collection,
    Folder { rel_path: PathBuf },
    Request { rel_path: PathBuf },
}
