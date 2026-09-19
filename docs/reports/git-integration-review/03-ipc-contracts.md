# Frontend Git API / IPC contract review

**Scope:** frontend Git types and wrappers in `src/lib/tauri-api.ts`; production consumers in `src/stores/git-store.ts`, `src/components/git/*`, and `src/components/workspace/WorkspaceGitTab.tsx`; Tauri commands in `src-tauri/src/commands/git.rs`; command registration and service wiring in `src-tauri/src/lib.rs`; application methods in `crates/rocket-app/src/git_service.rs`; the `GitService` trait and libgit2 implementation in `crates/rocket-git`.

**Method:** static end-to-end contract tracing, plus `yarn tsc --noEmit`. No production code was changed. The overlapping path, credential, host-verification, operation-atomicity, event, and architecture risks are analyzed in more depth in [`04-orchestration-security.md`](04-orchestration-security.md); this report focuses on what crosses IPC and what frontend callers can safely infer from it.

## Executive summary

The nominal command wiring is complete: all **39** Git-related Tauri commands registered in `src-tauri/src/lib.rs:359-397` have matching frontend wrappers and production UI/store consumers. The camelCase wire names for current structs and tagged enums also line up in the common paths.

The contract is nevertheless unsafe and misleading in several important ways:

1. Repository roots and Git-relative file paths are renderer-controlled strings. In particular, `git_diff`, `git_discard`, and `git_resolve_conflict` can escape the repository through absolute or `..` paths before any containment check.
2. `DomainError` is serialized as a display string. The frontend consequently classifies authentication and conflict behavior by English substring matching, and cannot distinguish no mutation from conflict/partial mutation.
3. Zustand actions catch command rejection and still resolve `Promise<void>`, while several components treat `await` as success. Failed commits can clear the commit message; failed fetch/pull calls can update “last fetched”; remote edits close after failure.
4. `push`, `pull`, and `fetch` can call the typed wrapper with a runtime `undefined` remote when the repository has no remotes. TypeScript does not catch this because array indexing is not checked by the current compiler configuration.
5. Rust `Option<String>` values serialize as `null`, but several frontend DTOs model them as optional properties. More seriously, diff/conflict implementations collapse absent, binary, unreadable, and invalid UTF-8 content into `None`/empty strings, which the UI interprets as added/deleted or editable text.
6. Every handler is synchronous, including clone/fetch/pull/push, keyring calls, repository scans, and config I/O. There is no cancellation, timeout, progress contract, or per-repository operation serialization.
7. Credentials are round-tripped as plaintext through renderer memory and selected only by caller-provided workspace ID; SSH/HTTPS server identity is unconditionally accepted by the libgit2 callback.

### Finding summary

| ID | Severity | Contract issue |
|---|---:|---|
| IPC-01 | Critical | Renderer-controlled repository/file paths are authority-bearing and permit repository escape |
| IPC-02 | Critical | Remote certificate/SSH host identity is accepted unconditionally while credentials cross IPC |
| IPC-03 | High | Credential records are caller-addressable and secrets are returned to renderer memory |
| API-01 | High | Store actions resolve after failure, causing false-success UI behavior |
| API-02 | High | Network operations can invoke Rust without a `remote` argument |
| ERR-01 | Medium | String-only errors destroy category, context, and mutation semantics |
| TYPE-01 | Medium | Diff/conflict content conflates null, absent, binary, unreadable, and invalid UTF-8 states |
| TYPE-02 | Medium | Frontend nullability, numeric bounds, and duplicated DTO ownership can drift silently |
| IPC-04 | Medium | All Git/keyring/filesystem commands are synchronous and non-cancellable |
| EVT-01 | Medium | Event/result data is exposed but unused or too coarse to support reliable refresh behavior |
| NAME-01 | Low | `collectionPath` names a workspace/repository root, while identity uses the generic wire key `path` |

## Contract conventions and matrix legend

- **`DE-string`** means `Result<T, DomainError>` whose error crosses IPC as a plain string because `DomainError::serialize` calls `serializer.serialize_str(&self.to_string())` (`crates/rocket-shared/src/error.rs:37-43`).
- **`direct`** means the Tauri command bypasses `GitAppService`/`GitService` and performs I/O in `src-tauri/src/commands/git.rs`.
- Every command below is registered in `src-tauri/src/lib.rs:359-397`.
- `Git2Service` is injected into `GitAppService` at `src-tauri/src/lib.rs:243-246`. The trait-to-implementation delegation is explicit in `crates/rocket-git/src/git2_service/mod.rs:39-180`.
- Store consumers generally convert rejection to `error: String(e)` and return fulfilled `Promise<void>`; direct component consumers generally receive the raw rejected `invoke` promise.

## Command matrix

### Repository, status, diff, staging, commit, and network

| Frontend wrapper | Tauri command → app method | Trait / implementation | Production consumer | Success / error shape |
|---|---|---|---|---|
| `gitIsRepo(collectionPath)` | `git_is_repo(collection_path)` → `is_repo` | `is_repo` → `repo::is_repo` | `git-store.setCollection`; reached from `GitPanel` | `boolean`; **never errors**—all repository-open failures become `false` |
| `gitInit(collectionPath)` | `git_init` → `init` | `init` → `repo::init` | `git-store.initRepo` → `GitPanel` | `void` / DE-string |
| `gitClone(url, destPath, creds)` | `git_clone` → `clone_repo` | `clone_repo` → `repo::clone_repo` | `GitCloneDialog` | `void` / DE-string; no progress/cancel/partial-destination outcome |
| `gitStatus(collectionPath)` | `git_status` → `status` | `status` → `status_diff::status` | `setCollection`, refreshes, file list, landing/branch panels | `RepoStatus` / DE-string |
| `gitDiff(collectionPath, file)` | `git_diff` → `diff_file` | `diff_file` → `status_diff::diff_file` | `DiffViewForFile`, `DiffViewer` | `FileDiff` / DE-string, but worktree read failures become `newContent: null` |
| `gitDiffStaged(collectionPath, file)` | `git_diff_staged` → `diff_staged` | `diff_staged` → `status_diff::diff_staged` | `DiffViewForFile`, `DiffViewer` | `FileDiff` / DE-string; index lookup/non-UTF8 becomes `newContent: null` |
| `gitDiffCommit(collectionPath, oid)` | `git_diff_commit` → `diff_commit` | `diff_commit` → `staging::diff_commit` | `GitPanel` commit-detail view | `FileDiff[]` / DE-string; content conversion has the same null ambiguity |
| `gitStage(collectionPath, files)` | `git_stage` → `stage` | `stage` → `staging::stage` | `stageFiles`, `stageAll` → `GitFileList` | `void` / DE-string; batch outcome is not per-file |
| `gitUnstage(collectionPath, files)` | `git_unstage` → `unstage` | `unstage` → `staging::unstage` | `unstageFiles`, `unstageAll` → `GitFileList` | `void` / DE-string |
| `gitDiscard(collectionPath, files)` | `git_discard` → `discard` | `discard` → `staging::discard` | `discardFiles` → `GitFileList` | `void` / DE-string; destructive batch can partially complete |
| `gitCommit(collectionPath, message)` | `git_commit` → `commit` | `commit` → `staging::commit` | `commitChanges` → `GitCommitForm` | `CommitInfo` / DE-string; returned commit is discarded by store |
| `gitLog(collectionPath, limit)` | `git_log` → `log` | `log` → `staging::log` | `refreshLog` → `GitCommitLog` | `CommitInfo[]` / DE-string |
| `gitPush(collectionPath, remote, creds)` | `git_push` → `push` | `push` → `remote::push` | `git-store.push` → `GitLandingPanel` | `void` / DE-string; store fulfills after failure |
| `gitPull(collectionPath, remote, creds)` | `git_pull` → `pull` | `pull` → `remote::pull` | `git-store.pull` → `GitLandingPanel` | `void` / DE-string; conflict/partial mutation is only an error string |
| `gitFetch(collectionPath, remote, creds)` | `git_fetch` → `fetch` | `fetch` → `remote::fetch` | `git-store.fetch` → `GitLandingPanel` | `FetchResult` / DE-string; result is discarded by store/UI |

Evidence: wrappers at `src/lib/tauri-api.ts:715-760`; commands at `src-tauri/src/commands/git.rs:17-98`; app methods at `crates/rocket-app/src/git_service.rs:19-147`; trait at `crates/rocket-git/src/service.rs:10-43`; store calls at `src/stores/git-store.ts:133-341` and `611-700`; direct diff/clone consumers at `src/components/git/DiffViewForFile.tsx:25-40`, `src/components/git/DiffViewer.tsx:70-84`, and `src/components/git/GitCloneDialog.tsx:97-135`.

### Branches, stashes, conflicts, and remotes

| Frontend wrapper | Tauri command → app method | Trait / implementation | Production consumer | Success / error shape |
|---|---|---|---|---|
| `gitBranches(collectionPath)` | `git_branches` → `branches` | `branches` → `branch::branches` | `refreshBranches` → `BranchSelector` | `BranchList` / DE-string |
| `gitSwitchBranch(collectionPath, name)` | `git_switch_branch` → `switch_branch` | `switch_branch` → `branch::switch_branch` | `git-store.switchBranch` → `BranchSelector` | `void` / DE-string |
| `gitCheckoutRemoteBranch(collectionPath, name)` | `git_checkout_remote_branch` → `checkout_remote_branch` | same → `branch::checkout_remote_branch` | `git-store.checkoutRemoteBranch` → `BranchSelector` | `void` / DE-string; force checkout/partial state not represented |
| `gitCreateBranch(collectionPath, name)` | `git_create_branch` → `create_branch` | same → `branch::create_branch` | `git-store.createBranch` → `BranchSelector` | `void` / DE-string; creation also switches branch |
| `gitDeleteBranch(collectionPath, name)` | `git_delete_branch` → `delete_branch` | same → `branch::delete_branch` | `git-store.deleteBranch` → `BranchSelector` | `void` / DE-string; no app event |
| `gitMergeBranch(collectionPath, name)` | `git_merge_branch` → `merge_branch` | same → `branch::merge_branch` | `git-store.mergeBranch` → `BranchSelector` | `void` / DE-string; UI identifies conflict by substring |
| `gitStashList(collectionPath)` | `git_stash_list` → `stash_list` | same → `stash::stash_list` | `refreshStashes` → `GitStashSection` | `StashEntry[]` / DE-string |
| `gitStashSave(collectionPath, message)` | `git_stash_save` → `stash_save` | same → `stash::stash_save` | `saveStash` → stash section/auto-stash pull | `void` / DE-string |
| `gitStashPop(collectionPath, index)` | `git_stash_pop` → `stash_pop` | same → `stash::stash_pop` | single/batch pop, auto-stash pull | `void` / DE-string |
| `gitStashApply(collectionPath, index)` | `git_stash_apply` → `stash_apply` | same → `stash::stash_apply` | single/batch apply | `void` / DE-string |
| `gitStashDrop(collectionPath, index)` | `git_stash_drop` → `stash_drop` | same → `stash::stash_drop` | single/batch drop | `void` / DE-string |
| `gitConflicts(collectionPath)` | `git_conflicts` → `conflicts` | `conflicts` → `conflict::conflicts` | `refreshConflicts` → `GitFileList`/resolver | `ConflictFile[]` / DE-string; store converts any error to `[]` |
| `gitResolveConflict(collectionPath, file, resolution)` | `git_resolve_conflict` → `resolve_conflict` | same → `conflict::resolve_conflict` | `git-store.resolveConflict` → `ConflictResolver` | `void` / DE-string; no app event |
| `gitAbortMerge(collectionPath)` | `git_abort_merge` → `abort_merge` | same → `conflict::abort_merge` | `git-store.abortMerge` → `ConflictResolver` | `void` / DE-string; performs hard reset |
| `gitListRemotes(collectionPath)` | `git_list_remotes` → `list_remotes` | same → `remote::list_remotes` | `refreshRemotes` → `GitRemotesDialog`/network default | `RemoteInfo[]` / DE-string |
| `gitAddRemote(collectionPath, name, url)` | `git_add_remote` → `add_remote` | same → `remote::add_remote` | `git-store.addRemote` → `GitRemotesDialog` | `void` / DE-string |
| `gitRemoveRemote(collectionPath, name)` | `git_remove_remote` → `remove_remote` | same → `remote::remove_remote` | `git-store.removeRemote` → `GitRemotesDialog` | `void` / DE-string |
| `gitSetRemoteUrl(collectionPath, name, url)` | `git_set_remote_url` → `set_remote_url` | same → `remote::set_remote_url` | `git-store.setRemoteUrl` → `GitRemotesDialog` | `void` / DE-string; no app event |

Evidence: wrappers at `src/lib/tauri-api.ts:759-814`; commands at `src-tauri/src/commands/git.rs:95-183`; app methods at `crates/rocket-app/src/git_service.rs:145-250`; trait at `crates/rocket-git/src/service.rs:42-60`; store calls at `src/stores/git-store.ts:178-550`; component consumers at `src/components/git/BranchSelector.tsx:16-95`, `src/components/git/GitFileList.tsx:25-73`, `src/components/git/GitStashSection.tsx:39-118`, and `src/components/git/GitRemotesDialog.tsx:15-47`.

### Credential, SSH-discovery, and identity helpers

These commands are registered beside Git commands but bypass `GitAppService` and `GitService`.

| Frontend wrapper | Tauri command / implementation | Production consumer | Success / error shape |
|---|---|---|---|
| `getDefaultSshKeyPath()` | `get_default_ssh_key_path`; scans standard names under `~/.ssh` | `GitCredentialsDialog` auto-detect/browse default | `string | null`; home/metadata failures silently become `null` |
| `listSshKeyPaths()` | `list_ssh_key_paths`; synchronous `read_dir`, paired-`.pub` heuristic | `GitCredentialsDialog` key selector | `string[]`; directory/read failures silently become `[]` |
| `saveGitCredentials(workspaceId, creds)` | `save_git_credentials`; serializes `GitCredentialsPayload` and writes keyring | `GitCredentialsDialog.handleConnect` | `void` / DE-string; persistence failure does not block credential activation |
| `loadGitCredentials(workspaceId)` | `load_git_credentials`; reads/deserializes full secret payload | `setCollection`, `GitCredentialsDialog` | `GitCredentials | null` / DE-string; secret returned to JS |
| `gitGetIdentity(collectionPath)` | `git_get_identity(path)`; opens repository/config directly | `GitCommitForm`, credential identity flow | `GitIdentity` / DE-string; missing fields become empty strings |
| `gitSetIdentity(collectionPath, name, email)` | `git_set_identity(path, name, email)`; writes local config directly | `GitCommitForm`, `GitPanel` | `void` / DE-string; two writes can partially complete |

Evidence: wrappers at `src/lib/tauri-api.ts:816-837`; commands at `src-tauri/src/commands/git.rs:185-371`; consumers at `src/components/git/GitCredentialsDialog.tsx:40-130`, `src/components/git/GitCommitForm.tsx:27-57`, `src/components/git/GitPanel.tsx:107-117`, and `src/stores/git-store.ts:140-150`, `552-609`.

## Reachability and unused surface

### Commands with no UI

None. Every one of the 39 registered commands has a production frontend path.

That does **not** mean every exposed result or wrapper is useful:

- `git_fetch` returns `FetchResult { updatedRefs, receivedObjects, receivedBytes }` (`crates/rocket-git/src/remote.rs:10-20`; `src/lib/tauri-api.ts:398-402`), but `git-store.fetch` discards it (`src/stores/git-store.ts:666-688`). The only other frontend reference is a test fixture (`src/stores/__tests__/git-store.test.ts:111-117`).
- `git_commit` returns `CommitInfo`, but `commitChanges` discards it and reloads status/log (`src/stores/git-store.ts:300-310`).
- `onGitChanged` is exported at `src/lib/tauri-api.ts:864-865` but has no consumer. `CollectionsSidebar` bypasses it with raw `listen('git-changed', ...)` (`src/components/layout/CollectionsSidebar.tsx:289-301`).
- Event payloads are emitted, but the raw listener ignores them. All Git events are collapsed onto one channel (`src-tauri/src/tauri_event_bus.rs:48-59`).

**Action:** either use these results/payloads for targeted UI updates and progress/status messaging, or simplify the public contract. Avoid retaining nominal result types that callers consistently discard, because they create false confidence that the data is part of tested product behavior.

## Detailed findings and contract changes

### IPC-01 — Repository and file paths are unvalidated authority-bearing strings

**Severity: Critical**

**Evidence**

- The UI passes a workspace filesystem path into a prop and API named `collectionPath`: `WorkspaceGitTab` computes `workspacePath` and passes it to `GitPanel collectionPath` (`src/components/workspace/WorkspaceGitTab.tsx:9-25`).
- All service-backed commands accept that renderer-provided root directly (`src-tauri/src/commands/git.rs:17-183`); identity commands separately accept the same value under the wire key `path` (`src/lib/tauri-api.ts:816-820`; `src-tauri/src/commands/git.rs:347-370`).
- `git_diff` computes `Path::new(path).join(file)` and reads it (`crates/rocket-git/src/git2_service/status_diff.rs:100-113`). Absolute paths replace the base; `..` can escape it.
- `discard` joins and removes a file or recursively removes a directory if the path is not in `HEAD` (`crates/rocket-git/src/git2_service/staging.rs:58-89`).
- Conflict resolution writes the joined path before `index.add_path` can reject it (`crates/rocket-git/src/git2_service/conflict.rs:140-154`). An IPC call can therefore write outside the repo and still return an error.
- `git_init`, clone destination, and identity config paths are likewise unconstrained (`crates/rocket-git/src/git2_service/repo.rs:15-42`; `src-tauri/src/commands/git.rs:347-370`).

**Contract change**

1. Replace `collectionPath: string` with an opaque `repositoryId` resolved in Rust against registered workspaces/external collections.
2. For native-picker clone destinations, return a short-lived, destination-bound capability token; do not round-trip the selected absolute path as authority.
3. Define `RepoRelativePath` validation once: reject absolute paths, prefixes, `..`, NULs, and symlink escapes before any read/write/delete.
4. For destructive/conflict operations, validate membership in the current status/index/conflict set before mutation.

**Compatibility**

This is a wire-breaking change. Add `*_v2` commands accepting `{ repositoryId, ... }`, keep legacy path commands temporarily behind a deprecation flag, migrate all wrappers together, then remove legacy registration. Do not merely add frontend validation; direct IPC callers bypass it.

See also `04-orchestration-security.md`, GIT-SEC-02.

### IPC-02 / IPC-03 — Credential and remote-trust boundaries are not represented safely

**Severity: Critical / High**

**Evidence**

- `GitCredentials` contains plaintext password/token/passphrase fields and is accepted by clone/push/pull/fetch (`src/lib/tauri-api.ts:387-391`, `715-757`).
- Keyring lookup is selected by renderer-provided `workspaceId`; the account is only `git-credentials-{workspace_id}` and no workspace/repository association is checked (`src-tauri/src/commands/git.rs:185-189`, `318-340`).
- `load_git_credentials` returns the full decrypted payload to JavaScript (`src-tauri/src/commands/git.rs:330-337`), where it is retained in Zustand and dialog strings (`src/stores/git-store.ts:46-64`, `115-131`; `src/components/git/GitCredentialsDialog.tsx:27-35`, `54-70`).
- The same credential is reused for the first/default or selected remote, even after remote URL edits (`src/stores/git-store.ts:611-688`).
- `build_callbacks` unconditionally approves every certificate/host identity (`crates/rocket-git/src/git2_service/helpers.rs:15-20`).
- `Token` hardcodes the transport username to `oauth2` (`crates/rocket-git/src/git2_service/helpers.rs:56-58`), although HTTPS Git providers differ in accepted username/token conventions; the frontend type does not express provider or username semantics.

**Contract change**

- Return opaque credential profile metadata/IDs to the frontend, never reusable secrets.
- Resolve the secret in Rust for a backend-validated `repositoryId` and normalized remote authority.
- Add typed host-trust challenges (`unknownHostKey`, `hostKeyChanged`, TLS validation failure) and fail closed by default.
- Replace generic `{ type: 'token', token }` with an explicit HTTPS credential model, or retain a configurable username/provider profile.
- Add forget/delete lifecycle and prevent a profile bound to one authority from being sent to another.

**Compatibility**

Introduce profile commands alongside current save/load commands. Migrate existing keyring JSON on first backend use and return only redacted metadata. Existing credential unions should remain readable for migration but cease crossing IPC after rollout. Removing unconditional certificate acceptance may surface failures for users with missing SSH trust configuration; provide an explicit fingerprint-confirmation flow rather than a silent fallback.

See also `04-orchestration-security.md`, GIT-SEC-01, GIT-SEC-03, and GIT-SEC-04.

### API-01 — Store actions fulfill after failure, but UI treats fulfillment as success

**Severity: High**

**Evidence**

- Store actions catch command errors and set a global string without rethrowing; representative cases include commit (`src/stores/git-store.ts:300-310`), branches (`453-513`), remotes (`516-549`), and network operations (`611-688`). Their public signatures remain `Promise<void>` (`src/stores/git-store.ts:67-105`).
- `GitCommitForm.doCommit` always clears the message after `await commitChanges(...)` (`src/components/git/GitCommitForm.tsx:17-24`), including when IPC failed and the store swallowed rejection.
- `GitLandingPanel` updates `lastFetched` after `await fetch()` and after `await pull()` (`src/components/git/GitLandingPanel.tsx:53-67`, `83-89`, `114-121`). Those calls resolve even on failure.
- `handleFetchAndPush` may continue to push after a failed fetch because `fetch()` fulfills and status may remain stale (`src/components/git/GitLandingPanel.tsx:147-160`).
- Remote URL editing closes after `await setRemoteUrl(...)` without checking store error (`src/components/git/GitRemotesDialog.tsx:43-47`).
- Branch UI works around the contract by comparing global error strings before/after (`src/components/git/BranchSelector.tsx:41-95`), which fails if the same error repeats or a concurrent action changes the shared error.
- Stash UI uses yet another convention: clear the shared error, await, then inspect global state (`src/components/git/GitStashSection.tsx:62-118`).

**Contract change**

Choose one action contract and enforce it consistently:

```ts
type GitActionResult<T = void> =
  | { ok: true; value: T }
  | { ok: false; error: GitIpcError };
```

Prefer returning `GitActionResult` from store actions while optionally mirroring the latest error into presentation state. Components must update success-only UI from `result.ok`, not from promise fulfillment or global-error comparison. Alternatively, let actions reject and centralize presentation separately; do not catch-and-fulfill.

**Compatibility**

Changing `Promise<void>` to a result union is compile-time breaking but localized to store consumers. Migrate one action family at a time and add tests proving failed commit/fetch/pull/remote-edit operations do not clear input, advance timestamps, close editors, or chain subsequent operations.

### API-02 — No-remote repositories produce an invalid IPC call

**Severity: High**

**Evidence**

- `push`, `pull`, and `fetch` compute `remote ?? get().remotes[0]?.name` and immediately call wrappers requiring `remote: string` (`src/stores/git-store.ts:619-623`, `645-648`, `674-677`; wrapper signatures at `src/lib/tauri-api.ts:750-757`).
- With no remotes, the runtime value is `undefined`. Tauri argument serialization omits or rejects the required Rust `String remote` before command execution (`src-tauri/src/commands/git.rs:80-93`).
- The current `tsconfig.json` enables `strict` but not `noUncheckedIndexedAccess` (`tsconfig.json:1-28`), so `remotes[0]` is treated as present and `yarn tsc --noEmit` does not detect the defect.
- `GitLandingPanel` leaves fetch/pull/push available independent of remote count (`src/components/git/GitLandingPanel.tsx:245-280`).

**Contract change**

- Store action preflight must return `{ ok: false, error: { code: 'noRemote' } }` before invoking IPC when no explicit/default remote exists.
- Represent a default/upstream remote in backend status or add `git_get_sync_target`; do not infer policy from `remotes[0]` ordering.
- Disable or redirect network actions to remote setup when no target exists.
- Enable `noUncheckedIndexedAccess` when feasible; this is defense-in-depth, not the primary runtime guard.

**Compatibility**

This can be additive: preserve command signatures and add frontend preflight first. A later v2 command may accept `remote?: string` and resolve configured upstream/default in Rust, returning typed `noRemote`/`noUpstream` errors.

### ERR-01 — String-only errors lose structured semantics

**Severity: Medium**

**Evidence**

- `DomainError` has variants (`NotFound`, `InvalidInput`, `Conflict`, etc.) but serializes only `Display` text (`crates/rocket-shared/src/error.rs:4-29`, `37-43`).
- Most libgit2 errors are flattened further into `DomainError::Internal(e.to_string())`; examples include repository open (`crates/rocket-git/src/git2_service/helpers.rs:64-67`) and remote operations (`crates/rocket-git/src/git2_service/remote.rs:68-106`, `109-152`).
- Network retry behavior matches `class=Ssh`, `authentication failed`, `Repository not found`, or `Permission denied` substrings (`src/stores/git-store.ts:624-633`, `649-656`, `680-687`).
- Branch merge behavior matches the word `conflict` (`src/components/git/BranchSelector.tsx:82-95`).
- Pull/merge can mutate index/worktree into conflict state and then return `Err(DomainError::Conflict(...))` (`crates/rocket-git/src/git2_service/remote.rs:233-270`; `crates/rocket-git/src/git2_service/branch.rs:235-267`). The error does not encode that mutation occurred.
- `git_is_repo` erases permission, corruption, unsupported format, and not-a-repository into the same `false` (`crates/rocket-git/src/git2_service/repo.rs:10-13`).
- `refreshConflicts` turns every error into an empty conflict list (`src/stores/git-store.ts:178-187`), so “could not inspect conflicts” is represented as “no conflicts.”

**Contract change**

Define a dedicated IPC error DTO, for example:

```ts
interface GitIpcError {
  code:
    | 'notRepository'
    | 'repositoryUnauthorized'
    | 'invalidRepoPath'
    | 'invalidGitPath'
    | 'noRemote'
    | 'authenticationRequired'
    | 'authenticationRejected'
    | 'unknownHostKey'
    | 'hostKeyChanged'
    | 'conflictCreated'
    | 'busy'
    | 'partialMutation'
    | 'internal';
  message: string;
  retryable: boolean;
  mutation: 'none' | 'completed' | 'conflictStateCreated' | 'partial' | 'unknown';
  details?: Record<string, string | number | boolean | string[]>;
}
```

Map `git2::ErrorClass`/`ErrorCode`, I/O kinds, and keyring errors centrally. Keep raw causes in redacted backend logs, not in frontend control flow. Make repository probing `Result<RepoProbe, GitIpcError>` with `RepoProbe = 'repository' | 'notRepository'`.

**Compatibility**

Changing Tauri rejection serialization can break consumers that expect strings. Safest rollout: add v2 commands returning an explicit result envelope, or add a frontend normalizer that accepts both legacy strings and structured objects during migration. Do not ask the frontend to parse prefixes from `DomainError::Display`.

### TYPE-01 — Content nullability is both inaccurate and semantically overloaded

**Severity: Medium**

**Evidence**

- Rust defines `FileDiff.old_content/new_content`, `Branch.upstream`, and `ConflictFile.ancestor` as `Option<String>` (`crates/rocket-git/src/diff.rs:3-10`, `branch.rs:3-10`, `conflict.rs:3-10`). Serde emits present keys with `null` for `None`.
- Frontend models these as optional properties (`oldContent?: string`, `newContent?: string`, `upstream?: string`, `ancestor?: string`) rather than `string | null` (`src/lib/tauri-api.ts:329-355`, `375-380`). Runtime callers already use null-aware fallbacks, masking the type mismatch (`src/components/git/DiffViewForFile.tsx:31-38`).
- Worktree diff uses `fs::read_to_string(...).ok()` (`crates/rocket-git/src/git2_service/status_diff.rs:100-106`), conflating deletion, permission/read error, directory, and non-UTF8/binary content into `None`.
- HEAD/index blob helpers return `None` for missing entries and invalid UTF-8 alike (`crates/rocket-git/src/git2_service/helpers.rs:106-121`).
- Commit-diff content similarly drops non-UTF8 blobs to `None` (`crates/rocket-git/src/git2_service/staging.rs:167-189`).
- `CommitDiffView.fileStatus` infers `added` when `oldContent == null` and `deleted` when `newContent == null` (`src/components/git/CommitDiffView.tsx:25-29`), so binary/undecodable modifications can be misclassified.
- Conflict enumeration converts missing/non-UTF8 sides to empty strings (`crates/rocket-git/src/git2_service/conflict.rs:31-49`); the resolver treats those strings as valid content and can write/stage them.

**Contract change**

1. Immediately correct generated/manual TS nullability to `string | null` for Rust `Option` fields.
2. Replace nullable diff content with an explicit content union, e.g. `{ kind: 'text', text } | { kind: 'binary', size?, oid? } | { kind: 'absent' }`; return read failures as errors, not `absent`.
3. Include explicit change status/old path in `FileDiff`; do not infer added/deleted from content availability.
4. Represent binary conflicts explicitly and disable text resolution unless the backend supports safe binary-side selection.

**Compatibility**

Correcting optional-to-null TypeScript declarations is source-compatible for current nullish-coalescing consumers but may expose compile errors elsewhere. The explicit content union is wire-breaking; add fields (`oldContentState`, `newContentState`, `changeKind`) first, keep legacy nullable content for one version, then remove it.

### TYPE-02 — Numeric and DTO ownership drift lacks a contract check

**Severity: Medium**

**Evidence**

- Rust uses `usize` for ahead/behind, files changed, stash indexes/stats, fetch counters, and log limit (`crates/rocket-git/src/status.rs:43-48`, `stash.rs:6-18`, `remote.rs:13-20`; `src-tauri/src/commands/git.rs:75-77`, `135-147`). Frontend uses unconstrained JavaScript `number` (`src/lib/tauri-api.ts:306-312`, `336-344`, `364-372`, `398-402`). Negative/fractional values fail deserialization generically; values beyond `Number.MAX_SAFE_INTEGER` cannot round-trip precisely.
- IPC structs are hand-copied into TypeScript. There is no compile-time link between Rust and `src/lib/tauri-api.ts`.
- `GitCredentials` is serialized directly in the domain crate (`crates/rocket-git/src/credentials.rs:3-11`) and mirrored as `GitCredentialsPayload` in Tauri (`src-tauri/src/commands/git.rs:191-237`), despite the repository rule that camelCase serde belongs on IPC DTOs. The “stable mirror” has the same shape and conversion burden but no explicit schema version.
- Current camelCase shapes do match: structs use `rename_all = "camelCase"`; `GitStatus`/`LineType` are lowercase; credentials use `type`; conflict resolution uses `resolution` (`crates/rocket-git/src/status.rs:3-49`, `diff.rs:32-55`, `credentials.rs:3-11`, `conflict.rs:12-18`). This is a positive current-state observation, not a drift prevention mechanism.

**Contract change**

- Define IPC-only request/response DTOs at the Tauri boundary and map them to non-serialized domain types.
- Generate TypeScript bindings from Rust when practical, or add serde golden tests checked against TypeScript runtime schemas.
- Validate `limit` and stash indexes with bounded integer newtypes/DTO validation; use strings for counters only if values can realistically exceed JS safe integer range.
- Version persisted keyring payloads independently from transient operation credentials.

**Compatibility**

Moving serde derives off domain types is wire-compatible if DTO field/tag names stay identical. Introduce runtime schemas in validation-only mode first, then enforce. Bounds should be documented and return typed `invalidArgument` errors rather than generic Tauri deserialization failures.

### IPC-04 — Blocking operations have no asynchronous operation contract

**Severity: Medium**

**Evidence**

- Every Git command is synchronous `pub fn`, including clone/push/pull/fetch (`src-tauri/src/commands/git.rs:17-183`).
- SSH discovery, keyring access, and identity config are also synchronous (`src-tauri/src/commands/git.rs:239-371`).
- libgit2 network and filesystem work is invoked inline; no `spawn_blocking`, cancellation token, timeout, or operation ID is present.
- Clone UI can only show an indeterminate spinner (`src/components/git/GitCloneDialog.tsx:141-153`). `FetchResult` reports only final counters and is discarded.

**Contract change**

- Make Tauri handlers async and execute libgit2/keyring/filesystem work on a bounded blocking pool.
- Add operation IDs, cancellation, timeouts, and typed progress events for clone/fetch/push.
- Serialize mutations per repository while allowing independent repositories to proceed concurrently.
- Include final repository mutation state in cancellation/error outcomes.

**Compatibility**

Changing a Tauri handler from sync to async is transparent to existing `invoke` callers if arguments/results remain stable. Progress/cancel commands and events are additive. Define cancellation semantics before exposing a cancel button—network cancellation may leave fetch refs or clone temp data requiring cleanup.

See also `04-orchestration-security.md`, GIT-IPC-01.

### EVT-01 — Events and result values do not support precise frontend synchronization

**Severity: Medium**

**Evidence**

- All Git domain events map to `git-changed` (`src-tauri/src/tauri_event_bus.rs:48-59`).
- `CollectionsSidebar` listens raw and reloads collections for every event, including push and remote metadata changes that do not change worktree files (`src/components/layout/CollectionsSidebar.tsx:289-301`).
- The typed `onGitChanged` wrapper discards payload and is unused (`src/lib/tauri-api.ts:864-865`).
- `set_remote_url`, `delete_branch`, `resolve_conflict`, and `init` publish no app event (`crates/rocket-app/src/git_service.rs:24-26`, `61-63`, `178-180`, `240-242`). Fetch mutates remote-tracking refs but publishes none (`141-143`).
- Pull/merge publish only on `Ok`, even though a conflict error leaves mutated merge/index state. Conversely, the `conflicts` query publishes every time a nonempty list is read (`crates/rocket-app/src/git_service.rs:228-237`).

**Contract change**

Emit typed outcome events with stable repository ID and affected facets, for example `{ operation, outcome, changed: ['worktree', 'index', 'refs', 'remotes'] }`. Use a side-effect-free conflicts query. Route only worktree changes to collection reload. Make the frontend wrapper preserve the discriminated payload and consume it instead of raw `listen`.

**Compatibility**

Add a `git-operation` v2 event while retaining `git-changed` during migration. Existing listeners can continue broad refreshes until moved; then remove the coarse event. Event payloads should not include absolute paths or secret-bearing remote URLs.

### NAME-01 — Naming obscures the actual repository boundary

**Severity: Low**

**Evidence**

- The Git tab operates on `workspace.path`, not a collection path (`src/components/workspace/WorkspaceGitTab.tsx:9-25`).
- Frontend wrappers and most Tauri commands call the value `collectionPath`/`collection_path` (`src/lib/tauri-api.ts:715-814`; `src-tauri/src/commands/git.rs:17-183`).
- Identity wrappers take `collectionPath` but translate it to a wire argument named `path` (`src/lib/tauri-api.ts:816-820`).
- Domain events use a field named `collection` but populate it with the arbitrary repository path (`crates/rocket-app/src/git_service.rs:83-103` and analogous methods; variants at `crates/rocket-shared/src/events.rs:79-90`).

**Contract change**

Use `repositoryId` for authority and `repositoryRoot` only in backend-internal/path DTOs. If the product intentionally treats a workspace root as one repository, name it explicitly. Reserve `collectionId`/`collectionPath` for OpenCollection entities. Event payloads should carry stable IDs, not fields named `collection` containing absolute paths.

**Compatibility**

Argument-key renames are wire-breaking even when values are unchanged. Perform them in v2 commands or accept serde aliases during a bounded migration. TypeScript symbol renames can be staged with deprecated aliases.

## Positive observations

- Registration is complete and matches wrappers; no production command is orphaned.
- Current struct field casing and enum tags align across Rust and TypeScript.
- Credentials are stored at rest in the OS keychain rather than collection/workspace files (`src-tauri/src/commands/git.rs:314-340`).
- Credential values are omitted from tracing fields for network methods (`crates/rocket-git/src/git2_service/repo.rs:21-25`; `remote.rs:68-69`, `109-110`, `155-156`).
- The frontend refreshes status/conflicts/branches after pull whether the operation succeeds or conflicts (`src/stores/git-store.ts:658-663`), which partially compensates for the current error/event contract.
- Local branch switching already has a dirty-tree preflight and safe checkout (`crates/rocket-git/src/git2_service/branch.rs:52-105`), demonstrating a reusable backend pattern.

## Recommended contract migration order

1. **Close path and host-trust boundaries:** backend-resolved repository IDs, validated Git-relative paths, and fail-closed host verification.
2. **Keep secrets backend-side:** credential profiles scoped to validated repository + remote authority; add forget lifecycle.
3. **Introduce structured errors/outcomes:** preserve authentication, no-remote, conflict-created, busy, cancellation, and partial-mutation semantics.
4. **Fix frontend action semantics:** result-returning or rejecting store actions; remove global-error comparisons and false-success UI updates.
5. **Fix content DTOs:** explicit nullability immediately, then text/binary/absent unions and explicit change kinds.
6. **Move blocking work off handlers:** operation IDs, progress, cancellation/timeouts, and per-repository mutation locks.
7. **Version events and naming:** precise facet events with stable repository IDs; retire `collectionPath` and coarse `git-changed`.
8. **Automate drift detection:** IPC-only Rust DTOs plus generated TypeScript or golden/runtime-schema tests.

## Validation performed

- `yarn tsc --noEmit` — passed with no output.
- Static command-registration comparison — all 39 Git-related registrations have frontend wrappers and production consumers.
- No production code was edited.
