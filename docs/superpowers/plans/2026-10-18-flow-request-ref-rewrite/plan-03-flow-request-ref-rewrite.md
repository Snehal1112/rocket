# Flow Request Ref Rewrite, Plan 3 (app wiring and frontend foundations) Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** The running app uses the backend rewrite, the frontend can call the lookup and listen for the event, and open flow tabs can be patched from it.

**Architecture:** One shared `Arc<FlowService>` built early in `lib.rs` feeds both `CollectionService` builders, so every move (sidebar, folder rename, assistant proposals) rewrites flows. A thin command serves the lookup. A pure helper and a pane-store action patch open tabs.

**Tech Stack:** Rust (Tauri), TypeScript, Zustand, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-18-flow-request-ref-rewrite-design.md`

## Global Constraints

- Plans 1 and 2 are merged.
- Commands stay thin. The managed `FlowService` stays as it is.
- Tab patching changes the saved paths only. The `isDirty` flag is never touched.
- shadcn primitives and lucide icons only. No full destructure of Zustand state: use `usePaneStore.getState()` or narrow selectors.
- The event carries `{ type, collection, from, to, flows, failed }` with `failed: { flow, message }[]`.
- Cargo: `cargo check -j4 -p rocket`. Frontend: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern> --run`.

## Review Focus

1. A tab in another collection, or a path outside the moved folder, keeps the same node object (test `rewriteFlowRequestPaths leaves other collections and unrelated paths alone`).
2. Undo back to the saved state still reads as clean after a rewrite: the tab's `nodes` and `history.saved.nodes` stay the same array (test `rewriteFlowRequestPaths keeps a clean tab clean and a dirty tab dirty`).
3. A flow tab parked in another collection's snapshot is patched too (test `rewriteFlowRequestPaths patches parked collection tabs`).
4. A tab with no `history`, or a null `collectionName`, never throws (test `rewriteFlowRequestPaths ignores tabs without history or collection`).
5. An empty `from` never rewrites anything on the frontend either (test `rewriteTabPaths never matches an empty from`).

## File Structure

- Modify `src-tauri/src/lib.rs`: build `flow_refs`, pass it to both builders, register the command.
- Modify `src-tauri/src/commands/flow.rs`: `flows_using_request`.
- Modify `src/lib/tauri-api.ts`: `flowsUsingRequest`, `onFlowRequestPathsRewritten`, event types.
- Create `src/lib/flow-request-paths.ts`: `rewriteTabPaths`.
- Modify `src/stores/pane-store.ts`: `rewriteFlowRequestPaths`.
- Tests: `src/lib/__tests__/flow-request-paths.test.ts`, `src/stores/__tests__/pane-store-flow-request-paths.test.ts`.

### Task 1: Wire the updater into both CollectionService instances

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

This task is wiring only, with no new logic: the behaviour is pinned by the tests of Plans 1 and 2. Its checks are a compile and a source scan.

**Files:**
- Modify: `src-tauri/src/lib.rs` (`collection_svc` ~385, `proposal_svc` ~399, `flow_svc` ~715)

**Interfaces:**
- Consumes: `rocket_app::FlowReferenceUpdater`, `CollectionService::with_flow_references`.
- Produces: both `CollectionService` instances in `lib.rs` hold the same `Arc<dyn FlowReferenceUpdater>`.

- [ ] **Step 1: Read** `lib.rs:375-412,712-720` and `crates/rocket-infra/src/shared_path_flow_repo.rs:1-30`.
- [ ] **Step 2: Implement.** Before `collection_svc`, build `let flow_refs: Arc<dyn rocket_app::FlowReferenceUpdater> = Arc::new(rocket_app::FlowService::new(Box::new(rocket_infra::SharedPathFlowRepo::new(Arc::clone(&active_workspace_path)))));`. Chain `.with_flow_references(Arc::clone(&flow_refs))` onto both `CollectionService::new_with_audit(...)` calls (after `.with_trust_store(...)` on the first). Leave `flow_svc` and its `app.manage` as they are, with a one-line comment above `flow_refs` saying it follows workspace switches through the shared path.
- [ ] **Step 3: Verify.** Run `cargo check -j4 -p rocket`. Expected: clean. Run `grep -c with_flow_references src-tauri/src/lib.rs`. Expected: `2`. Run `cargo test -j4 -p rocket-app flow_request_ref_e2e`. Expected: 5 passed.

### Task 2: Lookup command, API wrappers and the tab rewrite helper

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/flow.rs` (near `rename_flow`, ~446), `src-tauri/src/lib.rs` (`generate_handler!` ~967)
- Modify: `src/lib/tauri-api.ts` (flow commands ~2344-2371, event helpers ~1491 and ~2683)
- Create: `src/lib/flow-request-paths.ts`
- Test: `src/lib/__tests__/flow-request-paths.test.ts`

**Interfaces:**
- Produces:
  - Tauri command `flows_using_request(collection: String, path: String, svc: State<'_, FlowService>) -> Result<Vec<String>, DomainError>`, which calls `FlowReferenceUpdater::flows_using(&*svc, &collection, &path)` and nothing else.
  - `export const flowsUsingRequest = (collection: string, path: string) => invoke<string[]>('flows_using_request', { collection, path })`.
  - `export interface FlowRewriteFailure { flow: string; message: string }` and `export interface FlowRequestPathsRewrittenEvent { collection: string; from: string; to: string; flows: string[]; failed: FlowRewriteFailure[] }`.
  - `export const onFlowRequestPathsRewritten = (handler: (event: FlowRequestPathsRewrittenEvent) => void): Promise<UnlistenFn> => listen<FlowRequestPathsRewrittenEvent>('flow-request-paths-rewritten', (e) => handler(e.payload))`.
  - `export function rewriteTabPaths(tab: FlowTab, from: string, to: string): FlowTab` in `src/lib/flow-request-paths.ts`. It returns the same tab object when nothing changes.

- [ ] **Step 1: Read** `commands/flow.rs:420-450`, `lib.rs:960-975`, `pane-store.ts:145-170`, `src/lib/flow-history.ts:8-40,121`, `src/types/pane-types.ts:207-245`, `tauri-api.ts:2263-2330` (`RequestSource`, `FlowNode`) and the `isPathWithin` helper re-exported by `tree-utils.ts`.
- [ ] **Step 2: Write the failing tests** in `flow-request-paths.test.ts`. Add `requestNode(id, path)` building `{ id, kind: { kind: 'Request', label: id, source: { type: 'Saved', requestPath: path } }, position: { x: 0, y: 0 } }` and a `flowTab` helper as `pane-store-flow-rename.test.ts` has. Tests:
  - `rewriteTabPaths replaces a folder prefix in nodes and every history snapshot`: nodes `auth/login.yml` and `auth/sub/me.yml`, `history = { past: [snap], future: [snap], saved: snap }`, `rewriteTabPaths(tab,'auth','identity')` reads `identity/login.yml` and `identity/sub/me.yml` in `nodes`, `history.past[0].nodes`, `history.future[0].nodes` and `history.saved.nodes`.
  - `rewriteTabPaths keeps nodes and saved nodes as one array`: with `history.saved = snapOf(tab)`, the result satisfies `result.nodes === result.history.saved.nodes` and `isAtSaved(snapOf(result), result.history.saved)` is `true`.
  - `rewriteTabPaths returns the same tab when nothing matches`: the result `toBe(tab)`, and an unrelated node keeps its identity when another node changes.
  - `rewriteTabPaths never matches an empty from`: `from = ''` and `from = '/'` return the same tab.
  - `rewriteTabPaths does not touch author for auth`: `author/x.yml` stays.
  - `rewriteTabPaths survives a tab without history`: `history` undefined, nodes still patched.
- [ ] **Step 3: Run** `yarn test flow-request-paths --run`. Expected: failure, module `@/lib/flow-request-paths` not found.
- [ ] **Step 4: Implement.** The helper keeps one `Map<FlowNode[], FlowNode[]>` per call so arrays that were the same before map to the same new array. A node changes only when `kind.kind === 'Request'`, `kind.source.type === 'Saved'` and its `requestPath` equals `from` or starts with `from + '/'` (one trailing `/` stripped from `from`, never a match for an empty `from`). Unchanged nodes and arrays keep their identity. The mapping is applied to `nodes`, `history.past`, `history.future` and `history.saved`, each snapshot rebuilt as `{ ...snap, nodes }` only when its nodes changed. Add the Rust command and register `commands::flow::flows_using_request`. Add the `tauri-api.ts` exports.
- [ ] **Step 5: Run** `yarn test flow-request-paths --run` (expected: 6 passed), `cargo check -j4 -p rocket` (clean), `grep -n flows_using_request src-tauri/src/lib.rs src-tauri/src/commands/flow.rs` (expected: one registration, one definition), `yarn tsc --noEmit` and `yarn check` (clean).

### Task 3: Pane-store action

**Files:**
- Modify: `src/stores/pane-store.ts` (action declarations ~320-327, `renameFolderTabs` ~1575)
- Test: `src/stores/__tests__/pane-store-flow-request-paths.test.ts`

**Interfaces:**
- Consumes: `rewriteTabPaths` from Task 2, `updateTabEverywhere` (`pane-store.ts:145`), `collectAllTabs`, `isFlowTab`.
- Produces: store action `rewriteFlowRequestPaths: (collection: string, from: string, to: string) => void`.

- [ ] **Step 1: Read** `pane-store.ts:1575-1605` (`renameFolderTabs`, the model), `src/lib/flow-tabs.ts` and `src/stores/__tests__/pane-store-flow-rename.test.ts` (mocks, `flowTab`, `liveTab`).
- [ ] **Step 2: Write the failing tests** in the new file with those mocks, `usePaneStore.getState().reset()` in `beforeEach`, and a `requestNode` helper. Tests:
  - `rewriteFlowRequestPaths patches nodes and history snapshots`: a tab in `c` with `history = { past: [snap], future: [], saved: snap }`. After `rewriteFlowRequestPaths('c','auth','identity')` the live tab's `nodes`, `history.past[0].nodes` and `history.saved.nodes` read `identity/login.yml`.
  - `rewriteFlowRequestPaths keeps a clean tab clean and a dirty tab dirty`: one clean and one dirty tab keep their `isDirty`, and for the clean tab `isAtSaved(snapOf(tab), tab.history.saved)` is `true`.
  - `rewriteFlowRequestPaths patches parked collection tabs`: a tab set through `usePaneStore.setState({ collectionTabState: { c: { tabs: [tab], activeTabId: tab.id } } })` is patched.
  - `rewriteFlowRequestPaths leaves other collections and unrelated paths alone`: a tab in `other` with `auth/login.yml` is the same object afterwards, and in `c` a node at `author/x.yml` keeps its object identity.
  - `rewriteFlowRequestPaths ignores tabs without history or collection`: a tab with `history` undefined is patched in `nodes`, a tab with `collectionName: null` is untouched and nothing throws.
  - `rewriteFlowRequestPaths patches a tab that is both live and parked once`: a tab id present in the tree and in a snapshot is rewritten, and `rewriteTabPaths` is applied to it once per location without error.
- [ ] **Step 3: Run** `yarn test pane-store-flow-request-paths --run`. Expected: failures, `rewriteFlowRequestPaths is not a function`.
- [ ] **Step 4: Implement** the action modelled on `renameFolderTabs`: collect flow tabs of the collection from `collectAllTabs(state.root)` and every `collectionTabState` entry, dedupe by `tab.id`, apply `rewriteTabPaths` through `updateTabEverywhere`, and `set` only when something changed.
- [ ] **Step 5: Run** `yarn test pane-store-flow-request-paths --run` (expected: 6 passed), `yarn test pane-store --run` (expected: all pass), `yarn tsc --noEmit` and `yarn check` (clean).

Next plan: docs/superpowers/plans/2026-10-18-flow-request-ref-rewrite/plan-04-flow-request-ref-rewrite.md
