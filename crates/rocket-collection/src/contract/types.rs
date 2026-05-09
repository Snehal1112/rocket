use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

/// A party (provider or consumer) in a contract.
/// Backward-compat: deserialises from a plain YAML string (old format)
/// or a full object (new format).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractParty {
    pub id: String,
    pub name: String,
    pub kind: PartyKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

/// Custom Deserialize: accepts both a plain string ("Billing Team") and
/// a full object ({id: ..., name: ..., kind: ...}).
impl<'de> serde::Deserialize<'de> for ContractParty {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct ContractPartyVisitor;

        impl<'de> Visitor<'de> for ContractPartyVisitor {
            type Value = ContractParty;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a string or a ContractParty object")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<ContractParty, E> {
                Ok(ContractParty::from_name(v))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<ContractParty, E> {
                Ok(ContractParty::from_name(&v))
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<ContractParty, A::Error> {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct ContractPartyHelper {
                    id: String,
                    name: String,
                    #[serde(default)]
                    kind: PartyKind,
                    #[serde(default)]
                    avatar_seed: Option<String>,
                    #[serde(default)]
                    avatar_color: Option<String>,
                }
                let h = ContractPartyHelper::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?;
                Ok(ContractParty {
                    id: h.id,
                    name: h.name,
                    kind: h.kind,
                    avatar_seed: h.avatar_seed,
                    avatar_color: h.avatar_color,
                })
            }
        }

        d.deserialize_any(ContractPartyVisitor)
    }
}

/// What kind of entity a party represents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PartyKind {
    #[default]
    Team,
    Company,
    Service,
}

/// Policy governing how contract drift is evaluated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractPolicy {
    #[serde(default)]
    pub breaking_change_policy: BreakingChangePolicy,
    #[serde(default = "default_notice_days")]
    pub notice_days: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

fn default_notice_days() -> u32 {
    30
}

/// How strictly drift is classified as breaking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BreakingChangePolicy {
    Strict,
    #[default]
    Lenient,
    AdditiveOk,
}

/// A contract between a provider and one or more consumers.
///
/// camelCase is intentional: this type serves as both the on-disk YAML format
/// and the Tauri IPC wire type. A separate DTO split is tracked as future work.
///
/// Backward compat: `provider` accepts both a plain string and an object.
/// `consumer` (singular, old format) is also accepted and mapped to `consumers`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub id: Ulid,
    pub title: String,

    pub provider: ContractParty,
    pub consumers: Vec<ContractParty>,

    pub project: String,

    #[serde(default = "default_version")]
    pub version: String,

    #[serde(default)]
    pub status: ContractStatus,

    pub effective_date: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<NaiveDate>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_paths: Vec<PathBuf>,

    pub enforcement_mode: ContractEnforcementMode,
    pub scope: ContractScope,

    #[serde(default)]
    pub policy: ContractPolicy,

    #[serde(default)]
    pub drift_count: u32,
    #[serde(default)]
    pub breach_count: u32,
    #[serde(default)]
    pub endpoint_count: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// Custom Deserialize for `Contract` handles the old `consumer: String`
/// field (singular) and maps it to `consumers: Vec<ContractParty>`.
impl<'de> serde::Deserialize<'de> for Contract {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{MapAccess, Visitor};
        use std::fmt;

        struct ContractVisitor;

        impl<'de> Visitor<'de> for ContractVisitor {
            type Value = Contract;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a Contract object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Contract, A::Error> {
                use serde::de::Error;

                let mut id: Option<Ulid> = None;
                let mut title: Option<String> = None;
                let mut provider: Option<ContractParty> = None;
                let mut consumers: Option<Vec<ContractParty>> = None;
                let mut consumer_singular: Option<ContractParty> = None;
                let mut project: Option<String> = None;
                let mut version: Option<String> = None;
                let mut status: Option<ContractStatus> = None;
                let mut effective_date: Option<NaiveDate> = None;
                let mut expiry_date: Option<NaiveDate> = None;
                let mut document_paths: Option<Vec<PathBuf>> = None;
                let mut enforcement_mode: Option<ContractEnforcementMode> = None;
                let mut scope: Option<ContractScope> = None;
                let mut policy: Option<ContractPolicy> = None;
                let mut drift_count: Option<u32> = None;
                let mut breach_count: Option<u32> = None;
                let mut endpoint_count: Option<u32> = None;
                let mut created_by: Option<String> = None;
                let mut created_at: Option<chrono::DateTime<chrono::Utc>> = None;
                let mut updated_at: Option<chrono::DateTime<chrono::Utc>> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => id = Some(map.next_value()?),
                        "title" => title = Some(map.next_value()?),
                        "provider" => provider = Some(map.next_value()?),
                        "consumers" => consumers = Some(map.next_value()?),
                        "consumer" => consumer_singular = Some(map.next_value()?),
                        "project" => project = Some(map.next_value()?),
                        "version" => version = Some(map.next_value()?),
                        "status" => status = Some(map.next_value()?),
                        "effectiveDate" => effective_date = Some(map.next_value()?),
                        // Deserialize as Option<NaiveDate> directly: YAML `null`
                        // maps to None rather than causing a parse error.
                        "expiryDate" => expiry_date = map.next_value()?,
                        "documentPaths" => document_paths = Some(map.next_value()?),
                        "enforcementMode" => enforcement_mode = Some(map.next_value()?),
                        "scope" => scope = Some(map.next_value()?),
                        "policy" => policy = Some(map.next_value()?),
                        "driftCount" => drift_count = Some(map.next_value()?),
                        "breachCount" => breach_count = Some(map.next_value()?),
                        "endpointCount" => endpoint_count = Some(map.next_value()?),
                        "createdBy" => created_by = map.next_value()?,
                        "createdAt" => created_at = map.next_value()?,
                        "updatedAt" => updated_at = map.next_value()?,
                        _ => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }

                let resolved_consumers = consumers
                    .or_else(|| consumer_singular.map(|c| vec![c]))
                    .unwrap_or_default();

                Ok(Contract {
                    id: id.ok_or_else(|| A::Error::missing_field("id"))?,
                    title: title.ok_or_else(|| A::Error::missing_field("title"))?,
                    provider: provider.ok_or_else(|| A::Error::missing_field("provider"))?,
                    consumers: resolved_consumers,
                    project: project.unwrap_or_default(),
                    version: version.unwrap_or_else(default_version),
                    status: status.unwrap_or_default(),
                    effective_date: effective_date
                        .ok_or_else(|| A::Error::missing_field("effectiveDate"))?,
                    expiry_date,
                    document_paths: document_paths.unwrap_or_default(),
                    enforcement_mode: enforcement_mode.unwrap_or_default(),
                    scope: scope.ok_or_else(|| A::Error::missing_field("scope"))?,
                    policy: policy.unwrap_or_default(),
                    drift_count: drift_count.unwrap_or(0),
                    breach_count: breach_count.unwrap_or(0),
                    endpoint_count: endpoint_count.unwrap_or(0),
                    created_by,
                    created_at,
                    updated_at,
                })
            }
        }

        d.deserialize_map(ContractVisitor)
    }
}

/// Lifecycle status of a contract. Stored explicitly in YAML.
///
/// Backward compat: `active` and `expired` retain their serialised values.
/// `expiring_in_30_days` is stored but also recomputed on load if expiry
/// is approaching, so the stored value may lag by one session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractEnforcementMode {
    #[default]
    Informational,
    Warn,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContractScope {
    Collection,
    Folder { rel_path: PathBuf },
    Request { rel_path: PathBuf },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_party_roundtrip() {
        let party = ContractParty {
            id: "billing-team".into(),
            name: "Billing Team".into(),
            kind: PartyKind::Team,
            avatar_seed: None,
            avatar_color: Some("#3B82F6".into()),
        };
        let yaml = serde_yaml::to_string(&party).unwrap();
        let back: ContractParty = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(party, back);
    }

    #[test]
    fn contract_policy_defaults() {
        let policy: ContractPolicy = serde_yaml::from_str("{}").unwrap();
        assert_eq!(policy.breaking_change_policy, BreakingChangePolicy::Lenient);
        assert_eq!(policy.notice_days, 30);
        assert!(policy.uptime_sla.is_none());
    }

    #[test]
    fn breaking_change_policy_snake_case() {
        let y = serde_yaml::to_string(&BreakingChangePolicy::AdditiveOk).unwrap();
        assert!(y.contains("additive_ok"));
    }

    #[test]
    fn contract_scope_folder_serializes_rel_path_as_snake_case() {
        let scope = ContractScope::Folder { rel_path: PathBuf::from("auth/login.yml") };
        let yaml = serde_yaml::to_string(&scope).unwrap();
        // rel_path must stay snake_case — the frontend wire type uses rel_path.
        assert!(yaml.contains("rel_path:"), "expected rel_path in:\n{yaml}");
        assert!(!yaml.contains("relPath:"), "camelCase relPath must not appear in:\n{yaml}");
    }

    #[test]
    fn contract_scope_request_serializes_rel_path_as_snake_case() {
        let scope = ContractScope::Request { rel_path: PathBuf::from("users/get.yml") };
        let yaml = serde_yaml::to_string(&scope).unwrap();
        assert!(yaml.contains("rel_path:"), "expected rel_path in:\n{yaml}");
        assert!(!yaml.contains("relPath:"), "camelCase relPath must not appear in:\n{yaml}");
    }

    #[test]
    fn contract_scope_folder_roundtrips() {
        let scope = ContractScope::Folder { rel_path: PathBuf::from("auth/login.yml") };
        let yaml = serde_yaml::to_string(&scope).unwrap();
        let back: ContractScope = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(scope, back);
    }

    #[test]
    fn contract_scope_collection_roundtrips() {
        let scope = ContractScope::Collection;
        let yaml = serde_yaml::to_string(&scope).unwrap();
        let back: ContractScope = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(scope, back);
    }

    #[test]
    fn old_yaml_provider_string_deserialises() {
        let yaml = r#"
id: 01ARZ3NDEKTSV4RRFFQ69G5FAV
title: Payments API
provider: "Billing Team"
consumer: "Platform Team"
project: Checkout
version: "1.0.0"
effectiveDate: "2026-01-15"
enforcementMode: informational
scope:
  type: collection
"#;
        let c: Contract = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(c.provider.name, "Billing Team");
        assert_eq!(c.provider.id, "billing-team");
        assert_eq!(c.consumers.len(), 1);
        assert_eq!(c.consumers[0].name, "Platform Team");
        assert_eq!(c.status, ContractStatus::Active);
        assert_eq!(c.version, "1.0.0");
    }

    #[test]
    fn new_yaml_consumers_vec_deserialises() {
        let yaml = r#"
id: 01ARZ3NDEKTSV4RRFFQ69G5FAV
title: Payments API
provider:
  id: billing-team
  name: Billing Team
  kind: team
consumers:
  - id: platform-team
    name: Platform Team
    kind: team
  - id: mobile-team
    name: Mobile Team
    kind: team
project: Checkout
version: "2.0.0"
status: drift
effectiveDate: "2026-01-15"
enforcementMode: informational
scope:
  type: collection
policy:
  breakingChangePolicy: strict
  noticeDays: 14
driftCount: 3
breachCount: 1
"#;
        let c: Contract = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(c.consumers.len(), 2);
        assert_eq!(c.status, ContractStatus::Drift);
        assert_eq!(c.drift_count, 3);
        assert_eq!(c.policy.breaking_change_policy, BreakingChangePolicy::Strict);
    }

    #[test]
    fn new_status_variants_roundtrip() {
        for status in [
            ContractStatus::Draft,
            ContractStatus::Drift,
            ContractStatus::Breach,
            ContractStatus::InReview,
            ContractStatus::Paused,
        ] {
            let yaml = serde_yaml::to_string(&status).unwrap();
            let back: ContractStatus = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn existing_status_values_unchanged() {
        let active = serde_yaml::to_string(&ContractStatus::Active).unwrap();
        let expired = serde_yaml::to_string(&ContractStatus::Expired).unwrap();
        assert!(active.trim() == "active");
        assert!(expired.trim() == "expired");
    }

    #[test]
    fn old_yaml_expiry_date_null_deserialises() {
        // Reproduces the exact warning seen in production:
        //   "expiryDate: input contains invalid characters at line 8 column 13"
        // Old serializers wrote `expiryDate: null` explicitly; the custom
        // Deserialize visitor must map YAML null to None, not try to parse
        // the word "null" as a NaiveDate.
        let yaml = r#"
id: 01KR4KAPFKR5YM1T1WSD2YYGC0
title: Jjjjj
provider: Jhggg
consumer: Yyyy
project: Uuuu
version: V222
effectiveDate: 2026-05-08
expiryDate: null
documentPaths: []
enforcementMode: informational
scope:
  type: collection
"#;
        let c: Contract = serde_yaml::from_str(yaml).unwrap();
        assert!(c.expiry_date.is_none(), "expiryDate: null should deserialise to None");
    }

    #[test]
    fn old_yaml_created_at_null_deserialises() {
        // Same latent bug for createdBy/createdAt/updatedAt nullable fields.
        let yaml = r#"
id: 01ARZ3NDEKTSV4RRFFQ69G5FAV
title: Test
provider: Provider
consumer: Consumer
project: ''
version: "1.0.0"
effectiveDate: "2026-01-15"
enforcementMode: informational
scope:
  type: collection
createdBy: null
createdAt: null
updatedAt: null
"#;
        let c: Contract = serde_yaml::from_str(yaml).unwrap();
        assert!(c.created_by.is_none());
        assert!(c.created_at.is_none());
        assert!(c.updated_at.is_none());
    }
}
