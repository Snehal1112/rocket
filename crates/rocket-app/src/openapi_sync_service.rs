//! Checks a collection's OpenAPI source for a newer spec and keeps the GitHub token for it.
//!
//! A check never changes the collection. A newer valid spec is kept as a pending copy next to
//! the source file. If a fetch or parse fails, the last good copy stays and nothing is applied.
//! The token lives in the OS keychain only. It is never written to the collection folder.
//! The keychain entry is keyed by the source id, so a copied collection folder shares its
//! token with the original.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rocket_collection::openapi_local::{OpenApiLocalRepository, OpenApiLocalState};
use rocket_collection::openapi_schedule::{clamp_retry_after, next_check_at, SchedulePlan};
use rocket_collection::openapi_source::{
    OpenApiSource, OpenApiSourceRepository, SpecLocation, SyncInterval, SyncStatus,
};
use rocket_environment::SecretStore;
use rocket_http::spec_fetch::{
    SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest, SpecFetcher, DEFAULT_MAX_SPEC_BYTES,
};
use rocket_import::{
    bundle_spec, check_spec_text, fetch_failure_message, fetch_target, lost_refs_message,
    ref_failure_message, refs_unchanged_labeled, whole_bundle_failed_message, Bundle,
};
use rocket_shared::error::{DomainError, DomainResult};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::openapi_sync_apply::ApplyDeps;

const TOKEN_KEY: &str = "token";

/// What one check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    Unchanged,
    Changed {
        spec_sha256: String,
    },
    Failed {
        message: String,
        retry_after_secs: Option<u64>,
    },
}

/// The sync state as shown to the user. It says whether a token is stored, never the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncStatusView {
    pub location: String,
    pub interval: SyncInterval,
    pub last_checked_at: Option<String>,
    pub last_status: Option<SyncStatus>,
    pub has_pending: bool,
    pub has_token: bool,
}

/// A collection whose background check is due.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncTarget {
    pub collection: String,
    pub root: std::path::PathBuf,
}

type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

pub struct OpenApiSyncService {
    fetcher: Arc<dyn SpecFetcher>,
    pub(crate) sources: Box<dyn OpenApiSourceRepository>,
    secrets: Arc<dyn SecretStore>,
    clock: Clock,
    /// Set by `with_apply`. Without it `apply` and `recover` refuse.
    pub(crate) apply_deps: Option<ApplyDeps>,
    /// Set by `with_local`. Without it the repository of the apply deps is used.
    local: Option<Box<dyn OpenApiLocalRepository>>,
}

type LockMap =
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Arc<std::sync::Mutex<()>>>>;

static COLLECTION_LOCKS: std::sync::OnceLock<LockMap> = std::sync::OnceLock::new();

/// The lock of one collection, shared by every service in the process (commands build a
/// service per call). A holder never awaits while it holds the lock and takes the file
/// transaction's own guard only after this one, so the two cannot deadlock.
pub(crate) fn collection_lock(root: &Path) -> Arc<std::sync::Mutex<()>> {
    let key = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut map = COLLECTION_LOCKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    Arc::clone(map.entry(key).or_default())
}

/// The collection folder as stored in `opt_in_root`: canonical, else as given.
pub(crate) fn canonical_root_string(root: &Path) -> String {
    std::fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// The interval that counts on this machine: Off unless it was turned on for this folder.
fn effective_interval(local: &OpenApiLocalState, root: &Path) -> SyncInterval {
    match local.opt_in_root.as_deref() {
        Some(opted) if opted == canonical_root_string(root) => local.sync_interval,
        _ => SyncInterval::Off,
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A short machine word for the failed status.
fn failure_code(failure: &SpecFetchFailure) -> &'static str {
    match failure {
        SpecFetchFailure::NotFound => "not_found",
        SpecFetchFailure::Unauthorized => "unauthorized",
        SpecFetchFailure::RateLimited { .. } => "rate_limited",
        SpecFetchFailure::TooLarge => "too_large",
        SpecFetchFailure::NotHttps => "not_https",
        SpecFetchFailure::TooManyRedirects => "too_many_redirects",
        SpecFetchFailure::BlockedRedirect => "blocked_redirect",
        SpecFetchFailure::Status(_) => "status",
        SpecFetchFailure::Network(_) => "network",
    }
}

/// What the keychain gave for a source's token.
enum TokenRead {
    Found(Zeroizing<String>),
    None,
    /// The keychain could not be read. The cause is not kept, so it cannot reach a message.
    Unreadable,
}

const KEYCHAIN_UNREADABLE: &str = "The file was not found, or it needs a sign-in. \
     The keychain could not be read, so no token was sent.";

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn tokens_need_github() -> DomainError {
    DomainError::InvalidInput("Tokens work with GitHub links only.".to_string())
}

impl OpenApiSyncService {
    pub fn new(
        fetcher: Arc<dyn SpecFetcher>,
        sources: Box<dyn OpenApiSourceRepository>,
        secrets: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            fetcher,
            sources,
            secrets,
            clock: Arc::new(Utc::now),
            apply_deps: None,
            local: None,
        }
    }

    /// Adds the repository of the local state, where the interval and the check timing live.
    pub fn with_local(mut self, local: Box<dyn OpenApiLocalRepository>) -> Self {
        self.local = Some(local);
        self
    }

    /// The local state repository: the one given to `with_local`, else the apply deps' one.
    /// A service with neither refuses, so timing is never dropped without a word.
    pub(crate) fn local_repo(&self) -> DomainResult<&dyn OpenApiLocalRepository> {
        if let Some(local) = &self.local {
            return Ok(local.as_ref());
        }
        match &self.apply_deps {
            Some(deps) => Ok(deps.locals.as_ref()),
            None => Err(DomainError::Internal(
                "The local sync state is not set up.".to_string(),
            )),
        }
    }

    /// Adds what applying changes needs, so existing call sites keep compiling.
    pub fn with_apply(mut self, deps: ApplyDeps) -> Self {
        self.apply_deps = Some(deps);
        self
    }

    /// Replaces the clock, so tests can use a fixed time.
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    pub(crate) fn load(&self, root: &Path) -> DomainResult<OpenApiSource> {
        self.sources.load(root)?.ok_or_else(|| {
            DomainError::NotFound("This collection has no OpenAPI source.".to_string())
        })
    }

    /// Reads the token strictly, so a locked keychain is told apart from no token.
    fn read_token(&self, source: &OpenApiSource) -> TokenRead {
        if source.source_id.is_empty() {
            return TokenRead::None;
        }
        match self.secrets.get_strict(&source.source_id, TOKEN_KEY) {
            Ok(Some(t)) if !t.is_empty() => TokenRead::Found(Zeroizing::new(t)),
            Ok(_) => TokenRead::None,
            Err(_) => TokenRead::Unreadable,
        }
    }

    fn stored_token(&self, source: &OpenApiSource) -> Option<Zeroizing<String>> {
        if source.source_id.is_empty() {
            return None;
        }
        self.secrets
            .get(&source.source_id, TOKEN_KEY)
            .ok()
            .flatten()
            .filter(|t| !t.is_empty())
            .map(Zeroizing::new)
    }

    /// Records a failed check in the local state and returns its outcome.
    fn fail(
        &self,
        root: &Path,
        now: &DateTime<Utc>,
        code: &str,
        message: String,
        retry_after_secs: Option<u64>,
    ) -> DomainResult<CheckOutcome> {
        let locals = self.local_repo()?;
        let mut local = locals.load(root)?.unwrap_or_default();
        local.last_checked_at = Some(now.to_rfc3339());
        local.last_status = Some(SyncStatus::Failed {
            code: code.to_string(),
        });
        local.consecutive_failures = local.consecutive_failures.saturating_add(1);
        local.not_before = retry_after_secs.map(|secs| {
            let secs = i64::try_from(clamp_retry_after(secs)).unwrap_or(0);
            (*now + chrono::Duration::seconds(secs)).to_rfc3339()
        });
        locals.save(root, &local)?;
        Ok(CheckOutcome::Failed {
            message,
            retry_after_secs,
        })
    }

    /// Records a finished check in the local state. A success resets the failure state.
    fn record_success(
        &self,
        root: &Path,
        now: &DateTime<Utc>,
        status: SyncStatus,
    ) -> DomainResult<()> {
        let locals = self.local_repo()?;
        let mut local = locals.load(root)?.unwrap_or_default();
        local.last_checked_at = Some(now.to_rfc3339());
        local.last_status = Some(status);
        local.consecutive_failures = 0;
        local.not_before = None;
        locals.save(root, &local)
    }

    /// True when the pending hash has its file and the file matches. No pending hash is fine.
    fn pending_is_sound(&self, root: &Path, source: &OpenApiSource) -> bool {
        let Some(hash) = source.pending_sha256.as_deref() else {
            return true;
        };
        matches!(self.sources.load_pending(root), Ok(Some(bytes)) if sha256_hex(&bytes) == hash)
    }

    /// Settles an interrupted apply before anything writes the collection's files.
    fn settle(&self, root: &Path) -> DomainResult<()> {
        if let Some(deps) = &self.apply_deps {
            deps.tx.recover(root)?;
        }
        Ok(())
    }

    pub async fn check(&self, collection_root: &Path) -> DomainResult<CheckOutcome> {
        self.settle(collection_root)?;
        // A local state that cannot be read fails the check before any request goes out.
        self.local_repo()?.load(collection_root)?;
        // Rule 1: a source file is required. Only a GitHub location gets a token.
        let mut source = self.load(collection_root)?;
        let read = match source.location {
            SpecLocation::Github { .. } => self.read_token(&source),
            SpecLocation::Https { .. } => TokenRead::None,
        };
        let keychain_failed = matches!(read, TokenRead::Unreadable);
        let token = match read {
            TokenRead::Found(t) => Some(t),
            _ => None,
        };
        let token_sent = token.is_some();
        let now = (self.clock)();

        // Rule 2: send the stored validators.
        // A pending hash without its file would make a 304 answer stick forever, so the
        // validators are dropped and the spec is fetched again.
        if !self.pending_is_sound(collection_root, &source) {
            source.etag = None;
            source.last_modified = None;
        }
        let target = fetch_target(&source.location, token_sent);
        let request = SpecFetchRequest {
            url: target.url,
            accept: target.accept,
            bearer_token: token.as_ref().map(|t| t.to_string()),
            if_none_match: source.etag.clone(),
            if_modified_since: source.last_modified.clone(),
            max_bytes: DEFAULT_MAX_SPEC_BYTES,
        };
        let mut result = self.fetcher.fetch(request.clone()).await;

        // Rule 3 refinement: a 304 of the main spec says nothing about the files it refers to.
        // A failure of a stored file carries the file's label, so its message names that file.
        let mut ref_label: Option<String> = None;
        if matches!(result, Ok(SpecFetchOutcome::NotModified))
            && (!source.remote_refs.is_empty() || source.remote_refs_incomplete)
        {
            let unchanged = if source.remote_refs_incomplete {
                // Stored validators are not trusted, so the full path runs.
                Ok(false)
            } else {
                refs_unchanged_labeled(
                    self.fetcher.as_ref(),
                    &source.location,
                    token.as_ref().map(|t| t.as_str()),
                    &source.remote_refs,
                )
                .await
            };
            match unchanged {
                Ok(true) => {}
                Ok(false) => {
                    let mut fresh = request.clone();
                    fresh.if_none_match = None;
                    fresh.if_modified_since = None;
                    result = match self.fetcher.fetch(fresh).await {
                        Ok(SpecFetchOutcome::NotModified) => Err(SpecFetchFailure::Network(
                            "the server did not send the file".to_string(),
                        )),
                        other => other,
                    };
                }
                Err((label, failure)) => {
                    ref_label = Some(label);
                    result = Err(failure);
                }
            }
        }

        // A fresh body is bundled with the files it refers to before the lock is taken.
        let mut bundled: Option<Result<Bundle, String>> = None;
        if let Ok(SpecFetchOutcome::Fetched { body, .. }) = &result {
            if check_spec_text(body).is_ok() {
                bundled = Some(match std::str::from_utf8(body) {
                    Ok(text) => bundle_spec(
                        self.fetcher.as_ref(),
                        text,
                        &source.location,
                        token.as_ref().map(|t| t.as_str()),
                        &source.remote_refs,
                    )
                    .await
                    .map_err(|e| capitalize(&e.to_string())),
                    Err(_) => Err("The file is not valid text.".to_string()),
                });
            }
        }
        drop(token);

        // The fetch can take long, so the lock was not held. Reload the source now and apply
        // only what the check owns, so a change made by an apply meanwhile is not undone.
        let lock = collection_lock(collection_root);
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut source = self.load(collection_root)?;

        let (body, etag, last_modified) = match result {
            // Rule 8: a fetch failure changes nothing but the check record.
            Err(failure) => {
                let retry = match &failure {
                    SpecFetchFailure::RateLimited { retry_after_secs } => *retry_after_secs,
                    _ => None,
                };
                let message = match failure {
                    _ if ref_label.is_some() => ref_failure_message(
                        ref_label.as_deref().unwrap_or_default(),
                        &source.location,
                        &failure,
                    ),
                    SpecFetchFailure::NotFound | SpecFetchFailure::Unauthorized
                        if keychain_failed =>
                    {
                        KEYCHAIN_UNREADABLE.to_string()
                    }
                    _ => fetch_failure_message(&failure, token_sent),
                };
                return self.fail(
                    collection_root,
                    &now,
                    failure_code(&failure),
                    message,
                    retry,
                );
            }
            // Rule 3: the server says nothing changed since the validators.
            Ok(SpecFetchOutcome::NotModified) => {
                if !self.pending_is_sound(collection_root, &source) {
                    // The pending copy is gone, so there is nothing to keep. Forget the
                    // validators, and the next check fetches the spec again.
                    source.etag = None;
                    source.last_modified = None;
                    source.pending_sha256 = None;
                    self.sources.save(collection_root, &source)?;
                }
                let (outcome, status) = if let Some(pending) = source.pending_sha256.clone() {
                    (
                        CheckOutcome::Changed {
                            spec_sha256: pending,
                        },
                        SyncStatus::Changed,
                    )
                } else {
                    (CheckOutcome::Unchanged, SyncStatus::Unchanged)
                };
                self.record_success(collection_root, &now, status)?;
                return Ok(outcome);
            }
            Ok(SpecFetchOutcome::Fetched {
                body,
                etag,
                last_modified,
                ..
            }) => (body, etag, last_modified),
        };

        // Rule 4: a body that is not a spec keeps everything as it was.
        if let Err(e) = check_spec_text(&body) {
            return self.fail(
                collection_root,
                &now,
                "not_a_spec",
                capitalize(&e.to_string()),
                None,
            );
        }
        let bundle = match bundled {
            Some(Ok(bundle)) => bundle,
            Some(Err(message)) => {
                return self.fail(collection_root, &now, "not_a_spec", message, None);
            }
            None => {
                return self.fail(
                    collection_root,
                    &now,
                    "not_a_spec",
                    "The file is not valid text.".to_string(),
                    None,
                );
            }
        };
        // A file that loaded last time and was met but not loaded now keeps everything as it
        // was. A file the spec dropped is never met, so it is never lost. A file that never
        // loaded only keeps the incomplete flag, so it is tried again at the next check.
        if bundle.incomplete {
            let lost: Vec<&str> = source
                .remote_refs
                .iter()
                .filter(|p| {
                    bundle.whole_failed
                        || bundle.unloaded.iter().any(|u| *u == p.url)
                        || (bundle.scan_truncated && !bundle.refs.iter().any(|r| r.url == p.url))
                })
                .map(|p| p.url.as_str())
                .collect();
            if !lost.is_empty() {
                let message = if bundle.whole_failed {
                    whole_bundle_failed_message()
                } else {
                    lost_refs_message(&lost)
                };
                return self.fail(
                    collection_root,
                    &now,
                    "ref_unavailable",
                    message,
                    bundle.retry_after_secs,
                );
            }
        }

        let text = bundle.text.into_bytes();
        let hash = sha256_hex(&text);
        source.etag = etag;
        source.last_modified = last_modified;
        source.remote_refs = bundle.refs;
        source.remote_refs_incomplete = bundle.fetch_failed;

        // Rule 5: the same spec as the imported one.
        if hash == source.spec_sha256 {
            // Save first: a leftover pending file is harmless, a hash without a file is not.
            source.pending_sha256 = None;
            self.sources.save(collection_root, &source)?;
            self.sources.clear_pending(collection_root)?;
            self.record_success(collection_root, &now, SyncStatus::Unchanged)?;
            return Ok(CheckOutcome::Unchanged);
        }

        // Rule 6: the same spec as the pending copy, which is already kept.
        // Rule 7: a new valid spec becomes the pending copy.
        let kept = source.pending_sha256.as_deref() == Some(hash.as_str())
            && self.pending_is_sound(collection_root, &source);
        if !kept {
            self.sources.save_pending(collection_root, &text)?;
            source.pending_sha256 = Some(hash.clone());
        }
        self.sources.save(collection_root, &source)?;
        self.record_success(collection_root, &now, SyncStatus::Changed)?;
        Ok(CheckOutcome::Changed { spec_sha256: hash })
    }

    pub fn status(&self, collection_root: &Path) -> DomainResult<SyncStatusView> {
        let source = self.load(collection_root)?;
        let local: OpenApiLocalState = self
            .local_repo()?
            .load(collection_root)?
            .unwrap_or_default();
        // The legacy source timing is only a fallback until the local state has a check.
        let (last_checked_at, last_status) = if local.last_checked_at.is_some() {
            (local.last_checked_at.clone(), local.last_status.clone())
        } else {
            (source.last_checked_at.clone(), source.last_status.clone())
        };
        Ok(SyncStatusView {
            location: source.location.display_url(),
            interval: effective_interval(&local, collection_root),
            last_checked_at,
            last_status,
            has_pending: source.pending_sha256.is_some(),
            has_token: self.stored_token(&source).is_some(),
        })
    }

    /// The collections of the pinned active workspace whose check is due at `now`, oldest due
    /// first. Timing comes from the local state only, never from the source file's legacy
    /// fields. Without apply deps (where the collection list lives) the list is empty. A
    /// collection whose local or source file cannot be read is listed first, so its check
    /// fails before any request and the caller can report and back off; only a collection
    /// whose path cannot be found is skipped (counted in a debug log). Cost per call is one
    /// local-state read per collection plus one source read for each enabled one. This does
    /// blocking file reads, so async callers run it through `spawn_blocking`.
    pub fn due_targets(&self, now: DateTime<Utc>) -> Vec<SyncTarget> {
        let (Some(deps), Ok(locals)) = (&self.apply_deps, self.local_repo()) else {
            return Vec::new();
        };
        let pinned = deps.collections.pinned();
        let repo: &dyn rocket_collection::CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let Ok(names) = repo.list() else {
            tracing::debug!(code = "list_failed", "sync targets could not be listed");
            return Vec::new();
        };
        let mut due: Vec<(DateTime<Utc>, SyncTarget)> = Vec::new();
        let mut skipped = 0usize;
        let mut unreadable: Vec<(DateTime<Utc>, SyncTarget)> = Vec::new();
        let first = || DateTime::<Utc>::MIN_UTC;
        let target = |collection: String, root: std::path::PathBuf| SyncTarget { collection, root };
        for summary in names {
            let Ok(root) = repo.collection_root_path(&summary.name) else {
                skipped += 1;
                continue;
            };
            let local = match locals.load(&root) {
                Ok(Some(local)) => local,
                Ok(None) => continue,
                Err(_) => {
                    unreadable.push((first(), target(summary.name, root)));
                    continue;
                }
            };
            // Off, or turned on in another folder (a copied or committed state): not listed.
            let interval = effective_interval(&local, &root);
            if interval == SyncInterval::Off {
                continue;
            }
            let source = match self.sources.load(&root) {
                Ok(Some(source)) => source,
                Ok(None) => continue,
                Err(_) => {
                    unreadable.push((first(), target(summary.name, root)));
                    continue;
                }
            };
            let plan = SchedulePlan {
                interval,
                source_id: &source.source_id,
                last_checked_at: local.last_checked_at.as_deref(),
                last_failed: matches!(local.last_status, Some(SyncStatus::Failed { .. })),
                consecutive_failures: local.consecutive_failures,
                not_before: local.not_before.as_deref(),
                now,
            };
            if let Some(at) = next_check_at(&plan).filter(|at| *at <= now) {
                due.push((
                    at,
                    SyncTarget {
                        collection: summary.name,
                        root,
                    },
                ));
            }
        }
        if skipped > 0 || !unreadable.is_empty() {
            tracing::debug!(
                skipped,
                unreadable = unreadable.len(),
                "sync targets with problems"
            );
        }
        due.extend(unreadable);
        due.sort_by_key(|(at, _)| *at);
        due.into_iter().map(|(_, target)| target).collect()
    }

    pub fn set_interval(&self, collection_root: &Path, interval: SyncInterval) -> DomainResult<()> {
        self.settle(collection_root)?;
        let lock = collection_lock(collection_root);
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        // The source file must exist, so a stray folder gets no local state.
        self.load(collection_root)?;
        let locals = self.local_repo()?;
        let mut local = locals.load(collection_root)?.unwrap_or_default();
        local.sync_interval = interval;
        local.opt_in_root = match interval {
            SyncInterval::Off => None,
            _ => Some(canonical_root_string(collection_root)),
        };
        locals.save(collection_root, &local)
    }

    pub fn set_token(&self, collection_root: &Path, token: &str) -> DomainResult<()> {
        self.settle(collection_root)?;
        let lock = collection_lock(collection_root);
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut source = self.load(collection_root)?;
        if !matches!(source.location, SpecLocation::Github { .. }) {
            return Err(tokens_need_github());
        }
        let token = token.trim();
        if token.is_empty() {
            return Err(DomainError::InvalidInput("The token is empty.".to_string()));
        }
        // A file from an older version has no id yet.
        if source.source_id.is_empty() {
            source.source_id = OpenApiSource::new_source_id();
            self.sources.save(collection_root, &source)?;
        }
        self.secrets.set(&source.source_id, TOKEN_KEY, token)
    }

    pub fn clear_token(&self, collection_root: &Path) -> DomainResult<()> {
        let source = self.load(collection_root)?;
        if !matches!(source.location, SpecLocation::Github { .. }) {
            return Err(tokens_need_github());
        }
        if source.source_id.is_empty() {
            return Ok(());
        }
        self.secrets.delete(&source.source_id, TOKEN_KEY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rocket_infra::{FsOpenApiLocalRepo, FsOpenApiSourceRepo};
    use std::collections::HashMap;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use tempfile::TempDir;

    type Reply = Result<SpecFetchOutcome, SpecFetchFailure>;

    #[derive(Default)]
    struct FakeFetcher {
        replies: Mutex<VecDeque<Reply>>,
        requests: Mutex<Vec<SpecFetchRequest>>,
    }

    impl FakeFetcher {
        fn push(&self, reply: Reply) {
            self.replies.lock().expect("lock").push_back(reply);
        }
        fn last_request(&self) -> SpecFetchRequest {
            self.requests
                .lock()
                .expect("lock")
                .last()
                .cloned()
                .expect("a request")
        }
    }

    /// Lets the test keep a handle on the fetcher the service owns.
    struct SharedFetcher(Arc<FakeFetcher>);

    #[async_trait]
    impl SpecFetcher for SharedFetcher {
        async fn fetch(&self, request: SpecFetchRequest) -> Reply {
            self.0.requests.lock().expect("lock").push(request);
            self.0
                .replies
                .lock()
                .expect("lock")
                .pop_front()
                .expect("a queued reply")
        }
    }

    #[derive(Default)]
    struct MemSecrets(Mutex<HashMap<(String, String), String>>);

    /// A store whose strict read always fails, like a locked keychain.
    struct LockedSecrets;

    impl SecretStore for LockedSecrets {
        fn get(&self, _: &str, _: &str) -> DomainResult<Option<String>> {
            Ok(None)
        }
        fn get_strict(&self, _: &str, _: &str) -> DomainResult<Option<String>> {
            Err(DomainError::Internal(
                "keychain locked: secret-detail".into(),
            ))
        }
        fn set(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    /// Delegates to the file repo, but can be told to fail every save.
    struct FlakyRepo(Arc<std::sync::atomic::AtomicBool>);

    impl OpenApiSourceRepository for FlakyRepo {
        fn load(&self, root: &Path) -> DomainResult<Option<OpenApiSource>> {
            FsOpenApiSourceRepo.load(root)
        }
        fn save(&self, root: &Path, source: &OpenApiSource) -> DomainResult<()> {
            if self.0.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(DomainError::Internal("disk full".into()));
            }
            FsOpenApiSourceRepo.save(root, source)
        }
        fn delete(&self, root: &Path) -> DomainResult<()> {
            FsOpenApiSourceRepo.delete(root)
        }
        fn save_pending(&self, root: &Path, spec: &[u8]) -> DomainResult<()> {
            FsOpenApiSourceRepo.save_pending(root, spec)
        }
        fn load_pending(&self, root: &Path) -> DomainResult<Option<Vec<u8>>> {
            FsOpenApiSourceRepo.load_pending(root)
        }
        fn clear_pending(&self, root: &Path) -> DomainResult<()> {
            FsOpenApiSourceRepo.clear_pending(root)
        }
    }

    impl SecretStore for MemSecrets {
        fn get(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
            Ok(self
                .0
                .lock()
                .expect("lock")
                .get(&(scope.to_string(), key.to_string()))
                .cloned())
        }
        fn set(&self, scope: &str, key: &str, value: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock")
                .insert((scope.to_string(), key.to_string()), value.to_string());
            Ok(())
        }
        fn delete(&self, scope: &str, key: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock")
                .remove(&(scope.to_string(), key.to_string()));
            Ok(())
        }
    }

    const TOKEN: &str = "ghp_supersecret_token_value";

    fn fixture() -> Vec<u8> {
        let path = format!(
            "{}/../rocket-import/tests/fixtures/openapi/orders-3.0.json",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(path).expect("fixture")
    }

    /// The fixture with one extra path.
    fn changed(tag: &str) -> Vec<u8> {
        let mut v: serde_json::Value = serde_json::from_slice(&fixture()).expect("json");
        v["paths"][format!("/extra-{tag}")] = serde_json::json!({
            "get": { "operationId": format!("extra{tag}"), "responses": { "200": { "description": "ok" } } }
        });
        serde_json::to_vec(&v).expect("json")
    }

    fn fixed_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-26T12:00:00Z")
            .expect("date")
            .with_timezone(&Utc)
    }

    fn github() -> SpecLocation {
        SpecLocation::Github {
            owner: "o".into(),
            repo: "r".into(),
            git_ref: "main".into(),
            path: "api.json".into(),
        }
    }

    fn source(location: SpecLocation) -> OpenApiSource {
        OpenApiSource {
            version: 1,
            location,
            group_by: "tags".into(),
            spec_sha256: sha256_hex(&fixture()),
            etag: Some("\"v1\"".into()),
            last_modified: Some("Mon, 01 Jan 2026 00:00:00 GMT".into()),
            operations: Vec::new(),
            source_id: "src-1".into(),
            sync_interval: SyncInterval::Off,
            last_checked_at: None,
            last_status: None,
            pending_sha256: None,
            remote_refs: Vec::new(),
            remote_refs_incomplete: false,
        }
    }

    struct Fixture {
        dir: TempDir,
        fetcher: Arc<FakeFetcher>,
        secrets: Arc<MemSecrets>,
        svc: OpenApiSyncService,
    }

    impl Fixture {
        fn new(src: OpenApiSource) -> Self {
            let dir = TempDir::new().expect("tempdir");
            FsOpenApiSourceRepo.save(dir.path(), &src).expect("seed");
            let fetcher = Arc::new(FakeFetcher::default());
            let secrets = Arc::new(MemSecrets::default());
            let svc = OpenApiSyncService::new(
                Arc::new(SharedFetcher(Arc::clone(&fetcher))),
                Box::new(FsOpenApiSourceRepo),
                Arc::clone(&secrets) as Arc<dyn SecretStore>,
            )
            .with_local(Box::new(FsOpenApiLocalRepo))
            .with_clock(Arc::new(fixed_now));
            Self {
                dir,
                fetcher,
                secrets,
                svc,
            }
        }
        fn root(&self) -> &Path {
            self.dir.path()
        }
        fn load(&self) -> OpenApiSource {
            FsOpenApiSourceRepo
                .load(self.root())
                .expect("load")
                .expect("source")
        }
        fn local(&self) -> OpenApiLocalState {
            FsOpenApiLocalRepo
                .load(self.root())
                .expect("load")
                .unwrap_or_default()
        }
        fn source_bytes(&self) -> Vec<u8> {
            std::fs::read(self.root().join(".rocket/openapi-source.yml")).expect("source file")
        }
        fn pending(&self) -> Option<Vec<u8>> {
            FsOpenApiSourceRepo
                .load_pending(self.root())
                .expect("pending")
        }
    }

    fn fetched(body: Vec<u8>, etag: &str) -> Reply {
        Ok(SpecFetchOutcome::Fetched {
            body,
            etag: Some(etag.into()),
            last_modified: None,
            final_url: "https://x".into(),
        })
    }

    const DEFS: &str = "https://example.com/defs.yaml";
    const DEFS_BODY: &str = "parameters:\n  p: { name: p, in: query, schema: { type: string } }\n";

    /// The fixture with one extra path whose operation refers to the given files.
    fn with_refs(refs: &[&str]) -> Vec<u8> {
        let mut v: serde_json::Value = serde_json::from_slice(&fixture()).expect("json");
        let params: Vec<serde_json::Value> = refs
            .iter()
            .map(|r| serde_json::json!({ "$ref": r }))
            .collect();
        v["paths"]["/with-refs"] = serde_json::json!({
            "get": { "operationId": "withRefs", "parameters": params,
                     "responses": { "200": { "description": "ok" } } }
        });
        serde_json::to_vec(&v).expect("json")
    }

    fn ref_state(url: &str, etag: &str) -> rocket_collection::openapi_source::RemoteRefState {
        rocket_collection::openapi_source::RemoteRefState {
            url: url.to_string(),
            etag: Some(etag.to_string()),
            last_modified: None,
        }
    }

    fn with_ref_source(location: SpecLocation, url: &str) -> OpenApiSource {
        let mut src = source(location);
        src.remote_refs = vec![ref_state(url, "\"r1\"")];
        src
    }

    fn request_count(f: &Fixture) -> usize {
        f.fetcher.requests.lock().expect("lock").len()
    }

    #[tokio::test]
    async fn all_304_is_unchanged_with_no_main_refetch() {
        let f = Fixture::new(with_ref_source(
            SpecLocation::Https {
                url: "https://example.com/a.json".into(),
            },
            DEFS,
        ));
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        let out = f.svc.check(f.root()).await.expect("check");
        assert_eq!(out, CheckOutcome::Unchanged);
        assert_eq!(request_count(&f), 2);
        let last = f.fetcher.last_request();
        assert_eq!(last.url, DEFS);
        assert_eq!(last.if_none_match.as_deref(), Some("\"r1\""));
        assert_eq!(f.local().last_status, Some(SyncStatus::Unchanged));
    }

    #[tokio::test]
    async fn a_changed_referenced_file_makes_the_spec_changed() {
        let f = Fixture::new(with_ref_source(
            SpecLocation::Https {
                url: "https://example.com/a.json".into(),
            },
            DEFS,
        ));
        // The main spec is 304, the file is new, then the main spec and the file are fetched.
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.fetcher
            .push(fetched(DEFS_BODY.as_bytes().to_vec(), "\"r2\""));
        f.fetcher.push(fetched(
            with_refs(&[&format!("{DEFS}#/parameters/p")]),
            "\"m2\"",
        ));
        f.fetcher
            .push(fetched(DEFS_BODY.as_bytes().to_vec(), "\"r2\""));
        let out = f.svc.check(f.root()).await.expect("check");
        let pending = f.pending().expect("pending copy");
        assert_eq!(
            out,
            CheckOutcome::Changed {
                spec_sha256: sha256_hex(&pending)
            }
        );
        let text = String::from_utf8(pending).expect("utf8");
        assert!(!text.contains(DEFS), "the file is inlined");
        let s = f.load();
        assert_eq!(s.pending_sha256, Some(sha256_hex(text.as_bytes())));
        assert_eq!(s.remote_refs[0].etag.as_deref(), Some("\"r2\""));
        assert_eq!(s.etag.as_deref(), Some("\"m2\""));
        assert!(!s.remote_refs_incomplete);
        // The third request is the main spec again, without validators.
        let requests = f.fetcher.requests.lock().expect("lock");
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[2].if_none_match, None);
        assert_eq!(requests[2].if_modified_since, None);
    }

    #[tokio::test]
    async fn an_unreachable_file_fails_the_check_and_changes_nothing() {
        let f = Fixture::new(with_ref_source(
            SpecLocation::Https {
                url: "https://example.com/a.json".into(),
            },
            DEFS,
        ));
        f.fetcher.push(fetched(
            with_refs(&[&format!("{DEFS}#/parameters/p")]),
            "\"m2\"",
        ));
        f.fetcher
            .push(Err(SpecFetchFailure::Network("down".into())));
        let before = f.source_bytes();
        let out = f.svc.check(f.root()).await.expect("check");
        match out {
            CheckOutcome::Failed { message, .. } => {
                assert!(
                    message.starts_with("A file the spec refers to could not be loaded: defs.yaml"),
                    "{message}"
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(f.source_bytes(), before);
        assert_eq!(f.pending(), None);
        let l = f.local();
        assert_eq!(l.consecutive_failures, 1);
        assert_eq!(
            l.last_status,
            Some(SyncStatus::Failed {
                code: "ref_unavailable".into()
            })
        );
    }

    #[tokio::test]
    async fn a_spec_without_remote_refs_behaves_as_before() {
        let f = Fixture::new(source(github()));
        f.fetcher.push(fetched(changed("a"), "\"v2\""));
        let out = f.svc.check(f.root()).await.expect("check");
        assert_eq!(
            out,
            CheckOutcome::Changed {
                spec_sha256: sha256_hex(&changed("a"))
            }
        );
        assert_eq!(request_count(&f), 1);
        assert_eq!(f.pending(), Some(changed("a")));
        assert!(f.load().remote_refs.is_empty());
    }

    #[tokio::test]
    async fn the_token_is_not_sent_to_a_file_in_another_repository() {
        let f = Fixture::new(source(github()));
        f.svc.set_token(f.root(), TOKEN).expect("token");
        let same = "https://raw.githubusercontent.com/o/r/main/defs.yaml#/parameters/p";
        let other = "https://raw.githubusercontent.com/z/q/main/defs.yaml#/parameters/p";
        f.fetcher.push(fetched(with_refs(&[same, other]), "\"m2\""));
        f.fetcher
            .push(fetched(DEFS_BODY.as_bytes().to_vec(), "\"r1\""));
        f.fetcher
            .push(fetched(DEFS_BODY.as_bytes().to_vec(), "\"r2\""));
        f.svc.check(f.root()).await.expect("check");
        let requests = f.fetcher.requests.lock().expect("lock");
        assert_eq!(requests.len(), 3);
        for r in requests.iter().skip(1) {
            if r.url.contains("/z/q/") {
                assert_eq!(r.bearer_token, None, "{}", r.url);
            } else {
                assert_eq!(r.bearer_token.as_deref(), Some(TOKEN), "{}", r.url);
            }
        }
        assert!(requests.iter().any(|r| r.url.contains("/z/q/")));
    }

    #[tokio::test]
    async fn refs_are_fetched_again_only_when_something_changed() {
        let f = Fixture::new(with_ref_source(
            SpecLocation::Https {
                url: "https://example.com/a.json".into(),
            },
            DEFS,
        ));
        for _ in 0..2 {
            f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
            f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
            f.svc.check(f.root()).await.expect("check");
        }
        // Two checks, each one validator request for the main spec and one for the file.
        assert_eq!(request_count(&f), 4);
        assert!(f
            .fetcher
            .requests
            .lock()
            .expect("lock")
            .iter()
            .all(|r| r.if_none_match.is_some()));
        // Now one file answers 200: the main spec is fetched again without validators.
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.fetcher
            .push(fetched(DEFS_BODY.as_bytes().to_vec(), "\"r2\""));
        f.fetcher.push(fetched(
            with_refs(&[&format!("{DEFS}#/parameters/p")]),
            "\"m2\"",
        ));
        f.fetcher
            .push(fetched(DEFS_BODY.as_bytes().to_vec(), "\"r2\""));
        let out = f.svc.check(f.root()).await.expect("check");
        assert!(matches!(out, CheckOutcome::Changed { .. }), "{out:?}");
        assert_eq!(request_count(&f), 8);
        let requests = f.fetcher.requests.lock().expect("lock");
        assert_eq!(requests[6].url, "https://example.com/a.json");
        assert_eq!(requests[6].if_none_match, None);
        drop(requests);
        assert_eq!(f.load().remote_refs[0].etag.as_deref(), Some("\"r2\""));
        assert!(f.pending().is_some());
    }

    fn https_main() -> SpecLocation {
        SpecLocation::Https {
            url: "https://example.com/a.json".into(),
        }
    }

    fn ref_to_defs() -> Vec<u8> {
        with_refs(&[&format!("{DEFS}#/parameters/p")])
    }

    #[tokio::test]
    async fn an_incomplete_flag_with_no_stored_refs_loads_the_files_on_a_304() {
        let mut src = source(https_main());
        src.remote_refs_incomplete = true;
        let f = Fixture::new(src);
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.fetcher.push(fetched(ref_to_defs(), "\"m2\""));
        f.fetcher
            .push(fetched(DEFS_BODY.as_bytes().to_vec(), "\"r1\""));
        let out = f.svc.check(f.root()).await.expect("check");
        assert!(matches!(out, CheckOutcome::Changed { .. }), "{out:?}");
        assert_eq!(request_count(&f), 3);
        {
            let requests = f.fetcher.requests.lock().expect("lock");
            assert!(requests[0].if_none_match.is_some());
            assert_eq!(requests[1].url, "https://example.com/a.json");
            assert_eq!(requests[1].if_none_match, None);
            assert_eq!(requests[2].url, DEFS);
        }
        let s = f.load();
        assert!(!s.remote_refs_incomplete);
        assert_eq!(s.remote_refs.len(), 1);
        let text = String::from_utf8(f.pending().expect("pending")).expect("utf8");
        assert!(text.contains("x-rocket-remote"), "the file is inlined");
    }

    #[tokio::test]
    async fn an_incomplete_flag_skips_the_per_file_check() {
        let mut src = with_ref_source(https_main(), DEFS);
        src.remote_refs_incomplete = true;
        let f = Fixture::new(src);
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.fetcher.push(fetched(ref_to_defs(), "\"m2\""));
        f.fetcher
            .push(fetched(DEFS_BODY.as_bytes().to_vec(), "\"r2\""));
        let out = f.svc.check(f.root()).await.expect("check");
        assert!(matches!(out, CheckOutcome::Changed { .. }), "{out:?}");
        let requests = f.fetcher.requests.lock().expect("lock");
        assert_eq!(requests.len(), 3);
        // No request between the main 304 and the unconditional main fetch.
        assert_eq!(requests[1].url, "https://example.com/a.json");
        assert_eq!(requests[1].if_none_match, None);
        assert!(requests
            .iter()
            .all(|r| r.if_none_match.as_deref() != Some("\"r1\"")));
        drop(requests);
        assert!(!f.load().remote_refs_incomplete);
    }

    #[tokio::test]
    async fn a_failed_file_check_fails_with_the_usual_code_and_changes_nothing() {
        let f = Fixture::new(with_ref_source(https_main(), DEFS));
        let before = f.source_bytes();
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.fetcher.push(Err(SpecFetchFailure::NotFound));
        let out = f.svc.check(f.root()).await.expect("check");
        match out {
            CheckOutcome::Failed { message, .. } => {
                assert!(message.contains("defs.yaml"), "{message}");
                assert!(!message.contains("keychain"), "{message}");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(f.source_bytes(), before);
        assert_eq!(f.pending(), None);
        assert_eq!(
            f.local().last_status,
            Some(SyncStatus::Failed {
                code: "not_found".into()
            })
        );
    }

    #[tokio::test]
    async fn a_file_check_network_error_never_quotes_the_error_text() {
        let f = Fixture::new(with_ref_source(https_main(), DEFS));
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.fetcher.push(Err(SpecFetchFailure::Network(
            "redirected to https://evil.example/x?k=secret".into(),
        )));
        let out = f.svc.check(f.root()).await.expect("check");
        match out {
            CheckOutcome::Failed { message, .. } => {
                assert!(message.contains("defs.yaml"), "{message}");
                assert!(
                    !message.contains("evil") && !message.contains("secret"),
                    "{message}"
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            f.local().last_status,
            Some(SyncStatus::Failed {
                code: "network".into()
            })
        );
    }

    #[tokio::test]
    async fn a_refetch_that_answers_304_is_a_network_failure() {
        let f = Fixture::new(with_ref_source(https_main(), DEFS));
        let before = f.source_bytes();
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.fetcher
            .push(fetched(DEFS_BODY.as_bytes().to_vec(), "\"r2\""));
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        let out = f.svc.check(f.root()).await.expect("check");
        assert!(matches!(out, CheckOutcome::Failed { .. }), "{out:?}");
        assert_eq!(f.source_bytes(), before);
        assert_eq!(f.pending(), None);
        assert_eq!(
            f.local().last_status,
            Some(SyncStatus::Failed {
                code: "network".into()
            })
        );
    }

    fn never_loaded_source(main: &[u8]) -> OpenApiSource {
        let mut src = source(https_main());
        src.remote_refs_incomplete = true;
        src.spec_sha256 = sha256_hex(main);
        src
    }

    #[tokio::test]
    async fn a_file_that_never_loaded_does_not_fail_an_unchanged_check() {
        let main = ref_to_defs();
        let f = Fixture::new(never_loaded_source(&main));
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.fetcher.push(fetched(main.clone(), "\"m2\""));
        f.fetcher.push(Err(SpecFetchFailure::NotFound));
        let out = f.svc.check(f.root()).await.expect("check");
        assert_eq!(out, CheckOutcome::Unchanged);
        let s = f.load();
        assert!(s.remote_refs_incomplete);
        assert!(s.remote_refs.is_empty());
        assert_eq!(f.local().consecutive_failures, 0);
    }

    #[tokio::test]
    async fn a_file_that_never_loaded_does_not_fail_a_changed_check() {
        let f = Fixture::new(never_loaded_source(&ref_to_defs()));
        let mut v: serde_json::Value = serde_json::from_slice(&ref_to_defs()).expect("json");
        v["info"]["title"] = serde_json::json!("Other title");
        let main = serde_json::to_vec(&v).expect("json");
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.fetcher.push(fetched(main.clone(), "\"m2\""));
        f.fetcher.push(Err(SpecFetchFailure::NotFound));
        let out = f.svc.check(f.root()).await.expect("check");
        assert_eq!(
            out,
            CheckOutcome::Changed {
                spec_sha256: sha256_hex(&main)
            }
        );
        assert!(f.load().remote_refs_incomplete);
        assert_eq!(f.pending(), Some(main));
    }

    #[tokio::test]
    async fn a_loaded_file_that_now_answers_with_html_fails_the_check() {
        let f = Fixture::new(with_ref_source(https_main(), DEFS));
        let before = f.source_bytes();
        f.fetcher.push(fetched(ref_to_defs(), "\"m2\""));
        f.fetcher
            .push(fetched(b"<html>oops</html>".to_vec(), "\"r2\""));
        let out = f.svc.check(f.root()).await.expect("check");
        match out {
            CheckOutcome::Failed { message, .. } => {
                assert!(message.contains("defs.yaml"), "{message}")
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(f.source_bytes(), before);
        assert_eq!(f.pending(), None);
        assert_eq!(
            f.local().last_status,
            Some(SyncStatus::Failed {
                code: "ref_unavailable".into()
            })
        );
    }

    #[tokio::test]
    async fn a_loaded_file_lost_to_the_size_limit_fails_the_check() {
        let f = Fixture::new(with_ref_source(https_main(), DEFS));
        let before = f.source_bytes();
        f.fetcher.push(fetched(ref_to_defs(), "\"m2\""));
        f.fetcher.push(Err(SpecFetchFailure::TooLarge));
        let out = f.svc.check(f.root()).await.expect("check");
        assert!(matches!(out, CheckOutcome::Failed { .. }), "{out:?}");
        assert_eq!(f.source_bytes(), before);
        assert_eq!(f.pending(), None);
    }

    #[tokio::test]
    async fn a_rate_limited_lost_file_keeps_the_retry_after() {
        let f = Fixture::new(with_ref_source(https_main(), DEFS));
        f.fetcher.push(fetched(ref_to_defs(), "\"m2\""));
        f.fetcher.push(Err(SpecFetchFailure::RateLimited {
            retry_after_secs: Some(90),
        }));
        let out = f.svc.check(f.root()).await.expect("check");
        match out {
            CheckOutcome::Failed {
                retry_after_secs, ..
            } => assert_eq!(retry_after_secs, Some(90)),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn a_dropped_file_and_a_new_failing_file_do_not_trap_the_check() {
        let f = Fixture::new(with_ref_source(https_main(), DEFS));
        // The spec drops the stored file and refers to a new one that answers 404.
        f.fetcher.push(fetched(
            with_refs(&["https://example.com/new.yaml#/parameters/p"]),
            "\"m2\"",
        ));
        f.fetcher.push(Err(SpecFetchFailure::NotFound));
        let out = f.svc.check(f.root()).await.expect("check");
        assert!(matches!(out, CheckOutcome::Changed { .. }), "{out:?}");
        let s = f.load();
        assert!(s.remote_refs.is_empty());
        assert!(s.remote_refs_incomplete);
        assert_eq!(f.local().consecutive_failures, 0);
    }

    #[tokio::test]
    async fn a_file_the_spec_no_longer_refers_to_does_not_fail() {
        let f = Fixture::new(with_ref_source(https_main(), DEFS));
        f.fetcher.push(fetched(changed("a"), "\"m2\""));
        let out = f.svc.check(f.root()).await.expect("check");
        assert!(matches!(out, CheckOutcome::Changed { .. }), "{out:?}");
        let s = f.load();
        assert!(s.remote_refs.is_empty());
        assert!(!s.remote_refs_incomplete);
    }

    #[tokio::test]
    async fn a_deterministic_incomplete_bundle_is_a_success_with_the_flag_clear() {
        let f = Fixture::new(source(https_main()));
        let urls: Vec<String> = (0..21)
            .map(|i| format!("https://example.com/f{i}.yaml#/parameters/p"))
            .collect();
        let refs: Vec<&str> = urls.iter().map(String::as_str).collect();
        f.fetcher.push(fetched(with_refs(&refs), "\"m2\""));
        for _ in 0..21 {
            f.fetcher
                .push(fetched(DEFS_BODY.as_bytes().to_vec(), "\"r\""));
        }
        let out = f.svc.check(f.root()).await.expect("check");
        assert!(matches!(out, CheckOutcome::Changed { .. }), "{out:?}");
        let s = f.load();
        assert!(!s.remote_refs_incomplete);
        assert_eq!(s.remote_refs.len(), 20);
        assert_eq!(f.local().last_status, Some(SyncStatus::Changed));
    }

    #[tokio::test]
    async fn not_modified_records_the_check_and_sends_the_validators() {
        let f = Fixture::new(source(github()));
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        let out = f.svc.check(f.root()).await.expect("check");
        assert_eq!(out, CheckOutcome::Unchanged);
        let req = f.fetcher.last_request();
        assert_eq!(req.if_none_match.as_deref(), Some("\"v1\""));
        assert_eq!(
            req.if_modified_since.as_deref(),
            Some("Mon, 01 Jan 2026 00:00:00 GMT")
        );
        assert_eq!(req.max_bytes, DEFAULT_MAX_SPEC_BYTES);
        let l = f.local();
        assert_eq!(l.last_checked_at, Some(fixed_now().to_rfc3339()));
        assert_eq!(l.last_status, Some(SyncStatus::Unchanged));
    }

    #[tokio::test]
    async fn same_body_with_new_etag_is_unchanged_and_updates_the_validators() {
        let mut src = source(github());
        src.pending_sha256 = Some("old".into());
        let f = Fixture::new(src);
        FsOpenApiSourceRepo
            .save_pending(f.root(), b"leftover")
            .expect("pending");
        f.fetcher.push(fetched(fixture(), "\"v2\""));
        let out = f.svc.check(f.root()).await.expect("check");
        assert_eq!(out, CheckOutcome::Unchanged);
        let s = f.load();
        assert_eq!(s.etag.as_deref(), Some("\"v2\""));
        assert_eq!(s.pending_sha256, None);
        assert_eq!(f.pending(), None);
    }

    #[tokio::test]
    async fn changed_valid_spec_becomes_pending() {
        let f = Fixture::new(source(github()));
        let body = changed("a");
        f.fetcher.push(fetched(body.clone(), "\"v2\""));
        let before = f.load();
        let out = f.svc.check(f.root()).await.expect("check");
        let hash = sha256_hex(&body);
        assert_eq!(
            out,
            CheckOutcome::Changed {
                spec_sha256: hash.clone()
            }
        );
        assert_eq!(f.pending(), Some(body));
        let s = f.load();
        assert_eq!(s.spec_sha256, before.spec_sha256);
        assert_eq!(s.operations, before.operations);
        assert_eq!(s.pending_sha256, Some(hash));
        assert_eq!(f.local().last_status, Some(SyncStatus::Changed));
        assert_eq!(s.etag.as_deref(), Some("\"v2\""));
    }

    #[tokio::test]
    async fn a_second_different_spec_replaces_the_pending_copy() {
        let f = Fixture::new(source(github()));
        f.fetcher.push(fetched(changed("a"), "\"v2\""));
        f.fetcher.push(fetched(changed("b"), "\"v3\""));
        f.svc.check(f.root()).await.expect("first");
        let second = changed("b");
        let out = f.svc.check(f.root()).await.expect("second");
        assert_eq!(
            out,
            CheckOutcome::Changed {
                spec_sha256: sha256_hex(&second)
            }
        );
        assert_eq!(f.pending(), Some(second.clone()));
        assert_eq!(f.load().pending_sha256, Some(sha256_hex(&second)));
    }

    #[tokio::test]
    async fn the_same_pending_spec_again_keeps_the_pending_copy() {
        let f = Fixture::new(source(github()));
        f.fetcher.push(fetched(changed("a"), "\"v2\""));
        f.fetcher.push(fetched(changed("a"), "\"v3\""));
        f.svc.check(f.root()).await.expect("first");
        let out = f.svc.check(f.root()).await.expect("second");
        assert!(matches!(out, CheckOutcome::Changed { .. }));
        assert_eq!(f.pending(), Some(changed("a")));
        assert_eq!(f.load().etag.as_deref(), Some("\"v3\""));
    }

    #[tokio::test]
    async fn not_modified_after_a_change_keeps_the_change() {
        let f = Fixture::new(source(github()));
        f.fetcher.push(fetched(changed("a"), "\"v2\""));
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.svc.check(f.root()).await.expect("first");
        let out = f.svc.check(f.root()).await.expect("second");
        assert!(matches!(out, CheckOutcome::Changed { .. }), "{out:?}");
        assert_eq!(f.pending(), Some(changed("a")));
        assert_eq!(f.local().last_status, Some(SyncStatus::Changed));
    }

    #[tokio::test]
    async fn html_body_fails_and_keeps_everything() {
        let mut src = source(github());
        src.pending_sha256 = Some(sha256_hex(b"kept"));
        let f = Fixture::new(src.clone());
        FsOpenApiSourceRepo
            .save_pending(f.root(), b"kept")
            .expect("pending");
        f.fetcher
            .push(fetched(b"<html></html>".to_vec(), "\"new\""));
        let out = f.svc.check(f.root()).await.expect("check");
        match &out {
            CheckOutcome::Failed { message, .. } => {
                assert!(message.starts_with("Not an OpenAPI"), "{message}");
                assert_eq!(message.matches("not an OpenAPI").count(), 0, "{message}");
            }
            other => panic!("{other:?}"),
        }
        let req = f.fetcher.last_request();
        assert_eq!(req.if_none_match, src.etag);
        assert_eq!(req.if_modified_since, src.last_modified);
        let s = f.load();
        assert_eq!(s.etag, src.etag);
        assert_eq!(s.last_modified, src.last_modified);
        assert_eq!(s.spec_sha256, src.spec_sha256);
        assert_eq!(s.pending_sha256, src.pending_sha256);
        assert_eq!(f.pending(), Some(b"kept".to_vec()));
        let l = f.local();
        assert_eq!(l.last_checked_at, Some(fixed_now().to_rfc3339()));
        assert_eq!(
            l.last_status,
            Some(SyncStatus::Failed {
                code: "not_a_spec".into()
            })
        );
    }

    #[tokio::test]
    async fn fetch_failures_map_to_messages_and_retry_after() {
        let cases = [
            (SpecFetchFailure::NotFound, "not_found", None, "private"),
            (
                SpecFetchFailure::RateLimited {
                    retry_after_secs: Some(60),
                },
                "rate_limited",
                Some(60),
                "60 seconds",
            ),
            (
                SpecFetchFailure::Network("x".into()),
                "network",
                None,
                "could not be fetched: x",
            ),
        ];
        for (failure, code, retry, wording) in cases {
            let src = source(github());
            let f = Fixture::new(src.clone());
            f.fetcher.push(Err(failure));
            let out = f.svc.check(f.root()).await.expect("check");
            match out {
                CheckOutcome::Failed {
                    message,
                    retry_after_secs,
                } => {
                    assert!(message.contains(wording), "{message}");
                    assert_eq!(retry_after_secs, retry);
                }
                other => panic!("{other:?}"),
            }
            let s = f.load();
            let l = f.local();
            assert_eq!(l.last_checked_at, Some(fixed_now().to_rfc3339()));
            assert_eq!(
                l.last_status,
                Some(SyncStatus::Failed { code: code.into() })
            );
            assert_eq!(s.etag, src.etag);
            assert_eq!(s.spec_sha256, src.spec_sha256);
            assert_eq!(s.pending_sha256, None);
            assert_eq!(f.pending(), None);
        }
    }

    #[tokio::test]
    async fn github_token_is_sent_as_a_bearer_to_the_contents_api() {
        let f = Fixture::new(source(github()));
        f.svc.set_token(f.root(), TOKEN).expect("token");
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.svc.check(f.root()).await.expect("check");
        let req = f.fetcher.last_request();
        assert_eq!(req.bearer_token.as_deref(), Some(TOKEN));
        assert!(
            req.url.starts_with("https://api.github.com/"),
            "{}",
            req.url
        );
        assert_eq!(
            req.accept.as_deref(),
            Some("application/vnd.github.raw+json")
        );
    }

    #[tokio::test]
    async fn https_location_never_sends_a_token_and_refuses_to_store_one() {
        let f = Fixture::new(source(SpecLocation::Https {
            url: "https://example.com/a.json".into(),
        }));
        assert!(f.svc.set_token(f.root(), TOKEN).is_err());
        assert!(f.svc.clear_token(f.root()).is_err());
        // Even a token stored under the id is not read for an Https location.
        f.secrets.set("src-1", TOKEN_KEY, TOKEN).expect("seed");
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.svc.check(f.root()).await.expect("check");
        assert_eq!(f.fetcher.last_request().bearer_token, None);
    }

    fn files_under(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                files_under(&path, out);
            } else {
                out.push(path);
            }
        }
    }

    #[tokio::test]
    async fn token_never_reaches_the_collection_folder() {
        let f = Fixture::new(source(github()));
        f.svc.set_token(f.root(), TOKEN).expect("token");
        f.fetcher.push(fetched(changed("a"), "\"v2\""));
        f.svc.check(f.root()).await.expect("check");
        f.fetcher.push(Err(SpecFetchFailure::Unauthorized));
        let out = f.svc.check(f.root()).await.expect("check");
        assert!(!format!("{out:?}").contains(TOKEN));
        let mut files = Vec::new();
        files_under(f.root(), &mut files);
        assert!(!files.is_empty());
        for file in files {
            let bytes = std::fs::read(&file).expect("read");
            let text = String::from_utf8_lossy(&bytes);
            assert!(!text.contains(TOKEN), "{}", file.display());
        }
        let status = f.svc.status(f.root()).expect("status");
        assert!(!format!("{status:?}").contains(TOKEN));
    }

    #[test]
    fn unauthorized_message_depends_on_the_token() {
        let with = fetch_failure_message(&SpecFetchFailure::Unauthorized, true);
        assert!(with.contains("token"), "{with}");
        let without = fetch_failure_message(&SpecFetchFailure::Unauthorized, false);
        assert!(without.contains("private"), "{without}");
    }

    #[tokio::test]
    async fn unauthorized_with_a_token_names_the_token() {
        let f = Fixture::new(source(github()));
        f.svc.set_token(f.root(), TOKEN).expect("token");
        f.fetcher.push(Err(SpecFetchFailure::Unauthorized));
        match f.svc.check(f.root()).await.expect("check") {
            CheckOutcome::Failed { message, .. } => {
                assert!(message.contains("token"), "{message}")
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn unauthorized_without_a_token_says_to_add_one() {
        let f = Fixture::new(source(github()));
        f.fetcher.push(Err(SpecFetchFailure::Unauthorized));
        match f.svc.check(f.root()).await.expect("check") {
            CheckOutcome::Failed { message, .. } => {
                assert!(message.contains("private"), "{message}");
                assert!(message.contains("access token"), "{message}");
                assert!(!message.contains("rejected"), "{message}");
            }
            other => panic!("{other:?}"),
        }
    }

    fn locked_fixture() -> (TempDir, Arc<FakeFetcher>, OpenApiSyncService) {
        let dir = TempDir::new().expect("tempdir");
        FsOpenApiSourceRepo
            .save(dir.path(), &source(github()))
            .expect("seed");
        let fetcher = Arc::new(FakeFetcher::default());
        let svc = OpenApiSyncService::new(
            Arc::new(SharedFetcher(Arc::clone(&fetcher))),
            Box::new(FsOpenApiSourceRepo),
            Arc::new(LockedSecrets),
        )
        .with_local(Box::new(FsOpenApiLocalRepo))
        .with_clock(Arc::new(fixed_now));
        (dir, fetcher, svc)
    }

    #[tokio::test]
    async fn a_locked_keychain_still_fetches_a_public_file_without_a_token() {
        let (dir, fetcher, svc) = locked_fixture();
        fetcher.push(Ok(SpecFetchOutcome::NotModified));
        let out = svc.check(dir.path()).await.expect("check");
        assert_eq!(out, CheckOutcome::Unchanged);
        let req = fetcher.last_request();
        assert_eq!(req.bearer_token, None);
        assert!(req.url.starts_with("https://raw.githubusercontent.com/"));
    }

    #[tokio::test]
    async fn a_locked_keychain_explains_a_not_found_without_the_error_text() {
        let (dir, fetcher, svc) = locked_fixture();
        for failure in [SpecFetchFailure::NotFound, SpecFetchFailure::Unauthorized] {
            fetcher.push(Err(failure));
            match svc.check(dir.path()).await.expect("check") {
                CheckOutcome::Failed { message, .. } => {
                    assert!(message.contains("keychain"), "{message}");
                    assert!(message.contains("token"), "{message}");
                    assert!(!message.contains("secret-detail"), "{message}");
                    assert!(!message.contains("locked"), "{message}");
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn an_unchanged_spec_saves_the_source_before_removing_the_pending_copy() {
        let mut src = source(github());
        src.pending_sha256 = Some("old".into());
        let dir = TempDir::new().expect("tempdir");
        FsOpenApiSourceRepo.save(dir.path(), &src).expect("seed");
        FsOpenApiSourceRepo
            .save_pending(dir.path(), b"leftover")
            .expect("pending");
        let fail = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let fetcher = Arc::new(FakeFetcher::default());
        let svc = OpenApiSyncService::new(
            Arc::new(SharedFetcher(Arc::clone(&fetcher))),
            Box::new(FlakyRepo(Arc::clone(&fail))),
            Arc::new(MemSecrets::default()),
        )
        .with_local(Box::new(FsOpenApiLocalRepo));
        fetcher.push(fetched(fixture(), "\"v2\""));
        assert!(svc.check(dir.path()).await.is_err());
        // The failed save left the hash and the file in step.
        let loaded = FsOpenApiSourceRepo
            .load(dir.path())
            .expect("load")
            .expect("src");
        assert_eq!(loaded.pending_sha256, Some("old".into()));
        assert_eq!(
            FsOpenApiSourceRepo
                .load_pending(dir.path())
                .expect("pending"),
            Some(b"leftover".to_vec())
        );
    }

    #[test]
    fn token_follows_a_copied_collection_folder() {
        let f = Fixture::new(source(github()));
        f.svc.set_token(f.root(), TOKEN).expect("token");
        let copy = TempDir::new().expect("tempdir");
        let mut files = Vec::new();
        files_under(f.root(), &mut files);
        for file in files {
            let rel = file.strip_prefix(f.root()).expect("rel");
            let to = copy.path().join(rel);
            std::fs::create_dir_all(to.parent().expect("parent")).expect("mkdir");
            std::fs::copy(&file, &to).expect("copy");
        }
        assert!(f.svc.status(copy.path()).expect("status").has_token);
    }

    #[test]
    fn set_token_adds_a_source_id_to_an_older_file() {
        let mut src = source(github());
        src.source_id = String::new();
        let f = Fixture::new(src);
        assert!(!f.svc.status(f.root()).expect("status").has_token);
        f.svc.set_token(f.root(), TOKEN).expect("token");
        assert!(!f.load().source_id.is_empty());
        assert!(f.svc.status(f.root()).expect("status").has_token);
    }

    #[test]
    fn set_interval_persists_and_clear_token_forgets() {
        let f = Fixture::new(source(github()));
        f.svc
            .set_interval(f.root(), SyncInterval::Daily)
            .expect("interval");
        assert_eq!(f.local().sync_interval, SyncInterval::Daily);
        f.svc.set_token(f.root(), TOKEN).expect("token");
        f.svc.clear_token(f.root()).expect("clear");
        assert!(!f.svc.status(f.root()).expect("status").has_token);
    }

    #[test]
    fn status_reports_pending_and_token_flags() {
        let mut src = source(github());
        src.pending_sha256 = Some("abc".into());
        let f = Fixture::new(src);
        f.svc
            .set_interval(f.root(), SyncInterval::Weekly)
            .expect("interval");
        let s = f.svc.status(f.root()).expect("status");
        assert!(s.has_pending);
        assert!(!s.has_token);
        assert_eq!(s.interval, SyncInterval::Weekly);
        assert_eq!(s.location, "https://github.com/o/r/blob/main/api.json");
        f.svc.set_token(f.root(), TOKEN).expect("token");
        assert!(f.svc.status(f.root()).expect("status").has_token);
    }

    #[tokio::test]
    async fn no_source_file_is_an_error() {
        let dir = TempDir::new().expect("tempdir");
        let svc = OpenApiSyncService::new(
            Arc::new(SharedFetcher(Arc::new(FakeFetcher::default()))),
            Box::new(FsOpenApiSourceRepo),
            Arc::new(MemSecrets::default()),
        )
        .with_local(Box::new(FsOpenApiLocalRepo));
        assert!(svc.check(dir.path()).await.is_err());
    }

    #[tokio::test]
    async fn check_writes_timing_to_the_local_state_not_the_source() {
        let f = Fixture::new(source(github()));
        let before = f.source_bytes();
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.svc.check(f.root()).await.expect("check");
        assert_eq!(f.source_bytes(), before);
        assert_eq!(f.local().last_checked_at, Some(fixed_now().to_rfc3339()));
    }

    #[tokio::test]
    async fn failure_counts_and_success_resets() {
        let f = Fixture::new(source(github()));
        f.fetcher.push(Err(SpecFetchFailure::NotFound));
        f.fetcher.push(Err(SpecFetchFailure::Network("x".into())));
        f.svc.check(f.root()).await.expect("first");
        assert_eq!(f.local().consecutive_failures, 1);
        f.svc.check(f.root()).await.expect("second");
        assert_eq!(f.local().consecutive_failures, 2);
        assert_eq!(f.local().not_before, None);
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.svc.check(f.root()).await.expect("third");
        let l = f.local();
        assert_eq!(l.consecutive_failures, 0);
        assert_eq!(l.not_before, None);
        assert_eq!(l.last_status, Some(SyncStatus::Unchanged));
    }

    #[tokio::test]
    async fn rate_limit_sets_not_before() {
        let f = Fixture::new(source(github()));
        f.fetcher.push(Err(SpecFetchFailure::RateLimited {
            retry_after_secs: Some(600),
        }));
        f.svc.check(f.root()).await.expect("check");
        let expected = (fixed_now() + chrono::Duration::seconds(600)).to_rfc3339();
        assert_eq!(f.local().not_before, Some(expected));
        // A success clears it.
        f.fetcher.push(Ok(SpecFetchOutcome::NotModified));
        f.svc.check(f.root()).await.expect("check");
        assert_eq!(f.local().not_before, None);
    }

    #[test]
    fn set_interval_writes_the_local_state() {
        let f = Fixture::new(source(github()));
        let before = f.source_bytes();
        f.svc
            .set_interval(f.root(), SyncInterval::Hourly)
            .expect("interval");
        assert_eq!(f.local().sync_interval, SyncInterval::Hourly);
        assert_eq!(f.source_bytes(), before);
        f.svc
            .set_interval(f.root(), SyncInterval::Off)
            .expect("interval");
        assert_eq!(f.local().sync_interval, SyncInterval::Off);
    }

    #[test]
    fn status_falls_back_to_legacy_source_timing() {
        let f = Fixture::new(source(github()));
        let mut yaml = String::from_utf8(f.source_bytes()).expect("utf8");
        yaml.push_str(
            "last_checked_at: '2026-10-25T12:00:00+00:00'\nlast_status:\n  state: changed\n",
        );
        std::fs::write(f.root().join(".rocket/openapi-source.yml"), yaml).expect("write");
        let s = f.svc.status(f.root()).expect("status");
        assert_eq!(
            s.last_checked_at.as_deref(),
            Some("2026-10-25T12:00:00+00:00")
        );
        assert_eq!(s.last_status, Some(SyncStatus::Changed));
        // Once the local state has a check, it wins.
        let mut l = OpenApiLocalState::default();
        l.last_checked_at = Some(fixed_now().to_rfc3339());
        l.last_status = Some(SyncStatus::Unchanged);
        FsOpenApiLocalRepo.save(f.root(), &l).expect("save");
        let s = f.svc.status(f.root()).expect("status");
        assert_eq!(s.last_status, Some(SyncStatus::Unchanged));
    }

    #[test]
    fn a_committed_hourly_source_does_not_turn_the_interval_on() {
        let f = Fixture::new(source(github()));
        let mut yaml = String::from_utf8(f.source_bytes()).expect("utf8");
        yaml.push_str("sync_interval: hourly\n");
        std::fs::write(f.root().join(".rocket/openapi-source.yml"), yaml).expect("write");
        assert_eq!(f.load().sync_interval, SyncInterval::Hourly);
        let s = f.svc.status(f.root()).expect("status");
        assert_eq!(s.interval, SyncInterval::Off);
    }

    #[tokio::test]
    async fn without_a_local_repository_check_and_set_interval_fail() {
        let dir = TempDir::new().expect("tempdir");
        FsOpenApiSourceRepo
            .save(dir.path(), &source(github()))
            .expect("seed");
        let fetcher = Arc::new(FakeFetcher::default());
        let svc = OpenApiSyncService::new(
            Arc::new(SharedFetcher(Arc::clone(&fetcher))),
            Box::new(FsOpenApiSourceRepo),
            Arc::new(MemSecrets::default()),
        );
        assert!(svc.check(dir.path()).await.is_err());
        assert!(svc.set_interval(dir.path(), SyncInterval::Daily).is_err());
        assert!(fetcher.requests.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn an_unreadable_local_state_fails_the_check_before_any_request() {
        for text in ["{ not json", "{\"version\": 2}"] {
            let f = Fixture::new(source(github()));
            std::fs::create_dir_all(f.root().join(".rocket")).expect("dir");
            std::fs::write(f.root().join(".rocket/openapi-local.json"), text).expect("write");
            assert!(f.svc.check(f.root()).await.is_err(), "{text}");
            assert!(
                f.fetcher.requests.lock().expect("lock").is_empty(),
                "{text}"
            );
        }
    }

    /// Runs a callback while the fetch is in flight, then answers 304.
    struct GatedFetcher(Box<dyn Fn() + Send + Sync>);

    #[async_trait]
    impl SpecFetcher for GatedFetcher {
        async fn fetch(&self, _request: SpecFetchRequest) -> Reply {
            (self.0)();
            Ok(SpecFetchOutcome::NotModified)
        }
    }

    #[tokio::test]
    async fn a_check_keeps_local_changes_made_during_the_fetch() {
        let dir = TempDir::new().expect("tempdir");
        FsOpenApiSourceRepo
            .save(dir.path(), &source(github()))
            .expect("seed");
        let root = dir.path().to_path_buf();
        let mid_fetch = {
            let root = root.clone();
            move || {
                let other = OpenApiSyncService::new(
                    Arc::new(SharedFetcher(Arc::new(FakeFetcher::default()))),
                    Box::new(FsOpenApiSourceRepo),
                    Arc::new(MemSecrets::default()),
                )
                .with_local(Box::new(FsOpenApiLocalRepo));
                other
                    .set_interval(&root, SyncInterval::Weekly)
                    .expect("interval");
                let mut local = FsOpenApiLocalRepo
                    .load(&root)
                    .expect("load")
                    .unwrap_or_default();
                local.skips.insert("listPets".into(), "fp".into());
                FsOpenApiLocalRepo.save(&root, &local).expect("save");
            }
        };
        let svc = OpenApiSyncService::new(
            Arc::new(GatedFetcher(Box::new(mid_fetch))),
            Box::new(FsOpenApiSourceRepo),
            Arc::new(MemSecrets::default()),
        )
        .with_local(Box::new(FsOpenApiLocalRepo))
        .with_clock(Arc::new(fixed_now));
        svc.check(&root).await.expect("check");
        let local = FsOpenApiLocalRepo
            .load(&root)
            .expect("load")
            .expect("state");
        assert_eq!(local.sync_interval, SyncInterval::Weekly);
        assert_eq!(local.skips.get("listPets").map(String::as_str), Some("fp"));
        assert_eq!(local.last_checked_at, Some(fixed_now().to_rfc3339()));
    }
}
