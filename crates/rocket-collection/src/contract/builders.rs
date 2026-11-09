use super::types::{
    Contract, ContractEnforcementMode, ContractParty, ContractPolicy, ContractScope, ContractStatus,
};
use ulid::Ulid;

/// What a person enters to create a contract. The service fills in the rest.
#[derive(Debug, Clone)]
pub struct NewContract {
    pub title: String,
    pub provider: ContractParty,
    pub consumers: Vec<ContractParty>,
    pub version: String,
    pub effective_date: chrono::NaiveDate,
    pub expiry_date: Option<chrono::NaiveDate>,
    pub scope: ContractScope,
    pub policy: ContractPolicy,
    /// `true` makes the contract `Active` with a baseline snapshot, `false` leaves a `Draft`.
    pub publish_immediately: bool,
}

impl NewContract {
    /// The contract before it is stored. `attach_contract` sets the final id, the
    /// timestamps, the document paths and `endpoint_count`.
    pub fn into_contract(self) -> Contract {
        let status = if self.publish_immediately {
            ContractStatus::Active
        } else {
            ContractStatus::Draft
        };
        Contract {
            id: Ulid::new(),
            title: self.title,
            provider: self.provider,
            consumers: self.consumers,
            // Superseded by the party identities; kept for older files.
            project: String::new(),
            version: self.version,
            status,
            effective_date: self.effective_date,
            expiry_date: self.expiry_date,
            document_paths: vec![],
            enforcement_mode: ContractEnforcementMode::Informational,
            scope: self.scope,
            policy: self.policy,
            drift_count: 0,
            breach_count: 0,
            endpoint_count: 0,
            created_by: None,
            created_at: None,
            updated_at: None,
        }
    }
}

/// The fields a person can change on a stored contract. Scope, status, counters and
/// creation data are not part of an edit.
#[derive(Debug, Clone)]
pub struct ContractEdit {
    pub title: String,
    pub provider: ContractParty,
    pub consumers: Vec<ContractParty>,
    pub version: String,
    pub effective_date: chrono::NaiveDate,
    pub expiry_date: Option<chrono::NaiveDate>,
    pub policy: ContractPolicy,
}

impl ContractEdit {
    /// `existing` with the edited fields replaced.
    pub fn apply_to(self, existing: Contract) -> Contract {
        Contract {
            title: self.title,
            provider: self.provider,
            consumers: self.consumers,
            version: self.version,
            effective_date: self.effective_date,
            expiry_date: self.expiry_date,
            policy: self.policy,
            ..existing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;

    fn new_contract(publish_immediately: bool) -> NewContract {
        NewContract {
            title: "Orders API".into(),
            provider: ContractParty::from_name("Team A"),
            consumers: vec![ContractParty::from_name("Team B")],
            version: "v1.0".into(),
            effective_date: Utc::now().date_naive(),
            expiry_date: None,
            scope: ContractScope::Collection,
            policy: ContractPolicy::default(),
            publish_immediately,
        }
    }

    fn contract_edit(title: &str) -> ContractEdit {
        ContractEdit {
            title: title.into(),
            provider: ContractParty::from_name("Team C"),
            consumers: vec![],
            version: "v2.0".into(),
            effective_date: Utc::now().date_naive(),
            expiry_date: None,
            policy: ContractPolicy {
                notice_days: 30,
                ..ContractPolicy::default()
            },
        }
    }

    #[test]
    fn into_contract_publish_immediately_is_active() {
        assert_eq!(
            new_contract(true).into_contract().status,
            ContractStatus::Active
        );
    }

    #[test]
    fn into_contract_otherwise_is_draft() {
        assert_eq!(
            new_contract(false).into_contract().status,
            ContractStatus::Draft
        );
    }

    #[test]
    fn into_contract_starts_with_zero_counters_and_informational_mode() {
        let c = new_contract(true).into_contract();
        assert_eq!(c.enforcement_mode, ContractEnforcementMode::Informational);
        assert_eq!((c.drift_count, c.breach_count, c.endpoint_count), (0, 0, 0));
        assert!(c.project.is_empty());
        assert!(c.document_paths.is_empty());
        assert!(c.created_by.is_none() && c.created_at.is_none() && c.updated_at.is_none());
    }

    #[test]
    fn apply_to_keeps_scope_status_counters_and_creation_fields() {
        let mut existing = new_contract(true).into_contract();
        existing.status = ContractStatus::Paused;
        existing.scope = ContractScope::Folder {
            rel_path: PathBuf::from("auth"),
        };
        existing.drift_count = 3;
        existing.breach_count = 1;
        existing.endpoint_count = 7;
        existing.project = "Project X".into();
        existing.created_by = Some("me".into());
        let id = existing.id;

        let edited = contract_edit("Renamed").apply_to(existing);

        assert_eq!(edited.id, id);
        assert_eq!(edited.status, ContractStatus::Paused);
        assert_eq!(
            edited.scope,
            ContractScope::Folder {
                rel_path: PathBuf::from("auth")
            }
        );
        assert_eq!(
            (
                edited.drift_count,
                edited.breach_count,
                edited.endpoint_count
            ),
            (3, 1, 7)
        );
        assert_eq!(edited.project, "Project X");
        assert_eq!(edited.created_by.as_deref(), Some("me"));
        assert_eq!(edited.title, "Renamed");
        assert_eq!(edited.version, "v2.0");
        assert_eq!(edited.policy.notice_days, 30);
        assert!(edited.consumers.is_empty());
    }
}
