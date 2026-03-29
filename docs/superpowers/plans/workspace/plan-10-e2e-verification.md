# Plan 10 — E2E verification and cleanup

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run a full end-to-end verification of the workspace feature, fix any integration issues, and clean up any leftover dead code (the old workspace-store stub types etc.).

**Architecture:** Integration check — create a workspace, add a collection to it, switch workspaces, verify the collection list changes, close/delete the workspace, verify tabs close. Also removes the old stub `Workspace` interface that was in `workspace-store.ts` since it's now in `tauri-api.ts`.

**Tech Stack:** Rust, TypeScript, Tauri v2

**Spec:** `docs/superpowers/specs/2026-03-28-workspace-feature-design.md`

**Previous plan:** `plan-09-workspace-switcher-ui.md`

---

### Task 1: Full workspace test suite (Rust)

**Files:**
- No new files — run existing tests

- [ ] **Step 1: Run the full Rust test suite**

```bash
cargo test --workspace
```

Expected: all tests pass with no failures.

- [ ] **Step 2: Fix any test failures**

If tests fail, investigate and fix before proceeding. Common issues:
- New workspace event variants not handled in exhaustive match arms → add wildcard or new arms
- `tempfile` not in dev-dependencies for a crate that uses it → add it

- [ ] **Step 3: Commit any fixes**

```bash
git add -A
git commit -m "fix(workspace): resolve test failures from workspace feature integration" --allow-empty
```

---

### Task 2: Frontend TypeScript check and dead code removal

**Files:**
- Modify: `src/stores/workspace-store.ts` (remove any old stub types if present)
- Check all files for unused imports

- [ ] **Step 1: Run full TypeScript check**

```bash
yarn tsc --noEmit
```

Fix any type errors before continuing.

- [ ] **Step 2: Check for duplicate `Workspace` interface**

The `Workspace` interface now lives in `tauri-api.ts`. Check that `workspace-store.ts` imports it from there and does not re-declare it locally:

```bash
grep -n "interface Workspace" src/stores/workspace-store.ts
```

Expected: no output (no local re-declaration). If found, remove it and import from `@/lib/tauri-api` instead.

- [ ] **Step 3: Check for any leftover hardcoded workspace data**

```bash
grep -rn "Default Workspace\|id: 'default'" src/stores/ src/components/title-bar/
```

Expected: no hardcoded fallback data in the store or switcher (the backend provides this now).

- [ ] **Step 4: Commit cleanup**

```bash
git add -A
git commit -m "chore(workspace): remove dead code and fix TypeScript issues"
```

---

### Task 3: Full E2E manual walkthrough

**Files:** None (manual verification)

- [ ] **Step 1: Run the app**

```bash
yarn tauri dev
```

- [ ] **Step 2: Complete the E2E checklist**

**First launch bootstrap:**
- [ ] App starts without error
- [ ] `workspaces.yml` is created in the app data dir
- [ ] `Default Workspace` folder is created in the app data dir
- [ ] Workspace switcher shows "Default Workspace"

**Create workspace:**
- [ ] Click "New workspace" in the switcher
- [ ] Enter a name (e.g. "Test API")
- [ ] Click Browse, pick a folder on disk
- [ ] Click Create
- [ ] New workspace appears in the dropdown

**Switch workspace:**
- [ ] Click the new workspace in the dropdown
- [ ] Collections sidebar clears (empty workspace)
- [ ] Create a collection inside the new workspace
- [ ] Switch back to Default Workspace
- [ ] Collections from Default Workspace appear
- [ ] Switch back to Test API — collection is still there

**Rename workspace:**
- [ ] Hover "Test API" → click `⋯` → Rename
- [ ] Enter "My Test API" → confirm
- [ ] Dropdown now shows "My Test API"

**Close workspace:**
- [ ] Hover "My Test API" → click `⋯` → Close
- [ ] Confirmation dialog appears
- [ ] Confirm — workspace removed from list, files remain on disk
- [ ] Can re-open by creating a new workspace pointing to the same folder

**Delete workspace:**
- [ ] Create another test workspace
- [ ] Hover it → `⋯` → Delete
- [ ] Destructive confirmation appears
- [ ] Confirm — workspace removed from list AND folder deleted from disk

**Edge cases:**
- [ ] `⋯` → Close is disabled when only 1 workspace remains
- [ ] `⋯` → Delete is disabled for the default workspace
- [ ] Closing/deleting the active workspace auto-switches to the first remaining one

- [ ] **Step 3: Commit final state**

```bash
git add -A
git commit -m "feat(workspace): workspace feature complete — all E2E checks passed"
```
