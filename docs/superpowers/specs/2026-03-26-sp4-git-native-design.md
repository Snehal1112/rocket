# SP4: Git Native Integration — Design Spec

**Date:** 2026-03-26
**Status:** Approved
**Depends on:** SP1 + SP2 (Tauri app with tabs, Monaco, shadcn/ui)

## Goal

Add full Git integration to RocketAPI — status indicators on collection files, commit/push/pull from the app, side-by-side diff viewer, branch management, stash support, and conflict resolution. Each collection directory can independently be a git repo.

## Architecture Decisions

### Git Library
- `git2` crate (libgit2 Rust bindings) — no external git CLI dependency
- Feature: `ssh` for SSH key auth on push/pull
- Each collection directory is independently a git repo (or not — graceful no-op when `.git` missing)

### New Bounded Context
- `rocket-git` crate — depends only on `git2`, `rocket-shared`, `chrono`
- Contains: domain model (status, diff, branch, commit, stash, conflict, credentials) + `GitService` trait + `Git2Service` implementation
- No dependency on other domain crates (collection, environment, etc.)

### UI Approach
- Sidebar toggle: "Collections" vs "Git" tabs
- Inline status badges on collection tree files (M, A, D, ?, C)
- Dedicated source control panel: commit message, staged/unstaged files, stash
- Diff viewer: Monaco side-by-side diff in editor area
- Branch bar: bottom status bar with branch name, ahead/behind, push/pull/fetch buttons
- Conflict resolver: accept ours/theirs/custom with Monaco 3-way view

### Domain Events
- `GitCommit { collection, message, sha }`
- `GitPush { collection, remote }`
- `GitPull { collection, remote }`
- `BranchSwitched { collection, branch }`

### Tauri Commands (~15)
`git_status`, `git_diff`, `git_stage`, `git_unstage`, `git_commit`, `git_push`, `git_pull`, `git_fetch`, `git_log`, `git_branches`, `git_switch_branch`, `git_create_branch`, `git_delete_branch`, `git_stash_save`, `git_stash_pop`, `git_stash_list`, `git_stash_drop`, `git_conflicts`, `git_resolve_conflict`, `git_init`, `git_is_repo`

## Plan Breakdown

| Plan | Scope | Est. |
|---|---|---|
| 1 | Rust `rocket-git` crate — domain model + git2 implementation | 4-5 days |
| 2 | Tauri commands + app service + API bridge | 2-3 days |
| 3 | Frontend — sidebar Git tab + status badges on collection tree | 3-4 days |
| 4 | Frontend — diff viewer with Monaco side-by-side | 2-3 days |
| 5 | Frontend — branch selector + stash + conflict resolver | 3-4 days |

## Frontend Stack
- shadcn/ui (all interactive components)
- Monaco editor diff view (`MonacoDiffEditor` from `@monaco-editor/react`)
- Zustand store for git state
- Tauri IPC via `invoke()`
