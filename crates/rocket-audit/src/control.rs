use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framework {
    Soc2,
    Iso27001,
    Iso42001,
    CsaStar,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlId {
    pub framework: Framework,
    pub code: String,
    pub title: String,
}

pub static CONTROL_CATALOG: &[ControlEntry] = &[
    // SOC 2 Type 2 — Trust Services Criteria.
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC6.1",
        title: "Logical access controls",
        kinds: &[
            "sensitive_auth_used",
            "secret_variable_written",
            "network_device_paired",
            "network_device_unpaired",
            "network_device_blocked",
            "network_link_removed",
        ],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC6.7",
        title: "Transmission of sensitive information",
        kinds: &[
            "collection_exported",
            "audit_evidence_exported",
            "network_collection_sent",
            "network_collection_received",
            "network_sync_sent",
            "network_sync_received",
            "network_contract_check_sent",
            "network_contract_check_received",
        ],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC7.2",
        title: "System monitoring",
        kinds: &["contract_violation", "audit_chain_broken"],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC8.1",
        title: "Change management",
        kinds: &[
            "contract_attached",
            "contract_deleted",
            "contract_violation",
        ],
    },
    // ISO 27001:2022 — Annex A controls.
    ControlEntry {
        framework: Framework::Iso27001,
        code: "A.8.15",
        title: "Logging",
        kinds: &[
            "contract_violation",
            "sensitive_auth_used",
            "collection_deleted",
        ],
    },
    ControlEntry {
        framework: Framework::Iso27001,
        code: "A.8.16",
        title: "Monitoring activities",
        kinds: &["contract_violation", "audit_chain_broken"],
    },
    ControlEntry {
        framework: Framework::Iso27001,
        code: "A.5.15",
        title: "Access control",
        kinds: &["sensitive_auth_used", "secret_variable_written"],
    },
    // ISO/IEC 42001:2023 — AI management system controls.
    ControlEntry {
        framework: Framework::Iso42001,
        code: "A.6.2.2",
        title: "AI system impact assessment",
        kinds: &["contract_attached", "contract_violation"],
    },
    ControlEntry {
        framework: Framework::Iso42001,
        code: "A.7.4",
        title: "Data quality for AI systems",
        kinds: &["contract_violation"],
    },
    // CSA Cloud Controls Matrix (STAR).
    ControlEntry {
        framework: Framework::CsaStar,
        code: "IAM-09",
        title: "Identity & access management — user access review",
        kinds: &["sensitive_auth_used"],
    },
    ControlEntry {
        framework: Framework::CsaStar,
        code: "LOG-02",
        title: "Audit logs protection",
        kinds: &["audit_chain_broken", "audit_evidence_exported"],
    },
];

#[derive(Debug)]
pub struct ControlEntry {
    pub framework: Framework,
    pub code: &'static str,
    pub title: &'static str,
    pub kinds: &'static [&'static str],
}

/// Returns every `ControlId` tagged against a given event kind across all frameworks.
pub fn controls_for_kind(kind: &str) -> Vec<ControlId> {
    CONTROL_CATALOG
        .iter()
        .filter(|e| e.kinds.contains(&kind))
        .map(|e| ControlId {
            framework: e.framework,
            code: e.code.to_string(),
            title: e.title.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_soc2_access_control() {
        assert!(CONTROL_CATALOG
            .iter()
            .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.1"));
    }

    #[test]
    fn catalog_contains_iso27001_a_8_15() {
        assert!(CONTROL_CATALOG
            .iter()
            .any(|c| c.framework == Framework::Iso27001 && c.code == "A.8.15"));
    }

    #[test]
    fn catalog_contains_iso42001_ai_lifecycle() {
        assert!(CONTROL_CATALOG
            .iter()
            .any(|c| c.framework == Framework::Iso42001 && c.code == "A.6.2.2"));
    }

    #[test]
    fn catalog_contains_csa_star_iam() {
        assert!(CONTROL_CATALOG
            .iter()
            .any(|c| c.framework == Framework::CsaStar && c.code == "IAM-09"));
    }

    #[test]
    fn framework_serializes_snake_case() {
        let s = serde_json::to_string(&Framework::Soc2).unwrap();
        assert_eq!(s, "\"soc2\"");
    }

    #[test]
    fn cc6_1_lists_the_network_kinds() {
        for tag in [
            "network_device_paired",
            "network_device_unpaired",
            "network_device_blocked",
        ] {
            let controls = controls_for_kind(tag);
            assert!(
                controls
                    .iter()
                    .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.1"),
                "{tag} missing from CC6.1"
            );
        }
    }

    #[test]
    fn cc6_7_lists_the_send_collection_kinds_and_keeps_the_old_ones() {
        for tag in [
            "collection_exported",
            "audit_evidence_exported",
            "network_collection_sent",
            "network_collection_received",
        ] {
            let controls = controls_for_kind(tag);
            assert!(
                controls
                    .iter()
                    .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.7"),
                "{tag} missing from CC6.7"
            );
        }
    }

    #[test]
    fn the_sync_kinds_are_listed_in_the_catalog() {
        for tag in ["network_sync_sent", "network_sync_received"] {
            let controls = controls_for_kind(tag);
            assert!(
                controls
                    .iter()
                    .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.7"),
                "{tag} missing from CC6.7"
            );
        }
        let controls = controls_for_kind("network_link_removed");
        assert!(
            controls
                .iter()
                .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.1"),
            "network_link_removed missing from CC6.1"
        );
    }

    #[test]
    fn the_contract_check_kinds_are_listed_in_the_catalog() {
        for tag in [
            "network_contract_check_sent",
            "network_contract_check_received",
        ] {
            let controls = controls_for_kind(tag);
            assert!(
                controls
                    .iter()
                    .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.7"),
                "{tag} missing from CC6.7"
            );
            assert!(
                !controls
                    .iter()
                    .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.1"),
                "{tag} must not be in CC6.1"
            );
        }
    }
}
