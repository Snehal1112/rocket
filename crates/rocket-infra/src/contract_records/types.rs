//! Persistence records for `Contract` and its sub-types (`ContractParty`,
//! `ContractPolicy`, `ContractScope`, plus the four enums).

use rocket_collection::contract::types::{ContractParty, PartyKind};
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
