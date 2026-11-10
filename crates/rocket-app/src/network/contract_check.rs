//! The contract check service: a provider asks paired nodes whether its contract changes break
//! their requests, and a consumer answers after its user agreed.
//!
//! Provider side: `start` reads the pending changes, redacts them, sends one offer per node and
//! waits for one result per offer. Consumer side: an offer waits for the user, `review` matches
//! it against the local requests, and `reply` or `decline` answers. The local request names
//! stay on the consumer node; a reply carries only verdicts by index.
//!
//! One `std::sync::Mutex` holds both maps. It is never held while a port or the network is
//! called. Frames leave from their own tokio task. Every error is a fixed sentence and never
//! quotes a peer string.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use rocket_audit::{AuditActor, AuditEventKind, SecurityAuditPublisher};
use rocket_network::limits::{
    CONTRACT_CHECK_MAX_CHANGES, CONTRACT_CHECK_PER_PEER, CONTRACT_CHECK_TOTAL, CONTRACT_CHECK_TTL,
    CONTRACT_NAME_MAX, CONTRACT_VERSION_MAX,
};
use rocket_network::{
    ContractChange, ContractCheckEvent, ContractCheckId, ContractCheckOutcome, ContractCheckRow,
    ContractCheckRowDto, ContractPolicyWire, ContractVerdict, DeviceId, PeerMessage,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::EventPublisher;

use super::contract_check_match::{redact_change, verdicts};
use super::contract_check_ports::{ContractChangesPort, LocalRequestsPort};
use super::transfer::PeerSender;

pub(super) const TOO_LARGE: &str = "Check fewer changes at a time.";
pub(super) const NO_CHANGES: &str = "This contract has no changes to check.";
pub(super) const NOT_WAITING: &str = "This check is no longer waiting for an answer.";
const NO_NODES: &str = "Choose at least one node.";
const NOT_PAIRED: &str = "That node is not paired.";
const OFFLINE: &str = "That node is offline.";
const NO_NAME: &str = "The contract needs a name to be checked.";
const CANNOT_SEND: &str = "This contract change cannot be sent.";
const NOT_REVIEWED: &str = "Review the check before you send the result.";
const LOCALS_UNREADABLE: &str = "The requests of this workspace could not be read.";

/// Everything the service needs from outside. All of it is a port.
pub struct ContractCheckDeps {
    pub network: Arc<dyn PeerSender>,
    pub changes: Arc<dyn ContractChangesPort>,
    pub locals: Arc<dyn LocalRequestsPort>,
    pub events: Arc<dyn EventPublisher>,
    pub audit: Arc<dyn SecurityAuditPublisher>,
}

/// What `start` returns: the check id of every node and the changes as they were sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCheckStarted {
    pub check_ids: Vec<(DeviceId, ContractCheckId)>,
    /// The redacted changes, in offer order. The provider view shows them as sent.
    pub changes: Vec<ContractChange>,
}

/// One local request that matched a change. For the local UI only, never sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalMatch {
    pub collection: String,
    pub name: String,
}

/// One offered change with its local verdict and matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewedChange {
    pub change: ContractChange,
    pub verdict: ContractVerdict,
    pub matches: Vec<LocalMatch>,
}

/// The review of an incoming check, for the consumer's dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCheckReview {
    pub check_id: String,
    pub device_id: String,
    pub contract: String,
    pub version: String,
    pub policy: ContractPolicyWire,
    pub changes: Vec<ReviewedChange>,
}

/// A check this node sent and still waits for.
struct Outgoing {
    device: DeviceId,
    contract: String,
    change_count: usize,
    deadline: Instant,
}

/// A check a paired node sent to this node, waiting for the user.
struct Incoming {
    device: DeviceId,
    contract: String,
    version: String,
    policy: ContractPolicyWire,
    changes: Vec<ContractChange>,
    deadline: Instant,
    /// The verdicts the user saw in the last review. `reply` sends exactly these.
    reviewed: Option<Vec<ContractVerdict>>,
}

#[derive(Default)]
struct State {
    outgoing: HashMap<ContractCheckId, Outgoing>,
    incoming: HashMap<ContractCheckId, Incoming>,
}

/// The two sides of the contract check. See the module comment.
pub struct ContractCheckService {
    deps: ContractCheckDeps,
    state: Mutex<State>,
}

impl ContractCheckService {
    pub fn new(deps: ContractCheckDeps) -> Arc<Self> {
        Arc::new(ContractCheckService {
            deps,
            state: Mutex::new(State::default()),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ----- provider -----

    /// Sends the pending changes of a contract to every chosen node.
    ///
    /// Every node must be paired and online, the contract must have 1 to 200 changes, and the
    /// offer must fit one frame. Otherwise nothing is sent. Values are redacted first.
    pub fn start(
        &self,
        collection_root: &str,
        contract_id: &str,
        devices: &[DeviceId],
    ) -> DomainResult<ContractCheckStarted> {
        let mut seen = HashSet::new();
        let devices: Vec<DeviceId> = devices
            .iter()
            .filter(|d| seen.insert(**d))
            .copied()
            .collect();
        if devices.is_empty() {
            return Err(invalid(NO_NODES));
        }
        for device in &devices {
            if self.deps.network.peer_label(device).is_none() {
                return Err(invalid(NOT_PAIRED));
            }
            if !self.deps.network.is_online(device) {
                return Err(invalid(OFFLINE));
            }
        }
        let pending = self
            .deps
            .changes
            .pending_changes(collection_root, contract_id)?;
        if pending.changes.is_empty() {
            return Err(invalid(NO_CHANGES));
        }
        if pending.changes.len() > CONTRACT_CHECK_MAX_CHANGES {
            return Err(invalid(TOO_LARGE));
        }
        let contract = clean(&pending.name, CONTRACT_NAME_MAX);
        if contract.is_empty() {
            return Err(invalid(NO_NAME));
        }
        let version = clean(&pending.version, CONTRACT_VERSION_MAX);
        let changes: Vec<ContractChange> = pending
            .changes
            .into_iter()
            .map(|change| {
                let mut change = redact_change(change);
                change.cut_values();
                change
            })
            .collect();
        let mut offers = Vec::with_capacity(devices.len());
        for device in devices {
            let id = ContractCheckId::random();
            let offer = PeerMessage::ContractCheckOffer {
                check_id: id.as_str().to_string(),
                contract: contract.clone(),
                version: version.clone(),
                policy: pending.policy,
                changes: changes.clone(),
            };
            if !offer.fits_frame() {
                return Err(invalid(TOO_LARGE));
            }
            // The receiver runs the same check, so a frame it would refuse is not sent.
            if PeerMessage::decode(&offer.encode()).is_err() {
                return Err(invalid(CANNOT_SEND));
            }
            offers.push((device, id, offer));
        }
        let deadline = Instant::now() + CONTRACT_CHECK_TTL;
        {
            let mut state = self.lock();
            for (device, id, _) in &offers {
                state.outgoing.insert(
                    id.clone(),
                    Outgoing {
                        device: *device,
                        contract: contract.clone(),
                        change_count: changes.len(),
                        deadline,
                    },
                );
            }
        }
        let mut check_ids = Vec::with_capacity(offers.len());
        for (device, id, offer) in offers {
            self.tell_later(device, offer);
            check_ids.push((device, id));
        }
        Ok(ContractCheckStarted { check_ids, changes })
    }

    /// Takes a result for a check this node sent. Anything that is not the one valid answer
    /// of the node the offer went to is dropped.
    fn on_result(
        &self,
        from: DeviceId,
        check_id: &str,
        outcome: ContractCheckOutcome,
        rows: Vec<ContractCheckRow>,
    ) {
        let Ok(id) = ContractCheckId::parse(check_id) else {
            tracing::debug!(code = "network_contract_result_bad_id", "result dropped");
            return;
        };
        let taken = {
            let mut state = self.lock();
            let valid = state.outgoing.get(&id).is_some_and(|o| {
                o.device == from && rows_are_valid(outcome, &rows, o.change_count)
            });
            if valid {
                state.outgoing.remove(&id)
            } else {
                None
            }
        };
        let Some(check) = taken else {
            tracing::debug!(code = "network_contract_result_dropped", "result dropped");
            return;
        };
        let mut rows: Vec<ContractCheckRowDto> = rows
            .into_iter()
            .map(|r| ContractCheckRowDto {
                index: r.index,
                verdict: r.verdict,
            })
            .collect();
        rows.sort_by_key(|r| r.index);
        self.deps
            .events
            .publish(&ContractCheckEvent::ResultReceived {
                check_id: id.as_str().to_string(),
                device_id: from.to_base32(),
                outcome: outcome_name(outcome).to_string(),
                rows,
            });
        self.deps.audit.publish(
            AuditActor::User.into(),
            None,
            AuditEventKind::NetworkContractCheckSent {
                device_id: from.short(),
                contract: check.contract,
                outcome: outcome_name(outcome).to_string(),
            },
        );
    }

    // ----- consumer -----

    /// Takes an offer from a paired node. A valid offer waits for the user; past the limits
    /// the node answers `busy`. A bad offer is dropped without an answer.
    fn on_offer(&self, from: DeviceId, message: PeerMessage) {
        // The decoder already checked the frame. Checking again keeps this side safe alone.
        let checked = PeerMessage::decode(&message.encode());
        let Ok(PeerMessage::ContractCheckOffer {
            check_id,
            contract,
            version,
            policy,
            changes,
        }) = checked
        else {
            tracing::debug!(code = "network_contract_offer_bad", "offer dropped");
            return;
        };
        let Ok(id) = ContractCheckId::parse(&check_id) else {
            tracing::debug!(code = "network_contract_offer_bad", "offer dropped");
            return;
        };
        if self.deps.network.peer_label(&from).is_none() {
            tracing::debug!(code = "network_contract_offer_unpaired", "offer dropped");
            return;
        }
        let change_count = changes.len();
        enum Next {
            Wait,
            Busy,
            Drop,
        }
        let next = {
            let mut state = self.lock();
            let from_peer = state.incoming.values().filter(|c| c.device == from).count();
            if from_peer >= CONTRACT_CHECK_PER_PEER || state.incoming.len() >= CONTRACT_CHECK_TOTAL
            {
                Next::Busy
            } else if state.incoming.contains_key(&id) {
                Next::Drop
            } else {
                state.incoming.insert(
                    id.clone(),
                    Incoming {
                        device: from,
                        contract: contract.clone(),
                        version,
                        policy,
                        changes,
                        deadline: Instant::now() + CONTRACT_CHECK_TTL,
                        reviewed: None,
                    },
                );
                Next::Wait
            }
        };
        match next {
            Next::Wait => self
                .deps
                .events
                .publish(&ContractCheckEvent::OfferReceived {
                    check_id: id.as_str().to_string(),
                    device_id: from.to_base32(),
                    contract,
                    change_count: u32::try_from(change_count).unwrap_or(u32::MAX),
                }),
            Next::Busy => {
                self.tell_later(
                    from,
                    PeerMessage::ContractCheckResult {
                        check_id: id.as_str().to_string(),
                        outcome: ContractCheckOutcome::Busy,
                        rows: Vec::new(),
                    },
                );
                self.audit_received(from, contract, ContractCheckOutcome::Busy);
            }
            Next::Drop => {
                tracing::debug!(code = "network_contract_offer_id_taken", "offer dropped");
            }
        }
    }

    /// Matches an incoming check against the local requests. Sends nothing.
    ///
    /// The verdicts are kept, so `reply` sends what the user saw.
    pub fn review(&self, check_id: &ContractCheckId) -> DomainResult<ContractCheckReview> {
        let (device, contract, version, policy, changes) = {
            let state = self.lock();
            let check = waiting(&state, check_id)?;
            (
                check.device,
                check.contract.clone(),
                check.version.clone(),
                check.policy,
                check.changes.clone(),
            )
        };
        let locals = self.deps.locals.local_requests().map_err(|e| {
            tracing::debug!(code = "network_contract_locals_failed", error = %e, "review failed");
            invalid(LOCALS_UNREADABLE)
        })?;
        let found = verdicts(&changes, policy, &locals);
        {
            let mut state = self.lock();
            let check = state
                .incoming
                .get_mut(check_id)
                .filter(|c| c.deadline > Instant::now())
                .ok_or_else(|| invalid(NOT_WAITING))?;
            check.reviewed = Some(found.iter().map(|(v, _)| *v).collect());
        }
        let changes = changes
            .into_iter()
            .zip(found)
            .map(|(change, (verdict, matched))| ReviewedChange {
                change,
                verdict,
                matches: matched
                    .into_iter()
                    .filter_map(|i| locals.get(i))
                    .map(|l| LocalMatch {
                        collection: l.collection.clone(),
                        name: l.name.clone(),
                    })
                    .collect(),
            })
            .collect();
        Ok(ContractCheckReview {
            check_id: check_id.as_str().to_string(),
            device_id: device.to_base32(),
            contract,
            version,
            policy,
            changes,
        })
    }

    /// Sends the reviewed verdicts, one row per change in offer order.
    pub fn reply(&self, check_id: &ContractCheckId) -> DomainResult<()> {
        self.check_online(check_id)?;
        let (device, contract, rows) = {
            let mut state = self.lock();
            let check = waiting(&state, check_id)?;
            let Some(reviewed) = check.reviewed.clone() else {
                return Err(invalid(NOT_REVIEWED));
            };
            let rows: Vec<ContractCheckRow> = reviewed
                .into_iter()
                .enumerate()
                .map(|(index, verdict)| ContractCheckRow {
                    index: u32::try_from(index).unwrap_or(u32::MAX),
                    verdict,
                })
                .collect();
            let check = state
                .incoming
                .remove(check_id)
                .filter(|c| c.deadline > Instant::now())
                .ok_or_else(|| invalid(NOT_WAITING))?;
            (check.device, check.contract, rows)
        };
        self.answer(
            device,
            check_id,
            contract,
            ContractCheckOutcome::Answered,
            rows,
        );
        Ok(())
    }

    /// Answers `declined`. Needs no review.
    pub fn decline(&self, check_id: &ContractCheckId) -> DomainResult<()> {
        self.check_online(check_id)?;
        let (device, contract) = {
            let mut state = self.lock();
            let check = state
                .incoming
                .remove(check_id)
                .filter(|c| c.deadline > Instant::now())
                .ok_or_else(|| invalid(NOT_WAITING))?;
            (check.device, check.contract)
        };
        self.answer(
            device,
            check_id,
            contract,
            ContractCheckOutcome::Declined,
            Vec::new(),
        );
        Ok(())
    }

    /// Fails when the check no longer waits or its node is offline. The network is asked
    /// after the lock is released.
    fn check_online(&self, check_id: &ContractCheckId) -> DomainResult<()> {
        let device = waiting(&self.lock(), check_id)?.device;
        if self.deps.network.is_online(&device) {
            Ok(())
        } else {
            Err(invalid(OFFLINE))
        }
    }

    fn answer(
        &self,
        device: DeviceId,
        check_id: &ContractCheckId,
        contract: String,
        outcome: ContractCheckOutcome,
        rows: Vec<ContractCheckRow>,
    ) {
        self.tell_later(
            device,
            PeerMessage::ContractCheckResult {
                check_id: check_id.as_str().to_string(),
                outcome,
                rows,
            },
        );
        self.audit_received(device, contract, outcome);
    }

    fn audit_received(&self, device: DeviceId, contract: String, outcome: ContractCheckOutcome) {
        self.deps.audit.publish(
            AuditActor::User.into(),
            None,
            AuditEventKind::NetworkContractCheckReceived {
                device_id: device.short(),
                contract,
                outcome: outcome_name(outcome).to_string(),
            },
        );
    }

    // ----- router and timers -----

    /// True for the two contract check frames.
    pub fn claims(&self, message: &PeerMessage) -> bool {
        matches!(
            message,
            PeerMessage::ContractCheckOffer { .. } | PeerMessage::ContractCheckResult { .. }
        )
    }

    /// One frame from the router. Other frames are ignored.
    pub fn on_frame(&self, from: DeviceId, message: PeerMessage) {
        match message {
            PeerMessage::ContractCheckOffer { .. } => self.on_offer(from, message),
            PeerMessage::ContractCheckResult {
                check_id,
                outcome,
                rows,
            } => self.on_result(from, &check_id, outcome, rows),
            _ => {}
        }
    }

    /// The peer went away: its checks on both sides end as expired.
    pub fn on_peer_gone(&self, device: DeviceId) {
        let ended = {
            let mut state = self.lock();
            let mut ended = Vec::new();
            state.outgoing.retain(|id, c| {
                let keep = c.device != device;
                if !keep {
                    ended.push(id.clone());
                }
                keep
            });
            state.incoming.retain(|id, c| {
                let keep = c.device != device;
                if !keep {
                    ended.push(id.clone());
                }
                keep
            });
            ended
        };
        self.emit_expired(ended);
    }

    /// Ends every check whose deadline is at or before `now`, with one `Expired` each.
    pub fn sweep(&self, now: Instant) {
        let ended = {
            let mut state = self.lock();
            let mut ended = Vec::new();
            state.outgoing.retain(|id, c| {
                let keep = c.deadline > now;
                if !keep {
                    ended.push(id.clone());
                }
                keep
            });
            state.incoming.retain(|id, c| {
                let keep = c.deadline > now;
                if !keep {
                    ended.push(id.clone());
                }
                keep
            });
            ended
        };
        self.emit_expired(ended);
    }

    fn emit_expired(&self, ended: Vec<ContractCheckId>) {
        for id in ended {
            self.deps.events.publish(&ContractCheckEvent::Expired {
                check_id: id.as_str().to_string(),
            });
        }
    }

    /// Sends a frame from its own task and logs a failure.
    fn tell_later(&self, device: DeviceId, message: PeerMessage) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::debug!(code = "network_contract_no_runtime", "frame not sent");
            return;
        };
        let network = self.deps.network.clone();
        drop(runtime.spawn(async move {
            if let Err(e) = network.send_to(&device, message).await {
                tracing::debug!(code = "network_contract_send_failed", error = %e, "frame not sent");
            }
        }));
    }
}

fn invalid(message: &str) -> DomainError {
    DomainError::InvalidInput(message.to_string())
}

/// The incoming check, if it still waits for an answer.
fn waiting<'a>(state: &'a State, check_id: &ContractCheckId) -> DomainResult<&'a Incoming> {
    state
        .incoming
        .get(check_id)
        .filter(|c| c.deadline > Instant::now())
        .ok_or_else(|| invalid(NOT_WAITING))
}

/// Whether the rows are the one valid answer: for `answered` one row per change with every
/// index in range and unique, otherwise no rows.
fn rows_are_valid(outcome: ContractCheckOutcome, rows: &[ContractCheckRow], count: usize) -> bool {
    match outcome {
        ContractCheckOutcome::Answered => {
            let mut seen = HashSet::new();
            rows.len() == count
                && rows.iter().all(|r| {
                    usize::try_from(r.index).is_ok_and(|i| i < count) && seen.insert(r.index)
                })
        }
        ContractCheckOutcome::Declined | ContractCheckOutcome::Busy => rows.is_empty(),
    }
}

fn outcome_name(outcome: ContractCheckOutcome) -> &'static str {
    match outcome {
        ContractCheckOutcome::Answered => "answered",
        ContractCheckOutcome::Declined => "declined",
        ContractCheckOutcome::Busy => "busy",
    }
}

/// The text without control characters, trimmed and cut to `max` characters.
fn clean(text: &str, max: usize) -> String {
    let plain: String = text.chars().filter(|c| !c.is_control()).collect();
    plain.trim().chars().take(max).collect()
}
