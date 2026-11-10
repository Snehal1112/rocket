//! Ports of the contract check: the provider's pending changes and the consumer's requests.
//!
//! The helpers turn domain values into what the ports return, so the `src-tauri` adapters stay
//! thin. They are pure.

use std::path::Path;

use rocket_collection::contract::changelog::{ChangeType, ChangelogEntry};
use rocket_collection::contract::snapshot::RequestSignatureSnapshot;
use rocket_collection::contract::types::BreakingChangePolicy;
use rocket_collection::contract::walk_folder;
use rocket_collection::Collection;
use rocket_network::limits::{CONTRACT_FIELD_MAX, CONTRACT_METHOD_MAX, CONTRACT_PATH_MAX};
use rocket_network::{is_format_char, ContractChange, ContractChangeType, ContractPolicyWire};
use rocket_shared::error::DomainResult;

use super::contract_check_match::{path_only, LocalRequest};

/// A contract with the changes since its signed snapshot, as the offer carries them.
///
/// Values are cut to the wire limit but not redacted. The service redacts them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingContract {
    pub name: String,
    pub version: String,
    pub policy: ContractPolicyWire,
    pub changes: Vec<ContractChange>,
}

/// Reads the provider's pending contract changes.
pub trait ContractChangesPort: Send + Sync {
    fn pending_changes(
        &self,
        collection_root: &str,
        contract_id: &str,
    ) -> DomainResult<PendingContract>;
}

/// Reads the HTTP requests of every collection of the active workspace. Read-only.
pub trait LocalRequestsPort: Send + Sync {
    fn local_requests(&self) -> DomainResult<Vec<LocalRequest>>;
}

/// The wire policy of a contract policy.
pub fn policy_to_wire(policy: &BreakingChangePolicy) -> ContractPolicyWire {
    match policy {
        BreakingChangePolicy::Strict => ContractPolicyWire::Strict,
        BreakingChangePolicy::Lenient => ContractPolicyWire::Lenient,
        BreakingChangePolicy::AdditiveOk => ContractPolicyWire::AdditiveOk,
    }
}

/// The wire changes of changelog entries, in order.
///
/// Only the path of the request URL is sent. An entry is skipped when its field has no wire
/// name (`method`, `url_pattern`, `request`, legacy `body_field.*`) or when its method, path or
/// field is empty or too long after control and format characters are stripped, because the
/// receiver refuses such an offer. Values are cut with `ContractChange::cut_values`.
pub fn wire_changes(entries: &[ChangelogEntry]) -> Vec<ContractChange> {
    entries.iter().filter_map(wire_change).collect()
}

fn wire_change(entry: &ChangelogEntry) -> Option<ContractChange> {
    let method = plain(entry.request_method.as_deref()?, CONTRACT_METHOD_MAX)?;
    let path_template = plain(&sent_path(entry.http_path.as_deref()?)?, CONTRACT_PATH_MAX)?;
    let field = plain(
        &wire_field(&strip_control(&entry.field))?,
        CONTRACT_FIELD_MAX,
    )?;
    let mut change = ContractChange {
        method,
        path_template,
        field,
        change_type: match entry.change_type {
            ChangeType::Changed => ContractChangeType::Changed,
            ChangeType::Added => ContractChangeType::Added,
            ChangeType::Removed => ContractChangeType::Removed,
        },
        old_value: entry.old_value.as_deref().map(strip_control),
        new_value: entry.new_value.as_deref().map(strip_control),
        breaking: entry.is_breaking,
    };
    change.cut_values();
    Some(change)
}

/// The wire name of a `diff_signature` field, or `None` when it has none.
fn wire_field(field: &str) -> Option<String> {
    let named = |prefix: &str, name: &str| (!name.is_empty()).then(|| format!("{prefix}:{name}"));
    if let Some(name) = field.strip_prefix("header.") {
        return named("header", name);
    }
    if let Some(name) = field.strip_prefix("query_param.") {
        return named("query", name);
    }
    if let Some(name) = field.strip_prefix("form_field.") {
        return named("form", name);
    }
    match field {
        "auth_type" => Some("auth".into()),
        "auth_detail" => Some("auth_detail".into()),
        "body" => Some("body".into()),
        _ => None,
    }
}

/// The path of a request URL, or `None` when the URL is empty after control and format
/// characters are stripped.
///
/// Only the path leaves the node: the scheme, user info, host, query and fragment can hold
/// secrets, and the consumer ignores them when it matches.
fn sent_path(url: &str) -> Option<String> {
    let url = strip_control(url);
    (!url.trim().is_empty()).then(|| path_only(&url))
}

/// The text without control and format characters, when it is not empty and at most `max`
/// characters.
fn plain(text: &str, max: usize) -> Option<String> {
    let text = strip_control(text);
    (!text.is_empty() && text.chars().count() <= max).then_some(text)
}

/// The text without control characters and Unicode format characters, which the receiver
/// refuses.
pub(super) fn strip_control(text: &str) -> String {
    text.chars()
        .filter(|&c| !c.is_control() && !is_format_char(c))
        .collect()
}

/// Every HTTP request of a collection with its signature. Names stay on this node.
pub fn local_requests_of(collection: &Collection) -> Vec<LocalRequest> {
    let mut walked = Vec::new();
    walk_folder(&collection.root, Path::new(""), &mut walked);
    walked
        .into_iter()
        .map(|(path, request)| LocalRequest {
            collection: collection.name.clone(),
            name: request.name.clone(),
            signature: RequestSignatureSnapshot::from_request(&path, request),
        })
        .collect()
}
