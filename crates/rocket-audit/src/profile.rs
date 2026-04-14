use crate::control::Framework;
use rocket_shared::error::DomainResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementLevel {
    /// Events are recorded only.
    Record,
    /// Events are recorded and the user is warned inline.
    Warn,
    /// Events are recorded and the triggering operation is blocked.
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceProfile {
    pub active_frameworks: BTreeSet<Framework>,
    pub enforcement: EnforcementLevel,
    /// When empty, all kinds matched by active_frameworks are recorded.
    #[serde(default)]
    pub muted_kinds: BTreeSet<String>,
}

pub fn default_profile() -> ComplianceProfile {
    ComplianceProfile {
        active_frameworks: BTreeSet::new(),
        enforcement: EnforcementLevel::Record,
        muted_kinds: BTreeSet::new(),
    }
}

impl ComplianceProfile {
    /// Returns true when an event kind is active under the current profile.
    pub fn records(&self, event_kind: &str) -> bool {
        if self.muted_kinds.contains(event_kind) {
            return false;
        }
        if self.active_frameworks.is_empty() {
            // No frameworks selected = still record everything (audit is always on).
            return true;
        }
        crate::control::CONTROL_CATALOG.iter().any(|e| {
            self.active_frameworks.contains(&e.framework) && e.kinds.contains(&event_kind)
        })
    }
}

pub trait ProfileRepository: Send + Sync {
    fn load(&self) -> DomainResult<ComplianceProfile>;
    fn save(&self, profile: &ComplianceProfile) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_has_no_frameworks() {
        assert!(default_profile().active_frameworks.is_empty());
    }

    #[test]
    fn records_when_no_frameworks_selected() {
        assert!(default_profile().records("contract_violation"));
    }

    #[test]
    fn records_only_kinds_matching_active_framework() {
        let mut p = default_profile();
        p.active_frameworks.insert(Framework::Soc2);
        assert!(p.records("contract_violation"));
        assert!(!p.records("made_up_kind"));
    }

    #[test]
    fn muted_kinds_are_never_recorded() {
        let mut p = default_profile();
        p.muted_kinds.insert("collection_deleted".into());
        assert!(!p.records("collection_deleted"));
    }
}
