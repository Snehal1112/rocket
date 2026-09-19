# Git orchestration and security review

**Scope:** `crates/rocket-app/src/git_service.rs`, `src-tauri/src/commands/git.rs`, Git service wiring and command registration in `src-tauri/src/lib.rs`, `rocket-git` operations needed to assess orchestration behavior, domain events, keyring credential commands, SSH-key discovery, identity handling, and the frontend credential/network-operation flow.

**Method:** Static review against the repository architecture rules in `CLAUDE.md`, `crates/rocket-app/CLAUDE.md`, and `crates/rocket-git/CLAUDE.md`. No production code was changed. Findings assume the Tauri renderer is not a security boundary: an XSS, compromised frontend dependency, dev-server compromise, or future secondary webview able to invoke registered commands can call the same IPC surface as normal UI code.

## Executive summary

The Git UI is functionally well connected, but the current IPC/backend boundary gives renderer input substantially more authority than the UI exposes. The two most urgent issues are:

1. Remote certificate validation is unconditionally overridden as successful for every clone/fetch/pull/push callback. This defeats SSH host-key verification and may also override failed HTTPS certificate validation.
2. Repository and file paths are accepted directly from IPC. Some file operations join unvalidated paths and consequently permit reads, writes, and deletes outside the repository, including on an IPC call that ultimately returns an error.

Credentials are protected at rest by the OS keychain, but are retrieved into renderer memory as plaintext, keyed only by caller-provided workspace ID, reused across every repository/host in that workspace, and retained in Zustand/dialog state. A renderer compromise can therefore recover credentials for known workspaces or induce their use against an attacker-controlled remote.

The service wiring otherwise follows the trait-object injection shape at the `rocket-app` boundary, but the concrete libgit2 implementation remains in the domain crate and keyring/filesystem/git-config use cases live directly in Tauri commands. This conflicts with the root architecture rule that domain crates contain abstractions and `rocket-infra` owns I/O.

### Severity summary

| ID | Severity | Finding |
|---|---:|---|
| GIT-SEC-01 | Critical | Remote certificate/host identity is accepted unconditionally |
| GIT-SEC-02 | Critical | Renderer-controlled paths enable repository escape and arbitrary local file access |
| GIT-SEC-03 | High | Keyring scope and plaintext renderer retrieval enable cross-repository/host credential exposure |
| GIT-OPS-01 | High | Forced checkout paths can destroy work and expose partial state through a single IPC operation |
| GIT-SEC-04 | Medium | Secret lifetime is unnecessarily long and secret-bearing URLs can reach events/logging |
| GIT-IPC-01 | Medium | Blocking Git, filesystem, and keyring work runs in synchronous command handlers |
| GIT-EVT-01 | Medium | Events are incomplete and can misrepresent repository transitions |
| GIT-OPS-02 | Medium | Multi-step commands return ordinary errors after partial mutation |
| GIT-ARCH-01 | Medium | Git I/O and credential orchestration cross documented architecture boundaries |
| GIT-ERR-01 | Medium | Raw string errors prevent reliable and safe IPC behavior |
| GIT-SSH-01 | Low | SSH discovery trusts names/paired `.pub` files rather than key validity and permissions |
| GIT-ID-01 | Low | Identity semantics and validation are split between UI and direct Tauri I/O |

## Findings

### GIT-SEC-01 — Remote certificate/host identity is accepted unconditionally

**Severity: Critical**

**Evidence**

- `crates/rocket-git/src/git2_service/helpers.rs:15-20` installs `certificate_check` and always returns `CertificateCheckStatus::CertificateOk`.
- The same callbacks are used by clone (`crates/rocket-git/src/git2_service/repo.rs:35-41`), push (`crates/rocket-git/src/git2_service/remote.rs:99-105`), and fetch (`crates/rocket-git/src/git2_service/remote.rs:124-146`); pull invokes fetch first (`crates/rocket-git/src/git2_service/remote.rs:155-159`).
- The comment limits the rationale to empty SSH `known_hosts` on Windows, but the callback itself does not inspect certificate type, hostname, fingerprint, or prior trust (`crates/rocket-git/src/git2_service/helpers.rs:17-20`).

**Threat/user impact**

A network attacker, malicious proxy, poisoned DNS result, or attacker-controlled endpoint can impersonate a Git server. With SSH, this removes the server-authentication property of host keys. Because the callback approves every certificate object, failed HTTPS certificate verification may also be overridden, depending on the libgit2 transport path. The attacker can receive username/password or token credentials, manipulate fetched source, or accept a push containing private workspace data. Public-key authentication does not authenticate the server and is not a substitute for host-key verification.

**Incremental remediation**

1. Remove the unconditional approval callback immediately; permit libgit2's default verification to fail closed.
2. Add a platform-aware known-hosts verifier for SSH. Compare hostname, key type, and fingerprint against OpenSSH `known_hosts`; reject changed keys.
3. If first-use support is required, return a typed `UnknownHostKey { host, algorithm, fingerprint }` challenge and require an explicit user confirmation before persisting trust. Do not silently trust on first use.
4. Keep HTTPS on native/libgit2 CA verification. Any enterprise/custom-CA support should select an explicit trust store, not a universal callback.
5. Never retry with verification disabled.

**Tests**

- An SSH server with an unknown key yields a typed unknown-host error.
- A known host with a changed key is rejected and no credential callback is allowed to complete authentication.
- A valid known host succeeds.
- HTTPS with an invalid/self-signed certificate fails unless its CA was explicitly configured.
- Clone, fetch, pull, and push all exercise the same verifier.

---

### GIT-SEC-02 — Renderer-controlled paths enable repository escape and arbitrary local file access

**Severity: Critical**

**Evidence**

- Every Git command accepts a renderer-provided repository path without resolving it against managed workspace or external-collection state; examples include `src-tauri/src/commands/git.rs:17-29`, `33-68`, `80-182`, and identity commands at `347-370`.
- Frontend wrappers pass these strings directly to IPC (`src/lib/tauri-api.ts:715-820`).
- `git_diff` joins `path` and `file`, then reads the result and returns its content (`crates/rocket-git/src/git2_service/status_diff.rs:100-113`). An absolute `file` replaces the base path, and `../` components can escape it.
- `git_discard` joins an unvalidated file path and deletes the resulting file or directory when it is not found in `HEAD` (`crates/rocket-git/src/git2_service/staging.rs:58-89`).
- Conflict resolution joins and writes the renderer-provided path before asking libgit2 to add it (`crates/rocket-git/src/git2_service/conflict.rs:62-67`, `140-154`). A traversal path can therefore write outside the repo and then return an error from `add_path`, hiding the completed external mutation behind a failed IPC result.
- Identity commands can open and modify the local config of any repository reachable by the process (`src-tauri/src/commands/git.rs:347-370`).
- `git_init` and `git_clone` can create repositories at arbitrary renderer-selected locations (`src-tauri/src/commands/git.rs:22-29`; `crates/rocket-git/src/git2_service/repo.rs:15-18`, `21-42`).

**Threat/user impact**

A renderer compromise can read arbitrary UTF-8 files through `git_diff`, overwrite files through custom conflict resolution, recursively delete directories through `git_discard`, modify arbitrary local repository config, or initialize/clone into arbitrary writable paths. The attack is not limited to the active workspace. Symlinks inside an otherwise trusted repository create an additional escape route unless resolution is symlink-aware.

**Incremental remediation**

1. Stop accepting repository roots as authority-bearing strings. Resolve an opaque workspace/collection/repository ID in the backend against the workspace registry and explicitly registered external roots.
2. For clone destinations chosen through a native picker, issue a short-lived backend capability/token bound to that exact destination rather than accepting a later arbitrary path string.
3. Introduce one `ValidatedRepoPath`/repository-handle boundary before `GitAppService`. Canonicalize the repository root and verify it is an allowed root. Account for non-existent clone destinations by validating a canonical existing parent.
4. Introduce one Git-relative path validator: reject absolute paths, prefixes, `..`, empty components where inappropriate, and NULs. For destructive operations, require the path to be present in status/index/conflicts rather than merely syntactically valid.
5. Make containment checks symlink-aware. Prefer directory-relative APIs (`cap-std`/`openat`-style on supported platforms) for writes/deletes; a simple string-prefix check is insufficient.
6. In `resolve_conflict`, verify the file is an actual unresolved index conflict before any working-tree write.

**Tests**

- For diff, discard, stage, and conflict resolution, reject `../../outside`, absolute Unix paths, Windows drive/UNC paths, and mixed-separator variants.
- A symlink under the repo pointing outside cannot be read, overwritten, or recursively deleted.
- `resolve_conflict` on a non-conflict path performs no write.
- Commands reject a valid repository outside the active/registered roots.
- Registered external collections remain usable without broadening access to sibling paths.
- Clone capability tokens are one-time, destination-bound, and expire.

---

### GIT-SEC-03 — Keyring scope and plaintext renderer retrieval enable cross-repository/host credential exposure

**Severity: High**

**Evidence**

- Keyring entries are scoped only as `rocket-api` / `git-credentials-{workspace_id}` (`src-tauri/src/commands/git.rs:185-189`).
- `workspace_id` is supplied by the renderer and is neither checked against the registry nor constrained to the active workspace (`src-tauri/src/commands/git.rs:318-340`; frontend calls at `src/lib/tauri-api.ts:833-837`).
- `load_git_credentials` returns the full passphrase/password/token over IPC (`src-tauri/src/commands/git.rs:330-337`).
- The frontend automatically loads the entry when a collection is recognized as a repository (`src/stores/git-store.ts:133-157`) and stores it in long-lived global Zustand state (`src/stores/git-store.ts:46-64`, `115-131`).
- The same workspace credential is used for whichever remote is selected (`src/stores/git-store.ts:611-688`), while remote URLs can be added or changed independently (`src-tauri/src/commands/git.rs:170-182`).
- There is no delete/forget command and no credential cleanup tied to workspace close/delete in command registration (`src-tauri/src/lib.rs:392-397`, `398-407`).

**Threat/user impact**

Any renderer code with IPC access can request credentials for any known workspace ID and receive the secret itself. Workspace IDs are available through normal workspace APIs, so UUID entropy is not an authorization control. One token/password is also reused across all repositories and hosts in a workspace; changing a remote to an attacker-controlled URL can cause the stored credential to be presented to the wrong host. Deleted or closed workspaces leave credentials behind indefinitely.

**Incremental remediation**

1. Do not return reusable secrets to the renderer. Have IPC identify a credential profile, then resolve and consume the secret entirely in Rust for the requested Git operation.
2. Scope stored credentials by a backend-validated stable workspace/repository identity plus normalized remote authority (scheme, host, port, and optionally username). Never scope only by a caller-selected string.
3. Before use, bind the resolved credential profile to the current remote URL and reject authority mismatches. Do not send HTTPS tokens/passwords to arbitrary rewritten remotes.
4. Validate workspace/repository IDs through `WorkspaceService`; require the repository to belong to that workspace or be an explicitly registered external collection.
5. Add `forget_git_credentials` and invoke it intentionally on user request. Define explicit close/delete semantics; deletion should normally remove entries, while close-without-delete may retain them only if documented.
6. Consider separate profiles per remote instead of one implicit workspace credential.

**Tests**

- A command cannot load/use credentials for a non-active or nonexistent workspace without an authorized repository association.
- A credential saved for `github.com` is never sent to `evil.example` after a remote URL change.
- Multiple repositories/remotes in one workspace select the correct profile.
- Forget and workspace-delete remove the exact keyring entries; workspace-close behavior is explicit.
- IPC responses and frontend state snapshots never contain token/password/passphrase values.

---

### GIT-OPS-01 — Forced checkout paths can destroy work and expose partial state through a single IPC operation

**Severity: High**

**Evidence**

- Normal local branch switching performs a dirty-tree preflight and uses safe checkout with best-effort `HEAD` rollback (`crates/rocket-git/src/git2_service/branch.rs:52-105`).
- Remote-branch checkout does not share that guard. It creates a branch, sets upstream, changes `HEAD`, and then force-checks out (`crates/rocket-git/src/git2_service/branch.rs:108-152`).
- Branch creation similarly creates a branch, changes `HEAD`, and force-checks out without a dirty-tree preflight (`crates/rocket-git/src/git2_service/branch.rs:155-178`).
- Fast-forward pull and merge force-check out after moving refs (`crates/rocket-git/src/git2_service/remote.rs:205-230`; `crates/rocket-git/src/git2_service/branch.rs:219-232`).
- If a later step fails, earlier branch/ref/upstream/working-tree mutations are not rolled back. Nevertheless the IPC contract is a plain `Result<(), DomainError>` (`src-tauri/src/commands/git.rs:85-87`, `105-123`).

**Threat/user impact**

Uncommitted tracked work can be overwritten by create/remote-checkout/pull/merge paths that use forced checkout. Untracked path collisions may also be overwritten or cause a late error after branch/ref changes. The renderer receives only failure, not the actual partially changed repository state, so subsequent actions can target an unexpected branch or worktree.

**Incremental remediation**

1. Extract and reuse one checkout preflight for switch, create, remote checkout, pull fast-forward, and merge fast-forward. Include untracked collision detection against the target tree rather than assuming untracked files are safe.
2. Default to safe checkout. Reserve force checkout for an explicit destructive operation with confirmation and a typed impact preview.
3. Precompute/validate the complete target state before creating branches or moving refs.
4. Where libgit2 cannot make the sequence atomic, keep a rollback plan for original `HEAD`, refs, upstream, index, and worktree. Return a typed `PartialMutation` only if rollback fails.
5. Serialize mutating operations per repository to prevent concurrent IPC calls from invalidating preflight assumptions.

**Tests**

- Dirty tracked and colliding untracked files block every checkout-producing operation without mutation.
- Injected failures after branch creation, upstream set, `set_head`, and ref movement restore the original branch/ref/worktree.
- Two concurrent mutating IPC operations on one repository are serialized or one is rejected as busy.
- A destructive force path, if retained, requires an explicit flag/capability and reports affected files.

---

### GIT-SEC-04 — Secret lifetime is unnecessarily long and secret-bearing URLs can reach events/logging

**Severity: Medium**

**Evidence**

- Both credential enums derive `Debug` and `Clone` while containing plaintext secrets (`crates/rocket-git/src/credentials.rs:3-11`; `src-tauri/src/commands/git.rs:194-211`).
- Saving creates a plaintext JSON `String` before sending it to the keyring (`src-tauri/src/commands/git.rs:318-323`), and callback construction clones the entire credential (`crates/rocket-git/src/git2_service/helpers.rs:21-28`).
- The dialog loads secrets into multiple React string states (`src/components/git/GitCredentialsDialog.tsx:27-35`, `54-70`) and does not clear those fields when it closes; Zustand also retains the selected credential until reset (`src/stores/git-store.ts:115-131`, `703-723`).
- Clone traces include the full URL (`crates/rocket-git/src/git2_service/repo.rs:21-22`), and successful clone/add-remote events carry full URLs (`crates/rocket-app/src/git_service.rs:28-33`, `42-48`; event fields at `crates/rocket-shared/src/events.rs:88-90`). URLs containing userinfo/PATs can therefore be copied into logs and emitted to renderer listeners.

**Threat/user impact**

The OS keychain protects credentials at rest, but plaintext copies remain in Rust heap allocations and renderer memory for longer than an operation requires. Debug formatting or future instrumentation can accidentally expose them. Credentials embedded in remote URLs can be sent through tracing and domain events even though separate credential fields are skipped by instrumentation.

**Incremental remediation**

1. Implement GIT-SEC-03 so secrets remain backend-side and operation-scoped.
2. Remove `Debug` for secret-bearing payloads or implement redacted `Debug`; avoid unnecessary `Clone` and use secrecy/zeroizing wrappers where practical.
3. Zeroize transient serialized buffers and secret payloads after keyring/credential callback use, recognizing that JavaScript strings cannot be reliably zeroized.
4. Clear dialog secret fields on close, credential change, workspace switch, and operation completion. Do not keep decrypted secrets in global frontend state.
5. Reject remote URLs containing passwords/tokens in userinfo, or strip credentials before persistence. Centralize URL redaction for traces, errors, events, and UI.
6. Events should carry a normalized/redacted remote identifier, not the complete URL unless a consumer demonstrably requires it.

**Tests**

- `Debug` output and tracing capture never contain seeded secret values.
- Credentialed URLs are rejected or redacted in clone/add-remote events and logs.
- Closing the dialog and switching workspaces clears frontend secret state.
- Backend operation tests show credential buffers are not retained in managed service state.

---

### GIT-IPC-01 — Blocking Git, filesystem, and keyring work runs in synchronous command handlers

**Severity: Medium**

**Evidence**

- All Git commands are synchronous `pub fn` Tauri handlers, including clone/push/pull/fetch (`src-tauri/src/commands/git.rs:17-183`).
- SSH discovery performs synchronous home-directory traversal (`src-tauri/src/commands/git.rs:239-312`).
- Keyring get/set are synchronous and may block on desktop secret-service interaction (`src-tauri/src/commands/git.rs:314-341`).
- Identity commands synchronously open repositories and config (`src-tauri/src/commands/git.rs:344-371`).
- There is no `spawn_blocking` use in `src-tauri/src`, and all handlers are globally registered (`src-tauri/src/lib.rs:359-397`).

**Threat/user impact**

Slow DNS/network/server responses, large repositories/diffs/status scans, a locked keychain, or a slow filesystem occupy the command execution thread for the operation duration. This can delay unrelated commands and event processing, make the UI appear hung, and provides no cancellation or bounded timeout. Concurrent commands can also mutate the same repository without coordination.

**Incremental remediation**

1. Convert potentially blocking handlers to `async` and move libgit2/keyring/filesystem work to `tauri::async_runtime::spawn_blocking` (or an equivalent bounded worker pool).
2. Manage `GitAppService` through `Arc` if needed to give blocking tasks owned state safely.
3. Add per-repository operation locks and explicit operation IDs. Do not hold a global lock across network I/O.
4. Add timeouts/cancellation for remote operations and emit progress for clone/fetch/push.
5. Keep tiny validation/state-resolution steps on the command path; move only blocking work to workers.

**Tests**

- A deliberately slow remote operation does not prevent a lightweight unrelated command from completing.
- Cancellation/timeout ends an operation with a typed result and a known repository state.
- Two commands for separate repositories can progress concurrently; mutations for the same repository are serialized.
- A blocked/locked keyring does not freeze other IPC handling.

---

### GIT-EVT-01 — Events are incomplete and can misrepresent repository transitions

**Severity: Medium**

**Evidence**

- `rocket-app` documentation says every mutating Git operation publishes a domain event (`crates/rocket-app/CLAUDE.md:49-55`), but `set_remote_url`, `delete_branch`, and `resolve_conflict` delegate without publishing (`crates/rocket-app/src/git_service.rs:61-63`, `178-180`, `240-242`). `init` also has no event (`crates/rocket-app/src/git_service.rs:24-26`).
- Fetch mutates remote-tracking refs but publishes no event (`crates/rocket-app/src/git_service.rs:141-143`), despite being described as a read-only remote operation in `crates/rocket-app/CLAUDE.md:53`.
- Pull and merge publish only after `Ok` (`crates/rocket-app/src/git_service.rs:132-139`, `182-189`), while their implementations deliberately leave merge/index state on conflict before returning `Err` (`crates/rocket-git/src/git2_service/remote.rs:233-270`; `crates/rocket-git/src/git2_service/branch.rs:235-267`). No `git-changed` or conflict event is emitted for that mutation.
- `conflicts`, a query, publishes `GitConflictDetected` whenever called and the list is nonempty (`crates/rocket-app/src/git_service.rs:228-237`), producing duplicate notifications and no corresponding clear transition.
- All Git variants collapse to `git-changed` (`src-tauri/src/tauri_event_bus.rs:48-59`). The sidebar ignores payload and refreshes collections after every Git event, including push and remote metadata changes that do not alter collection files (`src/components/layout/CollectionsSidebar.tsx:289-301`).
- Event `collection` values are actually arbitrary repository paths (`crates/rocket-app/src/git_service.rs:85-87` and analogous sites), which is both semantically ambiguous and an unnecessary path disclosure.

**Threat/user impact**

Other views can remain stale after successful mutations or conflicted operations. An IPC error may coexist with a changed worktree/index and no event. Querying conflicts repeatedly can trigger repeated global refreshes. Conversely, unrelated metadata/network events cause expensive collection reloads. Consumers cannot reliably derive operation outcome from event type.

**Incremental remediation**

1. Define events around state transitions/outcomes rather than method success alone: e.g. `GitOperationCompleted`, `GitOperationConflicted`, and `GitOperationPartiallyFailed`, with a backend repository ID and affected facets (`worktree`, `index`, `refs`, `remotes`).
2. Emit a conflict transition directly from pull/merge when conflict state is created, even though the command returns a conflict result.
3. Publish events for init, fetch/ref updates, remote URL changes, branch deletion, conflict resolution, and conflict clearing as appropriate.
4. Make `conflicts()` a side-effect-free query; emit detection/clear events only when operations create or remove conflict state.
5. Route only worktree-changing events to `collection-changed`; let status/ref/remotes consumers subscribe to narrower channels.
6. Replace raw path fields with stable repository/collection IDs and explicitly named optional display paths only where needed.

**Tests**

- App-service mock publisher tests cover every mutation and assert no event for pure queries.
- Conflicted pull/merge emits exactly one conflicted outcome with affected files despite returning a typed conflict.
- Resolving the final conflict emits conflict-cleared/status-changed.
- Fetch/ref and remote metadata events do not cause collection-tree refresh.
- Event payload serialization uses stable IDs and contains no secret-bearing URLs.

---

### GIT-OPS-02 — Multi-step commands return ordinary errors after partial mutation

**Severity: Medium**

**Evidence**

- `discard` mutates files one at a time; an error on a later item leaves earlier files discarded, while `GitAppService` emits no event because the overall call failed (`crates/rocket-git/src/git2_service/staging.rs:58-89`; `crates/rocket-app/src/git_service.rs:99-105`).
- Conflict resolution writes working-tree content before staging/index persistence (`crates/rocket-git/src/git2_service/conflict.rs:140-154`).
- Identity update writes `user.name` and then `user.email`; a second-write failure leaves a partial identity (`src-tauri/src/commands/git.rs:357-371`).
- Clone can leave a partially populated destination on failure, and the next retry is rejected when that directory is nonempty (`crates/rocket-git/src/git2_service/repo.rs:27-42`).
- Pull always fetches first, so even an eventual merge error changes remote refs (`crates/rocket-git/src/git2_service/remote.rs:155-159`). This may be acceptable Git behavior, but a plain `Result<()>` does not communicate it.

**Threat/user impact**

From IPC, `Err` looks like “nothing completed,” but files, refs, config, or destination contents may already have changed. Automatic retry can then fail differently or compound the mutation. The lack of an event on most failed commands increases stale-state risk.

**Incremental remediation**

1. Prevalidate the full file set before destructive multi-file operations, then execute only after all paths and permissions pass.
2. For batch discard, either offer per-file outcomes or create a recoverable snapshot/stash before mutation; do not imply transactionality with `Result<()>`.
3. Stage conflict content in a temp file and validate index conflict membership first; use atomic rename where applicable, then update index with rollback/recovery reporting.
4. Update repo-local identity through a config transaction/lockfile if supported; otherwise restore the prior name if email update fails.
5. Clone into a sibling temporary directory and atomically rename into the destination on success. Clean up temporary state on failure and report cleanup failure explicitly.
6. Return structured outcomes that distinguish `NoMutation`, `Completed`, `ConflictStateCreated`, and `PartialMutation`.

**Tests**

- Inject failure on the second discard path and assert either no files changed or exact per-file/rollback outcome.
- Inject index-write failure after conflict content preparation and verify no unreported external mutation.
- Inject failure on the second identity write and verify the previous identity is restored.
- Failed clone leaves the requested destination absent/empty and is immediately retryable.
- Pull merge failure reports that fetch/ref updates completed.

---

### GIT-ARCH-01 — Git I/O and credential orchestration cross documented architecture boundaries

**Severity: Medium**

**Evidence**

- Root architecture says domain crates contain pure domain logic/traits, `rocket-infra` owns I/O implementations, and `rocket-app` orchestrates through traits (`CLAUDE.md:26-55`; especially `38-40`, `52-54`).
- `rocket-app` itself correctly holds `Box<dyn GitService>` and `Box<dyn EventPublisher>` (`crates/rocket-app/src/git_service.rs:9-16`).
- Startup correctly acts as composition root, but injects concrete `rocket_git::Git2Service` directly (`src-tauri/src/lib.rs:243-246`) because the libgit2 filesystem/network implementation lives in `rocket-git`, not `rocket-infra`.
- `crates/rocket-git/CLAUDE.md:21-29` describes `Git2Service` as the implementation inside a “pure domain crate,” which conflicts with the root rule and the implementation's network/filesystem behavior.
- Keyring persistence, SSH filesystem discovery, and repository config I/O are implemented directly in the IPC module (`src-tauri/src/commands/git.rs:185-371`), bypassing `GitAppService` and any backend policy boundary.
- Domain credentials derive their IPC serialization directly (`crates/rocket-git/src/credentials.rs:3-11`) even though the root rule reserves camel-case serde renaming for IPC DTOs (`CLAUDE.md:85-93`). A second mirror DTO in Tauri (`src-tauri/src/commands/git.rs:191-237`) demonstrates the boundary ambiguity.

**Threat/user impact**

Security policy is fragmented: path authorization, credential scope, redaction, and event semantics cannot be enforced once in an application use case. Direct command implementations are harder to mock and audit, and future call sites can bypass whichever checks are added elsewhere. The contradictory crate guidance also encourages further drift.

**Incremental remediation**

1. Keep `GitService` and domain types in `rocket-git`; move `Git2Service` and all libgit2/fs/network code to `rocket-infra` (or explicitly revise the root architecture if Git is intentionally an exception).
2. Add credential-store, host-trust, repository-locator, and identity-config traits at the domain/application boundary; implement them in `rocket-infra`.
3. Move keyring/identity/SSH-discovery use cases behind `GitAppService` (or focused app services). Tauri commands should deserialize IPC DTOs, call one use case, and map its result.
4. Keep IPC DTO serde shapes in `src-tauri`; map to non-serialized domain values. Avoid duplicate secret-bearing enums by making the IPC type the only deserializable secret payload.
5. Update both CLAUDE files together so implementation placement has one authoritative rule.

**Tests**

- App-layer tests inject fake Git, credential-store, trust-store, and repository-locator traits and verify policy before I/O.
- A dependency-boundary check prevents `rocket-git` from depending on filesystem/network implementation crates if the root architecture remains authoritative.
- Tauri command tests verify DTO mapping only; policy tests live below IPC.

---

### GIT-ERR-01 — Raw string errors prevent reliable and safe IPC behavior

**Severity: Medium**

**Evidence**

- Most libgit2 and keyring failures are mapped to `DomainError::Internal(e.to_string())`; examples include repository open (`crates/rocket-git/src/git2_service/helpers.rs:64-67`), remote operations (`crates/rocket-git/src/git2_service/remote.rs:68-106`, `109-152`), keyring (`src-tauri/src/commands/git.rs:318-340`), and identity (`src-tauri/src/commands/git.rs:347-370`).
- `DomainError` serializes as one display string, losing variant/code and structured context (`crates/rocket-shared/src/error.rs:4-29`, `37-43`).
- The frontend classifies authentication failures by substring matching libgit2 text (`src/stores/git-store.ts:624-633`, `649-656`, `680-687`).
- `git_is_repo` reduces all open failures, including permission/corruption, to `false` (`src-tauri/src/commands/git.rs:17-20`; `crates/rocket-git/src/git2_service/repo.rs:10-13`).
- SSH discovery silently maps home/read-directory failures to no keys (`src-tauri/src/commands/git.rs:242-252`, `260-268`).

**Threat/user impact**

Behavior depends on platform/version-specific English text. Authentication prompts may not appear, genuine repository corruption may be shown as “not a repo,” and paths/remote details from low-level errors are exposed directly to the renderer. Operations that partially mutated state are indistinguishable from preflight failures.

**Incremental remediation**

1. Define a Git application error taxonomy with stable codes and safe fields: `NotRepository`, `RepositoryUnauthorized`, `InvalidRepoPath`, `InvalidGitPath`, `AuthenticationRequired`, `AuthenticationRejected`, `HostKeyUnknown`, `HostKeyChanged`, `RemoteNotFound`, `ConflictCreated`, `Busy`, and `PartialMutation`.
2. Map `git2::ErrorClass`/`ErrorCode`, keyring variants, and I/O kinds centrally; retain raw causes only in redacted backend logs.
3. Serialize a dedicated IPC error DTO (`code`, safe `message`, optional structured details), not `DomainError`'s display string.
4. Make repo detection fallible (`Result<RepoProbe, GitError>`) so “not a repo” differs from inaccessible/corrupt.
5. Return discovery warnings/errors where they affect user action rather than silently presenting an empty list.

**Tests**

- Error mapping table tests cover auth, certificate, not-found, conflict, permission, locked keyring, and corrupt repository cases.
- Frontend behavior branches on codes only; tests do not contain libgit2 message fragments.
- Seeded sensitive paths/URLs are redacted from IPC errors while retained safely in controlled diagnostic logs.

---

### GIT-SSH-01 — SSH discovery trusts names/paired `.pub` files rather than key validity and permissions

**Severity: Low**

**Evidence**

- Default discovery returns the first existing standard path, not necessarily a regular readable private key (`src-tauri/src/commands/git.rs:239-252`).
- Full discovery treats any non-skipped regular file with a sibling `.pub` as a private key (`src-tauri/src/commands/git.rs:255-301`). It does not parse the private key, inspect ownership/permissions, or reject symlinks.
- Legacy `id_dsa` is included in preferred standard names (`src-tauri/src/commands/git.rs:246`, `270`).
- Discovery ignores `~/.ssh/config`, host-specific `IdentityFile`, and keys available only through an agent.

**Threat/user impact**

The UI can offer invalid, overly permissive, symlinked, or obsolete key material and can select a wrong key for a host. This is primarily reliability/security-hardening rather than direct disclosure because actual key content is not returned by discovery.

**Incremental remediation**

1. Use metadata that does not follow symlinks and require a regular file; warn/reject insecure permissions on Unix.
2. Parse key headers with a maintained SSH-key library before listing a file. Treat a sibling `.pub` as a hint, not proof.
3. Remove DSA from defaults unless a documented legacy mode explicitly enables it.
4. Prefer SSH agent and host-specific `IdentityFile` resolution where available; show the source of each candidate.
5. Return structured candidates (display name/fingerprint/path/source), not bare absolute paths where the frontend does not need the full path.

**Tests**

- Directories, FIFOs, symlinks, invalid files with `.pub` siblings, and insecure-permission keys are excluded or warned.
- Valid custom keys without a `.pub` sibling are handled according to the documented policy.
- DSA is absent by default.
- SSH config host aliases select the expected identity without exposing key content.

---

### GIT-ID-01 — Identity semantics and validation are split between UI and direct Tauri I/O

**Severity: Low**

**Evidence**

- Identity is read through the merged repository config chain (local → global → system) but written only to local config (`src-tauri/src/commands/git.rs:344-370`). The UI can therefore display inherited values and silently materialize them as local overrides.
- Validation exists only in React and merely requires nonempty trimmed name plus an `@` in email (`src/components/git/GitIdentityDialog.tsx:36-41`). Direct IPC has no equivalent validation.
- Commit flow treats read failure as missing identity and blocks until save succeeds (`src/components/git/GitCommitForm.tsx:27-57`), while credential-triggered identity setup ignores save failure and activates credentials anyway (`src/components/git/GitPanel.tsx:107-117`).
- Identity updates bypass `GitAppService`, so there is no event or shared policy (`src-tauri/src/commands/git.rs:344-371`; registration at `src-tauri/src/lib.rs:396-397`).

**Threat/user impact**

Users can unintentionally create local overrides, direct callers can store malformed values, and two UI paths report the same write failure inconsistently. The split is unlikely to create a standalone security compromise, but it weakens predictability and auditability.

**Incremental remediation**

1. Return identity values with source/scope (`local`, `global`, `system`, unset), and let the user explicitly choose local override behavior.
2. Validate name/email in the backend use case; keep UI validation only for immediate feedback.
3. Move read/write behind the app service and emit an identity-changed/status event if consumers need it.
4. Make commit and credential flows handle save failure consistently; credential activation should not imply that identity was saved.
5. Apply the rollback/transaction guidance from GIT-OPS-02.

**Tests**

- Inherited identity reports its source and is not written locally without explicit confirmation.
- Direct IPC rejects empty/malformed/control-character values.
- Commit and credential setup present consistent save-failure behavior.
- A failed two-field update leaves the prior identity intact.

## Positive observations

- `GitAppService` depends on `Box<dyn GitService>` and `Box<dyn EventPublisher>`, preserving testability at its immediate boundary (`crates/rocket-app/src/git_service.rs:9-16`).
- The composition root injects the Git implementation and event bus rather than constructing them inside the app service (`src-tauri/src/lib.rs:243-246`).
- Credential fields are skipped in Git network tracing spans (`crates/rocket-git/src/git2_service/repo.rs:21`; `crates/rocket-git/src/git2_service/remote.rs:68`, `109`, `155`).
- Credentials are stored in an OS keychain rather than a workspace file (`src-tauri/src/commands/git.rs:314-340`).
- Local branch switching already demonstrates useful preflight, safe-checkout, and best-effort rollback patterns that can be generalized (`crates/rocket-git/src/git2_service/branch.rs:52-105`).
- Pull conflict handling intentionally leaves merge state available for resolution and the frontend refreshes status/conflicts/branches after either success or failure (`crates/rocket-git/src/git2_service/remote.rs:240-270`; `src/stores/git-store.ts:637-663`).
- Identity writes deliberately target repository-local config rather than changing the user's global Git identity (`src-tauri/src/commands/git.rs:357-370`).

## Recommended remediation order

1. **Fail closed on remote identity:** remove unconditional certificate acceptance and add verified host trust (GIT-SEC-01).
2. **Close filesystem authority gaps:** backend-resolve repository IDs and validate every Git-relative path before any I/O (GIT-SEC-02).
3. **Keep secrets out of the renderer:** backend credential profiles scoped to validated repository + remote authority, plus forget/delete lifecycle (GIT-SEC-03 and GIT-SEC-04).
4. **Prevent destructive/partial checkout:** common safe preflight, per-repository mutation lock, rollback, and structured outcomes (GIT-OPS-01 and GIT-OPS-02).
5. **Move blocking work off command handling and add cancellation/progress** (GIT-IPC-01).
6. **Correct event and error contracts:** state-transition events and typed IPC errors (GIT-EVT-01 and GIT-ERR-01).
7. **Consolidate boundaries:** move concrete I/O to infra and route credential/identity/discovery policies through application services (GIT-ARCH-01).
8. **Harden discovery and identity UX** (GIT-SSH-01 and GIT-ID-01).

## Suggested first test slice

A small, high-value first change set can be guarded by the following focused tests before broader refactoring:

1. `rocket-git`: reject invalid certificates/host keys; no permissive fallback.
2. `rocket-git`: reject absolute, parent-traversal, Windows-prefix, and symlink-escape file paths for diff/discard/conflict resolution.
3. `rocket-app`: fake repository locator proves an arbitrary IPC path cannot select a repository; fake event publisher covers conflict and omitted mutations.
4. `src-tauri`: keyring account resolution uses a validated workspace/repository/remote profile and no command returns plaintext credentials.
5. Frontend: credentials are represented by profile IDs, cleared on workspace changes, and auth behavior uses typed error codes.
6. End-to-end: a slow remote operation leaves unrelated IPC responsive, dirty work survives every branch/pull/merge path, and failed clone is cleanly retryable.
