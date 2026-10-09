//! Holds the assistant's proposed workspace changes until the user accepts
//! or rejects them. Nothing is written when a change is proposed. An
//! accepted change is applied through `CollectionService` or
//! `EnvironmentService`, the same paths as a manual edit, so name checks
//! and events stay the same. Proposals live in memory only.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rocket_acp::proposal::{
    AgentProposal, ProposalStatus, ProposedChange, ProposedRequest, RequestPatch, ScriptPhase,
};
use rocket_collection::{
    request_filename_for, CollectionItem, Folder, Request, RequestScriptPhase,
};
use rocket_environment::{EnvironmentRepositoryFactory, Variable};
use rocket_shared::description::Documentation;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_shared::types::BodyMode;
use sha2::{Digest, Sha256};

use crate::collection_service::CollectionService;
use crate::environment_service::EnvironmentService;
use crate::flow_run_cache::saved_request_text;
use crate::runner_sequence::folder_dir_name;

/// How many proposals may wait for the user in one session.
pub const MAX_PENDING_PER_SESSION: usize = 50;

/// How many answered proposals one session keeps for `list`. The oldest go
/// first.
pub const MAX_RESOLVED_PER_SESSION: usize = 100;

/// One error for a secret variable, matching the old MCP tool's wording.
const VARIABLE_NOT_ACCESSIBLE: &str = "variable not accessible";

pub struct ProposalService {
    collections: CollectionService,
    environment_repo_factory: Arc<dyn EnvironmentRepositoryFactory>,
    events: Arc<dyn EventPublisher>,
    store: Mutex<Store>,
}

#[derive(Default)]
struct Store {
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
            store: Mutex::new(Store::default()),
        }
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
        let workspace: Vec<String> = self
            .collections
            .list()?
            .into_iter()
            .map(|summary| summary.name)
            .collect();
        let mut prepared = Vec::with_capacity(changes.len());
        for mut change in changes {
            self.prepare(&mut change, &workspace)?;
            prepared.push(change);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let mut store = self.lock();
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
        for change in prepared {
            let proposal =
                AgentProposal::new(ulid::Ulid::new().to_string(), session.clone(), change, now);
            created.push((proposal.id.clone(), proposal.summary.clone()));
            list.push(proposal);
        }
        prune_resolved(list);
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
    /// the proposal `Stale`; a failed write makes it `Failed`. Either way
    /// nothing is written.
    pub fn accept(&self, session_id: &str, proposal_id: &str) -> DomainResult<AgentProposal> {
        let mut store = self.lock();
        let session = session_id.to_string();
        let change = pending_mut(&mut store, &session, proposal_id)?
            .change
            .clone();
        // The lock stays held while the change is applied, so a second
        // accept of the same proposal waits and then finds it answered.
        let status = match self.still_applies(&change) {
            Ok(false) => ProposalStatus::Stale,
            Ok(true) => match self.apply(&change) {
                Ok(()) => ProposalStatus::Accepted,
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

    /// Drops the session's proposals. Called from `TauriSessionCleanup` when
    /// the session ends. Safe to call more than once.
    pub fn clear_session(&self, session_id: &str) {
        self.lock().sessions.remove(session_id);
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
    fn prepare(&self, change: &mut ProposedChange, workspace: &[String]) -> DomainResult<()> {
        normalize_paths(change);
        for path in paths_of(change) {
            validate_relative_path(path)?;
        }
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
                self.check_free_target(&collection, parent_path, name)?;
                None
            }
            ProposedChange::CreateRequest {
                folder_path,
                request,
                ..
            } => {
                validate_item_name(&request.name)?;
                self.check_free_target(
                    &collection,
                    folder_path,
                    &request_filename_for(&request.name),
                )?;
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
                if !is_free(&root, &destination) {
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
                    if !is_free(&root, &destination) {
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
                None
            }
        };
        if let Some(fingerprint) = fingerprint {
            change.set_base_fingerprint(fingerprint);
        }
        Ok(())
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
        if !is_free(&root, &target) {
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
                Ok(is_folder(&root, parent_path) && is_free(&root, &join_path(parent_path, name)))
            }
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => {
                let root = self.tree(collection)?;
                let target = join_path(folder_path, &request_filename_for(&request.name));
                Ok(is_folder(&root, folder_path) && is_free(&root, &target))
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
                if !is_folder(&root, to_folder) || !is_free(&root, &destination) {
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
                if is_folder(&root, path) && !is_free(&root, &join_path(parent_of(path), new_name))
                {
                    return Ok(false);
                }
                unchanged(
                    self.item_fingerprint(collection, &root, path),
                    base_fingerprint,
                )
            }
            // Environment writes carry no fingerprint. `apply_env_var`
            // re-checks the secret flag itself.
            ProposedChange::SetEnvVar { .. } => Ok(true),
        }
    }

    /// Applies one change with one write through the manual-edit services.
    fn apply(&self, change: &ProposedChange) -> DomainResult<()> {
        match change {
            ProposedChange::CreateFolder {
                collection,
                parent_path,
                name,
            } => self
                .collections
                .create_folder(collection, &join_path(parent_path, name)),
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => {
                let path = join_path(folder_path, &request_filename_for(&request.name));
                self.collections
                    .save_request(collection, &path, &build_request(request))
                    .map(|_| ())
            }
            ProposedChange::UpdateRequest {
                collection,
                request_path,
                patch,
                ..
            } => {
                let mut request = self.collections.get_request(collection, request_path)?;
                apply_patch(&mut request, patch);
                self.collections
                    .save_request(collection, request_path, &request)
                    .map(|_| ())
            }
            ProposedChange::EditScript {
                collection,
                request_path,
                phase,
                body,
                ..
            } => self.collections.save_request_script(
                collection,
                request_path,
                collection_phase(*phase),
                body.clone(),
            ),
            ProposedChange::MoveItem {
                collection,
                from_path,
                to_folder,
                ..
            } => {
                let destination = move_destination(from_path, to_folder)?;
                self.collections
                    .move_item(collection, from_path, collection, &destination)
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
                    self.collections.move_item(
                        collection,
                        path,
                        collection,
                        &join_path(parent_of(path), new_name),
                    )
                } else {
                    self.collections.rename_request(collection, path, new_name)
                }
            }
            ProposedChange::SetEnvVar {
                collection,
                environment,
                key,
                value,
            } => self.apply_env_var(collection, environment, key, value),
        }
    }

    /// Writes one non-secret variable through `EnvironmentService`, so the
    /// usual validation and `EnvironmentSaved` event apply.
    fn apply_env_var(
        &self,
        collection: &str,
        environment: &str,
        key: &str,
        value: &str,
    ) -> DomainResult<()> {
        let mut env = self
            .environment_repo_factory
            .for_collection(collection)
            .get(environment)?;
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

fn child_folder<'a>(folder: &'a Folder, name: &str) -> Option<&'a Folder> {
    folder.items.iter().find_map(|item| match item {
        CollectionItem::Folder(sub) if folder_dir_name(sub) == name => Some(sub),
        _ => None,
    })
}

/// Finds what `path` points at. `""` is the collection root.
fn locate(root: &Folder, path: &str) -> Target {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let Some((last, parents)) = segments.split_last() else {
        return Target::Folder(folder_shape(root));
    };
    let mut folder = root;
    for segment in parents {
        match child_folder(folder, segment) {
            Some(next) => folder = next,
            None => return Target::Missing,
        }
    }
    if let Some(found) = child_folder(folder, last) {
        return Target::Folder(folder_shape(found));
    }
    for item in &folder.items {
        match item {
            CollectionItem::Summary(summary) if summary.file_name.as_deref() == Some(*last) => {
                return if summary.kind.is_http() {
                    Target::HttpRequest
                } else {
                    Target::Other
                };
            }
            CollectionItem::ScriptFile(file) if file.file_name == *last => return Target::Other,
            _ => {}
        }
    }
    Target::Missing
}

fn is_folder(root: &Folder, path: &str) -> bool {
    matches!(locate(root, path), Target::Folder(_))
}

fn is_free(root: &Folder, path: &str) -> bool {
    matches!(locate(root, path), Target::Missing)
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

/// Strips leading and trailing slashes from every path in the change.
fn normalize_paths(change: &mut ProposedChange) {
    let trim = |path: &mut String| *path = path.trim_matches('/').to_string();
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

fn collection_phase(phase: ScriptPhase) -> RequestScriptPhase {
    match phase {
        ScriptPhase::PreRequest => RequestScriptPhase::PreRequest,
        ScriptPhase::PostResponse => RequestScriptPhase::PostResponse,
        ScriptPhase::Tests => RequestScriptPhase::Tests,
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
        request.headers = headers.clone();
    }
    if let Some(query_params) = &patch.query_params {
        request.query_params = query_params.clone();
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

    use rocket_collection::CollectionRepository;
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
}
