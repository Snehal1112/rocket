//! Changes the workspace assistant proposes. A proposal is only data. It is
//! applied by `rocket-app`'s `ProposalService` after the user accepts it, so
//! this module has no I/O and no dependency on other domain crates.

use rocket_shared::types::{Body, Header, HttpMethod, QueryParam};

/// The script field an `EditScript` change targets. `rocket-app` maps it to
/// `rocket_collection::RequestScriptPhase`, because this crate must not
/// depend on `rocket-collection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptPhase {
    PreRequest,
    PostResponse,
    Tests,
}

impl ScriptPhase {
    /// Short label used in proposal summaries.
    pub fn label(self) -> &'static str {
        match self {
            ScriptPhase::PreRequest => "pre-request",
            ScriptPhase::PostResponse => "post-response",
            ScriptPhase::Tests => "tests",
        }
    }
}

/// A new HTTP request the agent wants to create. There is no auth field on
/// purpose: a new request inherits auth from its folder or collection.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedRequest {
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub query_params: Vec<QueryParam>,
    pub body: Option<Body>,
    pub docs: Option<String>,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    pub tests: Option<String>,
}

/// A partial update of an existing HTTP request. `None` keeps a field as it
/// is. Auth cannot be patched in v1.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RequestPatch {
    pub method: Option<HttpMethod>,
    pub url: Option<String>,
    pub headers: Option<Vec<Header>>,
    pub query_params: Option<Vec<QueryParam>>,
    pub body: Option<Body>,
    pub docs: Option<String>,
}

impl RequestPatch {
    /// Names of the fields this patch sets, in a fixed order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        let mut fields = Vec::new();
        if self.method.is_some() {
            fields.push("method");
        }
        if self.url.is_some() {
            fields.push("url");
        }
        if self.headers.is_some() {
            fields.push("headers");
        }
        if self.query_params.is_some() {
            fields.push("query params");
        }
        if self.body.is_some() {
            fields.push("body");
        }
        if self.docs.is_some() {
            fields.push("docs");
        }
        fields
    }

    /// True when the patch sets no field.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }
}

/// One proposed operation. Paths are relative to the collection root, and
/// `""` is the root. Update-style changes carry `base_fingerprint`, the hash
/// of the item's stored form when the change was proposed.
#[derive(Debug, Clone, PartialEq)]
pub enum ProposedChange {
    CreateFolder {
        collection: String,
        parent_path: String,
        name: String,
    },
    CreateRequest {
        collection: String,
        folder_path: String,
        request: ProposedRequest,
    },
    UpdateRequest {
        collection: String,
        request_path: String,
        patch: RequestPatch,
        base_fingerprint: String,
    },
    EditScript {
        collection: String,
        request_path: String,
        phase: ScriptPhase,
        body: String,
        base_fingerprint: String,
    },
    MoveItem {
        collection: String,
        from_path: String,
        to_folder: String,
        base_fingerprint: String,
    },
    RenameItem {
        collection: String,
        path: String,
        new_name: String,
        base_fingerprint: String,
    },
    SetEnvVar {
        collection: String,
        environment: String,
        key: String,
        value: String,
    },
}

impl ProposedChange {
    /// The collection this change targets.
    pub fn collection(&self) -> &str {
        match self {
            ProposedChange::CreateFolder { collection, .. }
            | ProposedChange::CreateRequest { collection, .. }
            | ProposedChange::UpdateRequest { collection, .. }
            | ProposedChange::EditScript { collection, .. }
            | ProposedChange::MoveItem { collection, .. }
            | ProposedChange::RenameItem { collection, .. }
            | ProposedChange::SetEnvVar { collection, .. } => collection,
        }
    }

    /// The fingerprint of an update-style change, `None` for the others.
    pub fn base_fingerprint(&self) -> Option<&str> {
        match self {
            ProposedChange::UpdateRequest {
                base_fingerprint, ..
            }
            | ProposedChange::EditScript {
                base_fingerprint, ..
            }
            | ProposedChange::MoveItem {
                base_fingerprint, ..
            }
            | ProposedChange::RenameItem {
                base_fingerprint, ..
            } => Some(base_fingerprint),
            ProposedChange::CreateFolder { .. }
            | ProposedChange::CreateRequest { .. }
            | ProposedChange::SetEnvVar { .. } => None,
        }
    }

    /// Sets the fingerprint of an update-style change. Creates and
    /// environment changes have none, so the call does nothing for them.
    pub fn set_base_fingerprint(&mut self, fingerprint: String) {
        match self {
            ProposedChange::UpdateRequest {
                base_fingerprint, ..
            }
            | ProposedChange::EditScript {
                base_fingerprint, ..
            }
            | ProposedChange::MoveItem {
                base_fingerprint, ..
            }
            | ProposedChange::RenameItem {
                base_fingerprint, ..
            } => *base_fingerprint = fingerprint,
            ProposedChange::CreateFolder { .. }
            | ProposedChange::CreateRequest { .. }
            | ProposedChange::SetEnvVar { .. } => {}
        }
    }

    /// One line for the proposal card and the agent. It never contains a
    /// script body, a header value or a variable value.
    pub fn summary(&self) -> String {
        match self {
            ProposedChange::CreateFolder {
                collection,
                parent_path,
                name,
            } => format!(
                "Create folder '{}' in {collection}",
                display_path(parent_path, name)
            ),
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => format!(
                "Create request {} '{}' in {collection}",
                request.method,
                display_path(folder_path, &request.name)
            ),
            ProposedChange::UpdateRequest {
                collection,
                request_path,
                patch,
                ..
            } => format!(
                "Update {} of '{request_path}' in {collection}",
                patch.changed_fields().join(", ")
            ),
            ProposedChange::EditScript {
                collection,
                request_path,
                phase,
                ..
            } => format!(
                "Edit the {} script of '{request_path}' in {collection}",
                phase.label()
            ),
            ProposedChange::MoveItem {
                collection,
                from_path,
                to_folder,
                ..
            } => {
                let target = match to_folder.trim_matches('/') {
                    "" => "the collection root".to_string(),
                    folder => format!("'{folder}'"),
                };
                format!("Move '{from_path}' to {target} in {collection}")
            }
            ProposedChange::RenameItem {
                collection,
                path,
                new_name,
                ..
            } => format!("Rename '{path}' to '{new_name}' in {collection}"),
            ProposedChange::SetEnvVar {
                collection,
                environment,
                key,
                ..
            } => format!("Set variable '{key}' in environment '{environment}' of {collection}"),
        }
    }
}

/// `parent/name`, or just `name` at the collection root.
fn display_path(parent: &str, name: &str) -> String {
    match parent.trim_matches('/') {
        "" => name.to_string(),
        parent => format!("{parent}/{name}"),
    }
}

/// Where a proposal stands. Only a `Pending` proposal can be accepted or
/// rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalStatus {
    Pending,
    Accepted,
    Rejected,
    /// The target changed after the proposal was made. Nothing was written.
    Stale,
    /// Applying failed. Nothing was written.
    Failed {
        message: String,
    },
}

impl ProposalStatus {
    /// The wire name used in events, DTOs and tool results.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProposalStatus::Pending => "pending",
            ProposalStatus::Accepted => "accepted",
            ProposalStatus::Rejected => "rejected",
            ProposalStatus::Stale => "stale",
            ProposalStatus::Failed { .. } => "failed",
        }
    }

    /// The failure message, if the proposal failed.
    pub fn message(&self) -> Option<&str> {
        match self {
            ProposalStatus::Failed { message } => Some(message),
            _ => None,
        }
    }
}

/// One proposed change and its state, owned by one assistant session.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentProposal {
    pub id: String,
    pub session_id: String,
    pub change: ProposedChange,
    pub summary: String,
    pub status: ProposalStatus,
    pub created_at_ms: i64,
}

impl AgentProposal {
    /// A new pending proposal. The summary is taken from the change.
    pub fn new(id: String, session_id: String, change: ProposedChange, created_at_ms: i64) -> Self {
        let summary = change.summary();
        Self {
            id,
            session_id,
            change,
            summary,
            status: ProposalStatus::Pending,
            created_at_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit_script() -> ProposedChange {
        ProposedChange::EditScript {
            collection: "demo".into(),
            request_path: "users/get-users.yml".into(),
            phase: ScriptPhase::Tests,
            body: "rok.test('secret-body-text', () => {});".into(),
            base_fingerprint: String::new(),
        }
    }

    #[test]
    fn a_new_proposal_is_pending_and_carries_the_change_summary() {
        let proposal = AgentProposal::new("p1".into(), "s1".into(), edit_script(), 42);
        assert_eq!(proposal.status, ProposalStatus::Pending);
        assert_eq!(proposal.created_at_ms, 42);
        assert_eq!(
            proposal.summary,
            "Edit the tests script of 'users/get-users.yml' in demo"
        );
    }

    #[test]
    fn summaries_never_contain_script_bodies_or_variable_values() {
        let set_var = ProposedChange::SetEnvVar {
            collection: "demo".into(),
            environment: "dev".into(),
            key: "HOST".into(),
            value: "value-that-must-not-leak".into(),
        };
        assert!(!set_var.summary().contains("value-that-must-not-leak"));
        assert!(!edit_script().summary().contains("secret-body-text"));
        assert_eq!(
            set_var.summary(),
            "Set variable 'HOST' in environment 'dev' of demo"
        );
    }

    #[test]
    fn set_base_fingerprint_only_touches_update_style_changes() {
        let mut edit = edit_script();
        edit.set_base_fingerprint("abc".into());
        assert_eq!(edit.base_fingerprint(), Some("abc"));

        let mut create = ProposedChange::CreateFolder {
            collection: "demo".into(),
            parent_path: String::new(),
            name: "reports".into(),
        };
        create.set_base_fingerprint("abc".into());
        assert_eq!(create.base_fingerprint(), None);
        assert_eq!(create.collection(), "demo");
        assert_eq!(create.summary(), "Create folder 'reports' in demo");
    }

    #[test]
    fn a_patch_lists_the_fields_it_sets_in_a_fixed_order() {
        assert!(RequestPatch::default().is_empty());
        let patch = RequestPatch {
            url: Some("https://x".into()),
            method: Some(HttpMethod::Post),
            ..RequestPatch::default()
        };
        assert!(!patch.is_empty());
        assert_eq!(patch.changed_fields(), vec!["method", "url"]);
    }

    #[test]
    fn move_to_the_root_names_the_root() {
        let change = ProposedChange::MoveItem {
            collection: "demo".into(),
            from_path: "old/a.yml".into(),
            to_folder: String::new(),
            base_fingerprint: String::new(),
        };
        assert_eq!(
            change.summary(),
            "Move 'old/a.yml' to the collection root in demo"
        );
    }

    #[test]
    fn status_names_are_stable_wire_strings() {
        assert_eq!(ProposalStatus::Pending.as_str(), "pending");
        assert_eq!(ProposalStatus::Accepted.as_str(), "accepted");
        assert_eq!(ProposalStatus::Rejected.as_str(), "rejected");
        assert_eq!(ProposalStatus::Stale.as_str(), "stale");
        let failed = ProposalStatus::Failed {
            message: "disk full".into(),
        };
        assert_eq!(failed.as_str(), "failed");
        assert_eq!(failed.message(), Some("disk full"));
        assert_eq!(ProposalStatus::Stale.message(), None);
    }

    #[test]
    fn script_phase_labels() {
        assert_eq!(ScriptPhase::PreRequest.label(), "pre-request");
        assert_eq!(ScriptPhase::PostResponse.label(), "post-response");
        assert_eq!(ScriptPhase::Tests.label(), "tests");
    }
}
