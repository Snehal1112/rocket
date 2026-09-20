# Identity Setup Cancel Semantics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make "Cancel" on the SSH identity setup dialog actually cancel — today it silently activates the pending SSH credentials and retries the network operation (push/pull/fetch) that triggered them, exactly like "Save Identity" does.

**Architecture:** When the user picks an SSH-key credential, `git-store.ts`'s `setCredentials` fetches the repository's current git identity and, instead of activating the credentials immediately, opens `showIdentitySetupDialog` and stashes the credentials in `pendingCredentialsForIdentitySetup`. `GitPanel.tsx` renders `GitIdentityDialog` with two handlers: `handleIdentitySetupConfirm` (saves the identity, then calls `activatePendingCredentials()`) and `handleIdentitySetupCancel` (calls `activatePendingCredentials()` directly, with no identity save). `activatePendingCredentials` unconditionally sets `credentials`, clears the pending-setup state, and — if a `pendingNetworkOp` was recorded — immediately invokes it (`get()[pendingNetworkOp]()`). So today, Confirm and Cancel do the same thing from the store's point of view; the only difference is whether the identity gets saved first. This plan adds a distinct store action, `discardPendingIdentitySetup`, that clears the same pending-setup fields *without* setting `credentials` and *without* invoking `pendingNetworkOp`, and wires `GitPanel`'s cancel handler to it.

**Tech Stack:** TypeScript, Zustand vanilla store, Vitest.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-05 "Credential dialog can retain and apply secrets from a previous workspace" — specifically: *"Identity setup cancellation calls `activatePendingCredentials`... so 'Cancel' still activates credentials and may retry a network operation; the UI wording does not communicate that semantic."* and its recommended fix: *"Rename the identity cancel action or make it truly cancel credential activation/retry."*). Verified present in current `src/stores/git-store.ts:639-654` and `src/components/git/GitPanel.tsx:116-118`.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-commit-form-failure-preserves-message.md` next (plan 6 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- Rust: not touched by this plan (frontend-only).
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Add `discardPendingIdentitySetup` and wire Cancel to it

**Files:**
- Modify: `src/stores/git-store.ts:69` (interface), `src/stores/git-store.ts:639-654` (implementation area)
- Modify: `src/components/git/GitPanel.tsx:116-118`
- Test: `src/stores/__tests__/git-store.test.ts`

**Interfaces:**
- Produces: `discardPendingIdentitySetup: () => void` added to `GitState` in `src/stores/git-store.ts`, alongside the existing `activatePendingCredentials: () => void`.
- Consumes (`GitPanel.tsx`): `useStore(store, (state) => state.discardPendingIdentitySetup)` replacing the current `activatePendingCredentials` read used for the cancel path (the confirm path keeps using `activatePendingCredentials`, so both remain in `GitPanel`'s selector list).

- [ ] **Step 1: Write the failing test**

Add to `src/stores/__tests__/git-store.test.ts` (it already has the `vi.mock('@/lib/tauri-api', ...)` scaffolding covering `gitPush`/`gitPull`/`gitFetch` as resolved-undefined mocks — reuse that):

```ts
describe('git-store discardPendingIdentitySetup', () => {
  it('cancelling identity setup does not activate credentials or retry the pending operation', async () => {
    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '/home/user/.ssh/id_ed25519' };
    store.setState({
      repositoryId: 'repo-1',
      pendingCredentialsForIdentitySetup: creds,
      showIdentitySetupDialog: true,
      identitySetupInitialName: 'Some Name',
      identitySetupInitialEmail: 'some@example.com',
      pendingNetworkOp: 'push',
    });

    store.getState().discardPendingIdentitySetup();

    const state = store.getState();
    expect(state.credentials).toBeNull();
    expect(state.showIdentitySetupDialog).toBe(false);
    expect(state.pendingCredentialsForIdentitySetup).toBeNull();
    expect(state.pendingNetworkOp).toBeNull();
    expect(state.identitySetupInitialName).toBe('');
    expect(state.identitySetupInitialEmail).toBe('');
    expect(tauriApi.gitPush).not.toHaveBeenCalled();
  });
});
```

Add `GitCredentials` to the existing top-of-file `import type { GitCredentials } from '@/lib/tauri-api';` if not already imported in that test file (check first — the mock block already references `GitCredentials` as a type in other describe blocks; reuse the existing import rather than adding a duplicate).

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "cancelling identity setup"`
Expected: FAIL with `store.getState().discardPendingIdentitySetup is not a function`

- [ ] **Step 3: Implement the store action**

In `src/stores/git-store.ts`, add to the `GitState` interface next to `activatePendingCredentials`:

```ts
  activatePendingCredentials: () => void;
  /** Discard a pending SSH-identity-setup prompt without activating its credentials
   *  or retrying the network operation that triggered it — used when the user
   *  explicitly cancels, as opposed to confirming/saving the identity. */
  discardPendingIdentitySetup: () => void;
```

In the store body, add the implementation next to `activatePendingCredentials`:

```ts
    activatePendingCredentials: () => {
      const { pendingCredentialsForIdentitySetup, pendingNetworkOp } = get();
      if (!pendingCredentialsForIdentitySetup) return;
      const creds = pendingCredentialsForIdentitySetup;
      set({
        credentials: creds,
        showIdentitySetupDialog: false,
        pendingCredentialsForIdentitySetup: null,
        identitySetupInitialName: '',
        identitySetupInitialEmail: '',
        pendingNetworkOp: null,
      });
      if (pendingNetworkOp && creds) {
        get()[pendingNetworkOp]();
      }
    },

    discardPendingIdentitySetup: () => {
      set({
        showIdentitySetupDialog: false,
        pendingCredentialsForIdentitySetup: null,
        identitySetupInitialName: '',
        identitySetupInitialEmail: '',
        pendingNetworkOp: null,
      });
    },
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "cancelling identity setup"`
Expected: PASS

- [ ] **Step 5: Wire `GitPanel`'s cancel handler to the new action**

In `src/components/git/GitPanel.tsx`, add the new selector next to the existing `activatePendingCredentials` read:

```tsx
  const activatePendingCredentials = useStore(store, (state) => state.activatePendingCredentials);
  const discardPendingIdentitySetup = useStore(store, (state) => state.discardPendingIdentitySetup);
```

Change:

```tsx
  const handleIdentitySetupCancel = () => {
    activatePendingCredentials();
  };
```

to:

```tsx
  const handleIdentitySetupCancel = () => {
    discardPendingIdentitySetup();
  };
```

`handleIdentitySetupConfirm` is unchanged — it still calls `activatePendingCredentials()` after saving the identity.

- [ ] **Step 6: Run the full git-store and GitPanel suites, and typecheck**

Run: `yarn test src/stores/__tests__/git-store.test.ts src/components/git/__tests__/GitPanel.test.tsx`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/stores/git-store.ts src/components/git/GitPanel.tsx src/stores/__tests__/git-store.test.ts
```

Commit message: `fix(git): make identity setup Cancel actually cancel credential activation`.
