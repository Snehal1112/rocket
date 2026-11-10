//! In-memory fakes for the `ContractCheckService` tests: the provider's pending contract, the
//! consumer's requests, an event log and a rig that wires them with the fake peers.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use rocket_audit::publisher::RecordingSecurityAuditPublisher;
use rocket_audit::AuditEventKind;
use rocket_collection::contract::snapshot::KeyValueEntry;
use rocket_collection::contract::RequestSignatureSnapshot;
use rocket_network::{
    ContractChange, ContractChangeType, ContractCheckEvent, ContractPolicyWire, PeerMessage,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{EventPublisher, PublishedEvent};

use super::contract_check::{ContractCheckDeps, ContractCheckService};
use super::contract_check_match::LocalRequest;
use super::contract_check_ports::{ContractChangesPort, LocalRequestsPort, PendingContract};
use super::sync_fakes::dev;
use super::transfer_fakes::FakePeers;

/// The provider's contract with its pending changes. `None` makes the read fail.
pub(crate) struct FakeChanges {
    pending: Mutex<Option<PendingContract>>,
    calls: AtomicUsize,
}

impl FakeChanges {
    pub fn new(pending: PendingContract) -> Arc<FakeChanges> {
        Arc::new(FakeChanges {
            pending: Mutex::new(Some(pending)),
            calls: AtomicUsize::new(0),
        })
    }

    pub fn set(&self, pending: Option<PendingContract>) {
        *self.pending.lock().expect("pending lock") = pending;
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ContractChangesPort for FakeChanges {
    fn pending_changes(&self, _root: &str, _contract_id: &str) -> DomainResult<PendingContract> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.pending
            .lock()
            .expect("pending lock")
            .clone()
            .ok_or_else(|| DomainError::NotFound("That contract does not exist.".into()))
    }
}

/// One local request: collection, name, method, URL and headers.
#[derive(Clone)]
pub(crate) struct LocalSpec {
    pub collection: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// The consumer's requests, rebuilt on every read.
#[derive(Default)]
pub(crate) struct FakeLocals {
    specs: Mutex<Vec<LocalSpec>>,
    calls: AtomicUsize,
}

impl FakeLocals {
    pub fn set(&self, specs: Vec<LocalSpec>) {
        *self.specs.lock().expect("specs lock") = specs;
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl LocalRequestsPort for FakeLocals {
    fn local_requests(&self) -> DomainResult<Vec<LocalRequest>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let specs = self.specs.lock().expect("specs lock").clone();
        Ok(specs.into_iter().map(local_request).collect())
    }
}

fn local_request(spec: LocalSpec) -> LocalRequest {
    LocalRequest {
        collection: spec.collection,
        name: spec.name,
        signature: RequestSignatureSnapshot {
            request_path: PathBuf::from("r.yml"),
            method: spec.method,
            url_pattern: spec.url,
            headers: spec
                .headers
                .into_iter()
                .map(|(key, value)| KeyValueEntry { key, value })
                .collect(),
            query_params: Vec::new(),
            body_content: None,
            form_fields: Vec::new(),
            auth_type: "none".into(),
            auth_detail: String::new(),
            captured_at: Utc::now(),
            query_param_keys: Vec::new(),
            header_keys: Vec::new(),
            body_field_keys: Vec::new(),
        },
    }
}

/// Records every `ContractCheckEvent`.
#[derive(Default)]
pub(crate) struct RecordingCheckEvents(Mutex<Vec<ContractCheckEvent>>);

impl RecordingCheckEvents {
    pub fn all(&self) -> Vec<ContractCheckEvent> {
        self.0.lock().expect("events lock").clone()
    }
}

impl EventPublisher for RecordingCheckEvents {
    fn publish(&self, event: &dyn PublishedEvent) {
        if let Some(event) = event.downcast_ref::<ContractCheckEvent>() {
            self.0.lock().expect("events lock").push(event.clone());
        }
    }
}

/// The service with fakes around it.
pub(crate) struct CheckRig {
    pub svc: Arc<ContractCheckService>,
    pub peers: Arc<FakePeers>,
    pub changes: Arc<FakeChanges>,
    pub locals: Arc<FakeLocals>,
    pub events: Arc<RecordingCheckEvents>,
    pub audit: Arc<RecordingSecurityAuditPublisher>,
}

/// A rig whose pending contract is `pending`, with paired online peers 1 to 5.
pub(crate) fn check_rig(pending: PendingContract) -> CheckRig {
    let peers = Arc::new(FakePeers::default());
    for seed in 1..=5u8 {
        peers.add(dev(seed), &format!("Node {seed}"));
    }
    let changes = FakeChanges::new(pending);
    let locals = Arc::new(FakeLocals::default());
    let events = Arc::new(RecordingCheckEvents::default());
    let audit = Arc::new(RecordingSecurityAuditPublisher::default());
    let svc = ContractCheckService::new(ContractCheckDeps {
        network: peers.clone(),
        changes: changes.clone(),
        locals: locals.clone(),
        events: events.clone(),
        audit: audit.clone(),
    });
    CheckRig {
        svc,
        peers,
        changes,
        locals,
        events,
        audit,
    }
}

impl CheckRig {
    pub fn audited(&self) -> Vec<AuditEventKind> {
        self.audit.records().into_iter().map(|r| r.kind).collect()
    }

    pub fn sent(&self, seed: u8) -> Vec<PeerMessage> {
        self.peers.sent_to(&dev(seed))
    }

    /// A frame from `seed` arrives.
    pub fn push(&self, seed: u8, message: PeerMessage) {
        self.svc.on_frame(dev(seed), message);
    }
}

/// Lets spawned sends run, so a test can check that nothing was sent.
pub(crate) async fn settle() {
    tokio::time::sleep(Duration::from_millis(30)).await;
}

/// A header change on `GET /users/{id}`.
pub(crate) fn header_change(
    name: &str,
    change_type: ContractChangeType,
    old: Option<&str>,
    new: Option<&str>,
    breaking: bool,
) -> ContractChange {
    ContractChange {
        method: "GET".into(),
        path_template: "/users/{id}".into(),
        field: format!("header:{name}"),
        change_type,
        old_value: old.map(Into::into),
        new_value: new.map(Into::into),
        breaking,
    }
}

/// A pending contract named "Orders API" with these changes.
pub(crate) fn pending(changes: Vec<ContractChange>) -> PendingContract {
    PendingContract {
        name: "Orders API".into(),
        version: "1.2.0".into(),
        policy: ContractPolicyWire::Strict,
        changes,
    }
}

/// A local request on `GET {{base}}/users/:id` with these headers.
pub(crate) fn user_request(name: &str, headers: &[(&str, &str)]) -> LocalSpec {
    LocalSpec {
        collection: "Shop".into(),
        name: name.into(),
        method: "GET".into(),
        url: "{{base}}/users/:id".into(),
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}
