//! Holds the assistant's proposed workspace changes until the user accepts
//! or rejects them. Nothing is written when a change is proposed. An
//! accepted change is applied through `CollectionService` or
//! `EnvironmentService`, the same paths as a manual edit, so name checks
//! and events stay the same. Proposals live in memory only.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rocket_acp::proposal::{
    AgentProposal, ProposalStatus, ProposedChange, ProposedRequest, RequestPatch, ScriptPhase,
};
use rocket_collection::{request_filename_for, CollectionItem, Folder, Request};
use rocket_environment::{Environment, EnvironmentRepositoryFactory, Variable};
use rocket_shared::description::Documentation;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_shared::types::BodyMode;
use sha2::{Digest, Sha256};

use crate::collection_service::CollectionService;
use crate::environment_service::EnvironmentService;
use crate::flow_run_cache::saved_request_text;
use crate::redaction::REDACTED;
use crate::runner_sequence::folder_dir_name;

/// How many proposals may wait for the user in one session.
pub const MAX_PENDING_PER_SESSION: usize = 50;

/// How many answered proposals one session keeps for `list`. The oldest go
/// first.
pub const MAX_RESOLVED_PER_SESSION: usize = 100;

/// One error for a secret variable, matching the old MCP tool's wording.
const VARIABLE_NOT_ACCESSIBLE: &str = "variable not accessible";

/// Names the active workspace. The service does no I/O, so the app injects
/// this. Two calls return the same text only while the workspace is the same.
pub type WorkspaceIdentity = Arc<dyn Fn() -> String + Send + Sync>;

/// File names the collection layout reserves, lowercase. A request must never
/// be saved over one of them. Mirrors `rocket-infra` `is_request_file`.
const RESERVED_FILE_NAMES: &[&str] = &[
    "opencollection.yml",
    "opencollection.yaml",
    "folder.yml",
    "folder.yaml",
    "workspace.yml",
    "workspace.yaml",
    "collection.json",
    "_order.json",
    "_order.yml",
    "_order.yaml",
];

/// Directory names the tree never shows, lowercase. Mirrors `rocket-infra`
/// `is_hidden_entry`. `flows` is hidden at the collection root only.
const RESERVED_DIR_NAMES: &[&str] = &["environments", "node_modules"];

pub struct ProposalService {
    collections: CollectionService,
    environment_repo_factory: Arc<dyn EnvironmentRepositoryFactory>,
    events: Arc<dyn EventPublisher>,
    workspace_identity: WorkspaceIdentity,
    store: Mutex<Store>,
}

/// What a pending proposal remembers besides the change itself.
struct ProposalMeta {
    /// The workspace that was active when the change was proposed.
    workspace: String,
    /// For `SetEnvVar`: the fingerprint of that one variable at propose time.
    env_fingerprint: Option<String>,
}

#[derive(Default)]
struct Store {
    /// Extra data of pending proposals, by proposal id. Removed on answer.
    meta: HashMap<String, ProposalMeta>,
    /// Sessions that ended. A late call must not recreate their entry.
    ended: HashSet<String>,
    /// Proposals per real ACP session id, oldest first. The MCP tool server
    /// reports that id through Plan 03's `McpSessionBinding`.
    sessions: HashMap<String, Vec<AgentProposal>>,
}

/// What a collection-relative path points at in the summaries tree.
enum Target {
    /// A folder, with its shape text for the fingerprint.
    Folder(String),
    HttpRequest,
    /// A GraphQL, WebSocket or gRPC request, or a script file.
    Other,
    Missing,
}

/// Lets an `Arc` publisher stand in where a service wants a `Box`.
struct ForwardEvents(Arc<dyn EventPublisher>);

impl EventPublisher for ForwardEvents {
    fn publish(&self, event: DomainEvent) {
        self.0.publish(event);
    }
}

impl ProposalService {
    pub fn new(
        collections: CollectionService,
        environment_repo_factory: Arc<dyn EnvironmentRepositoryFactory>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            collections,
            environment_repo_factory,
            events,
            workspace_identity: Arc::new(String::new),
            store: Mutex::new(Store::default()),
        }
    }

    /// Sets how the service learns which workspace is active. A proposal made
    /// in one workspace is `Stale` once another is active.
    pub fn with_workspace_identity(mut self, identity: WorkspaceIdentity) -> Self {
        self.workspace_identity = identity;
        self
    }

    fn lock(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Checks every change, then queues each one as its own pending
    /// proposal. If any change is invalid, none is queued.
    pub fn propose(
        &self,
        session_id: &str,
        changes: Vec<ProposedChange>,
    ) -> DomainResult<Vec<String>> {
        if changes.is_empty() {
            return Err(DomainError::InvalidInput(
                "propose at least one change".to_string(),
            ));
        }
        // Refuse an oversized batch before any filesystem read in prepare.
        {
            let store = self.lock();
            let pending = store.sessions.get(session_id).map_or(0, |list| {
                list.iter()
                    .filter(|p| p.status == ProposalStatus::Pending)
                    .count()
            });
            if pending + changes.len() > MAX_PENDING_PER_SESSION {
                return Err(DomainError::InvalidInput(format!(
                    "too many pending proposals: at most {MAX_PENDING_PER_SESSION} can wait for \
                     the user; ask the user to accept or reject some first"
                )));
            }
        }
        let workspace: Vec<String> = self
            .collections
            .list()?
            .into_iter()
            .map(|summary| summary.name)
            .collect();
        let workspace_id = (self.workspace_identity)();
        let mut prepared = Vec::with_capacity(changes.len());
        for mut change in changes {
            let env_fingerprint = self.prepare(&mut change, &workspace)?;
            prepared.push((change, env_fingerprint));
        }

        let now = chrono::Utc::now().timestamp_millis();
        let mut store = self.lock();
        if store.ended.contains(session_id) {
            return Err(DomainError::InvalidInput(
                "this assistant session has ended; start a new one to propose changes".to_string(),
            ));
        }
        let session = session_id.to_string();
        let list = store.sessions.entry(session.clone()).or_default();
        let pending = list
            .iter()
            .filter(|p| p.status == ProposalStatus::Pending)
            .count();
        if pending + prepared.len() > MAX_PENDING_PER_SESSION {
            return Err(DomainError::InvalidInput(format!(
                "too many pending proposals: at most {MAX_PENDING_PER_SESSION} can wait for \
                 the user; ask the user to accept or reject some first"
            )));
        }
        let mut created = Vec::with_capacity(prepared.len());
        let mut metas = Vec::with_capacity(prepared.len());
        for (change, env_fingerprint) in prepared {
            let proposal =
                AgentProposal::new(ulid::Ulid::new().to_string(), session.clone(), change, now);
            created.push((proposal.id.clone(), proposal.summary.clone()));
            metas.push((
                proposal.id.clone(),
                ProposalMeta {
                    workspace: workspace_id.clone(),
                    env_fingerprint,
                },
            ));
            list.push(proposal);
        }
        prune_resolved(list);
        store.meta.extend(metas);
        drop(store);

        let mut ids = Vec::with_capacity(created.len());
        for (proposal_id, summary) in created {
            self.events.publish(DomainEvent::AcpProposalCreated {
                session_id: session.clone(),
                proposal_id: proposal_id.clone(),
                summary,
            });
            ids.push(proposal_id);
        }
        Ok(ids)
    }

    /// The session's proposals, oldest first. Unknown sessions have none.
    pub fn list(&self, session_id: &str) -> Vec<AgentProposal> {
        self.lock()
            .sessions
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Re-checks the target and applies the change. A changed target makes
    /// the proposal `Stale` and nothing is written. A failed write makes it
    /// `Failed`.
    pub fn accept(&self, session_id: &str, proposal_id: &str) -> DomainResult<AgentProposal> {
        let mut store = self.lock();
        let session = session_id.to_string();
        let change = pending_mut(&mut store, &session, proposal_id)?
            .change
            .clone();
        let meta = store.meta.get(proposal_id);
        let env_fingerprint = meta.and_then(|m| m.env_fingerprint.clone());
        // A proposal from another workspace must not touch this one, even if
        // a collection of the same name exists here.
        let same_workspace =
            meta.is_some_and(|m| m.workspace == (self.workspace_identity)());
        // The lock stays held while the change is applied, so a second
        // accept of the same proposal waits and then finds it answered.
        let applies = if same_workspace {
            self.still_applies(&change)
        } else {
            Ok(false)
        };
        let status = match applies {
            Ok(false) => ProposalStatus::Stale,
            Ok(true) => match self.apply(&change, env_fingerprint.as_deref()) {
                Ok(true) => ProposalStatus::Accepted,
                Ok(false) => ProposalStatus::Stale,
                Err(e) => ProposalStatus::Failed {
                    message: e.to_string(),
                },
            },
            Err(e) => ProposalStatus::Failed {
                message: e.to_string(),
            },
        };
        let resolved = finish(&mut store, &session, proposal_id, status)?;
        drop(store);
        self.publish_resolved(&resolved);
        Ok(resolved)
    }

    /// Marks a pending proposal rejected. Nothing is written.
    pub fn reject(&self, session_id: &str, proposal_id: &str) -> DomainResult<AgentProposal> {
        let mut store = self.lock();
        let resolved = finish(
            &mut store,
            session_id,
            proposal_id,
            ProposalStatus::Rejected,
        )?;
        drop(store);
        self.publish_resolved(&resolved);
        Ok(resolved)
    }

    /// Drops the session's proposals and refuses later proposals for it.
    /// Called from `TauriSessionCleanup` when the session ends. Safe to call
    /// more than once. No `AcpProposalResolved` event goes out for the
    /// dropped proposals: the UI drops the session's list when the session
    /// ends (see the plan index).
    pub fn clear_session(&self, session_id: &str) {
        let mut store = self.lock();
        if let Some(dropped) = store.sessions.remove(session_id) {
            for proposal in dropped {
                store.meta.remove(&proposal.id);
            }
        }
        store.ended.insert(session_id.to_string());
    }

    fn publish_resolved(&self, proposal: &AgentProposal) {
        self.events.publish(DomainEvent::AcpProposalResolved {
            session_id: proposal.session_id.clone(),
            proposal_id: proposal.id.clone(),
            status: proposal.status.as_str().to_string(),
        });
    }

    fn tree(&self, collection: &str) -> DomainResult<Folder> {
        Ok(self.collections.get_summaries(collection)?.root)
    }

    /// Checks one change against the workspace as it is now and fills in
    /// its base fingerprint. Nothing is written.
    /// Returns the fingerprint of the one variable a `SetEnvVar` targets.
    fn prepare(
        &self,
        change: &mut ProposedChange,
        workspace: &[String],
    ) -> DomainResult<Option<String>> {
        normalize_paths(change);
        for path in paths_of(change) {
            validate_relative_path(path)?;
        }
        refuse_masked_values(change)?;
        let mut env_fingerprint = None;
        let collection = change.collection().to_string();
        if !workspace.iter().any(|name| *name == collection) {
            return Err(DomainError::InvalidInput(format!(
                "collection '{collection}' is not in this workspace"
            )));
        }
        let fingerprint = match &*change {
            ProposedChange::CreateFolder {
                parent_path, name, ..
            } => {
                validate_item_name(name)?;
                check_reserved_name(parent_path, name, true)?;
                self.check_free_target(&collection, parent_path, name)?;
                None
            }
            ProposedChange::CreateRequest {
                folder_path,
                request,
                ..
            } => {
                validate_item_name(&request.name)?;
                let file_name = request_filename_for(&request.name);
                check_reserved_name(folder_path, &file_name, false)?;
                self.check_free_target(&collection, folder_path, &file_name)?;
                None
            }
            ProposedChange::UpdateRequest {
                request_path,
                patch,
                ..
            } => {
                if patch.is_empty() {
                    return Err(DomainError::InvalidInput(
                        "the patch changes no field".to_string(),
                    ));
                }
                let root = self.tree(&collection)?;
                Some(self.http_request_fingerprint(&collection, &root, request_path)?)
            }
            ProposedChange::EditScript { request_path, .. } => {
                let root = self.tree(&collection)?;
                Some(self.http_request_fingerprint(&collection, &root, request_path)?)
            }
            ProposedChange::MoveItem {
                from_path,
                to_folder,
                ..
            } => {
                let root = self.tree(&collection)?;
                let destination = move_destination(from_path, to_folder)?;
                if !is_folder(&root, to_folder) {
                    return Err(DomainError::NotFound(format!(
                        "folder '{to_folder}' in '{collection}'"
                    )));
                }
                let moved_is_folder = is_folder(&root, from_path);
                check_reserved_name(to_folder, last_segment(&destination), moved_is_folder)?;
                if !self.target_free(&collection, &root, &destination)? {
                    return Err(DomainError::AlreadyExists(format!(
                        "'{destination}' in '{collection}'"
                    )));
                }
                Some(self.item_fingerprint(&collection, &root, from_path)?)
            }
            ProposedChange::RenameItem { path, new_name, .. } => {
                validate_item_name(new_name)?;
                let root = self.tree(&collection)?;
                if is_folder(&root, path) {
                    let destination = join_path(parent_of(path), new_name);
                    check_reserved_name(parent_of(path), new_name, true)?;
                    if !self.target_free(&collection, &root, &destination)? {
                        return Err(DomainError::AlreadyExists(format!(
                            "'{destination}' in '{collection}'"
                        )));
                    }
                }
                Some(self.item_fingerprint(&collection, &root, path)?)
            }
            ProposedChange::SetEnvVar {
                environment, key, ..
            } => {
                validate_environment_name(environment)?;
                if key.trim().is_empty() {
                    return Err(DomainError::InvalidInput(
                        "variable name cannot be empty".to_string(),
                    ));
                }
                let env = self
                    .environment_repo_factory
                    .for_collection(&collection)
                    .get(environment)?;
                if env.variables.iter().any(|v| v.key == *key && v.secret) {
                    return Err(DomainError::InvalidInput(
                        VARIABLE_NOT_ACCESSIBLE.to_string(),
                    ));
                }
                env_fingerprint = Some(env_var_fingerprint(&env, key));
                None
            }
        };
        if let Some(fingerprint) = fingerprint {
            change.set_base_fingerprint(fingerprint);
        }
        Ok(env_fingerprint)
    }

    /// True when nothing is at `path`: not in the summaries tree (compared
    /// case-folded) and not on disk either. The tree hides reserved
    /// directories, symlinks and script files, so it alone says "free" too
    /// often.
    fn target_free(&self, collection: &str, root: &Folder, path: &str) -> DomainResult<bool> {
        if !is_free(root, path) {
            return Ok(false);
        }
        Ok(!self.collections.path_exists(collection, path)?)
    }

    /// The parent must be a folder and `parent/name` must be unused.
    fn check_free_target(&self, collection: &str, parent: &str, name: &str) -> DomainResult<()> {
        let root = self.tree(collection)?;
        if !is_folder(&root, parent) {
            return Err(DomainError::NotFound(format!(
                "folder '{parent}' in '{collection}'"
            )));
        }
        let target = join_path(parent, name);
        if !self.target_free(collection, &root, &target)? {
            return Err(DomainError::AlreadyExists(format!(
                "'{target}' in '{collection}'"
            )));
        }
        Ok(())
    }

    fn http_request_fingerprint(
        &self,
        collection: &str,
        root: &Folder,
        path: &str,
    ) -> DomainResult<String> {
        match locate(root, path) {
            Target::HttpRequest => {
                let request = self.collections.get_request(collection, path)?;
                Ok(request_fingerprint(&request))
            }
            Target::Folder(_) => Err(DomainError::InvalidInput(format!(
                "'{path}' is a folder, not a request"
            ))),
            Target::Other => Err(DomainError::InvalidInput(format!(
                "'{path}' is not an HTTP request; proposals change HTTP requests and folders only"
            ))),
            Target::Missing => Err(DomainError::NotFound(format!("'{path}' in '{collection}'"))),
        }
    }

    fn item_fingerprint(
        &self,
        collection: &str,
        root: &Folder,
        path: &str,
    ) -> DomainResult<String> {
        if path.is_empty() {
            return Err(DomainError::InvalidInput(
                "the collection root cannot be moved or renamed".to_string(),
            ));
        }
        match locate(root, path) {
            Target::Folder(shape) => Ok(sha256_hex(&format!("folder\n{shape}"))),
            _ => self.http_request_fingerprint(collection, root, path),
        }
    }

    /// True when the target is still as it was when the change was proposed.
    fn still_applies(&self, change: &ProposedChange) -> DomainResult<bool> {
        match change {
            ProposedChange::CreateFolder {
                collection,
                parent_path,
                name,
            } => {
                let root = self.tree(collection)?;
                Ok(is_folder(&root, parent_path)
                    && self.target_free(collection, &root, &join_path(parent_path, name))?)
            }
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => {
                let root = self.tree(collection)?;
                let target = join_path(folder_path, &request_filename_for(&request.name));
                Ok(is_folder(&root, folder_path)
                    && self.target_free(collection, &root, &target)?)
            }
            ProposedChange::UpdateRequest {
                collection,
                request_path,
                base_fingerprint,
                ..
            }
            | ProposedChange::EditScript {
                collection,
                request_path,
                base_fingerprint,
                ..
            } => {
                let root = self.tree(collection)?;
                unchanged(
                    self.http_request_fingerprint(collection, &root, request_path),
                    base_fingerprint,
                )
            }
            ProposedChange::MoveItem {
                collection,
                from_path,
                to_folder,
                base_fingerprint,
            } => {
                let root = self.tree(collection)?;
                let destination = move_destination(from_path, to_folder)?;
                if !is_folder(&root, to_folder)
                    || !self.target_free(collection, &root, &destination)?
                {
                    return Ok(false);
                }
                unchanged(
                    self.item_fingerprint(collection, &root, from_path),
                    base_fingerprint,
                )
            }
            ProposedChange::RenameItem {
                collection,
                path,
                new_name,
                base_fingerprint,
            } => {
                let root = self.tree(collection)?;
                if is_folder(&root, path)
                    && !self.target_free(collection, &root, &join_path(parent_of(path), new_name))?
                {
                    return Ok(false);
                }
                unchanged(
                    self.item_fingerprint(collection, &root, path),
                    base_fingerprint,
                )
            }
            // `apply_env_var` compares the variable's fingerprint on the copy
            // it saves.
            ProposedChange::SetEnvVar { .. } => Ok(true),
        }
    }

    /// Reads the request and returns that exact copy only if it still matches
    /// the proposal's fingerprint. The caller patches and writes this copy, so
    /// no second unchecked read sits between the check and the write.
    fn checked_request(
        &self,
        collection: &str,
        request_path: &str,
        base_fingerprint: &str,
    ) -> DomainResult<Option<Request>> {
        let request = self.collections.get_request(collection, request_path)?;
        if request_fingerprint(&request) == base_fingerprint {
            Ok(Some(request))
        } else {
            Ok(None)
        }
    }

    /// Applies one change through the manual-edit services. Returns `false`
    /// without writing when the target no longer matches the proposal.
    fn apply(&self, change: &ProposedChange, env_fingerprint: Option<&str>) -> DomainResult<bool> {
        let applied = |result: DomainResult<()>| result.map(|()| true);
        match change {
            ProposedChange::CreateFolder {
                collection,
                parent_path,
                name,
            } => applied(
                self.collections
                    .create_folder(collection, &join_path(parent_path, name)),
            ),
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => {
                let path = join_path(folder_path, &request_filename_for(&request.name));
                applied(
                    self.collections
                        .save_request(collection, &path, &build_request(request))
                        .map(|_| ()),
                )
            }
            ProposedChange::UpdateRequest {
                collection,
                request_path,
                patch,
                base_fingerprint,
            } => {
                let Some(mut request) =
                    self.checked_request(collection, request_path, base_fingerprint)?
                else {
                    return Ok(false);
                };
                apply_patch(&mut request, patch);
                applied(
                    self.collections
                        .save_request(collection, request_path, &request)
                        .map(|_| ()),
                )
            }
            ProposedChange::EditScript {
                collection,
                request_path,
                phase,
                body,
                base_fingerprint,
            } => {
                let Some(mut request) =
                    self.checked_request(collection, request_path, base_fingerprint)?
                else {
                    return Ok(false);
                };
                set_script(&mut request, *phase, body.clone());
                applied(
                    self.collections
                        .save_request(collection, request_path, &request)
                        .map(|_| ()),
                )
            }
            ProposedChange::MoveItem {
                collection,
                from_path,
                to_folder,
                ..
            } => {
                let destination = move_destination(from_path, to_folder)?;
                applied(
                    self.collections
                        .move_item(collection, from_path, collection, &destination),
                )
            }
            ProposedChange::RenameItem {
                collection,
                path,
                new_name,
                ..
            } => {
                let root = self.tree(collection)?;
                if is_folder(&root, path) {
                    // A folder is renamed by moving it, like the sidebar does.
                    applied(self.collections.move_item(
                        collection,
                        path,
                        collection,
                        &join_path(parent_of(path), new_name),
                    ))
                } else {
                    applied(self.collections.rename_request(collection, path, new_name))
                }
            }
            ProposedChange::SetEnvVar {
                collection,
                environment,
                key,
                value,
            } => self.apply_env_var(collection, environment, key, value, env_fingerprint),
        }
    }

    /// Writes one non-secret variable through `EnvironmentService`, so the
    /// usual validation and `EnvironmentSaved` event apply.
    /// Returns `false` without writing when the variable changed since the
    /// proposal. The fingerprint is checked on the very copy that is saved.
    fn apply_env_var(
        &self,
        collection: &str,
        environment: &str,
        key: &str,
        value: &str,
        env_fingerprint: Option<&str>,
    ) -> DomainResult<bool> {
        let mut env = self
            .environment_repo_factory
            .for_collection(collection)
            .get(environment)?;
        if env_fingerprint != Some(env_var_fingerprint(&env, key).as_str()) {
            return Ok(false);
        }
        match env.variables.iter_mut().find(|v| v.key == key) {
            Some(variable) if variable.secret => {
                return Err(DomainError::InvalidInput(
                    VARIABLE_NOT_ACCESSIBLE.to_string(),
                ));
            }
            Some(variable) => variable.value = value.to_string(),
            None => env.variables.push(Variable::new(key, value)),
        }
        EnvironmentService::new(
            self.environment_repo_factory.for_collection(collection),
            Box::new(ForwardEvents(Arc::clone(&self.events))),
        )
        .save(&env)
        .map(|()| true)
    }
}

/// The pending proposal with this id, or an error that says why not.
fn pending_mut<'a>(
    store: &'a mut Store,
    session: &str,
    proposal_id: &str,
) -> DomainResult<&'a mut AgentProposal> {
    let proposal = store
        .sessions
        .get_mut(session)
        .and_then(|list| list.iter_mut().find(|p| p.id == proposal_id))
        .ok_or_else(|| DomainError::NotFound(format!("proposal '{proposal_id}'")))?;
    if proposal.status != ProposalStatus::Pending {
        return Err(DomainError::InvalidInput(format!(
            "proposal '{proposal_id}' is already {}",
            proposal.status.as_str()
        )));
    }
    Ok(proposal)
}

/// Sets the final status of a pending proposal and returns a copy.
fn finish(
    store: &mut Store,
    session: &str,
    proposal_id: &str,
    status: ProposalStatus,
) -> DomainResult<AgentProposal> {
    let proposal = pending_mut(store, session, proposal_id)?;
    proposal.status = status;
    let resolved = proposal.clone();
    store.meta.remove(proposal_id);
    if let Some(list) = store.sessions.get_mut(session) {
        prune_resolved(list);
    }
    Ok(resolved)
}

/// Drops the oldest answered proposals beyond `MAX_RESOLVED_PER_SESSION`.
fn prune_resolved(list: &mut Vec<AgentProposal>) {
    let resolved = list
        .iter()
        .filter(|p| p.status != ProposalStatus::Pending)
        .count();
    let mut excess = resolved.saturating_sub(MAX_RESOLVED_PER_SESSION);
    list.retain(|p| {
        if excess > 0 && p.status != ProposalStatus::Pending {
            excess -= 1;
            false
        } else {
            true
        }
    });
}

/// Compares a fresh fingerprint with the proposal's. A target that is gone
/// or changed kind counts as changed; other errors are real failures.
fn unchanged(current: DomainResult<String>, base: &str) -> DomainResult<bool> {
    match current {
        Ok(fingerprint) => Ok(fingerprint == base),
        Err(DomainError::NotFound(_)) | Err(DomainError::InvalidInput(_)) => Ok(false),
        Err(other) => Err(other),
    }
}

/// Lowercase hex SHA-256 of `text`.
fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The fingerprint of a request's stored form. `uid`, `file_name` and `seq`
/// are left out, so a reload of a file without a stored uid is not a change.
fn request_fingerprint(request: &Request) -> String {
    sha256_hex(&saved_request_text(request))
}

/// A folder's direct children, sorted, one per line.
fn folder_shape(folder: &Folder) -> String {
    let mut lines: Vec<String> = folder
        .items
        .iter()
        .filter_map(|item| match item {
            CollectionItem::Folder(sub) => Some(format!("folder:{}", folder_dir_name(sub))),
            CollectionItem::Summary(summary) => Some(format!(
                "request:{}:{}:{}:{}",
                summary.file_name.as_deref().unwrap_or_default(),
                summary.name,
                summary.method,
                summary.url
            )),
            CollectionItem::ScriptFile(file) => Some(format!("script:{}", file.file_name)),
            _ => None,
        })
        .collect();
    lines.sort();
    lines.join("\n")
}

/// Compares two names. With `fold`, case is ignored, so a case variant
/// counts as the same name on every filesystem.
fn same_name(fold: bool, a: &str, b: &str) -> bool {
    if fold {
        a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

fn child_folder<'a>(folder: &'a Folder, name: &str, fold: bool) -> Option<&'a Folder> {
    folder.items.iter().find_map(|item| match item {
        CollectionItem::Folder(sub) if same_name(fold, &folder_dir_name(sub), name) => Some(sub),
        _ => None,
    })
}

/// Finds what `path` points at. `""` is the collection root.
fn locate(root: &Folder, path: &str) -> Target {
    locate_with(root, path, false)
}

fn locate_with(root: &Folder, path: &str, fold: bool) -> Target {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let Some((last, parents)) = segments.split_last() else {
        return Target::Folder(folder_shape(root));
    };
    let mut folder = root;
    for segment in parents {
        match child_folder(folder, segment, fold) {
            Some(next) => folder = next,
            None => return Target::Missing,
        }
    }
    if let Some(found) = child_folder(folder, last, fold) {
        return Target::Folder(folder_shape(found));
    }
    for item in &folder.items {
        match item {
            CollectionItem::Summary(summary)
                if summary
                    .file_name
                    .as_deref()
                    .is_some_and(|n| same_name(fold, n, last)) =>
            {
                return if summary.kind.is_http() {
                    Target::HttpRequest
                } else {
                    Target::Other
                };
            }
            CollectionItem::ScriptFile(file) if same_name(fold, &file.file_name, last) => {
                return Target::Other;
            }
            _ => {}
        }
    }
    Target::Missing
}

fn is_folder(root: &Folder, path: &str) -> bool {
    matches!(locate(root, path), Target::Folder(_))
}

/// Free in the tree, with names compared case-folded.
fn is_free(root: &Folder, path: &str) -> bool {
    matches!(locate_with(root, path, true), Target::Missing)
}

fn last_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Refuses names the collection layout reserves or the tree never shows, so
/// an accepted change cannot overwrite settings or land where it is hidden.
fn check_reserved_name(parent: &str, name: &str, is_dir: bool) -> DomainResult<()> {
    let lower = name.to_lowercase();
    let reserved = if is_dir {
        RESERVED_DIR_NAMES.contains(&lower.as_str()) || (parent.is_empty() && lower == "flows")
    } else {
        RESERVED_FILE_NAMES.contains(&lower.as_str())
    };
    if reserved {
        return Err(DomainError::InvalidInput(format!(
            "'{name}' is a reserved name; choose another"
        )));
    }
    Ok(())
}

/// Fingerprint of one variable: whether it exists, its value, enabled flag
/// and secret flag.
fn env_var_fingerprint(env: &Environment, key: &str) -> String {
    let text = match env.variables.iter().find(|v| v.key == key) {
        Some(v) => format!("1\n{}\n{}\n{}", v.value, v.enabled, v.secret),
        None => "0".to_string(),
    };
    sha256_hex(&text)
}

/// Refuses a value that carries the placeholder the read tools show for a
/// hidden credential. Saving it would overwrite the real value.
fn refuse_masked_values(change: &ProposedChange) -> DomainResult<()> {
    let masked = |text: &str| text.contains(REDACTED);
    let pair_masked = |key: &str, value: &str| masked(key) || masked(value);
    let body_masked = |body: &rocket_shared::types::Body| {
        body.content.as_deref().is_some_and(masked)
            || body.form_data.iter().flatten().any(|e| pair_masked(&e.key, &e.value))
    };
    let found = match change {
        ProposedChange::CreateRequest { request, .. } => {
            masked(&request.url)
                || request.headers.iter().any(|h| pair_masked(&h.key, &h.value))
                || request.query_params.iter().any(|q| pair_masked(&q.key, &q.value))
                || request.body.as_ref().is_some_and(body_masked)
        }
        ProposedChange::UpdateRequest { patch, .. } => {
            patch.url.as_deref().is_some_and(masked)
                || patch
                    .headers
                    .iter()
                    .flatten()
                    .any(|h| pair_masked(&h.key, &h.value))
                || patch
                    .query_params
                    .iter()
                    .flatten()
                    .any(|q| pair_masked(&q.key, &q.value))
                || patch.body.as_ref().is_some_and(body_masked)
        }
        _ => false,
    };
    if found {
        return Err(DomainError::InvalidInput(format!(
            "a value contains the masked placeholder '{REDACTED}', which stands for a hidden \
             credential; propose only the real values you know, or leave that field out"
        )));
    }
    Ok(())
}

/// `parent/name`, or `name` at the root. Both inputs are normalized.
fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// The folder that holds `path`, `""` for the root.
fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..index],
        None => "",
    }
}

/// Where `from_path` lands when moved into `to_folder`.
fn move_destination(from_path: &str, to_folder: &str) -> DomainResult<String> {
    if from_path.is_empty() {
        return Err(DomainError::InvalidInput(
            "the collection root cannot be moved".to_string(),
        ));
    }
    if to_folder == from_path || to_folder.starts_with(&format!("{from_path}/")) {
        return Err(DomainError::InvalidInput(
            "a folder cannot be moved into itself".to_string(),
        ));
    }
    if parent_of(from_path) == to_folder {
        return Err(DomainError::InvalidInput(
            "the item is already in that folder".to_string(),
        ));
    }
    let name = from_path.rsplit('/').next().unwrap_or(from_path);
    Ok(join_path(to_folder, name))
}

/// Drops empty segments, so `a//b`, `/a/b` and `a/b/` all become `a/b`.
fn normalize_path(path: &str) -> String {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// Normalizes every path in the change.
fn normalize_paths(change: &mut ProposedChange) {
    let trim = |path: &mut String| *path = normalize_path(path);
    match change {
        ProposedChange::CreateFolder { parent_path, .. } => trim(parent_path),
        ProposedChange::CreateRequest { folder_path, .. } => trim(folder_path),
        ProposedChange::UpdateRequest { request_path, .. }
        | ProposedChange::EditScript { request_path, .. } => trim(request_path),
        ProposedChange::MoveItem {
            from_path,
            to_folder,
            ..
        } => {
            trim(from_path);
            trim(to_folder);
        }
        ProposedChange::RenameItem { path, .. } => trim(path),
        ProposedChange::SetEnvVar { .. } => {}
    }
}

fn paths_of(change: &ProposedChange) -> Vec<&str> {
    match change {
        ProposedChange::CreateFolder { parent_path, .. } => vec![parent_path.as_str()],
        ProposedChange::CreateRequest { folder_path, .. } => vec![folder_path.as_str()],
        ProposedChange::UpdateRequest { request_path, .. }
        | ProposedChange::EditScript { request_path, .. } => vec![request_path.as_str()],
        ProposedChange::MoveItem {
            from_path,
            to_folder,
            ..
        } => vec![from_path.as_str(), to_folder.as_str()],
        ProposedChange::RenameItem { path, .. } => vec![path.as_str()],
        ProposedChange::SetEnvVar { .. } => vec![],
    }
}

/// Refuses paths that could leave the collection. The repository checks
/// again, so this only gives the agent a clear message early.
fn validate_relative_path(path: &str) -> DomainResult<()> {
    if path.contains(['\\', '\0']) || path.split('/').any(|s| s == ".." || s == ".") {
        return Err(DomainError::InvalidInput(
            "paths must stay inside the collection".to_string(),
        ));
    }
    Ok(())
}

/// A folder or request name must be one path segment.
fn validate_item_name(name: &str) -> DomainResult<()> {
    if name.trim().is_empty() || name.contains(['/', '\\', '\0']) || name.starts_with('.') {
        return Err(DomainError::InvalidInput(format!(
            "'{name}' is not a valid name"
        )));
    }
    Ok(())
}

/// Same rule as `McpToolService::validate_environment_name`.
fn validate_environment_name(name: &str) -> DomainResult<()> {
    if name.is_empty()
        || name.contains('\0')
        || name.starts_with('/')
        || name.starts_with('\\')
        || name.starts_with('.')
        || std::path::Path::new(name)
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    {
        return Err(DomainError::InvalidInput(
            "invalid environment name".to_string(),
        ));
    }
    Ok(())
}

fn set_script(request: &mut Request, phase: ScriptPhase, body: String) {
    match phase {
        ScriptPhase::PreRequest => request.pre_request_script = Some(body),
        ScriptPhase::PostResponse => request.post_response_script = Some(body),
        ScriptPhase::Tests => request.tests = Some(body),
    }
}

/// A body of mode `none` is stored as no body, like the request editor does.
fn stored_body(body: &rocket_shared::types::Body) -> Option<rocket_shared::types::Body> {
    if matches!(body.mode, BodyMode::None) {
        None
    } else {
        Some(body.clone())
    }
}

fn build_request(proposed: &ProposedRequest) -> Request {
    let mut request = Request::new(
        proposed.name.clone(),
        proposed.method.clone(),
        proposed.url.clone(),
    );
    request.headers = proposed.headers.clone();
    request.query_params = proposed.query_params.clone();
    request.body = proposed.body.as_ref().and_then(stored_body);
    request.docs = proposed.docs.clone().map(Documentation::text);
    request.pre_request_script = proposed.pre_request_script.clone();
    request.post_response_script = proposed.post_response_script.clone();
    request.tests = proposed.tests.clone();
    request
}

fn apply_patch(request: &mut Request, patch: &RequestPatch) {
    if let Some(method) = &patch.method {
        request.method = method.clone();
    }
    if let Some(url) = &patch.url {
        request.url = url.clone();
    }
    if let Some(headers) = &patch.headers {
        let mut merged = headers.clone();
        for header in &mut merged {
            if header.description.is_none() {
                header.description = request
                    .headers
                    .iter()
                    .find(|old| old.key.eq_ignore_ascii_case(&header.key))
                    .and_then(|old| old.description.clone());
            }
        }
        request.headers = merged;
    }
    if let Some(query_params) = &patch.query_params {
        let mut merged = query_params.clone();
        for param in &mut merged {
            if param.description.is_none() {
                param.description = request
                    .query_params
                    .iter()
                    .find(|old| old.key == param.key)
                    .and_then(|old| old.description.clone());
            }
        }
        request.query_params = merged;
    }
    if let Some(body) = &patch.body {
        request.body = stored_body(body);
    }
    if let Some(docs) = &patch.docs {
        request.docs = Some(Documentation::text(docs.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex as StdMutex;

    use rocket_collection::{CollectionRepository, RequestScriptPhase};
    use rocket_environment::{Environment, EnvironmentRepository};
    use rocket_shared::types::{Header, HttpMethod};

    use crate::test_doubles::{RecordingPublisher, SharedPublisher};

    /// Environments shared by every repository handle the factory gives out.
    #[derive(Default)]
    struct MemoryEnvs {
        envs: StdMutex<HashMap<String, Environment>>,
        fail_saves: AtomicBool,
    }

    struct MemoryEnvFactory(Arc<MemoryEnvs>);
    struct MemoryEnvRepo(Arc<MemoryEnvs>);

    impl EnvironmentRepositoryFactory for MemoryEnvFactory {
        fn for_collection(&self, _collection: &str) -> Box<dyn EnvironmentRepository> {
            Box::new(MemoryEnvRepo(Arc::clone(&self.0)))
        }
    }

    impl EnvironmentRepository for MemoryEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self
                .0
                .envs
                .lock()
                .expect("lock")
                .values()
                .cloned()
                .collect())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.0
                .envs
                .lock()
                .expect("lock")
                .get(name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.to_string()))
        }
        fn save(&self, env: &Environment) -> DomainResult<()> {
            if self.0.fail_saves.load(Ordering::SeqCst) {
                return Err(DomainError::Io("disk full".to_string()));
            }
            self.0
                .envs
                .lock()
                .expect("lock")
                .insert(env.name.clone(), env.clone());
            Ok(())
        }
        fn delete(&self, name: &str) -> DomainResult<()> {
            self.0.envs.lock().expect("lock").remove(name);
            Ok(())
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        repo: rocket_infra::FsCollectionRepo,
        envs: Arc<MemoryEnvs>,
        events: Arc<RecordingPublisher>,
        svc: ProposalService,
    }

    /// A "demo" collection with one request, `get-users.yml`, and a "dev"
    /// environment with a plain `HOST` and a secret `TOKEN`.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("demo").expect("create collection");
        repo.save_request(
            "demo",
            "get-users.yml",
            &Request::new(
                "Get Users",
                HttpMethod::Get,
                "https://api.example.com/users",
            ),
        )
        .expect("save request");

        let envs = Arc::new(MemoryEnvs::default());
        let mut dev = Environment::new("dev");
        dev.set_variable(Variable::new("HOST", "api.example.com"));
        let mut token = Variable::new("TOKEN", "s3cr3t");
        token.secret = true;
        dev.set_variable(token);
        envs.envs.lock().expect("lock").insert("dev".into(), dev);

        let events = RecordingPublisher::new();
        let svc = ProposalService::new(
            CollectionService::new(
                Box::new(rocket_infra::FsCollectionRepo::new_standalone(
                    dir.path().to_path_buf(),
                )),
                Box::new(SharedPublisher(Arc::clone(&events))),
            ),
            Arc::new(MemoryEnvFactory(Arc::clone(&envs))),
            events.clone(),
        );
        Fixture {
            _dir: dir,
            repo,
            envs,
            events,
            svc,
        }
    }

    fn folder(name: &str) -> ProposedChange {
        ProposedChange::CreateFolder {
            collection: "demo".into(),
            parent_path: String::new(),
            name: name.into(),
        }
    }

    fn url_patch(url: &str) -> ProposedChange {
        ProposedChange::UpdateRequest {
            collection: "demo".into(),
            request_path: "get-users.yml".into(),
            patch: RequestPatch {
                url: Some(url.into()),
                ..RequestPatch::default()
            },
            base_fingerprint: String::new(),
        }
    }

    fn set_var(key: &str, value: &str) -> ProposedChange {
        ProposedChange::SetEnvVar {
            collection: "demo".into(),
            environment: "dev".into(),
            key: key.into(),
            value: value.into(),
        }
    }

    fn resolved_statuses(events: &RecordingPublisher) -> Vec<String> {
        events
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::AcpProposalResolved { status, .. } => Some(status),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn propose_queues_a_pending_proposal_and_writes_nothing() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![folder("reports")])
            .expect("propose");
        assert_eq!(ids.len(), 1);
        let listed = f.svc.list("s1");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, ids[0]);
        assert_eq!(listed[0].status, ProposalStatus::Pending);
        assert!(!f._dir.path().join("demo").join("reports").exists());
        assert!(f.events.events().iter().any(|e| matches!(
            e,
            DomainEvent::AcpProposalCreated { proposal_id, .. } if *proposal_id == ids[0]
        )));
    }

    #[test]
    fn accepting_a_create_folder_applies_it_through_collection_service() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![folder("reports")])
            .expect("propose");
        let resolved = f.svc.accept("s1", &ids[0]).expect("accept");
        assert_eq!(resolved.status, ProposalStatus::Accepted);
        assert!(f._dir.path().join("demo").join("reports").is_dir());
        assert!(f
            .events
            .events()
            .iter()
            .any(|e| matches!(e, DomainEvent::FolderCreated { path, .. } if path == "reports")));
        assert_eq!(resolved_statuses(&f.events), vec!["accepted".to_string()]);
    }

    #[test]
    fn creating_over_an_existing_folder_is_refused() {
        let f = fixture();
        f.repo
            .create_folder("demo", "reports")
            .expect("create folder");
        let err = f
            .svc
            .propose("s1", vec![folder("reports")])
            .expect_err("the folder already exists");
        assert!(matches!(err, DomainError::AlreadyExists(_)));
    }

    #[test]
    fn accepting_an_update_changes_only_the_patched_fields() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![url_patch("https://api.example.com/v2/users")])
            .expect("propose");
        f.svc.accept("s1", &ids[0]).expect("accept");
        let saved = f.repo.get_request("demo", "get-users.yml").expect("load");
        assert_eq!(saved.url, "https://api.example.com/v2/users");
        assert_eq!(saved.name, "Get Users");
        assert_eq!(saved.method, HttpMethod::Get);
    }

    #[test]
    fn accept_marks_stale_when_the_request_changed_after_proposing() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![url_patch("https://api.example.com/v2/users")])
            .expect("propose");
        let mut manual = f.repo.get_request("demo", "get-users.yml").expect("load");
        manual.url = "https://manual.example.com".into();
        f.repo
            .save_request("demo", "get-users.yml", &manual)
            .expect("manual save");

        let resolved = f.svc.accept("s1", &ids[0]).expect("accept");
        assert_eq!(resolved.status, ProposalStatus::Stale);
        let saved = f.repo.get_request("demo", "get-users.yml").expect("load");
        assert_eq!(
            saved.url, "https://manual.example.com",
            "a stale proposal writes nothing"
        );
        assert_eq!(resolved_statuses(&f.events), vec!["stale".to_string()]);
    }

    #[test]
    fn request_fingerprint_ignores_uid_file_name_and_seq() {
        let a = Request::new("A", HttpMethod::Get, "https://x/a");
        let mut b = a.clone();
        b.uid = "another-uid".into();
        b.file_name = Some("a.yml".into());
        b.seq = Some(3);
        assert_eq!(request_fingerprint(&a), request_fingerprint(&b));
        assert_eq!(request_fingerprint(&a).len(), 64);
        b.headers.push(Header::new("Accept", "application/json"));
        assert_ne!(request_fingerprint(&a), request_fingerprint(&b));
    }

    #[test]
    fn accepting_an_edit_script_saves_only_that_phase() {
        let f = fixture();
        let ids = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::EditScript {
                    collection: "demo".into(),
                    request_path: "get-users.yml".into(),
                    phase: ScriptPhase::Tests,
                    body: "rok.test('ok', () => {});".into(),
                    base_fingerprint: String::new(),
                }],
            )
            .expect("propose");
        f.svc.accept("s1", &ids[0]).expect("accept");
        let saved = f.repo.get_request("demo", "get-users.yml").expect("load");
        assert_eq!(saved.tests.as_deref(), Some("rok.test('ok', () => {});"));
        assert_eq!(saved.pre_request_script, None);
        assert!(f.events.events().iter().any(|e| matches!(
            e,
            DomainEvent::RequestSaved { path, .. } if path == "get-users.yml"
        )));
    }

    #[test]
    fn move_and_rename_are_applied() {
        let f = fixture();
        f.repo
            .create_folder("demo", "archive")
            .expect("create folder");
        let ids = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::MoveItem {
                    collection: "demo".into(),
                    from_path: "get-users.yml".into(),
                    to_folder: "archive".into(),
                    base_fingerprint: String::new(),
                }],
            )
            .expect("propose move");
        f.svc.accept("s1", &ids[0]).expect("accept move");
        assert!(f.repo.get_request("demo", "archive/get-users.yml").is_ok());

        let ids = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::RenameItem {
                    collection: "demo".into(),
                    path: "archive/get-users.yml".into(),
                    new_name: "List Users".into(),
                    base_fingerprint: String::new(),
                }],
            )
            .expect("propose rename");
        f.svc.accept("s1", &ids[0]).expect("accept rename");
        let saved = f
            .repo
            .get_request("demo", "archive/get-users.yml")
            .expect("load");
        assert_eq!(saved.name, "List Users");

        let ids = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::RenameItem {
                    collection: "demo".into(),
                    path: "archive".into(),
                    new_name: "old".into(),
                    base_fingerprint: String::new(),
                }],
            )
            .expect("propose folder rename");
        f.svc.accept("s1", &ids[0]).expect("accept folder rename");
        assert!(f.repo.get_request("demo", "old/get-users.yml").is_ok());
    }

    #[test]
    fn a_batch_with_one_invalid_change_stores_nothing() {
        let f = fixture();
        let missing = ProposedChange::UpdateRequest {
            collection: "demo".into(),
            request_path: "missing.yml".into(),
            patch: RequestPatch {
                url: Some("https://x".into()),
                ..RequestPatch::default()
            },
            base_fingerprint: String::new(),
        };
        let err = f
            .svc
            .propose("s1", vec![folder("reports"), missing])
            .expect_err("the second change targets a missing request");
        assert!(matches!(err, DomainError::NotFound(_)));
        assert!(f.svc.list("s1").is_empty());
        assert!(f.events.events().is_empty());
    }

    #[test]
    fn the_pending_cap_is_fifty_and_counts_only_pending() {
        let f = fixture();
        let changes: Vec<ProposedChange> = (0..MAX_PENDING_PER_SESSION)
            .map(|i| folder(&format!("f{i}")))
            .collect();
        let ids = f.svc.propose("s1", changes).expect("fifty fit");
        let err = f
            .svc
            .propose("s1", vec![folder("one-more")])
            .expect_err("the fifty-first is refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        f.svc.reject("s1", &ids[0]).expect("reject one");
        f.svc
            .propose("s1", vec![folder("one-more")])
            .expect("a rejected proposal frees a slot");
    }

    #[test]
    fn an_oversized_batch_is_refused_before_any_target_is_read() {
        let f = fixture();
        // Each change names a missing collection, so reaching prepare would
        // give a different error than the cap message.
        let changes: Vec<ProposedChange> = (0..=MAX_PENDING_PER_SESSION)
            .map(|i| ProposedChange::CreateFolder {
                collection: "elsewhere".into(),
                parent_path: String::new(),
                name: format!("f{i}"),
            })
            .collect();
        let err = f
            .svc
            .propose("s1", changes)
            .expect_err("over the cap");
        assert!(err.to_string().contains("too many pending proposals"));
    }

    #[test]
    fn a_collection_outside_the_workspace_is_refused() {
        let f = fixture();
        let err = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::CreateFolder {
                    collection: "elsewhere".into(),
                    parent_path: String::new(),
                    name: "x".into(),
                }],
            )
            .expect_err("not in the workspace");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn a_path_that_climbs_out_of_the_collection_is_refused() {
        let f = fixture();
        let err = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::CreateFolder {
                    collection: "demo".into(),
                    parent_path: "../other".into(),
                    name: "x".into(),
                }],
            )
            .expect_err("traversal");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn set_env_var_refuses_a_secret_variable() {
        let f = fixture();
        let err = f
            .svc
            .propose("s1", vec![set_var("TOKEN", "new")])
            .expect_err("secret");
        assert!(err.to_string().contains("not accessible"));
        assert!(f.svc.list("s1").is_empty());
    }

    #[test]
    fn set_env_var_writes_a_plain_variable_and_can_add_one() {
        let f = fixture();
        let ids = f
            .svc
            .propose(
                "s1",
                vec![set_var("HOST", "api2.example.com"), set_var("REGION", "eu")],
            )
            .expect("propose");
        for id in &ids {
            f.svc.accept("s1", id).expect("accept");
        }
        let envs = f.envs.envs.lock().expect("lock");
        let dev = envs.get("dev").expect("dev");
        assert_eq!(dev.get_value("HOST"), Some("api2.example.com"));
        assert_eq!(dev.get_value("REGION"), Some("eu"));
        let token = dev
            .variables
            .iter()
            .find(|v| v.key == "TOKEN")
            .expect("token");
        assert_eq!(token.value, "s3cr3t", "other variables are kept");
    }

    #[test]
    fn set_env_var_fails_without_writing_when_the_variable_became_secret() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![set_var("HOST", "api2.example.com")])
            .expect("propose");
        {
            let mut envs = f.envs.envs.lock().expect("lock");
            let dev = envs.get_mut("dev").expect("dev");
            if let Some(host) = dev.variables.iter_mut().find(|v| v.key == "HOST") {
                host.secret = true;
            }
        }
        let resolved = f.svc.accept("s1", &ids[0]).expect("accept");
        assert!(matches!(resolved.status, ProposalStatus::Failed { .. }));
        assert!(resolved
            .status
            .message()
            .is_some_and(|m| m.contains("not accessible")));
        let envs = f.envs.envs.lock().expect("lock");
        let host = envs["dev"]
            .variables
            .iter()
            .find(|v| v.key == "HOST")
            .expect("host");
        assert_eq!(host.value, "api.example.com");
    }

    /// In-memory secret store, so the test never touches a real keychain.
    #[derive(Default)]
    struct MemorySecrets {
        values: StdMutex<HashMap<(String, String), String>>,
    }

    impl rocket_environment::SecretStore for MemorySecrets {
        fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
            Ok(self
                .values
                .lock()
                .expect("lock")
                .get(&(scope_id.to_string(), key.to_string()))
                .cloned())
        }
        fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
            self.values
                .lock()
                .expect("lock")
                .insert((scope_id.to_string(), key.to_string()), value.to_string());
            Ok(())
        }
        fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
            self.values
                .lock()
                .expect("lock")
                .remove(&(scope_id.to_string(), key.to_string()));
            Ok(())
        }
    }

    #[test]
    fn accepting_a_plain_set_env_var_keeps_a_secret_variable_value() {
        let f = fixture();
        let ws = tempfile::tempdir().expect("workspace dir");
        let factory = Arc::new(rocket_infra::SharedCollectionEnvironmentRepo::with_secret_store(
            Arc::new(StdMutex::new(ws.path().to_path_buf())),
            Arc::new(MemorySecrets::default()),
        ));
        let mut dev = Environment::new("dev");
        dev.set_variable(Variable::new("HOST", "api.example.com"));
        let mut token = Variable::new("TOKEN", "s3cr3t");
        token.secret = true;
        dev.set_variable(token);
        factory
            .for_collection("demo")
            .save(&dev)
            .expect("save environment");

        let svc = ProposalService::new(
            CollectionService::new(
                Box::new(rocket_infra::FsCollectionRepo::new_standalone(
                    f._dir.path().to_path_buf(),
                )),
                Box::new(SharedPublisher(Arc::clone(&f.events))),
            ),
            factory.clone(),
            f.events.clone(),
        );
        let ids = svc
            .propose("s1", vec![set_var("HOST", "api2.example.com")])
            .expect("propose");
        let resolved = svc.accept("s1", &ids[0]).expect("accept");
        assert_eq!(resolved.status, ProposalStatus::Accepted);

        let saved = factory.for_collection("demo").get("dev").expect("get");
        assert_eq!(saved.get_value("HOST"), Some("api2.example.com"));
        let token = saved
            .variables
            .iter()
            .find(|v| v.key == "TOKEN")
            .expect("token");
        assert!(token.secret);
        assert_eq!(token.value, "s3cr3t", "the secret value must survive");
    }

    #[test]
    fn a_failed_write_marks_the_proposal_failed() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![set_var("HOST", "api2.example.com")])
            .expect("propose");
        f.envs.fail_saves.store(true, Ordering::SeqCst);
        let resolved = f.svc.accept("s1", &ids[0]).expect("accept");
        assert!(resolved
            .status
            .message()
            .is_some_and(|m| m.contains("disk full")));
        assert_eq!(resolved_statuses(&f.events), vec!["failed".to_string()]);
    }

    #[test]
    fn reject_marks_rejected_and_a_second_answer_is_refused() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![folder("reports")])
            .expect("propose");
        let resolved = f.svc.reject("s1", &ids[0]).expect("reject");
        assert_eq!(resolved.status, ProposalStatus::Rejected);
        assert!(!f._dir.path().join("demo").join("reports").exists());
        let err = f.svc.accept("s1", &ids[0]).expect_err("already answered");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        let err = f.svc.accept("s1", "no-such-id").expect_err("unknown id");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn clear_session_drops_only_that_sessions_proposals() {
        let f = fixture();
        f.svc
            .propose("acp-1", vec![folder("reports")])
            .expect("propose");
        f.svc
            .propose("acp-2", vec![folder("other")])
            .expect("propose");
        let listed = f.svc.list("acp-1");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].session_id, "acp-1");

        f.svc.clear_session("acp-1");
        f.svc.clear_session("acp-1");
        assert!(f.svc.list("acp-1").is_empty());
        assert_eq!(f.svc.list("acp-2").len(), 1);
    }

    #[test]
    fn propose_is_refused_after_the_session_was_cleared() {
        let f = fixture();
        f.svc
            .propose("s1", vec![folder("reports")])
            .expect("propose");
        f.svc.clear_session("s1");
        let err = f
            .svc
            .propose("s1", vec![folder("late")])
            .expect_err("the session ended");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(f.svc.list("s1").is_empty(), "no entry is recreated");
        f.svc
            .propose("s2", vec![folder("other")])
            .expect("other sessions are unaffected");
    }

    fn create_request(name: &str) -> ProposedChange {
        ProposedChange::CreateRequest {
            collection: "demo".into(),
            folder_path: String::new(),
            request: ProposedRequest {
                name: name.into(),
                method: HttpMethod::Get,
                url: "https://x.example.com".into(),
                headers: vec![],
                query_params: vec![],
                body: None,
                docs: None,
                pre_request_script: None,
                post_response_script: None,
                tests: None,
            },
        }
    }

    #[test]
    fn a_request_named_like_a_reserved_file_is_refused() {
        let f = fixture();
        for name in ["opencollection", "folder", "_order", "Folder", "OpenCollection"] {
            let err = f
                .svc
                .propose("s1", vec![create_request(name)])
                .expect_err("reserved name");
            assert!(matches!(err, DomainError::InvalidInput(_)), "{name}");
        }
        let settings = f._dir.path().join("demo").join("opencollection.yml");
        assert!(settings.is_file(), "collection settings stay in place");
        assert!(f.svc.list("s1").is_empty());
    }

    #[test]
    fn a_folder_named_like_a_hidden_directory_is_refused() {
        let f = fixture();
        for name in ["environments", "node_modules", "flows", "Environments"] {
            let err = f
                .svc
                .propose("s1", vec![folder(name)])
                .expect_err("reserved directory");
            assert!(matches!(err, DomainError::InvalidInput(_)), "{name}");
        }
    }

    #[test]
    fn a_target_that_exists_on_disk_but_not_in_the_tree_is_refused() {
        let f = fixture();
        std::fs::create_dir_all(f._dir.path().join("demo").join("extra-env")).expect("dir");
        std::fs::write(f._dir.path().join("demo").join("helper.js"), "// x").expect("script");
        // A script file shows in the tree, so use a path the tree never lists.
        std::fs::write(f._dir.path().join("demo").join("notes.txt"), "x").expect("file");
        let err = f
            .svc
            .propose("s1", vec![folder("notes.txt")])
            .expect_err("a file already sits there");
        assert!(matches!(err, DomainError::AlreadyExists(_)));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_directory_is_never_a_free_target() {
        let f = fixture();
        let outside = tempfile::tempdir().expect("outside");
        std::os::unix::fs::symlink(outside.path(), f._dir.path().join("demo").join("linked"))
            .expect("symlink");
        let err = f
            .svc
            .propose("s1", vec![folder("linked")])
            .expect_err("the symlink occupies the name");
        assert!(matches!(err, DomainError::AlreadyExists(_)));
        let err = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::CreateFolder {
                    collection: "demo".into(),
                    parent_path: "linked".into(),
                    name: "inner".into(),
                }],
            )
            .expect_err("a symlink is no parent");
        assert!(matches!(
            err,
            DomainError::NotFound(_) | DomainError::InvalidInput(_)
        ));
    }

    #[test]
    fn a_case_variant_of_an_existing_name_is_refused() {
        let f = fixture();
        let err = f
            .svc
            .propose("s1", vec![create_request("Get-Users")])
            .expect_err("get-users.yml exists");
        assert!(matches!(err, DomainError::AlreadyExists(_)));

        f.repo
            .create_folder("demo", "reports")
            .expect("create folder");
        let err = f
            .svc
            .propose("s1", vec![folder("Reports")])
            .expect_err("reports exists");
        assert!(matches!(err, DomainError::AlreadyExists(_)));

        f.repo
            .create_folder("demo", "archive")
            .expect("create folder");
        f.repo
            .save_request(
                "demo",
                "archive/GET-USERS.yml",
                &Request::new("Other", HttpMethod::Get, "https://other"),
            )
            .expect("save request");
        let err = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::MoveItem {
                    collection: "demo".into(),
                    from_path: "get-users.yml".into(),
                    to_folder: "archive".into(),
                    base_fingerprint: String::new(),
                }],
            )
            .expect_err("a case variant sits in the destination");
        assert!(matches!(err, DomainError::AlreadyExists(_)));
    }

    #[test]
    fn a_folder_cannot_be_renamed_onto_a_reserved_or_taken_name() {
        let f = fixture();
        f.repo
            .create_folder("demo", "archive")
            .expect("create folder");
        f.repo
            .create_folder("demo", "Taken")
            .expect("create folder");
        for new_name in ["environments", "taken"] {
            let err = f
                .svc
                .propose(
                    "s1",
                    vec![ProposedChange::RenameItem {
                        collection: "demo".into(),
                        path: "archive".into(),
                        new_name: new_name.into(),
                        base_fingerprint: String::new(),
                    }],
                )
                .expect_err("refused");
            assert!(
                matches!(
                    err,
                    DomainError::InvalidInput(_) | DomainError::AlreadyExists(_)
                ),
                "{new_name}"
            );
        }
    }

    #[test]
    fn a_masked_placeholder_in_a_proposed_value_is_refused() {
        let f = fixture();
        let masked = format!("Bearer {REDACTED}");
        let mut with_header = url_patch("https://x");
        if let ProposedChange::UpdateRequest { patch, .. } = &mut with_header {
            patch.url = None;
            patch.headers = Some(vec![Header::new("Authorization", masked.clone())]);
        }
        let mut with_query = url_patch("https://x");
        if let ProposedChange::UpdateRequest { patch, .. } = &mut with_query {
            patch.url = None;
            patch.query_params = Some(vec![rocket_shared::types::QueryParam {
                key: "api_key".into(),
                value: REDACTED.into(),
                enabled: true,
                description: None,
            }]);
        }
        let with_url = url_patch(&format!("https://{REDACTED}@host.example.com"));
        let mut with_body = url_patch("https://x");
        if let ProposedChange::UpdateRequest { patch, .. } = &mut with_body {
            patch.url = None;
            patch.body = Some(rocket_shared::types::Body {
                mode: BodyMode::Json,
                content: Some(format!("{{\"token\":\"{REDACTED}\"}}")),
                form_data: None,
                file_path: None,
            });
        }
        let mut created = create_request("Fresh");
        if let ProposedChange::CreateRequest { request, .. } = &mut created {
            request.headers = vec![Header::new("X-Api-Key", REDACTED)];
        }
        for change in [with_header, with_query, with_url, with_body, created] {
            let err = f
                .svc
                .propose("s1", vec![change])
                .expect_err("masked value");
            assert!(matches!(err, DomainError::InvalidInput(_)));
            assert!(err.to_string().contains("masked placeholder"));
        }
        assert!(f.svc.list("s1").is_empty());
    }

    #[test]
    fn patching_headers_keeps_the_descriptions_of_headers_with_the_same_key() {
        let f = fixture();
        let mut stored = f.repo.get_request("demo", "get-users.yml").expect("load");
        let mut accept = Header::new("Accept", "text/plain");
        accept.description = Some(rocket_shared::description::Description::text("what we take"));
        stored.headers = vec![accept];
        f.repo
            .save_request("demo", "get-users.yml", &stored)
            .expect("save");
        let mut patch = url_patch("https://x");
        if let ProposedChange::UpdateRequest { patch, .. } = &mut patch {
            patch.url = None;
            patch.headers = Some(vec![Header::new("accept", "application/json")]);
        }
        let ids = f.svc.propose("s1", vec![patch]).expect("propose");
        f.svc.accept("s1", &ids[0]).expect("accept");
        let saved = f.repo.get_request("demo", "get-users.yml").expect("load");
        assert_eq!(saved.headers[0].value, "application/json");
        assert!(saved.headers[0].description.is_some());
    }

    #[test]
    fn a_proposal_is_stale_after_the_workspace_changed() {
        let f = fixture();
        let current = Arc::new(StdMutex::new("ws-a".to_string()));
        let reader = Arc::clone(&current);
        let svc = ProposalService::new(
            CollectionService::new(
                Box::new(rocket_infra::FsCollectionRepo::new_standalone(
                    f._dir.path().to_path_buf(),
                )),
                Box::new(SharedPublisher(Arc::clone(&f.events))),
            ),
            Arc::new(MemoryEnvFactory(Arc::clone(&f.envs))),
            f.events.clone(),
        )
        .with_workspace_identity(Arc::new(move || reader.lock().expect("lock").clone()));
        let ids = svc.propose("s1", vec![folder("reports")]).expect("propose");
        *current.lock().expect("lock") = "ws-b".to_string();
        let resolved = svc.accept("s1", &ids[0]).expect("accept");
        assert_eq!(resolved.status, ProposalStatus::Stale);
        assert!(!f._dir.path().join("demo").join("reports").exists());
    }

    #[test]
    fn set_env_var_is_stale_when_the_variable_changed_after_proposing() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![set_var("HOST", "api2.example.com")])
            .expect("propose");
        {
            let mut envs = f.envs.envs.lock().expect("lock");
            let dev = envs.get_mut("dev").expect("dev");
            if let Some(host) = dev.variables.iter_mut().find(|v| v.key == "HOST") {
                host.value = "changed.by.user".into();
            }
        }
        let resolved = f.svc.accept("s1", &ids[0]).expect("accept");
        assert_eq!(resolved.status, ProposalStatus::Stale);
        let envs = f.envs.envs.lock().expect("lock");
        assert_eq!(envs["dev"].get_value("HOST"), Some("changed.by.user"));
    }

    #[test]
    fn set_env_var_is_stale_when_a_new_variable_appeared_meanwhile() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![set_var("REGION", "eu")])
            .expect("propose");
        f.envs
            .envs
            .lock()
            .expect("lock")
            .get_mut("dev")
            .expect("dev")
            .set_variable(Variable::new("REGION", "us"));
        let resolved = f.svc.accept("s1", &ids[0]).expect("accept");
        assert_eq!(resolved.status, ProposalStatus::Stale);
    }

    #[test]
    fn paths_with_empty_segments_are_normalized() {
        assert_eq!(normalize_path("a//b"), "a/b");
        assert_eq!(normalize_path("/a/b/"), "a/b");
        assert_eq!(normalize_path("///"), "");
        let f = fixture();
        f.repo
            .create_folder("demo", "archive")
            .expect("create folder");
        let ids = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::CreateFolder {
                    collection: "demo".into(),
                    parent_path: "//archive//".into(),
                    name: "inner".into(),
                }],
            )
            .expect("propose");
        f.svc.accept("s1", &ids[0]).expect("accept");
        assert!(f._dir.path().join("demo/archive/inner").is_dir());
    }

    /// Shared state of `RacyRepo`.
    struct RaceState {
        /// Reads that still return the real request. After that, reads return
        /// a copy with a different url, as if the user saved in between.
        normal_reads_left: std::sync::atomic::AtomicUsize,
        writes: std::sync::atomic::AtomicUsize,
    }

    /// Wraps a real repo and drifts `get_request` after a set number of reads.
    struct RacyRepo {
        inner: rocket_infra::FsCollectionRepo,
        state: Arc<RaceState>,
    }

    impl CollectionRepository for RacyRepo {
        fn list(&self) -> DomainResult<Vec<rocket_collection::CollectionSummary>> {
            self.inner.list()
        }
        fn get(&self, name: &str) -> DomainResult<rocket_collection::Collection> {
            self.inner.get(name)
        }
        fn get_summaries(&self, name: &str) -> DomainResult<rocket_collection::Collection> {
            self.inner.get_summaries(name)
        }
        fn create(&self, name: &str) -> DomainResult<rocket_collection::Collection> {
            self.inner.create(name)
        }
        fn delete(&self, name: &str) -> DomainResult<()> {
            self.inner.delete(name)
        }
        fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
            self.inner.rename(old_name, new_name)
        }
        fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
            let mut request = self.inner.get_request(collection, path)?;
            let normal = self
                .state
                .normal_reads_left
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok();
            if !normal {
                request.url = "https://changed.example.com".to_string();
            }
            Ok(request)
        }
        fn save_request(
            &self,
            collection: &str,
            path: &str,
            request: &Request,
        ) -> DomainResult<String> {
            self.state.writes.fetch_add(1, Ordering::SeqCst);
            self.inner.save_request(collection, path, request)
        }
        fn rename_request(&self, c: &str, old: &str, new: &str) -> DomainResult<()> {
            self.inner.rename_request(c, old, new)
        }
        fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
            self.inner.delete_request(collection, path)
        }
        fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
            self.inner.create_folder(collection, path)
        }
        fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
            self.inner.delete_folder(collection, path)
        }
        fn move_item(&self, sc: &str, sp: &str, dc: &str, dp: &str) -> DomainResult<()> {
            self.inner.move_item(sc, sp, dc, dp)
        }
        fn reorder_items(&self, c: &str, folder: &str, names: &[String]) -> DomainResult<()> {
            self.inner.reorder_items(c, folder, names)
        }
        fn path_exists(&self, collection: &str, path: &str) -> DomainResult<bool> {
            self.inner.path_exists(collection, path)
        }
        fn get_settings(&self, name: &str) -> DomainResult<rocket_collection::CollectionSettings> {
            self.inner.get_settings(name)
        }
        fn save_settings(
            &self,
            name: &str,
            settings: &rocket_collection::CollectionSettings,
        ) -> DomainResult<()> {
            self.inner.save_settings(name, settings)
        }
        fn get_folder_chain_variables(
            &self,
            collection: &str,
            request_path: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            self.inner
                .get_folder_chain_variables(collection, request_path)
        }
        fn get_folder_variables(
            &self,
            collection: &str,
            folder_path: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            self.inner.get_folder_variables(collection, folder_path)
        }
        fn save_folder_variables(
            &self,
            collection: &str,
            folder_path: &str,
            vars: Vec<rocket_collection::CollectionVariable>,
        ) -> DomainResult<()> {
            self.inner
                .save_folder_variables(collection, folder_path, vars)
        }
        fn get_request_variables(
            &self,
            collection: &str,
            request_path: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            self.inner.get_request_variables(collection, request_path)
        }
        fn save_request_variables(
            &self,
            collection: &str,
            request_path: &str,
            vars: Vec<rocket_collection::CollectionVariable>,
        ) -> DomainResult<()> {
            self.inner
                .save_request_variables(collection, request_path, vars)
        }
        fn save_request_script(
            &self,
            collection: &str,
            request_path: &str,
            phase: RequestScriptPhase,
            body: String,
        ) -> DomainResult<()> {
            self.state.writes.fetch_add(1, Ordering::SeqCst);
            self.inner
                .save_request_script(collection, request_path, phase, body)
        }
    }

    /// A service whose request drifts after the check but before the write:
    /// the propose step and the accept check read normally, the apply read
    /// returns a different copy.
    fn racy_service() -> (tempfile::TempDir, Arc<RaceState>, ProposalService) {
        let dir = tempfile::tempdir().expect("tempdir");
        let seed = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        seed.create("demo").expect("create collection");
        seed.save_request(
            "demo",
            "get-users.yml",
            &Request::new(
                "Get Users",
                HttpMethod::Get,
                "https://api.example.com/users",
            ),
        )
        .expect("save request");
        let state = Arc::new(RaceState {
            normal_reads_left: std::sync::atomic::AtomicUsize::new(usize::MAX),
            writes: std::sync::atomic::AtomicUsize::new(0),
        });
        let events = RecordingPublisher::new();
        let svc = ProposalService::new(
            CollectionService::new(
                Box::new(RacyRepo {
                    inner: rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf()),
                    state: Arc::clone(&state),
                }),
                Box::new(SharedPublisher(Arc::clone(&events))),
            ),
            Arc::new(MemoryEnvFactory(Arc::new(MemoryEnvs::default()))),
            events,
        );
        (dir, state, svc)
    }

    fn assert_stale_when_the_request_drifts_before_the_write(change: ProposedChange) {
        let (_dir, state, svc) = racy_service();
        let ids = svc.propose("s1", vec![change]).expect("propose");
        // The accept check reads once (normal), the apply read then drifts.
        state.normal_reads_left.store(1, Ordering::SeqCst);
        let resolved = svc.accept("s1", &ids[0]).expect("accept");
        assert_eq!(resolved.status, ProposalStatus::Stale);
        assert_eq!(state.writes.load(Ordering::SeqCst), 0, "nothing is written");
    }

    #[test]
    fn an_update_is_stale_when_the_request_changes_between_check_and_write() {
        assert_stale_when_the_request_drifts_before_the_write(url_patch("https://x.example.com"));
    }

    #[test]
    fn an_edit_script_is_stale_when_the_request_changes_between_check_and_write() {
        assert_stale_when_the_request_drifts_before_the_write(ProposedChange::EditScript {
            collection: "demo".into(),
            request_path: "get-users.yml".into(),
            phase: ScriptPhase::Tests,
            body: "rok.test('ok', () => {});".into(),
            base_fingerprint: String::new(),
        });
    }
}
