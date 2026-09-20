# Collection-Changed Listener Leak Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop `GitPanel`'s collection-changed listener from leaking when the component unmounts before the async `onCollectionChanged(...)` registration resolves.

**Architecture:** `GitPanel.tsx` registers a Tauri event listener with `void onCollectionChanged(cb).then((fn) => { unlisten = fn; })` and cleans up with `return () => { unlisten?.(); ... }`. If the effect's cleanup runs before the registration promise resolves (e.g. the panel unmounts quickly, or `repositoryId` changes and the effect re-runs), `unlisten` is still `undefined` at cleanup time — the `unlisten?.()` call is a no-op — and when the registration promise resolves afterward, `unlisten = fn` just assigns a local variable nothing will ever read again. The listener is never removed. The fix is the standard "cancelled flag" pattern: track whether cleanup has already run, and if the registration resolves after that point, call the returned unlisten function immediately instead of storing it.

**Tech Stack:** React 18, TypeScript, Vitest, React Testing Library, `@/test/deferred`.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-08 "Collection-change subscription can leak and is not repository-filtered"). Verified present in current `src/components/git/GitPanel.tsx:142-157`. The "not repository-filtered" half of F-08 no longer applies in its original global-store form — this effect now runs per-`GitPanel`-instance against that instance's own store (`refreshStatus` reads `get().repositoryId`/`get().isRepo` off the panel's own store, not a shared global one) — so this plan addresses only the listener-leak half, which is still present.

**Depends on:** `docs/superpowers/plans/2026-09-20-git-panel-load-state-unification.md` (Task 1) having already been applied — that plan renames `GitPanel`'s local `isRepo` boolean to a store-driven `loadStatus`. This plan's effect guard uses `loadStatus === 'ready'` in place of the old `isRepo` check. If that plan has not yet been applied, use `isRepo` (the current local boolean) as the guard condition instead — the cancelled-flag fix itself is independent of which guard variable is used.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-clone-stale-completion-guard.md` next (plan 4 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Guard the collection-changed listener registration with a cancelled flag

**Files:**
- Modify: `src/components/git/GitPanel.tsx:142-157`
- Test: `src/components/git/__tests__/GitPanel.test.tsx`

**Interfaces:**
- Consumes: `onCollectionChanged` from `@/lib/tauri-api` (unchanged signature: `(cb: (event) => void) => Promise<() => void>`).
- Produces: no new exports. Behavior-only change to the existing effect.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitPanel.test.tsx`:

```tsx
describe('GitPanel collection-changed listener cleanup', () => {
  it('unregisters the listener even if it unmounts before registration resolves', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(true);
    vi.mocked(tauriApi.gitStatus).mockResolvedValue({
      branch: 'main',
      files: [],
      ahead: 0,
      behind: 0,
      isClean: true,
    });
    vi.mocked(tauriApi.gitBranches).mockResolvedValue({ current: 'main', local: [], remote: [] });
    vi.mocked(tauriApi.gitStashList).mockResolvedValue([]);

    const deferredListen = createDeferred<() => void>();
    const unlisten = vi.fn();
    vi.mocked(tauriApi.onCollectionChanged).mockReturnValue(deferredListen.promise);

    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { unmount } = render(
      <QueryClientProvider client={queryClient}>
        <GitPanel repositoryId='repo-a' repositoryLabel='Repo A' />
      </QueryClientProvider>,
    );

    // Wait for the panel to reach its ready state — this is when the listener
    // registration effect runs and calls onCollectionChanged — but keep the
    // registration promise itself unresolved.
    await screen.findByText('Repo A');
    expect(tauriApi.onCollectionChanged).toHaveBeenCalled();

    // Unmount before the registration resolves.
    unmount();

    // The registration now resolves, after cleanup already ran.
    deferredListen.resolve(unlisten);
    await deferredListen.promise;
    await Promise.resolve();

    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
```

Add `createDeferred` to the test file's imports: `import { createDeferred } from '@/test/deferred';`. Also add `onCollectionChanged: vi.fn()` to the file's `vi.mock('@/lib/tauri-api', ...)` factory if it isn't already listed there (the existing mock already includes it as `vi.fn().mockResolvedValue(() => {})` for other tests — change that default to still resolve a no-op for tests that don't care, while this test overrides it per-test with `mockReturnValue`).

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx -t "unregisters the listener even if it unmounts"`
Expected: FAIL — `unlisten` is never called; the resolved function is assigned to a local variable in a cleanup closure that already ran.

- [ ] **Step 3: Implement the cancelled-flag guard**

In `src/components/git/GitPanel.tsx`, replace the collection-changed effect:

```tsx
  const statusDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (loadStatus !== 'ready') return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onCollectionChanged((event) => {
      if (event.type === 'branchSwitched' || event.type === 'branchMerged') return;
      if (statusDebounce.current) clearTimeout(statusDebounce.current);
      statusDebounce.current = setTimeout(() => void refreshStatus(), 300);
    }).then((fn) => {
      if (cancelled) {
        // Cleanup already ran before registration resolved — the effect's own
        // `unlisten` variable will never be read again, so unregister directly.
        fn();
        return;
      }
      unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
      if (statusDebounce.current) clearTimeout(statusDebounce.current);
    };
  }, [loadStatus, refreshStatus]);
```

(If `docs/superpowers/plans/2026-09-20-git-panel-load-state-unification.md` has not yet been applied to this codebase, use `if (!isRepo) return;` and the dependency array `[isRepo, refreshStatus]` instead — the `cancelled` mechanics are otherwise identical.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx -t "unregisters the listener even if it unmounts"`
Expected: PASS

- [ ] **Step 5: Run the full GitPanel suite and typecheck**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitPanel.tsx src/components/git/__tests__/GitPanel.test.tsx
```

Commit message: `fix(git): unregister the collection-changed listener even if unmount races its registration`.
