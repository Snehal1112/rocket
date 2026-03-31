# Plan 3: Remote CRUD — Tauri Commands, Frontend API & Store

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire 4 remote CRUD Tauri commands, add frontend API exports, and add remote state + actions to `git-store.ts`.

**Architecture:** Tauri commands in `src-tauri` call `GitAppService` methods. Frontend `tauri-api.ts` exports invoke wrappers. `git-store.ts` gets `remotes` state and CRUD actions. Follows the exact same pattern as existing git commands (e.g., `git_branches`, `git_stash_list`).

**Tech Stack:** Rust (Tauri v2), TypeScript, Zustand

**Spec:** `docs/superpowers/specs/2026-03-31-sp-git-polish-design.md` — Phase 1

**Depends on:** Plan 2 (GitAppService wrappers, domain events)

**Important:** The subagent MUST examine the existing Tauri command files to find the exact file where git commands are defined and the exact pattern used (state extraction, error handling, command registration in `lib.rs`). Look at any existing git command like `git_status` or `git_branches` as a reference. Do NOT guess file paths — find them by browsing `src-tauri/src/`.

---

## Chunk 1: Tauri Commands & Frontend Wiring

### Task 1: Add 4 Tauri commands for remote CRUD

**Files:**
- Modify: the file in `src-tauri/src/` where existing git commands like `git_status`, `git_branches` are defined (find it by browsing `src-tauri/src/`)
- Modify: `src-tauri/src/lib.rs` (register new commands in the `.invoke_handler()` call)

- [ ] **Step 1: Find the git commands file**

Browse `src-tauri/src/` to locate the file containing existing git commands. Look for functions like `git_status`, `git_branches`, `git_stash_list`. Note the exact file path.

- [ ] **Step 2: Study the existing command pattern**

Read one existing git command (e.g., `git_branches`) to understand:
- How `GitAppService` is extracted from Tauri state
- How parameters are received
- How errors are mapped to Tauri command errors
- The exact `#[tauri::command]` annotation pattern

- [ ] **Step 3: Add `git_list_remotes` command**

Follow the exact pattern from `git_branches`. The command takes `collection_path: String` and returns `Vec<RemoteInfo>`. Make sure `RemoteInfo` is imported from `rocket_git`.

```rust
#[tauri::command]
pub fn git_list_remotes(
    // ... state extraction matching existing pattern ...
    collection_path: String,
) -> Result<Vec<rocket_git::RemoteInfo>, String> {
    // ... call git_service.list_remotes(&collection_path) matching existing error handling pattern ...
}
```

- [ ] **Step 4: Add `git_add_remote` command**

Takes `collection_path: String`, `name: String`, `url: String`. Returns `Result<(), String>`.

- [ ] **Step 5: Add `git_remove_remote` command**

Takes `collection_path: String`, `name: String`. Returns `Result<(), String>`.

- [ ] **Step 6: Add `git_set_remote_url` command**

Takes `collection_path: String`, `name: String`, `url: String`. Returns `Result<(), String>`.

- [ ] **Step 7: Register all 4 new commands in `src-tauri/src/lib.rs`**

Find the `.invoke_handler(tauri::generate_handler![...])` call. Add the 4 new command functions to the list, adjacent to the existing git commands.

- [ ] **Step 8: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors

- [ ] **Step 9: Commit**

```bash
git add src-tauri/
git commit -m "feat(tauri): add remote CRUD commands (list, add, remove, set_url)"
```

### Task 2: Add frontend API exports in `tauri-api.ts`

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Add `RemoteInfo` interface**

Add in the `// Git types` section, after the `GitCredentials` type:

```typescript
export interface RemoteInfo {
  name: string;
  url: string;
}
```

- [ ] **Step 2: Add 4 API functions**

Add in the `// Git` section, after the existing `gitResolveConflict` export:

```typescript
export const gitListRemotes = (collectionPath: string) =>
  invoke<RemoteInfo[]>("git_list_remotes", { collectionPath });

export const gitAddRemote = (collectionPath: string, name: string, url: string) =>
  invoke<void>("git_add_remote", { collectionPath, name, url });

export const gitRemoveRemote = (collectionPath: string, name: string) =>
  invoke<void>("git_remove_remote", { collectionPath, name });

export const gitSetRemoteUrl = (collectionPath: string, name: string, url: string) =>
  invoke<void>("git_set_remote_url", { collectionPath, name, url });
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(frontend): add remote CRUD API exports to tauri-api.ts"
```

### Task 3: Add remote state and actions to `git-store.ts`

**Files:**
- Modify: `src/stores/git-store.ts`

- [ ] **Step 1: Read `git-store.ts` to understand the existing pattern**

Note how existing state fields are declared in the store interface and how actions like `refreshBranches` work. Follow the same pattern exactly.

- [ ] **Step 2: Add `remotes` to the store state interface**

Add to the state interface (next to `branches`):

```typescript
remotes: RemoteInfo[];
```

Import `RemoteInfo` from `@/lib/tauri-api` if not already imported.

- [ ] **Step 3: Add initial value for `remotes` in the store creator**

Set initial value: `remotes: [],`

Also add `remotes: [],` to the `reset` action's `set({...})` call.

- [ ] **Step 4: Add `refreshRemotes` action**

Follow the same pattern as `refreshBranches`:

```typescript
refreshRemotes: async () => {
  const { collectionPath } = get();
  if (!collectionPath) return;
  try {
    const remotes = await gitListRemotes(collectionPath);
    set({ remotes });
  } catch (e) {
    set({ error: String(e) });
  }
},
```

- [ ] **Step 5: Add `addRemote` action**

```typescript
addRemote: async (name: string, url: string) => {
  const { collectionPath } = get();
  if (!collectionPath) return;
  try {
    await gitAddRemote(collectionPath, name, url);
    await get().refreshRemotes();
  } catch (e) {
    set({ error: String(e) });
  }
},
```

- [ ] **Step 6: Add `removeRemote` action**

```typescript
removeRemote: async (name: string) => {
  const { collectionPath } = get();
  if (!collectionPath) return;
  try {
    await gitRemoveRemote(collectionPath, name);
    await get().refreshRemotes();
  } catch (e) {
    set({ error: String(e) });
  }
},
```

- [ ] **Step 7: Add `setRemoteUrl` action**

```typescript
setRemoteUrl: async (name: string, url: string) => {
  const { collectionPath } = get();
  if (!collectionPath) return;
  try {
    await gitSetRemoteUrl(collectionPath, name, url);
    await get().refreshRemotes();
  } catch (e) {
    set({ error: String(e) });
  }
},
```

- [ ] **Step 8: Add `refreshRemotes` call to `setCollection` action**

Find the `setCollection` action (which initializes the store when a collection is selected). Add `await get().refreshRemotes();` after the existing refresh calls (e.g., after `refreshBranches`).

- [ ] **Step 9: Add imports for the new API functions**

Add to the import from `@/lib/tauri-api`:

```typescript
import { gitListRemotes, gitAddRemote, gitRemoveRemote, gitSetRemoteUrl } from '@/lib/tauri-api';
```

(Or add to the existing import block if there's already an import from `@/lib/tauri-api`.)

- [ ] **Step 10: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 11: Commit**

```bash
git add src/stores/git-store.ts
git commit -m "feat(frontend): add remote CRUD state and actions to git-store"
```
