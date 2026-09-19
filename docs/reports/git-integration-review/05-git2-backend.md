# `rocket-git` / git2 backend review

**Scope:** `crates/rocket-git/src/service.rs`, every public domain type in `crates/rocket-git/src/*.rs`, every module under `crates/rocket-git/src/git2_service`, embedded crate tests, and the application/event-facing semantics needed to interpret the trait correctly.

**Method:** static review against `CLAUDE.md` and `crates/rocket-git/CLAUDE.md`; end-to-end call tracing through `GitAppService`, Tauri commands, and the Git store; `git2`/libgit2 API verification; execution of the existing crate suite; and a temporary 10-case audit probe against real temporary repositories and local bare remotes. The probe was deleted after execution. No production code was changed.

**Dependency baseline:** `git2 = 0.19.0`, backed by `libgit2 1.8.1` (`crates/rocket-git/Cargo.toml:6-13`; `Cargo.lock:1939-1954`, `2859-2873`).

Related cross-layer findings are covered in [`03-ipc-contracts.md`](03-ipc-contracts.md), [`04-orchestration-security.md`](04-orchestration-security.md), and [`06-testing-observability.md`](06-testing-observability.md). This report concentrates on concrete backend behavior and the smallest safe backend fixes.

## Executive summary

The implementation has good happy-path coverage and several correct choices: status recurses into untracked directories, a staged-and-unstaged file is represented twice, local branch switching uses safe checkout, pull resolves the current branch rather than trusting the first `FETCH_HEAD` entry, non-fast-forward pushes fail, conflicts remain available for resolution, and merge completion creates a two-parent commit.

However, the backend is not safe enough for destructive use. The highest-impact confirmed defects are:

1. **Remote identity verification is disabled** for clone/fetch/pull/push.
2. **Repository-relative path arguments are not validated**; audit probes read and deleted files outside the repository, and conflict resolution writes before libgit2 can reject the path.
3. **Forced checkout can silently overwrite local work.** A reproduced unborn pull replaced a differing untracked file with the remote version.
4. **Discarding an unstaged change on a staged+unstaged file loses both versions.** `checkout_head(force)` reset the worktree and index to `HEAD`, deleting the staged change.
5. **Conflict-side deletion is represented as an empty string.** Accepting a deleted side creates/stages an empty regular file instead of deleting the path.
6. **`abort_merge` is an unconditional hard reset.** Calling it when no merge is active silently destroys uncommitted work.
7. **Pull can merge from a different remote than the caller requested** because configured upstream resolution does not verify the upstream remote name.
8. Rename, binary/non-UTF-8, symlink, detached-HEAD, type-change, stash-stat, worktree, and bare-repository semantics are incomplete or misleading.

The public trait amplifies these issues. Most mutating operations return only `DomainResult<()>`; there is no typed distinction among no mutation, completed mutation, conflict state created, or partial mutation. Paths are arbitrary `&str`, conflict content is text-only, `RepoStatus.branch` cannot represent unborn/detached state, and `Branch.upstream` is only a display string. Those limitations force callers to infer repository state from strings and follow-up polling.

## Classification

- **Confirmed bug:** demonstrated by an audit probe, an existing test, or an unconditional implementation path whose result follows directly from the called API contract.
- **Risk:** a credible failure or data-integrity problem dependent on platform, transport, unusual repository configuration, or an injected later-step failure not reproduced in this review.
- **Feature/API gap:** unsupported behavior or insufficient representation rather than an implementation violating a reliable current contract.

### Finding summary

| ID | Class | Severity | Finding |
|---|---|---:|---|
| G2-01 | Confirmed bug | Critical | Certificate/SSH host identity is always accepted |
| G2-02 | Confirmed bug | Critical | File arguments permit out-of-repository read/write/delete |
| G2-03 | Confirmed bug | High | Force checkout overwrites local work during pull/remote checkout/fast-forward paths |
| G2-04 | Confirmed bug | High | Unstaged diff uses `HEAD`, and discard destroys staged content too |
| G2-05 | Confirmed bug | High | Deleted conflict sides become empty files; non-conflict resolution can write arbitrary content |
| G2-06 | Confirmed bug | High | `abort_merge` hard-resets even when no merge is active |
| G2-07 | Confirmed bug | High | Pull may merge an upstream belonging to a different remote than requested |
| G2-08 | Confirmed bug | Medium | Rename detection and rename identity are absent |
| G2-09 | Confirmed bug | Medium | Binary/non-UTF-8 content is collapsed into absence/empty text |
| G2-10 | Confirmed bug | Medium | Symlink diffs follow the worktree target and misrepresent the Git blob |
| G2-11 | Confirmed bug | Medium | Detached HEAD is reported as branch `HEAD`; branch/ref operations are not guarded |
| G2-12 | Confirmed bug | Medium | Type changes are emitted as `Unchanged` |
| G2-13 | Confirmed bug | Medium | Untracked-only stash metadata reports zero changed files |
| G2-14 | Confirmed bug | Medium | Push target branch is derived from any configured upstream, not the selected remote |
| G2-15 | Risk | High | Merge/pull can mutate worktree/index and then fail on signature/commit/cleanup |
| G2-16 | Risk | Medium | Credential callback rejects legitimate retries and ignores allowed credential types |
| G2-17 | Risk | Medium | Ahead/behind fallback can compare against an arbitrary configured remote |
| G2-18 | Risk | Medium | Bare repositories and linked worktrees are accepted without capability-aware behavior |
| G2-19 | Risk | Medium | Cross-platform/non-UTF-8 paths are lossy or handled inconsistently |
| G2-20 | Feature/API gap | Medium | Trait/result types cannot express repository state, mutation outcomes, bytes, modes, or rename pairs |
| G2-21 | Feature/API gap | Medium | Event-facing semantics omit successful and partial mutations and make a query publish events |
| G2-22 | Feature/API gap | Low | Diff/log/stash behavior lacks production-grade fidelity and policy controls |

## Behavior audit matrix

| Area | Current behavior | Assessment |
|---|---|---|
| Repository open/init/clone | Opens per call; `is_repo` erases all open errors; init is idempotent; clone rejects a nonempty directory | Basic behavior works; no repository capability/type result, atomic clone, cleanup, progress, cancellation, or path authority |
| Status | Reports index and worktree entries separately; recurses untracked directories; computes ahead/behind | Good staged+unstaged shape, but no type-change mapping, rename detection, path bytes, state kind, or reliable multi-remote target |
| Worktree diff | Compares `HEAD` blob to `fs::read_to_string(path.join(file))` | Wrong baseline for unstaged half of staged+unstaged files; path escape, symlink following, binary ambiguity |
| Staged diff | Compares `HEAD` blob to index stage 0 | Correct for ordinary text add/modify/delete; cannot express binary, mode, rename, conflict stages, or read failure |
| Commit diff | Tree-to-tree against first parent | Acceptable first-parent policy, but rename detection is off and content is text-only; merge-parent choice is implicit |
| Stage | Adds existing paths, removes missing paths; supports directory pathspecs | Ordinary add/delete works; paths are unvalidated; symlink/dangling and pathspec semantics need tests |
| Unstage | `reset_default` to `HEAD` | Works after a commit; fails for unborn repositories and has no explicit initial-index behavior |
| Discard | For tracked paths, forced `checkout_head`; otherwise removes joined path recursively | Data-loss and path-traversal defects; batch is partially mutating |
| Commit | Writes index tree and commits to `HEAD`; adds `MERGE_HEAD` as second parent | Initial and merge commits work; empty message/empty commit allowed at trait level; timestamp is generated separately; partial merge-state failures are opaque |
| Log | Revwalk from `HEAD`, sorted by commit time | Works for attached nonempty repos; unborn `HEAD` errors; time sorting is not topological; invalid UTF-8 metadata becomes empty text |
| Branch list | Enumerates local/remote branches and upstream names | Detached/unborn state is squeezed into a string; non-UTF-8 names become empty |
| Local switch | Rejects any tracked dirty state, then safe checkout with best-effort `HEAD` rollback | Safest branch path, though over-conservative and unaware of repository operation state |
| Remote checkout/create | Creates/moves refs then force-checks out | Can destroy work and leave partial refs/upstream/HEAD on failure |
| Merge branch | Up-to-date, fast-forward, or merge commit; conflicts are persisted | Core graph analysis is sound; force checkout, dirty state, signature failure, and outcome representation are unsafe |
| Remote management | CRUD through libgit2; URL change prunes tracking refs | Basic behavior works; URL-change deletion failures are ignored and event semantics are incomplete |
| Fetch | Uses remote configured refspecs; reports update/transfer counters | Good baseline; no prune/tags policy, timeout/cancel, structured auth/trust, or test of result semantics |
| Pull | Fetch then upstream/same-name/remote-HEAD resolution; FF or merge commit | Current-branch handling is improved; selected-remote mismatch and forced checkout remain serious defects |
| Push | Pushes current local branch to upstream branch name or same name | Non-FF rejection works; detached/unborn state and selected-remote/upstream mismatch are mishandled |
| Stash | Includes untracked; index-based list/apply/pop/drop | Mechanics work; metadata omits untracked files, index identity is transient, and staged-state restoration is not configurable |
| Conflicts | Enumerates index stages; supports ours/theirs/custom text; hard-reset abort | Ordinary text modify/modify works; delete/binary/symlink/mode/path validation and merge-state checks are incorrect or absent |

## Confirmed bugs

### G2-01 — Remote server identity verification is disabled

**Severity: Critical**

`build_callbacks` installs a certificate callback that always returns `CertificateOk` (`crates/rocket-git/src/git2_service/helpers.rs:15-20`). The callback is reused by clone (`git2_service/repo.rs:35-42`), push (`git2_service/remote.rs:99-105`), and fetch (`remote.rs:124-146`); pull always fetches first (`remote.rs:155-159`).

The `git2` API contract says the callback is invoked when certificate verification fails and `CertificateOk` explicitly accepts it. `CertificatePassthrough` delegates to libgit2’s built-in verification. See [`RemoteCallbacks::certificate_check`](https://docs.rs/git2/latest/git2/struct.RemoteCallbacks.html) and [`CertificateCheckStatus`](https://docs.rs/git2/latest/git2/enum.CertificateCheckStatus.html).

This defeats SSH host-key verification and can override failed HTTPS certificate validation. Public-key credentials authenticate the client, not the server.

**Smallest safe fix**

1. Remove the callback or return `CertificatePassthrough` so built-in verification fails closed.
2. Add an explicit known-host challenge flow later; do not preserve the current Windows workaround.
3. Keep HTTPS on normal CA verification.

**Regression tests**

- Local SSH server: unknown key, changed key, and known key.
- Local HTTPS server: self-signed certificate rejected.
- Assert clone/fetch/pull/push share the same verifier and never retry with verification disabled.

### G2-02 — File arguments escape the repository

**Severity: Critical**

- Worktree diff joins arbitrary `file` onto caller-provided `path` and reads it (`status_diff.rs:100-113`).
- Discard joins the path and removes a file or recursively removes a directory when the path is not found in `HEAD` (`staging.rs:58-89`).
- Conflict resolution joins and writes before `index.add_path` can reject the path (`conflict.rs:140-154`).
- `GitService` models repository and file authority as unconstrained strings (`service.rs:10-60`).

The audit probe passed `../victim.txt`: `diff_file` returned `secret-outside-repository`, and `discard` deleted the sibling file. Absolute paths have the same problem because `Path::join` replaces the base when the right operand is absolute. A symlink under the repository can create another escape path.

**Smallest safe fix**

- Add one internal validator used before every file operation: reject empty paths, absolute paths, platform prefixes, `..`, NULs, and non-normal components.
- Resolve against `repo.workdir()`, not the raw service argument.
- Before discard/resolve, require membership in the relevant status/conflict set.
- For write/delete, use symlink-aware directory-relative operations; canonicalization alone is insufficient for non-existent targets and TOCTOU races.

Longer term, replace repository path strings crossing IPC with backend-resolved repository IDs, as recommended in `03`/`04`.

**Regression tests**

Table-test diff/stage/unstage/discard/resolve against `../x`, absolute Unix paths, Windows drive and UNC paths, mixed separators, symlinks to outside, and a non-conflict path. Assert both `Err` and no filesystem mutation.

### G2-03 — Forced checkout overwrites local work

**Severity: High**

Force checkout is used after remote branch checkout (`branch.rs:134-150`), branch creation (`branch.rs:169-176`), fast-forward merge (`branch.rs:219-231`), and fast-forward pull (`remote.rs:205-229`). The git2 contract for `CheckoutBuilder::force` is: “take any action necessary to get the working directory to match the target including potentially discarding modified files.” Safe checkout is the default and does not overwrite existing changes.

The audit probe created an unborn local repository with an untracked `clash.txt` whose contents differed from remote `clash.txt`. `pull` returned success and replaced the local content with the remote content.

The existing `pull_into_unborn_repo_with_conflicting_untracked_file` test uses identical local and remote `workspace.yml` content (`git2_service/mod.rs:1252-1331`), so it does not detect this loss. `pull_into_unborn_repo_succeeds_and_clears_behind` checks survival only for an untracked path not present remotely (`mod.rs:1333-1419`).

**Smallest safe fix**

- Use one target-aware preflight plus safe checkout for every checkout-producing operation.
- In unborn pull, reject a target-tree collision unless the existing file is byte-identical and mode-compatible; even identical adoption should be a documented policy.
- Do not move refs/HEAD before checkout feasibility is established. If libgit2 requires sequencing, record and rollback the original state.

**Regression tests**

For create, remote checkout, merge FF, and pull FF: modified tracked file, staged file, colliding untracked file with equal bytes, colliding untracked file with different bytes, directory/file collision, and symlink collision. Assert no ref/index/worktree mutation on rejection.

### G2-04 — Unstaged diff and discard are wrong for staged+unstaged files

**Severity: High**

The status contract intentionally emits two entries for a file with both index and worktree changes (`status_diff.rs:35-77`; crate guidance at `crates/rocket-git/CLAUDE.md:48-51`). But `diff_file` always compares `HEAD` to worktree (`status_diff.rs:100-113`), not index to worktree. Therefore the unstaged row includes already-staged edits.

More seriously, discard uses forced `checkout_head` for any path found in `HEAD` (`staging.rs:62-76`). The audit probe established this sequence:

1. commit `v1`;
2. stage `v2`;
3. leave worktree at `v3`;
4. status correctly returns two entries;
5. unstaged diff reports `v1 → v3`, not `v2 → v3`;
6. discard returns success and leaves both worktree and index at `v1`, silently deleting staged `v2`.

The existing `discard_reverts_changes` test covers only an unstaged-only tracked modification (`git2_service/mod.rs:394-402`). `stage_and_unstage_file` also does not combine staged and unstaged states (`mod.rs:326-337`).

**Smallest safe fix**

- Make worktree diff compare index stage 0 to the worktree.
- Make discard restore the selected path from index to worktree without changing the index.
- Keep a separately named, explicit destructive operation if “reset path to `HEAD` including index” is required.

**Regression tests**

- `status_emits_two_entries_for_staged_and_unstaged_same_path`.
- `unstaged_diff_uses_index_as_old_content`.
- `discard_unstaged_preserves_index_blob_and_staged_status`.
- Repeat for staged add + later edit and staged delete + recreated worktree file.

### G2-05 — Conflict deletion and membership semantics are incorrect

**Severity: High**

`ConflictFile` represents `ours` and `theirs` as mandatory `String` (`conflict.rs:3-10`). Enumeration maps a missing side or invalid UTF-8 to `""` (`git2_service/conflict.rs:31-49`). Resolution repeats that collapse and always writes a regular file (`conflict.rs:70-143`) before adding it to stage 0 (`conflict.rs:145-154`).

The audit probe created a modify/delete conflict. `theirs` was reported as `""`; accepting `Theirs` created and staged an empty file instead of accepting the deletion.

Resolution also does not require a matching conflict. If no entry matches, ours/theirs resolves to an empty string; custom resolution writes arbitrary content. This combines with G2-02 because the write occurs before index rejection.

**Smallest safe fix**

- Represent each conflict side as `Option<ConflictSide>`, where `ConflictSide` includes bytes/text state, OID, and file mode.
- Look up and validate the exact conflict before any mutation.
- For an absent selected side, remove the worktree path and index stage-0 entry; do not create an empty file.
- Preserve symlink/executable mode and reject unsupported binary custom edits.

**Regression tests**

Modify/delete and delete/modify for both Ours/Theirs; add/add; rename/delete; symlink and executable-mode conflicts; binary conflict; nonexistent conflict; invalid path; injected index-write failure with explicit recovery outcome.

### G2-06 — `abort_merge` destroys work outside a merge

**Severity: High**

`abort_merge` never checks `repo.state()`. It resolves `HEAD`, performs `ResetType::Hard`, and then calls `cleanup_state` (`git2_service/conflict.rs:159-179`). A hard reset resets both index and worktree.

The audit probe modified a tracked file in a clean repository, called `abort_merge`, received success, and observed the file restored to committed content. No merge had existed.

Even during a real merge, unrelated pre-merge local changes that libgit2 allowed to remain can be lost. The current tests only call abort after creating a conflict (`git2_service/mod.rs:1421-1449`, `2019-2055`).

**Smallest safe fix**

- Require `RepositoryState::Merge`; otherwise return `InvalidInput` without mutation.
- Implement merge abort using libgit2 state/original-head semantics where available, preserving pre-merge local changes. If exact preservation is not possible, require a clean preflight before merge and document that invariant.
- Do not use a method named `abort_merge` to clean up rebase/revert/cherry-pick state.

**Regression tests**

No merge + dirty index/worktree; active merge + unrelated local change; active merge + untracked files; repeated abort; and other repository states. Assert no unrelated data loss.

### G2-07 — Pull can merge the wrong remote

**Severity: High**

After fetching the caller-selected `remote`, pull resolves the local branch’s configured upstream first (`remote.rs:163-177`). It does not verify that the upstream belongs to the requested remote. If local `main` tracks `origin/main` and the caller requests `upstream`, the method fetches `upstream` and then can merge the existing/fresh `origin/main` reference.

The fallback to same-name selected remote and selected remote `HEAD` is only reached when no configured upstream resolves (`remote.rs:174-183`). The current regression test proves that pull does not use the first `FETCH_HEAD` line (`git2_service/mod.rs:849-979`), but it does not cover a selected remote different from the configured upstream.

**Smallest safe fix**

Centralize sync-target resolution:

- If a remote is explicitly supplied, use an upstream only when its configured remote name equals that remote.
- Otherwise use `refs/remotes/<remote>/<merge-branch>` derived from branch config/refspec, then same-name, then remote `HEAD` only under an explicit documented policy.
- Return the resolved target in a typed result for diagnostics/UI.

**Regression tests**

Two remotes with different commits and branch names; configured upstream on one, explicit pull from the other. Assert only the requested remote’s target is analyzed/merged.

### G2-08 — Renames are not detected or represented

**Severity: Medium**

Status options enable only untracked recursion (`status_diff.rs:22-25`); they do not enable `renames_head_to_index` or `renames_index_to_workdir`. Commit diff likewise passes no rename-detection options (`staging.rs:146-154`). The git2 API exposes both rename settings explicitly.

The audit probe renamed `old.txt` to `new.txt` without changing bytes:

- before staging: `old.txt Deleted` + `new.txt Untracked`;
- after staging: `old.txt Deleted` + `new.txt Added`;
- no `Renamed` entry was emitted.

Even if libgit2 emitted `INDEX_RENAMED`, `FileStatus` contains only one `path` (`status.rs:33-39`), and `FileDiff` has no `old_path`/change kind (`diff.rs:3-10`). A staged rename diff looks like an addition because `diff_staged(new_path)` cannot find `new_path` in `HEAD`.

**Smallest safe fix**

Enable rename detection and evolve result types to include `old_path: Option<String>` plus explicit change kind. Do not attempt to infer rename identity in the frontend from delete/add pairs.

**Regression tests**

Pure rename, rename+modify, directory rename, case-only rename on relevant platforms, staged and unstaged rename, and commit diff rename.

### G2-09 — Binary and invalid-UTF-8 content becomes “absent”

**Severity: Medium**

HEAD/index helpers return `None` for both missing entries and non-UTF-8 blobs (`helpers.rs:106-121`). Worktree reads use `read_to_string(...).ok()`, also collapsing deletion, binary data, permission errors, and other I/O failures (`status_diff.rs:100-106`). Commit diff repeats the UTF-8-only conversion (`staging.rs:167-189`). Conflict sides become empty strings (`conflict.rs:31-49`).

The audit probe modified a non-UTF-8 binary file. Status reported `Modified`, but both diff contents were `None` and hunks were empty, indistinguishable from no textual change. Frontend consumers then infer add/delete from null content; see `03-ipc-contracts.md`, TYPE-01.

**Smallest safe fix**

Return explicit content states: text, binary/bytes metadata, absent, and read error. At minimum, include binary flag, OID/size, and explicit change kind. Never map I/O failure to deletion.

**Regression tests**

Binary add/modify/delete, invalid UTF-8 text, mixed text/binary conflict, unreadable worktree path, and large-file limits.

### G2-10 — Symlink diff follows the target and reports incompatible content

**Severity: Medium**

Git stores a symlink blob as the link target text. `get_head_content` correctly reads that blob (`helpers.rs:106-114`), but `diff_file` uses `fs::read_to_string` on the worktree path (`status_diff.rs:103-106`), which follows the symlink.

On Unix, the audit probe changed symlink `link` from `target-a` to `target-b`. The diff reported old content `"target-a"` and new content `"TARGET-B-CONTENT"`—the contents of the target file, not the new link value `"target-b"`. This is both incorrect diff behavior and a possible symlink-based read escape.

**Smallest safe fix**

Use `symlink_metadata`; when mode is symlink, read the link value with `read_link` without following it. Carry file mode/type in diff/content DTOs. Combine with symlink-aware containment from G2-02.

**Regression tests**

Symlink target change, symlink ↔ regular-file type change, dangling symlink, target outside repository, and Windows symlink behavior where privileges permit.

### G2-11 — Detached HEAD is represented as a branch named `HEAD`

**Severity: Medium**

`branch_name` returns `head.shorthand()` and falls back to `"main"` (`helpers.rs:184-190`). In detached state, the shorthand is `HEAD`. The audit probe observed `RepoStatus.branch == "HEAD"`, `BranchList.current == "HEAD"`, and no local branch with `is_head == true`.

Push then constructs `refs/heads/HEAD` (`remote.rs:75-97`); pull searches a local branch named `HEAD` and remote `HEAD` fallbacks (`remote.rs:160-195`); fast-forward merge constructs `refs/heads/HEAD` (`branch.rs:219-224`). These are not valid detached-HEAD semantics. Unborn state is also silently labeled `main`, regardless of the symbolic unborn branch configuration.

**Smallest safe fix**

Replace branch string semantics with `HeadState`:

```text
Unborn { branch }
Attached { branch, oid }
Detached { oid }
```

Reject branch-dependent push/pull/merge operations in detached state unless the caller supplies an explicit source/target ref.

**Regression tests**

Detached status/list/log/diff; push/pull/merge rejection without mutation; unborn branch named `master` or custom name; detached checkout back to a branch.

### G2-12 — Type changes become `Unchanged`

**Severity: Medium**

`Status::INDEX_TYPECHANGE` and `WT_TYPECHANGE` are absent from status splitting (`status_diff.rs:37-49`) and from `map_git2_status` (`helpers.rs:69-104`). The fallback path therefore emits `GitStatus::Unchanged` for a nonempty type-change status. `is_clean` still becomes false because a file entry exists, yielding the contradictory state “dirty repository containing an unchanged file.”

**Smallest safe fix**

Add a `TypeChanged`/`ModeChanged` domain variant, include both flags in staged/worktree splitting, and preserve old/new modes in diff data.

**Regression tests**

Regular file ↔ symlink, executable-bit change on Unix, and staged+unstaged type changes.

### G2-13 — Stash metadata omits included untracked files

**Severity: Medium**

`stash_save` explicitly uses `INCLUDE_UNTRACKED` (`stash.rs:93-106`). `stash_list`, however, computes stats only by diffing stash commit parent 0 against the stash commit tree (`stash.rs:44-75`). Untracked files are represented separately in Git’s stash structure, so an untracked-only stash is not included in that diff.

The audit probe saved an untracked-only file. `stash_list` returned `files_changed = 0`, zero insertions/deletions, and an empty `changed_files`, contradicting `StashEntry`’s “every file touched” documentation (`crates/rocket-git/src/stash.rs:11-18`). Existing tests verify that the file is captured/restored but do not inspect metadata (`git2_service/mod.rs:478-503`).

**Smallest safe fix**

Include the stash’s untracked parent/tree when aggregating changed paths and stats, deduplicating paths against tracked changes. If exact line stats are not meaningful for binary/untracked data, represent that explicitly.

**Regression tests**

Untracked-only, tracked+untracked, ignored-file policy, binary untracked, and multiple stashes with stable displayed indices.

### G2-14 — Push branch selection can use an upstream from another remote

**Severity: Medium**

`push(path, remote, ...)` opens the selected remote object, but derives its destination branch from the local branch’s configured upstream without checking the upstream remote name (`remote.rs:68-97`). A branch tracking `origin/release` pushed with selected remote `fork` therefore targets `refs/heads/release` on `fork`, not necessarily the same-name local branch or configured push refspec for `fork`.

The implementation also ignores `branch.<name>.pushRemote`, `remote.pushDefault`, and configured push refspecs. Existing tests cover same-name push and non-fast-forward rejection (`git2_service/mod.rs:260-274`, `1525-1617`) but not multiple remotes or differently named upstream branches.

**Smallest safe fix**

Resolve push source/destination as a dedicated policy object. If the caller explicitly selects a remote, use that remote’s configured push refspec/pushRemote policy; do not reuse a destination extracted from a different remote’s upstream. Return the chosen refspec for confirmation/diagnostics.

**Regression tests**

Two remotes, differently named upstream, `pushRemote`, custom push refspec, new branch without upstream, detached HEAD, and non-fast-forward status callback.

## Correctness and operational risks

### G2-15 — Multi-step merge/pull failures leave opaque partial state

**Severity: High risk**

Normal merge mutates index/worktree first (`branch.rs:235-246`; `remote.rs:233-249`), then writes a tree, obtains a signature, commits, and cleans up (`branch.rs:270-298`; `remote.rs:273-303`). If identity is missing or commit/cleanup fails, the operation returns `Internal` after mutation.

`pull_merge_commit_fails_without_identity` proves the error (`git2_service/mod.rs:2125-2223`) but does not assert worktree, index, `MERGE_HEAD`, ahead/behind, or recoverability afterward. Similar partial-state windows exist after branch creation/upstream assignment and after ref movement before checkout.

**Smallest safe fix:** preflight identity and repository state before merge mutation; use typed outcomes carrying post-operation state; add rollback where safe; never label a conflict-created or partial mutation as an ordinary internal failure.

### G2-16 — Credential callback is too rigid

**Severity: Medium risk**

The callback ignores libgit2’s `allowed` credential-type bitmask and rejects every callback after the first (`helpers.rs:12-28`). The git2 callback contract explicitly supplies allowed credential types and validates the returned credential type against them. Legitimate negotiation may call the callback more than once or request username then SSH key. The current one-shot guard can turn valid authentication flows into failure.

Additional limitations:

- `Token` hardcodes username `oauth2` (`helpers.rs:56-58`), which is provider-specific.
- SSH key `~` expansion treats any string starting with `~` as current-home syntax and uses `HOME`, which is not robust on Windows (`helpers.rs:33-41`).
- SSH agent/key credentials default missing usernames to `git`; transport/URL-specific username policy is not modeled.

**Smallest safe fix:** honor `CredentialType`, allow bounded retries per credential kind/username, preserve explicit provider username, use platform home resolution, and return typed authentication challenges/errors. Keep secrets backend-side.

### G2-17 — Ahead/behind fallback is arbitrary in multi-remote repositories

**Severity: Medium risk**

When no configured upstream resolves, `ahead_behind` iterates configured remote names and uses the first matching `<remote>/<branch>` (`helpers.rs:211-223`). Remote order is not a sync policy. Status can therefore compare against a different remote than the one the UI will fetch/pull/push.

The existing test intentionally validates an `origin/main` fallback with only one remote (`git2_service/mod.rs:588-628`).

**Smallest safe fix:** report `None`/unknown without upstream, or expose per-upstream/per-remote divergence. Do not silently pick the first remote.

### G2-18 — Bare repositories and linked worktrees are not capability-aware

**Severity: Medium risk**

`Repository::open` accepts bare repositories, and `is_repo` reports them as repositories (`repo.rs:10-13`). `status`, branches, log, and remote operations may partially work; stage explicitly fails because `workdir()` is `None` (`staging.rs:13-16`); diff/discard use the raw caller path rather than `repo.workdir()` (`status_diff.rs:104-105`; `staging.rs:79-85`). The trait does not expose `is_bare`, workdir, common Git dir, or operation capabilities.

Linked worktrees can be opened by libgit2, but raw-path joins and assumptions about `.git/MERGE_HEAD` in tests do not establish worktree-safe behavior. Merge state belongs in Git metadata, while working files belong under `repo.workdir()`.

**Smallest safe fix:** return repository capabilities/state from open/probe; reject worktree-required operations on bare repos with a typed error; derive all file paths from `repo.workdir()`; add linked-worktree fixtures.

### G2-19 — Cross-platform and non-UTF-8 path handling is lossy

**Severity: Medium risk**

- Status uses `entry.path().unwrap_or("")` (`status_diff.rs:30-33`), so non-UTF-8 paths become empty strings.
- Conflict paths require `String::from_utf8` and otherwise become empty/omitted (`conflict.rs:24-29`; conflict lists in `branch.rs:247-259` and `remote.rs:250-262`).
- Commit/stash paths use `to_string_lossy` (`staging.rs:158-165`; `stash.rs:57-66`), potentially producing names that cannot round-trip into index operations.
- Every path enters as `&str`, excluding native non-UTF-8 Unix paths by design.
- `/` parsing is hardcoded for remote branch names (`branch.rs:112-117`) and upstream names (`remote.rs:89-92`); Git refnames use `/`, but the split also assumes the first component is always the selected remote.
- The `~` handling described in G2-16 is not cross-platform robust.

**Smallest safe fix:** define Git paths as repository-relative byte strings internally, encode them losslessly for IPC (for example UTF-8-or-base64 with display text), and centralize ref parsing through config/ref APIs rather than generic string splitting.

## Trait and domain API limitations

### G2-20 — The trait cannot express important Git states or safe outcomes

**Severity: Medium feature/API gap**

`GitService` is object-safe and straightforward (`service.rs:10-61`; object-safety test at `service.rs:63-68`), but several signatures are too weak for correct behavior:

- Every repository/file argument is an unconstrained string.
- `is_repo -> bool` erases inaccessible, corrupt, unsupported, bare, linked-worktree, and not-a-repository distinctions.
- Mutations mostly return `()`, hiding changed refs/files, conflict creation, no-op, and partial mutation.
- `RepoStatus.branch: String` cannot model unborn/detached HEAD or operation state (`status.rs:41-49`).
- `Branch.upstream: Option<String>` lacks remote name, merge ref, push target, and whether the ref is gone (`branch.rs:3-10`).
- `FileStatus` lacks old path, mode, conflict stage, and stable identity (`status.rs:33-39`). Duplicate staged/unstaged entries are valid but make `path` an insufficient UI key.
- `FileDiff` cannot represent binary data, modes, read errors, or explicit change kind (`diff.rs:3-10`).
- `ConflictFile` cannot distinguish absent side from empty file or preserve bytes/modes (`conflict.rs:3-10`).
- `CommitInfo` omits committer, parents, decorations, signature status, and timezone offset; `commit` uses `Utc::now()` instead of the created commit’s stored time (`staging.rs:112-131`).
- Stash operations use mutable positional indices only (`service.rs:51-55`; `stash.rs:6-18`).
- Remote operations do not expose resolved refspec/sync target or remote progress/cancellation.

**Smallest safe evolution**

Prefer additive v2 DTOs/results before breaking the trait:

- `RepositoryProbe` and `RepositoryState`.
- validated `RepoRelativePath` internally.
- `HeadState` and optional `UpstreamTarget`.
- `OperationOutcome { mutation, state, changed_refs, conflicts }`.
- content/mode/change-kind unions for diff and conflict.
- stash OID/reflog selector in addition to display index.

### G2-21 — Event-facing semantics do not correspond to repository transitions

**Severity: Medium feature/API gap**

At the application layer:

- stage/unstage/discard/commit/push/pull/branch/stash successes publish events (`crates/rocket-app/src/git_service.rs:82-139`, `150-226`);
- init, set URL, delete branch, and resolve conflict mutate state without events (`git_service.rs:24-26`, `61-63`, `178-180`, `240-242`);
- fetch changes remote-tracking refs without an event (`git_service.rs:141-143`);
- pull/merge conflicts mutate index/worktree but publish no event because the backend returns `Err` (`git_service.rs:132-139`, `182-189`);
- `conflicts()`, a query, publishes `GitConflictDetected` every time it returns a nonempty list (`git_service.rs:228-237`).

All Git events collapse to `git-changed`, while branch switch/merge additionally emit `collection-changed` (`src-tauri/src/tauri_event_bus.rs:48-72`). The Git panel skips those branch collection events and relies on inline refresh (`src/components/git/GitPanel.tsx:139-157`).

The backend’s `Result<()>` shape makes it impossible for orchestration to publish accurate partial/conflict outcomes without re-inspecting the repository.

**Smallest safe fix:** have mutating backend methods return typed affected facets and post-state. Publish transition events from the operation that caused them; make queries side-effect-free. Emit conflict-created even when the operation’s user-facing result requires resolution.

### G2-22 — Fidelity and policy gaps

**Severity: Low/Medium feature gaps**

- `build_simple_diff` emits all removals followed by all additions in one hunk (`helpers.rs:124-163`). Comments correctly warn not to use hunks semantically, but `FileDiff::additions/deletions` exposes them as meaningful counts (`diff.rs:12-29`).
- Commit log starts only at `HEAD`, fails on unborn repos, and sorts by timestamp rather than topological/time ordering (`staging.rs:210-237`).
- Commit diffs always use first-parent semantics for merges without exposing that policy (`staging.rs:146-153`).
- No amend, signing, force-with-lease, tag, prune, delete-remote-branch, rebase, or explicit merge strategy policy is exposed.
- Fetch does not expose prune/tag/refspec policy despite relying on configured defaults (`remote.rs:141-152`).
- Stash always includes untracked files, despite the comment claiming this matches Git CLI default (`stash.rs:101-105`); standard `git stash push` excludes untracked files unless requested. There is no include-untracked/include-ignored or reinstate-index option.
- Stash apply/pop use positional indices and default apply options (`stash.rs:110-133`); staged state is not optionally restored.
- Branch deletion exposes libgit2 deletion directly with no “must be merged” policy (`branch.rs:181-190`).
- Init does not select/configure an explicit initial branch (`repo.rs:15-18`); tests manually set `main` because default-branch behavior otherwise depends on environment (`git2_service/mod.rs:192-220`).

These should be addressed only after the data-loss/security fixes. The smallest first step is to document exact supported policy and remove misleading comments/count semantics.

## Existing test assessment

### Strong current coverage

The 72 discovered crate tests (71 run, one ignored) meaningfully cover:

- modified/untracked/deleted status and untracked directory recursion (`git2_service/mod.rs:288-392`);
- ordinary stage/unstage/discard (`mod.rs:326-402`);
- commit/log and identity behavior (`mod.rs:404-430`, `1825-1849`, `2057-2223`);
- branch create/switch/delete and dirty-switch refusal (`mod.rs:432-460`, `1721-1780`);
- fast-forward and diverged merges/pulls (`mod.rs:447-460`, `732-1062`, `1851-1931`);
- current-branch pull rather than first `FETCH_HEAD` entry (`mod.rs:849-979`);
- non-fast-forward push rejection (`mod.rs:1559-1617`);
- conflict persistence, ours/theirs staging, merge commit completion, and abort (`mod.rs:675-730`, `1642-1719`, `1782-1823`, `1933-2055`);
- stash mechanics including untracked capture (`mod.rs:462-516`, `1619-1640`);
- remote CRUD and stale tracking-ref cleanup (`mod.rs:518-628`, `1147-1170`).

The stale-ref test’s comments say the assertion should currently fail (`mod.rs:1147-1169`), but it passed in this review. `git2::Repository::remote_delete` currently removes the planted tracking ref in that fixture. Update the test name/comments so they describe the protected behavior rather than a live bug.

### Important missing or misleading coverage

1. Staged+unstaged diff/discard preservation.
2. Differing untracked collision during force checkout; existing unborn tests cover equal or non-colliding files.
3. Rename detection and old/new identity.
4. Binary, invalid UTF-8, symlink, mode/type-change, and non-UTF-8 path behavior.
5. Delete-side conflicts and custom resolution membership.
6. `abort_merge` outside merge state and preservation of unrelated local changes.
7. Detached HEAD and custom unborn branch names.
8. Multiple remotes with selected-remote/upstream disagreement for pull/push/status.
9. Bare repository and linked-worktree capability behavior.
10. Credential allowed-type negotiation, callback retries, host verification, and token username policy.
11. Untracked stash metadata and staged-index restoration policy.
12. Repository state after signature/commit/checkout/cleanup failure.
13. Fetch result values and configured refspec/prune/tag behavior.
14. Event outcomes for conflict-created and partial mutations.

## Recommended minimal remediation order

1. **Stop remote impersonation:** remove unconditional certificate acceptance.
2. **Close path authority:** central repository/file validation and symlink-safe containment before any I/O.
3. **Eliminate implicit force checkout:** common preflight + safe checkout; protect colliding untracked files.
4. **Fix discard/diff semantics:** index→worktree diff and index-preserving discard.
5. **Fix conflict representation:** optional byte/mode-aware sides, validate membership first, support deletion.
6. **Guard abort and operation state:** require merge state and preserve unrelated changes.
7. **Centralize ref target resolution:** selected remote, configured fetch/push refspecs, upstream, detached/unborn handling.
8. **Add explicit repository/content states:** `HeadState`, operation outcome, rename/mode/binary DTOs.
9. **Harden credentials:** allowed types, bounded retries, provider username, platform path handling, typed trust/auth errors.
10. **Make events transition-based:** publish affected facets and conflict/partial outcomes; queries publish nothing.
11. **Then improve fidelity:** rename detection, stash metadata/options, proper hunks, log ordering, worktree/bare support.

## Suggested focused regression suite

A first backend patch series should add these deterministic tests using temporary repositories/local bare remotes:

```text
rejects_parent_absolute_windows_and_symlink_escape_paths
unstaged_diff_uses_index_and_discard_preserves_staged_blob
pull_rejects_differing_untracked_target_collision_without_mutation
remote_checkout_and_fast_forward_preserve_dirty_work
resolve_modify_delete_theirs_removes_file
resolve_non_conflict_performs_no_write
abort_merge_rejects_non_merge_without_reset
pull_explicit_remote_does_not_use_other_remote_upstream
push_explicit_remote_does_not_reuse_other_remote_destination
status_and_commit_diff_report_rename_pair
binary_diff_reports_binary_modified_not_absent
symlink_diff_compares_link_values_without_following
status_reports_type_change
detached_head_is_explicit_and_branch_dependent_ops_reject
stash_list_includes_untracked_paths
bare_repo_reports_capabilities_and_rejects_worktree_ops
linked_worktree_uses_repository_workdir
credential_callback_honors_allowed_types_and_verified_host
merge_signature_failure_reports_and_preserves_recoverable_state
```

Do not encode unsafe current behavior as the desired assertion. For security/data-loss fixes, tests should assert rejection and zero mutation.

## Validation performed

- `cargo check -p rocket-git` — passed.
- `cargo test -p rocket-git` — passed: **71 passed, 0 failed, 1 ignored**.
- Temporary audit probe: **10 passed**, reproducing traversal read/delete, staged-content loss on discard, wrong unstaged baseline, force-overwrite of a differing untracked file, missing rename detection, binary-content collapse, delete-side conflict corruption, unconditional abort reset, omitted untracked stash stats, detached-HEAD string semantics, and symlink-target following. The probe was removed after execution.
- No production code was edited.
