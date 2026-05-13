# Per-Workspace SSH Key Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scope git credentials (SSH key, passphrase, token, etc.) per workspace so that each workspace authenticates with the right identity, without ever writing sensitive data to any committed file.

**Architecture:** The OS keychain entry for git credentials is currently keyed by a global constant (`"git-credentials"`). This plan changes that key to `"git-credentials-{workspace_id}"`. The `save_git_credentials` and `load_git_credentials` Tauri commands gain a `workspace_id` parameter. The frontend reads the active workspace ID from `useWorkspaceStore` and passes it at every call site. No changes to `WorkspaceConfig` or any `.yml` file — credentials stay 100% machine-local.

**Tech Stack:** Rust (`keyring` crate, Tauri v2), TypeScript (Zustand, `@tauri-apps/api/core` invoke)

---

## File Map

| File | Change |
|---|---|
| `src-tauri/src/commands/git.rs` | `KEYRING_ACCOUNT` constant → `keyring_account(id)` helper; add `workspace_id` param to `save_git_credentials` + `load_git_credentials` |
| `src/lib/tauri-api.ts` | Update `saveGitCredentials` + `loadGitCredentials` signatures to accept `workspaceId: string` |
| `src/stores/git-store.ts` | Pass `activeWorkspaceId` from `useWorkspaceStore` to `loadGitCredentials` in `setCollection`; always reload (not only when null) |
| `src/components/git/GitCredentialsDialog.tsx` | Read `activeWorkspaceId` from `useWorkspaceStore`; pass to `loadGitCredentials` + `saveGitCredentials` |
| `src/stores/__tests__/git-store.test.ts` | Update mock + tests for workspace-scoped credential loading |

---

## Task 1: Scope keychain key per workspace (Rust)

**Files:**
- Modify: `src-tauri/src/commands/git.rs`

### Background

Today `KEYRING_ACCOUNT` is a global constant:
```rust
const KEYRING_ACCOUNT: &str = "git-credentials";
```
Both `save_git_credentials` and `load_git_credentials` use it. Every workspace writes and reads the same keychain slot.

The fix: replace the constant with a small helper function and add `workspace_id: String` to both commands.

- [ ] **Step 1: Write the failing test**

Add inside the existing `#[cfg(test)]` block at the bottom of `src-tauri/src/commands/git.rs`:

```rust
#[test]
fn keyring_account_includes_workspace_id() {
    assert_eq!(keyring_account("ws-abc"), "git-credentials-ws-abc");
    assert_eq!(keyring_account("default"), "git-credentials-default");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p src-tauri keyring_account_includes_workspace_id 2>&1 | tail -10
```

Expected: compile error — `keyring_account` not defined yet.

- [ ] **Step 3: Replace the constant with a helper and update both commands**

In `src-tauri/src/commands/git.rs`, make these changes:

**Remove the `KEYRING_ACCOUNT` constant and add the helper** (keep `KEYRING_SERVICE`):
```rust
const KEYRING_SERVICE: &str = "rocket-api";

fn keyring_account(workspace_id: &str) -> String {
    format!("git-credentials-{}", workspace_id)
}
```

**Update `save_git_credentials`:**
```rust
#[tauri::command]
pub fn save_git_credentials(workspace_id: String, creds: GitCredentialsPayload) -> Result<(), DomainError> {
    let json = serde_json::to_string(&creds)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(&workspace_id))
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    entry.set_password(&json).map_err(|e| DomainError::Internal(e.to_string()))
}
```

**Update `load_git_credentials`:**
```rust
#[tauri::command]
pub fn load_git_credentials(workspace_id: String) -> Result<Option<GitCredentialsPayload>, DomainError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(&workspace_id))
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    match entry.get_password() {
        Ok(json) => {
            let creds: GitCredentialsPayload =
                serde_json::from_str(&json).map_err(|e| DomainError::Internal(e.to_string()))?;
            Ok(Some(creds))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(DomainError::Internal(e.to_string())),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p src-tauri keyring_account_includes_workspace_id 2>&1 | tail -10
```

Expected: `test keyring_account_includes_workspace_id ... ok`

- [ ] **Step 5: Verify compilation**

```bash
cargo check -p src-tauri 2>&1 | tail -20
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/git.rs
git commit -m "feat: scope git keychain entry per workspace id"
```

---

## Task 2: Update tauri-api.ts

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Update the two credential functions**

Find the current definitions (around line 747–751) and replace:

```typescript
export const saveGitCredentials = (workspaceId: string, creds: GitCredentials): Promise<void> =>
  invoke<void>('save_git_credentials', { workspaceId, creds });

export const loadGitCredentials = (workspaceId: string): Promise<GitCredentials | null> =>
  invoke<GitCredentials | null>('load_git_credentials', { workspaceId });
```

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: TypeScript errors at the existing call sites (in `git-store.ts` and `GitCredentialsDialog.tsx`) — that means the types are enforcing the change. Fix those in Tasks 3 and 4.

- [ ] **Step 3: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat: add workspaceId param to saveGitCredentials and loadGitCredentials"
```

---

## Task 3: Update git-store.ts

**Files:**
- Modify: `src/stores/git-store.ts`

### Background

`setCollection` currently auto-loads credentials only when `credentials` is null, using the global keychain. Two problems:
1. It doesn't pass `workspace_id` → compile error after Task 2.
2. The `if (!get().credentials)` guard means switching workspace while credentials are already in memory skips the reload — the new workspace would use the old workspace's credential.

Fix: always load from the workspace-scoped keychain on every `setCollection` call.

- [ ] **Step 1: Update the import and setCollection**

At the top of `src/stores/git-store.ts`, add the workspace store import after the existing imports:

```typescript
import { useWorkspaceStore } from '@/stores/workspace-store';
```

In the `setCollection` action, replace:
```typescript
// Auto-load persisted credentials if none are set in memory.
if (!get().credentials) {
  try {
    const saved = await loadGitCredentials();
    if (saved) set({ credentials: saved });
  } catch {
    // Keychain unavailable — proceed without credentials.
  }
}
```

with:
```typescript
// Always reload workspace-scoped credentials so switching workspaces
// picks up the right identity without requiring a manual re-entry.
try {
  const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
  if (workspaceId) {
    const saved = await loadGitCredentials(workspaceId);
    set({ credentials: saved ?? null });
  }
} catch {
  // Keychain unavailable — proceed without credentials.
}
```

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: zero errors in `git-store.ts`; possibly one remaining error in `GitCredentialsDialog.tsx` (fixed in Task 4).

- [ ] **Step 3: Commit**

```bash
git add src/stores/git-store.ts
git commit -m "feat: reload workspace-scoped credentials on setCollection"
```

---

## Task 4: Update GitCredentialsDialog.tsx

**Files:**
- Modify: `src/components/git/GitCredentialsDialog.tsx`

- [ ] **Step 1: Add the workspace store import**

After the existing imports, add:

```typescript
import { useWorkspaceStore } from '@/stores/workspace-store';
```

- [ ] **Step 2: Read activeWorkspaceId inside the component**

Inside `GitCredentialsDialog`, after the existing `useState` declarations, add:

```typescript
const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
```

- [ ] **Step 3: Pass workspaceId to loadGitCredentials**

In the `useEffect`, change:
```typescript
const saved = await loadGitCredentials();
```
to:
```typescript
const saved = await loadGitCredentials(activeWorkspaceId);
```

- [ ] **Step 4: Pass workspaceId to saveGitCredentials**

In `handleConnect`, change:
```typescript
await saveGitCredentials(creds);
```
to:
```typescript
await saveGitCredentials(activeWorkspaceId, creds);
```

- [ ] **Step 5: Type-check**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: zero errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/git/GitCredentialsDialog.tsx
git commit -m "feat: pass workspaceId to credential save/load in GitCredentialsDialog"
```

---

## Task 5: Update frontend tests

**Files:**
- Modify: `src/stores/__tests__/git-store.test.ts`

### Background

The test file mocks `loadGitCredentials` as `vi.fn().mockResolvedValue(null)`. After Task 2, `loadGitCredentials` now takes a `workspaceId` argument. Two test cases assert credential auto-loading behaviour and need updating:

1. The mock should remain as-is (it still works — the `workspaceId` arg is ignored by `vi.fn()`).
2. The `setCollection` tests that assert credential loading need to also mock `useWorkspaceStore` to return a non-empty `activeWorkspaceId`, otherwise the `if (workspaceId)` guard skips the load.
3. The `loadGitCredentials` call should be asserted to receive the workspace ID.

- [ ] **Step 1: Add workspace store mock**

Near the top of the test file, after the `vi.mock('@/lib/tauri-api', ...)` block, add:

```typescript
vi.mock('@/stores/workspace-store', () => ({
  useWorkspaceStore: Object.assign(
    (selector: (s: { activeWorkspaceId: string; multiWorkspaceMode: boolean }) => unknown) =>
      selector({ activeWorkspaceId: 'ws-test', multiWorkspaceMode: false }),
    {
      getState: () => ({ activeWorkspaceId: 'ws-test', multiWorkspaceMode: false }),
    }
  ),
}));
```

- [ ] **Step 2: Run existing credential tests to see which fail**

```bash
yarn test git-store 2>&1 | grep -E "FAIL|PASS|✓|×|auto-loads|leaves credentials" | head -20
```

Expected: the two credential tests (`auto-loads saved credentials` and `leaves credentials null`) fail because they now need the workspace ID.

- [ ] **Step 3: Update the credential auto-load tests**

Find the test `'auto-loads saved credentials from keychain when collection is a repo'` and update it:

```typescript
it('auto-loads saved credentials from keychain when collection is a repo', async () => {
  const { loadGitCredentials, gitIsRepo } = await import('@/lib/tauri-api');
  const savedCreds = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519', passphrase: undefined };
  vi.mocked(gitIsRepo).mockResolvedValue(true);
  vi.mocked(loadGitCredentials).mockResolvedValue(savedCreds as unknown as GitCredentials);

  await useGitStore.getState().setCollection('/some/path');

  expect(vi.mocked(loadGitCredentials)).toHaveBeenCalledWith('ws-test');
  expect(useGitStore.getState().credentials).toEqual(savedCreds);
});
```

Find the test `'leaves credentials null when keychain returns null'` and update it:

```typescript
it('leaves credentials null when keychain returns null', async () => {
  const { loadGitCredentials, gitIsRepo } = await import('@/lib/tauri-api');
  vi.mocked(gitIsRepo).mockResolvedValue(true);
  vi.mocked(loadGitCredentials).mockResolvedValue(null);

  await useGitStore.getState().setCollection('/some/path');

  expect(vi.mocked(loadGitCredentials)).toHaveBeenCalledWith('ws-test');
  expect(useGitStore.getState().credentials).toBeNull();
});
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
yarn test git-store 2>&1 | tail -20
```

Expected: all tests pass, including the two credential tests.

- [ ] **Step 5: Run the full test suite**

```bash
yarn test 2>&1 | tail -10
```

Expected: no regressions.

- [ ] **Step 6: Commit**

```bash
git add src/stores/__tests__/git-store.test.ts
git commit -m "test: update git-store tests for workspace-scoped credential loading"
```

---

## Task 6: Manual smoke test

- [ ] **Step 1: Start the app**

```bash
yarn tauri dev
```

- [ ] **Step 2: Open a collection that is a git repo, open the Git panel**

Confirm the Git Credentials dialog opens when you click push/pull without saved credentials.

- [ ] **Step 3: Select SSH Key, browse to your key, click Connect**

Credentials should save to the keychain under `git-credentials-{your-workspace-id}`.

- [ ] **Step 4: Push to verify the correct SSH identity is used**

The push should authenticate as `snehaldangroshiya`, not `Snehal1112`.

- [ ] **Step 5: Switch to a second workspace, open the Git panel**

Credentials should be `null` (since no credentials are saved for that workspace ID yet). The dialog should prompt again — confirming credentials are now per-workspace.

- [ ] **Step 6: Final commit (if any cleanup)**

```bash
git commit -m "chore: verify per-workspace SSH key smoke test complete"
```

---

## Self-Review

**Spec coverage:**
- ✅ Bug fix: per-workspace keychain key eliminates global credential sharing
- ✅ No yml changes — credentials never reach committed files
- ✅ Multi-user safe: each collaborator sets their own credentials per workspace
- ✅ `workspaceId` guard (`if (workspaceId)`) prevents loading against empty key on app init
- ✅ Always-reload on `setCollection` fixes workspace-switch credential bleed

**Placeholder scan:** None — all steps contain full code.

**Type consistency:** `workspaceId: string` used consistently across Tasks 1–5. `keyring_account(&workspace_id)` called in both Rust commands. `loadGitCredentials(workspaceId)` / `saveGitCredentials(activeWorkspaceId, creds)` match the updated signature in Task 2.

**Migration note:** Users who previously saved credentials under the old global key (`"git-credentials"`) will not find them after this change. They will be prompted once to re-enter credentials per workspace — acceptable one-time inconvenience; no data is lost from the keychain, just not read.
