//! Persistence records for `Contract` and its sub-types (`ContractParty`,
//! `ContractPolicy`, `ContractScope`, plus the four enums).

use rocket_collection::contract::types::{BreakingChangePolicy, ContractParty, ContractPolicy, PartyKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractPartyRecord {
    pub id: String,
    pub name: String,
    pub kind: PartyKindRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PartyKindRecord {
    #[default]
    Team,
    Company,
    Service,
}

impl<'de> serde::Deserialize<'de> for ContractPartyRecord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = ContractPartyRecord;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a string or a ContractPartyRecord object")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<ContractPartyRecord, E> {
                Ok(ContractPartyRecord {
                    id: v.to_lowercase().replace(' ', "-"),
                    name: v.to_string(),
                    kind: PartyKindRecord::Team,
                    avatar_seed: None,
                    avatar_color: None,
                })
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<ContractPartyRecord, E> {
                self.visit_str(&v)
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<ContractPartyRecord, A::Error> {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Helper {
                    id: String,
                    name: String,
                    #[serde(default)]
                    kind: PartyKindRecord,
                    #[serde(default)]
                    avatar_seed: Option<String>,
                    #[serde(default)]
                    avatar_color: Option<String>,
                }
                let h = Helper::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(ContractPartyRecord {
                    id: h.id,
                    name: h.name,
                    kind: h.kind,
                    avatar_seed: h.avatar_seed,
                    avatar_color: h.avatar_color,
                })
            }
        }

        d.deserialize_any(V)
    }
}

// Conversions ---------------------------------------------------------------

impl From<&PartyKind> for PartyKindRecord {
    fn from(k: &PartyKind) -> Self {
        match k {
            PartyKind::Team => PartyKindRecord::Team,
            PartyKind::Company => PartyKindRecord::Company,
            PartyKind::Service => PartyKindRecord::Service,
        }
    }
}

impl From<PartyKindRecord> for PartyKind {
    fn from(r: PartyKindRecord) -> Self {
        match r {
            PartyKindRecord::Team => PartyKind::Team,
            PartyKindRecord::Company => PartyKind::Company,
            PartyKindRecord::Service => PartyKind::Service,
        }
    }
}

impl From<&ContractParty> for ContractPartyRecord {
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

impl From<ContractPartyRecord> for ContractParty {
    fn from(r: ContractPartyRecord) -> Self {
        Self {
            id: r.id,
            name: r.name,
            kind: r.kind.into(),
            avatar_seed: r.avatar_seed,
            avatar_color: r.avatar_color,
        }
    }
}

#[cfg(test)]
mod party_tests {
    use super::*;

    #[test]
    fn party_record_roundtrip_yaml() {
        let r = ContractPartyRecord {
            id: "billing-team".into(),
            name: "Billing Team".into(),
            kind: PartyKindRecord::Team,
            avatar_seed: None,
            avatar_color: Some("#3B82F6".into()),
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        let back: ContractPartyRecord = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn party_record_deserialises_plain_string() {
        let r: ContractPartyRecord = serde_yaml::from_str("\"Billing Team\"").unwrap();
        assert_eq!(r.name, "Billing Team");
        assert_eq!(r.id, "billing-team");
        assert_eq!(r.kind, PartyKindRecord::Team);
    }

    #[test]
    fn party_record_yaml_uses_camel_case_field_names() {
        let r = ContractPartyRecord {
            id: "x".into(),
            name: "X".into(),
            kind: PartyKindRecord::Team,
            avatar_seed: Some("seed-v".into()),
            avatar_color: None,
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("avatarSeed"), "expected camelCase field, got:\n{yaml}");
        assert!(!yaml.contains("avatar_seed"), "snake_case must not leak into YAML, got:\n{yaml}");
    }

    #[test]
    fn domain_to_record_roundtrip() {
        let domain = ContractParty::from_name("Platform Team");
        let record: ContractPartyRecord = (&domain).into();
        let back: ContractParty = record.into();
        assert_eq!(domain, back);
    }
}

// ---------------------------------------------------------------------------
// ContractPolicyRecord + BreakingChangePolicyRecord
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractPolicyRecord {
    #[serde(default)]
    pub breaking_change_policy: BreakingChangePolicyRecord,
    #[serde(default = "default_notice_days_record")]
    pub notice_days: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_sla: Option<f32>,
}

impl Default for ContractPolicyRecord {
    fn default() -> Self {
        Self {
            breaking_change_policy: BreakingChangePolicyRecord::Lenient,
            notice_days: 30,
            uptime_sla: None,
        }
    }
}

fn default_notice_days_record() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BreakingChangePolicyRecord {
    Strict,
    #[default]
    Lenient,
    AdditiveOk,
}

impl From<&BreakingChangePolicy> for BreakingChangePolicyRecord {
    fn from(p: &BreakingChangePolicy) -> Self {
        match p {
            BreakingChangePolicy::Strict => BreakingChangePolicyRecord::Strict,
            BreakingChangePolicy::Lenient => BreakingChangePolicyRecord::Lenient,
            BreakingChangePolicy::AdditiveOk => BreakingChangePolicyRecord::AdditiveOk,
        }
    }
}

impl From<BreakingChangePolicyRecord> for BreakingChangePolicy {
    fn from(r: BreakingChangePolicyRecord) -> Self {
        match r {
            BreakingChangePolicyRecord::Strict => BreakingChangePolicy::Strict,
            BreakingChangePolicyRecord::Lenient => BreakingChangePolicy::Lenient,
            BreakingChangePolicyRecord::AdditiveOk => BreakingChangePolicy::AdditiveOk,
        }
    }
}

impl From<&ContractPolicy> for ContractPolicyRecord {
    fn from(p: &ContractPolicy) -> Self {
        Self {
            breaking_change_policy: (&p.breaking_change_policy).into(),
            notice_days: p.notice_days,
            uptime_sla: p.uptime_sla,
        }
    }
}

impl From<ContractPolicyRecord> for ContractPolicy {
    fn from(r: ContractPolicyRecord) -> Self {
        Self {
            breaking_change_policy: r.breaking_change_policy.into(),
            notice_days: r.notice_days,
            uptime_sla: r.uptime_sla,
        }
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn policy_record_defaults_from_empty_yaml() {
        let p: ContractPolicyRecord = serde_yaml::from_str("{}").unwrap();
        assert_eq!(p.breaking_change_policy, BreakingChangePolicyRecord::Lenient);
        assert_eq!(p.notice_days, 30);
        assert!(p.uptime_sla.is_none());
    }

    #[test]
    fn policy_record_roundtrip() {
        let p = ContractPolicyRecord {
            breaking_change_policy: BreakingChangePolicyRecord::Strict,
            notice_days: 14,
            uptime_sla: Some(99.9),
        };
        let yaml = serde_yaml::to_string(&p).unwrap();
        assert!(yaml.contains("breakingChangePolicy"), "got:\n{yaml}");
        let back: ContractPolicyRecord = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn breaking_policy_record_uses_snake_case() {
        let y = serde_yaml::to_string(&BreakingChangePolicyRecord::AdditiveOk).unwrap();
        assert!(y.contains("additive_ok"), "got: {y}");
    }

    #[test]
    fn domain_policy_record_roundtrip() {
        let domain = ContractPolicy {
            breaking_change_policy: BreakingChangePolicy::AdditiveOk,
            notice_days: 7,
            uptime_sla: Some(95.0),
        };
        let r: ContractPolicyRecord = (&domain).into();
        let back: ContractPolicy = r.into();
        assert_eq!(domain, back);
    }
}

// ---------------------------------------------------------------------------
// ContractScopeRecord
// ---------------------------------------------------------------------------

use rocket_collection::contract::types::ContractScope;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContractScopeRecord {
    Collection,
    Folder { rel_path: PathBuf },
    Request { rel_path: PathBuf },
}

impl From<&ContractScope> for ContractScopeRecord {
    fn from(s: &ContractScope) -> Self {
        match s {
            ContractScope::Collection => ContractScopeRecord::Collection,
            ContractScope::Folder { rel_path } => ContractScopeRecord::Folder { rel_path: rel_path.clone() },
            ContractScope::Request { rel_path } => ContractScopeRecord::Request { rel_path: rel_path.clone() },
        }
    }
}

impl From<ContractScopeRecord> for ContractScope {
    fn from(r: ContractScopeRecord) -> Self {
        match r {
            ContractScopeRecord::Collection => ContractScope::Collection,
            ContractScopeRecord::Folder { rel_path } => ContractScope::Folder { rel_path },
            ContractScopeRecord::Request { rel_path } => ContractScope::Request { rel_path },
        }
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[test]
    fn scope_record_folder_yaml_uses_snake_case_rel_path() {
        let s = ContractScopeRecord::Folder { rel_path: PathBuf::from("auth/login.yml") };
        let yaml = serde_yaml::to_string(&s).unwrap();
        assert!(yaml.contains("rel_path:"), "expected rel_path in:\n{yaml}");
        assert!(!yaml.contains("relPath:"), "camelCase relPath must NOT appear in:\n{yaml}");
    }

    #[test]
    fn scope_record_request_yaml_uses_snake_case_rel_path() {
        let s = ContractScopeRecord::Request { rel_path: PathBuf::from("users/get.yml") };
        let yaml = serde_yaml::to_string(&s).unwrap();
        assert!(yaml.contains("rel_path:"));
    }

    #[test]
    fn scope_record_collection_roundtrip() {
        let s = ContractScopeRecord::Collection;
        let yaml = serde_yaml::to_string(&s).unwrap();
        let back: ContractScopeRecord = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn domain_scope_record_roundtrip() {
        let domain = ContractScope::Folder { rel_path: PathBuf::from("a/b.yml") };
        let r: ContractScopeRecord = (&domain).into();
        let back: ContractScope = r.into();
        assert_eq!(domain, back);
    }
}

// ---------------------------------------------------------------------------
// ContractStatusRecord + ContractEnforcementModeRecord
// ---------------------------------------------------------------------------

use rocket_collection::contract::types::{ContractEnforcementMode, ContractStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatusRecord {
    Draft,
    #[default]
    Active,
    Drift,
    Breach,
    InReview,
    Paused,
    // Note: serde rename_all=snake_case produces "expiring_in30_days" (no underscore
    // before digits). This matches what the domain ContractStatus already writes to
    // disk, so we keep the default behaviour for byte-equivalent YAML roundtrip.
    ExpiringIn30Days,
    Expired,
}

impl From<&ContractStatus> for ContractStatusRecord {
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

impl From<ContractStatusRecord> for ContractStatus {
    fn from(r: ContractStatusRecord) -> Self {
        match r {
            ContractStatusRecord::Draft => Self::Draft,
            ContractStatusRecord::Active => Self::Active,
            ContractStatusRecord::Drift => Self::Drift,
            ContractStatusRecord::Breach => Self::Breach,
            ContractStatusRecord::InReview => Self::InReview,
            ContractStatusRecord::Paused => Self::Paused,
            ContractStatusRecord::ExpiringIn30Days => Self::ExpiringIn30Days,
            ContractStatusRecord::Expired => Self::Expired,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractEnforcementModeRecord {
    #[default]
    Informational,
    Warn,
    Block,
}

impl From<&ContractEnforcementMode> for ContractEnforcementModeRecord {
    fn from(e: &ContractEnforcementMode) -> Self {
        match e {
            ContractEnforcementMode::Informational => Self::Informational,
            ContractEnforcementMode::Warn => Self::Warn,
            ContractEnforcementMode::Block => Self::Block,
        }
    }
}

impl From<ContractEnforcementModeRecord> for ContractEnforcementMode {
    fn from(r: ContractEnforcementModeRecord) -> Self {
        match r {
            ContractEnforcementModeRecord::Informational => Self::Informational,
            ContractEnforcementModeRecord::Warn => Self::Warn,
            ContractEnforcementModeRecord::Block => Self::Block,
        }
    }
}

#[cfg(test)]
mod enum_tests {
    use super::*;

    #[test]
    fn status_record_active_serialises_unchanged() {
        let y = serde_yaml::to_string(&ContractStatusRecord::Active).unwrap();
        assert_eq!(y.trim(), "active");
    }

    #[test]
    fn status_record_in_review_uses_snake_case() {
        let y = serde_yaml::to_string(&ContractStatusRecord::InReview).unwrap();
        assert_eq!(y.trim(), "in_review");
    }

    #[test]
    fn status_record_expiring_in_30_days_matches_domain_byte_equivalent() {
        // serde's rename_all=snake_case produces "expiring_in30_days" (no underscore
        // before the digit). The domain ContractStatus does the same, so the Record
        // matches it exactly to keep on-disk YAML byte-equivalent.
        let y = serde_yaml::to_string(&ContractStatusRecord::ExpiringIn30Days).unwrap();
        assert_eq!(y.trim(), "expiring_in30_days");
    }

    #[test]
    fn enforcement_mode_record_roundtrip() {
        for m in [ContractEnforcementMode::Informational, ContractEnforcementMode::Warn, ContractEnforcementMode::Block] {
            let r: ContractEnforcementModeRecord = (&m).into();
            let back: ContractEnforcementMode = r.into();
            assert_eq!(m, back);
        }
    }
}
