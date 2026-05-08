use chrono::{NaiveDate, Utc};
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

// camelCase is intentional: this type serves as both the on-disk YAML format
// and the Tauri IPC wire type. A separate DTO split is tracked as future work.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub id: Ulid,
    pub title: String,
    pub provider: String,
    pub consumer: String,
    pub project: String,
    pub version: String,
    pub effective_date: NaiveDate,
    pub expiry_date: Option<NaiveDate>,
    /// Relative paths to attachment files stored inside the collection.
    /// Stored under `.rocket/contracts/attachments/<id>/`.
    /// `default` handles old YAML files that pre-date this field.
    #[serde(default)]
    pub document_paths: Vec<PathBuf>,
    pub enforcement_mode: ContractEnforcementMode,
    pub scope: ContractScope,
}

impl Contract {
    pub fn status(&self) -> ContractStatus {
        let today = Utc::now().date_naive();
        match self.expiry_date {
            None => ContractStatus::Active,
            Some(exp) if exp < today => ContractStatus::Expired,
            Some(exp) if (exp - today).num_days() <= 30 => ContractStatus::ExpiringIn30Days,
            _ => ContractStatus::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Active,
    ExpiringIn30Days,
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
}
