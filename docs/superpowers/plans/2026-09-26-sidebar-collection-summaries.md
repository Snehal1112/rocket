# Sidebar Collection Summaries Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the collection sidebar from loading every request's full body/headers/auth/scripts on every expand/refresh — switch it to the already-existing, currently-unused `get_collection_summaries` backend endpoint, and fetch a single request's full data on demand only when the user actually opens it.

**Architecture:** `CollectionNode.tsx`'s tree-render fetch switches from `getCollection` (full data) to a new `getCollectionSummaries` wrapper. Each request row that only has summary data (`uid`/`name`/`method`/`fileName`, no body/headers/auth/scripts) now renders as before, but `RequestNode`'s click-to-open path becomes async and calls a new `getRequest(collection, path)` IPC command — wired through a new `CollectionService::get_request` method onto the already-existing `CollectionRepository::get_request` trait method — to fetch full data on demand before opening a tab. Every other `getCollection` call site in the app (breadcrumb pickers, contract modal, collection overview, duplicate handler, the request runner) is confirmed to need full data and is left untouched.

**Tech Stack:** Rust (Tauri v2 commands, `rocket-app`/`rocket-infra`/`rocket-collection` crates), React 18 + TypeScript (Zustand, Vitest, Testing Library).

**Spec:** None — this plan is derived directly from a live-codebase investigation completed in this session (four parallel read-only subagents tracing `getCollection`'s sidebar call site, the unused `get_collection_summaries` backend command, every other `getCollection` call site, and an adversarial regression trace of the sidebar's own click-to-open/search/caching logic). The findings are folded into the "Architecture" note and each task body below; there is no separate design doc.

## Global Constraints

- All `.yml` collection/request field names and shapes are unchanged by this plan — no schema changes, no new persisted fields.
- Rust: production code paths must not call `.expect`/`.unwrap` on a `Result`/`Option` (applies to the new `get_request` Tauri command and `CollectionService` method); test code below calls `.expect("...")` for clear failure messages, matching this crate's existing test conventions.
- Rust: `#[serde(rename_all = "camelCase")]` stays on IPC DTOs only — the new command reuses the existing `Request`/`RequestSummary` domain types, so no new DTOs are introduced.
- Commits: conventional commits format (`feat:`, `fix:`, `test:`, etc.), created only via the `dev-workflow-skills:1-git-commit` skill (per this user's global instructions — never a freeform `git commit -m` on this project).
- UI: no new interactive elements are introduced by this plan (no new buttons/dialogs/inputs), so the shadcn/ui-primitives-only rule has nothing new to check here.
- Every task below touches Tauri IPC commands and/or `CollectionItem`/`Request` data models, so each one is prefixed with the required OpenCollection spec-reference read per this repo's `CLAUDE.md` injection rule.

## Review Focus

- **Non-HTTP items mixed into a summarized folder** — after loosening the render guard from `item.type === 'summary' || item.type === 'opaque'` to just `item.type === 'opaque'`, an opaque GraphQL/gRPC/WebSocket item sitting next to summary items in the same folder must still be silently skipped, not crash or render as a broken request row. Covered by Task 3's test.
- **A request that filters out but sits in a non-matching folder** — the search filter must still always show folders (matched or not) while now also hiding non-matching `type: 'summary'` rows the same way it already hides non-matching `type: 'request'` rows. Covered by Task 3's test.
- **A request file valid enough to summary-parse but malformed in a full-parse-only field (e.g. `http.body.type`)** — it must still appear in the sidebar (summary parse succeeds) but clicking it must fail gracefully (full parse errors, no panic) instead of crashing the app. Covered by Task 1's backend regression test and Task 2's frontend fetch-rejection test.
- **Rapid double-click on the same summary row before its on-demand fetch resolves** — must not throw or duplicate a tab; `openTab`'s existing dedup-by-id logic (via `findTabInTree`) is relied on to just activate the same tab once the second fetch resolves. Covered by Task 2's test.
- **End-to-end type safety of the widened `RequestNode.itemData` prop** — a `'folder'` or `'opaque'` item must never be assignable to it, and `mapApiRequestToState`/`createTab` must never be reachable with one at runtime (Task 3's render guards prevent it structurally). Verified by `yarn tsc --noEmit` in Task 5.

---

### Task 1: Backend — expose `get_request` over Tauri IPC

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection/tests.rs` (new regression test near the existing `get_request` tests around line 86)
- Modify: `crates/rocket-app/src/collection_service.rs:49-51` (new method after `get_summaries`), and its `#[cfg(test)] mod tests` block (new tests after `rename_request_emits_request_saved`, around line 684)
- Modify: `src-tauri/src/commands/collections.rs:84-89` (new command after `get_collection_summaries`)
- Modify: `src-tauri/src/lib.rs:359` (register the new command)

**Interfaces:**
- Consumes: `CollectionRepository::get_request(&self, collection: &str, path: &str) -> DomainResult<Request>` — already exists (`crates/rocket-collection/src/repository.rs:31`, implemented at `crates/rocket-infra/src/fs_collection/mod.rs:127`).
- Produces: `CollectionService::get_request(&self, collection: &str, path: &str) -> DomainResult<Request>` and the Tauri command `get_request(collection: String, path: String) -> Result<Request, DomainError>`, callable from the frontend as `invoke<Request>('get_request', { collection, path })`. Task 2 depends on this exact command name and argument shape.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write a failing regression test pinning the existing malformed-file safety guarantee**

This pins the behavior the new IPC command will surface to the frontend: a request file valid enough to appear in the sidebar's summary tree, but malformed in a full-parse-only field, must fail `get_request` with an error — not a panic. This test targets the *already-existing* `get_request`/`get_summaries` implementation, so it should pass immediately once written; it exists to lock the guarantee in place before new code is built on top of it.

Add to `crates/rocket-infra/src/fs_collection/tests.rs` (near the other `get_request` tests, e.g. after the test at line 86):

```rust
#[test]
fn get_request_errors_on_body_malformed_but_summary_parseable_file() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    fs::write(
        dir.path().join("my-api/broken.yml"),
        "info:\n  name: Broken\n  type: http\nhttp:\n  method: GET\n  url: https://example.com\n  body:\n    type: bogus\n    data: x\n",
    )
    .expect("write broken.yml");

    // The lenient summary loader only reads uid/info.name/http.method/http.url,
    // so this file still appears in get_summaries()...
    let summaries = repo.get_summaries("my-api").expect("get_summaries");
    assert_eq!(summaries.root.items.len(), 1);

    // ...but the strict full loader used by get_request rejects the malformed
    // body.type discriminant instead of panicking. The frontend's on-demand
    // fetch (RequestNode.createTab) relies on this being a clean error.
    let result = repo.get_request("my-api", "broken.yml");
    assert!(result.is_err(), "expected malformed body to error, got {result:?}");
}
```

- [ ] **Step 3: Run it to verify it passes against existing behavior**

Run: `cargo test -p rocket-infra -j4 get_request_errors_on_body_malformed_but_summary_parseable_file -- --nocapture`
Expected: PASS (this pins pre-existing behavior; it is not testing new code yet).

- [ ] **Step 4: Write the failing `CollectionService::get_request` tests**

Add to `crates/rocket-app/src/collection_service.rs`'s `#[cfg(test)] mod tests` block, after `rename_request_emits_request_saved` (around line 684):

```rust
    #[test]
    fn get_request_returns_the_saved_request() {
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(NullEventPublisher),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        svc.save_request("my-api", "users.yml", &request)
            .expect("save_request");

        let loaded = svc.get_request("my-api", "users.yml").expect("get_request");
        assert_eq!(loaded.name, "Get Users");
        assert_eq!(loaded.url, "https://api.example.com/users");
    }

    #[test]
    fn get_request_errors_for_missing_request() {
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(NullEventPublisher),
        );
        assert!(svc.get_request("my-api", "ghost.yml").is_err());
    }
```

- [ ] **Step 5: Run the new tests to verify they fail**

Run: `cargo test -p rocket-app -j4 get_request_returns_the_saved_request get_request_errors_for_missing_request -- --nocapture`
Expected: FAIL with "no method named `get_request` found for struct `CollectionService`".

- [ ] **Step 6: Implement `CollectionService::get_request`**

In `crates/rocket-app/src/collection_service.rs`, add after `get_summaries` (line 51):

```rust
    /// Get the full request at `path`, including body/headers/auth/scripts.
    /// Used by the frontend to fetch full data on demand for a sidebar item
    /// that was loaded via `get_summaries`.
    pub fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
        self.repo.get_request(collection, path)
    }
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p rocket-app -j4 get_request_returns_the_saved_request get_request_errors_for_missing_request -- --nocapture`
Expected: PASS.

- [ ] **Step 8: Add the Tauri command**

In `src-tauri/src/commands/collections.rs`, add after `get_collection_summaries` (line 89):

```rust
#[tauri::command]
pub fn get_request(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<Request, DomainError> {
    svc.get_request(&collection, &path)
}
```

- [ ] **Step 9: Register the command**

In `src-tauri/src/lib.rs`, add `commands::collections::get_request,` immediately after `commands::collections::get_collection_summaries,` (line 359):

```rust
            commands::collections::get_collection,
            commands::collections::get_collection_summaries,
            commands::collections::get_request,
            commands::collections::create_collection,
```

- [ ] **Step 10: Verify the whole backend still compiles and passes**

Run: `cargo check -p rocket -j4` then `cargo test -p rocket-app -p rocket-infra -j4`
Expected: compiles clean, all tests pass (including the three new ones from this task).

- [ ] **Step 11: Commit**

Stage `crates/rocket-infra/src/fs_collection/tests.rs`, `crates/rocket-app/src/collection_service.rs`, `src-tauri/src/commands/collections.rs`, and `src-tauri/src/lib.rs`. Then invoke the `dev-workflow-skills:1-git-commit` skill (do not write a freeform commit message directly) to generate and create the commit for this task.

---

### Task 2: Frontend — `RequestNode` fetches full data on demand

**Files:**
- Modify: `src/lib/tauri-api.ts:613` (add `getRequest` wrapper after `getCollection`)
- Modify: `src/components/collections/RequestNode.tsx` (widen `itemData` prop type; make `createTab`/click-to-open async)
- Create: `src/components/collections/__tests__/RequestNode.test.tsx`

**Interfaces:**
- Consumes: the Tauri command `get_request` from Task 1; `findTabInTree`/`createDefaultLeaf` from `src/lib/pane-utils.ts` (already exported); `usePaneStore` (already exists).
- Produces: `export const getRequest = (collection: string, path: string) => invoke<Request>('get_request', { collection, path });` in `tauri-api.ts` — Task 3/4 do not need this directly, but any future caller can rely on this exact name/signature. `RequestNode`'s `itemData` prop now accepts `Extract<CollectionItem, { type: 'request' } | { type: 'summary' }>` — Task 3 relies on this widened type to pass summary items through without a cast.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing test file**

Create `src/components/collections/__tests__/RequestNode.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RequestNode } from '@/components/collections/RequestNode';
import { createDefaultLeaf, findTabInTree } from '@/lib/pane-utils';
import type { CollectionItem } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getRequest: vi.fn(),
  };
});

const fullItem: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }> = {
  type: 'request',
  uid: 'req-1',
  name: 'Get Users',
  method: 'GET',
  url: 'https://api.example.com/users',
  headers: [{ key: 'X-Test', value: '1', enabled: true }],
  auth: { authType: 'none' },
};

const summaryItem: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }> = {
  type: 'summary',
  uid: 'req-2',
  name: 'List Orders',
  method: 'GET',
  url: 'https://api.example.com/orders',
};

function renderNode(
  itemData: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }>,
  path: string,
) {
  return render(
    <RequestNode
      uid={itemData.uid}
      name={itemData.name}
      method={itemData.method}
      collectionName='my-api'
      collectionRoot='/workspace/collections/my-api'
      path={path}
      itemData={itemData}
      summaries={[]}
      onMove={vi.fn()}
      onDelete={vi.fn()}
      onDuplicate={vi.fn()}
    />,
  );
}

describe('RequestNode click-to-open', () => {
  beforeEach(() => {
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.getRequest).mockReset();
  });

  it('opens a tab directly from a full request item without calling getRequest', async () => {
    renderNode(fullItem, 'users.yml');
    const user = userEvent.setup();
    await user.click(screen.getByLabelText('Open GET Get Users'));

    await waitFor(() => {
      expect(findTabInTree(usePaneStore.getState().root, 'req-1')).not.toBeNull();
    });
    expect(tauriApi.getRequest).not.toHaveBeenCalled();
  });

  it('fetches the full request on demand when opening a summary item', async () => {
    vi.mocked(tauriApi.getRequest).mockResolvedValue({
      uid: 'req-2',
      name: 'List Orders',
      method: 'GET',
      url: 'https://api.example.com/orders',
      headers: [{ key: 'X-Order', value: 'abc', enabled: true }],
      auth: { authType: 'none' },
    });
    renderNode(summaryItem, 'orders.yml');
    const user = userEvent.setup();
    await user.click(screen.getByLabelText('Open GET List Orders'));

    await waitFor(() => {
      expect(tauriApi.getRequest).toHaveBeenCalledWith('my-api', 'orders.yml');
    });
    await waitFor(() => {
      const found = findTabInTree(usePaneStore.getState().root, 'req-2');
      expect(found?.tab.tabType).toBe('request');
      const headers = found?.tab.tabType === 'request' ? found.tab.request.headers : [];
      expect(headers.some((h) => h.key === 'X-Order' && h.value === 'abc')).toBe(true);
    });
  });

  it('does not open a tab when the on-demand fetch fails', async () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(tauriApi.getRequest).mockRejectedValue(new Error('not found'));
    renderNode(summaryItem, 'orders.yml');
    const user = userEvent.setup();
    await user.click(screen.getByLabelText('Open GET List Orders'));

    await waitFor(() => {
      expect(consoleError).toHaveBeenCalled();
    });
    expect(findTabInTree(usePaneStore.getState().root, 'req-2')).toBeNull();
    consoleError.mockRestore();
  });

  it('does not duplicate a tab on a rapid double-click of the same summary item', async () => {
    vi.mocked(tauriApi.getRequest).mockResolvedValue({
      uid: 'req-2',
      name: 'List Orders',
      method: 'GET',
      url: 'https://api.example.com/orders',
      headers: [],
      auth: { authType: 'none' },
    });
    renderNode(summaryItem, 'orders.yml');
    const user = userEvent.setup();
    const row = screen.getByLabelText('Open GET List Orders');
    await user.click(row);
    await user.click(row);

    await waitFor(() => {
      expect(tauriApi.getRequest).toHaveBeenCalledTimes(2);
    });
    const leaf = usePaneStore.getState().root;
    expect(leaf.type).toBe('leaf');
    expect(leaf.type === 'leaf' ? leaf.tabs.length : -1).toBe(1);
  });
});
```

- [ ] **Step 3: Run it to verify it fails**

Run: `yarn vitest run src/components/collections/__tests__/RequestNode.test.tsx`
Expected: FAIL — `tauriApi.getRequest` is not a function (the wrapper doesn't exist yet), and `RequestNode`'s `itemData` prop type doesn't accept a `'summary'` item yet.

- [ ] **Step 4: Add the `getRequest` wrapper**

In `src/lib/tauri-api.ts`, add immediately after `getCollection` (line 613):

```ts
export const getCollection = (name: string) => invoke<Collection>('get_collection', { name });

export const getRequest = (collection: string, path: string) =>
  invoke<Request>('get_request', { collection, path });
```

- [ ] **Step 5: Widen `RequestNode`'s `itemData` prop type and import `getRequest`**

In `src/components/collections/RequestNode.tsx`, change the import at line 38:

```ts
import { getRequest, renameRequest } from '@/lib/tauri-api';
```

Change the prop type at line 57:

```ts
  itemData: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }>;
```

- [ ] **Step 6: Make `createTab` async and add an `openInPane` helper**

Replace lines 142-169 (the `createTab`, `handleClick`, and `openInSplit` functions) with:

```ts
  // Builds a RequestTab, fetching full request data on demand if only a
  // lightweight summary (uid/name/method/fileName) was loaded from the sidebar.
  async function createTab(): Promise<RequestTab> {
    const full = itemData.type === 'request' ? itemData : await getRequest(collectionName, path);
    const request: RequestState = mapApiRequestToState(full, true);
    return {
      id: uid,
      title: name,
      tabType: 'request',
      request,
      response: null,
      isDirty: false,
      source: { collection: collectionName, path },
    };
  }

  // Builds the tab and opens it in the given pane (or the active pane when omitted).
  // Swallows fetch failures so a malformed/deleted file can't crash the sidebar —
  // mirrors the existing error handling in CollectionNode's refreshTree.
  async function openInPane(groupId?: string) {
    try {
      openTab(await createTab(), groupId);
    } catch (err) {
      console.error('[RequestNode] Failed to load request:', err);
    }
  }

  function handleClick() {
    if (isRenaming) return;
    void openInPane();
  }

  // Opens the tab in a new pane created by splitting in the given direction.
  async function openInSplit(direction: 'horizontal' | 'vertical') {
    const allCurrentIds = collectLeafGroupIds(root);
    splitGroup(activeGroupId, direction);
    const newRoot = usePaneStore.getState().root;
    const newIds = collectLeafGroupIds(newRoot);
    const newGroupId = newIds.find((id) => !allCurrentIds.includes(id));
    if (newGroupId) await openInPane(newGroupId);
  }
```

- [ ] **Step 7: Update the remaining `createTab()`/`openInSplit()` call sites**

`handleClick` is unchanged at its call site (`onClick={handleClick}`, line 186) since it stays synchronous. Update the four other call sites that directly called `createTab()`/`openInSplit()`:

In the dropdown menu (around lines 279, 292, 299, 302):

```tsx
              {otherGroupIds.length === 1 && (
                <DropdownMenuItem onClick={() => void openInPane(otherGroupIds[0])}>
                  <LayoutPanelLeft aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open in other
                  pane
                </DropdownMenuItem>
              )}
              {otherGroupIds.length > 1 && (
                <DropdownMenuSub>
                  <DropdownMenuSubTrigger>
                    <LayoutPanelLeft aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open in
                    other pane
                  </DropdownMenuSubTrigger>
                  <DropdownMenuSubContent className='w-48'>
                    {otherGroupIds.map((gid) => (
                      <DropdownMenuItem key={gid} onClick={() => void openInPane(gid)}>
                        Pane {allLeafIds.indexOf(gid) + 1}
                      </DropdownMenuItem>
                    ))}
                  </DropdownMenuSubContent>
                </DropdownMenuSub>
              )}
              <DropdownMenuItem onClick={() => void openInSplit('horizontal')}>
                <PanelRight aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open to right
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => void openInSplit('vertical')}>
                <PanelBottom aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open below
              </DropdownMenuItem>
```

And the mirrored right-click context menu block (around lines 351-374):

```tsx
        {otherGroupIds.length === 1 && (
          <ContextMenuItem onClick={() => void openInPane(otherGroupIds[0])}>
            <LayoutPanelLeft aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open in other pane
          </ContextMenuItem>
        )}
        {otherGroupIds.length > 1 && (
          <ContextMenuSub>
            <ContextMenuSubTrigger>
              <LayoutPanelLeft aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open in other pane
            </ContextMenuSubTrigger>
            <ContextMenuSubContent className='w-48'>
              {otherGroupIds.map((gid) => (
                <ContextMenuItem key={gid} onClick={() => void openInPane(gid)}>
                  Pane {allLeafIds.indexOf(gid) + 1}
                </ContextMenuItem>
              ))}
            </ContextMenuSubContent>
          </ContextMenuSub>
        )}
        <ContextMenuItem onClick={() => void openInSplit('horizontal')}>
          <PanelRight aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open to right
        </ContextMenuItem>
        <ContextMenuItem onClick={() => void openInSplit('vertical')}>
          <PanelBottom aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open below
        </ContextMenuItem>
```

- [ ] **Step 8: Run the test to verify it passes**

Run: `yarn vitest run src/components/collections/__tests__/RequestNode.test.tsx`
Expected: PASS (all four tests).

- [ ] **Step 9: Type-check**

Run: `yarn tsc --noEmit`
Expected: no errors.

- [ ] **Step 10: Commit**

Stage `src/lib/tauri-api.ts`, `src/components/collections/RequestNode.tsx`, and `src/components/collections/__tests__/RequestNode.test.tsx`. Then invoke the `dev-workflow-skills:1-git-commit` skill to generate and create the commit for this task.

---

### Task 3: Frontend — render and filter `'summary'` items in the tree

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx:320-323,573-589`
- Modify: `src/components/collections/FolderNode.tsx:160-165,345-363`
- Modify: `src/components/collections/__tests__/CollectionNode.test.tsx` (new tests)

**Interfaces:**
- Consumes: `RequestNode`'s widened `itemData` type from Task 2.
- Produces: no new exports; `CollectionNode`/`FolderNode` now render `type: 'summary'` items identically to how they already render `type: 'request'` items, and their filter predicates treat the two types identically. Task 4 depends on this being in place before it starts returning real `'summary'` items from the network.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

Add to `src/components/collections/__tests__/CollectionNode.test.tsx`, a new `describe` block after the existing one:

```tsx
describe('CollectionNode summary item rendering', () => {
  const collectionWithSummaryAndOpaqueItems: tauriApi.Collection = {
    name: 'my-collection',
    root: {
      uid: 'root',
      name: 'my-collection',
      items: [
        {
          type: 'summary',
          uid: 'req-1',
          name: 'List Orders',
          method: 'GET',
          url: 'https://api.example.com/orders',
          fileName: 'list-orders.yml',
        },
        {
          type: 'opaque',
          protocol: 'graphql',
          name: 'GraphQL Query',
          raw: {},
        },
      ],
    },
    settings: { headers: [], variables: [], sandboxMode: 'safe' },
  };

  beforeEach(() => {
    vi.mocked(tauriApi.getCollection).mockResolvedValue(collectionWithSummaryAndOpaqueItems);
    usePaneStore.setState({ activeCollection: summary.name });
  });

  it('renders a summary item as a request row and skips the opaque item', async () => {
    renderNode();

    await waitFor(() => {
      expect(screen.getByTestId('request-item-GET-List Orders')).toBeInTheDocument();
    });
    expect(screen.queryByText('GraphQL Query')).not.toBeInTheDocument();
  });

  it('filters out a non-matching summary item by name, like it does for request items', async () => {
    const { rerender } = render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <CollectionNode
          summary={summary}
          filter='does-not-match'
          summaries={[summary]}
          onNewFolder={vi.fn()}
          onMove={vi.fn()}
          onDelete={vi.fn()}
          onDuplicate={vi.fn()}
        />
      </QueryClientProvider>,
    );

    await waitFor(() => {
      expect(tauriApi.getCollection).toHaveBeenCalled();
    });
    expect(screen.queryByTestId('request-item-GET-List Orders')).not.toBeInTheDocument();
    rerender(<></>);
  });
});
```

Add `render, screen` to the existing `@testing-library/react` import at the top of the file (it currently only imports `render, waitFor`):

```tsx
import { render, screen, waitFor } from '@testing-library/react';
```

- [ ] **Step 3: Run it to verify it fails**

Run: `yarn vitest run src/components/collections/__tests__/CollectionNode.test.tsx`
Expected: FAIL — the summary item does not render (current code returns `null` for `type === 'summary'`), so `request-item-GET-List Orders` is never found.

- [ ] **Step 4: Loosen the render guard and filter predicate in `CollectionNode.tsx`**

Change the filter predicate at lines 320-323:

```tsx
  const filteredItems = sortItemsFoldersFirst(
    filter
      ? rawItems.filter(
          (item) =>
            (item.type !== 'request' && item.type !== 'summary') ||
            item.name.toLowerCase().includes(filter.toLowerCase()),
        )
      : rawItems,
  );
```

Change the render guard at line 573 (inside the `filteredItems.map` block):

```tsx
            if (item.type === 'opaque') return null;
```

- [ ] **Step 5: Loosen the render guard and filter predicate in `FolderNode.tsx`**

Change the filter predicate at lines 160-163:

```tsx
  const filteredItems = sortItemsFoldersFirst(
    filter
      ? items.filter(
          (item) =>
            (item.type !== 'request' && item.type !== 'summary') ||
            item.name.toLowerCase().includes(filter.toLowerCase()),
        )
      : items,
  );
```

Change the render guard at line 345:

```tsx
            if (item.type === 'opaque') return null;
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `yarn vitest run src/components/collections/__tests__/CollectionNode.test.tsx`
Expected: PASS.

- [ ] **Step 7: Type-check and lint**

Run: `yarn tsc --noEmit` then `yarn check`
Expected: no errors.

- [ ] **Step 8: Commit**

Stage `src/components/collections/CollectionNode.tsx`, `src/components/collections/FolderNode.tsx`, and `src/components/collections/__tests__/CollectionNode.test.tsx`. Then invoke the `dev-workflow-skills:1-git-commit` skill to generate and create the commit for this task.

---

### Task 4: Frontend — switch the sidebar's fetch to `get_collection_summaries`

**Files:**
- Modify: `src/lib/tauri-api.ts:613` (add `getCollectionSummaries` wrapper)
- Modify: `src/components/collections/CollectionNode.tsx:40,141-145` (swap the fetch call)
- Modify: `src/components/collections/__tests__/CollectionNode.test.tsx` (swap the mocked function)

**Interfaces:**
- Consumes: the backend `get_collection_summaries` command (already exists and registered; no backend change in this task).
- Produces: `export const getCollectionSummaries = (name: string) => invoke<Collection>('get_collection_summaries', { name });` in `tauri-api.ts`. `CollectionNode`'s `refreshTree` now calls this instead of `getCollection`, so its render tree receives `type: 'summary'` items in production, exercising Task 3's rendering path for real.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Update the existing tests to expect `getCollectionSummaries`**

In `src/components/collections/__tests__/CollectionNode.test.tsx`, change the `vi.mock('@/lib/tauri-api', ...)` factory:

```tsx
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionSummaries: vi.fn(),
    // biome-ignore lint/suspicious/noEmptyBlockStatements: unlisten stub.
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});
```

In the `describe('CollectionNode git-changed refresh', ...)` block's `beforeEach` and its test body, replace every `tauriApi.getCollection` reference with `tauriApi.getCollectionSummaries`:

```tsx
describe('CollectionNode git-changed refresh', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue(emptyCollection);
    usePaneStore.setState({ activeCollection: summary.name });
  });

  it('refreshes its tree when a workspace-scoped branch switch fires git-changed, even though the repo path never matches this collection by name', async () => {
    renderNode();

    await waitFor(() => {
      expect(tauriApi.getCollectionSummaries).toHaveBeenCalledWith(summary.name);
    });
    const callsBeforeGitChanged = vi.mocked(tauriApi.getCollectionSummaries).mock.calls.length;

    const { listen } = await import('@tauri-apps/api/event');
    const gitChangedHandler = vi
      .mocked(listen)
      .mock.calls.find(([eventName]) => eventName === 'git-changed')?.[1];
    expect(gitChangedHandler).toBeDefined();

    gitChangedHandler?.({
      event: 'git-changed',
      id: 1,
      payload: { type: 'branchSwitched', collection: '/workspace', branch: 'feature-x' },
    });

    await waitFor(() => {
      expect(vi.mocked(tauriApi.getCollectionSummaries).mock.calls.length).toBeGreaterThan(
        callsBeforeGitChanged,
      );
    });
  });
});
```

Also update the `describe('CollectionNode summary item rendering', ...)` block added in Task 3 to mock `getCollectionSummaries` instead of `getCollection`:

```tsx
  beforeEach(() => {
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue(collectionWithSummaryAndOpaqueItems);
    usePaneStore.setState({ activeCollection: summary.name });
  });
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn vitest run src/components/collections/__tests__/CollectionNode.test.tsx`
Expected: FAIL — `tauriApi.getCollectionSummaries` is not a function yet, and `CollectionNode` still calls `getCollection`, so the mocked `getCollectionSummaries` is never invoked.

- [ ] **Step 4: Add the `getCollectionSummaries` wrapper**

In `src/lib/tauri-api.ts`, add immediately after `getCollection` (and after `getRequest` from Task 2, to keep the three collection-fetch wrappers grouped):

```ts
export const getCollection = (name: string) => invoke<Collection>('get_collection', { name });

export const getCollectionSummaries = (name: string) =>
  invoke<Collection>('get_collection_summaries', { name });

export const getRequest = (collection: string, path: string) =>
  invoke<Request>('get_request', { collection, path });
```

- [ ] **Step 5: Switch `CollectionNode`'s fetch**

In `src/components/collections/CollectionNode.tsx`, change the import at line 40:

```ts
import {
  getCollectionSummaries,
  onCollectionChanged,
  renameCollection,
  saveRequest,
} from '@/lib/tauri-api';
```

Change `refreshTree` at lines 141-145:

```ts
  const refreshTree = useCallback(() => {
    getCollectionSummaries(summary.name)
      .then(setCollection)
      .catch((err) => console.error('[CollectionNode] fetch error', err));
  }, [summary.name]);
```

Note: `CollectionsSidebar.tsx`'s own separate `getCollection` calls (`handleNewFolder` at line 153, `handleDuplicate` at line 188) are **not** touched — they were confirmed in this plan's investigation to need full data (`handleDuplicate` spreads the full request body/headers/auth into `saveRequest`) and are out of scope.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `yarn vitest run src/components/collections/__tests__/CollectionNode.test.tsx`
Expected: PASS.

- [ ] **Step 7: Type-check and lint**

Run: `yarn tsc --noEmit` then `yarn check`
Expected: no errors.

- [ ] **Step 8: Commit**

Stage `src/lib/tauri-api.ts`, `src/components/collections/CollectionNode.tsx`, and `src/components/collections/__tests__/CollectionNode.test.tsx`. Then invoke the `dev-workflow-skills:1-git-commit` skill to generate and create the commit for this task.

---

### Task 5: Full regression verification

**Files:** none (verification only).

**Interfaces:** none.

- [ ] **Step 1: Run the full Rust workspace test suite**

Run: `cargo test --workspace -j4`
Expected: all tests pass, including the new ones from Task 1.

- [ ] **Step 2: Run the full frontend test suite**

Run: `yarn test run`
Expected: all tests pass, including the new `RequestNode.test.tsx` and the updated `CollectionNode.test.tsx`.

- [ ] **Step 3: Type-check and lint the whole frontend**

Run: `yarn tsc --noEmit` then `yarn check`
Expected: no errors (this is the check that verifies the Review Focus item on `RequestNode.itemData`'s type narrowing end to end).

- [ ] **Step 4: Manual smoke test**

Run `yarn tauri dev`, open a collection in the sidebar with at least one request that has a non-empty body/headers/auth on disk, and verify:
- The request row appears in the sidebar (proves the summary fetch renders correctly).
- Clicking it opens a tab with the body/headers/auth populated (proves the on-demand `getRequest` fetch works).
- Opening the same request "in other pane" and "to the right"/"below" also populates full data.
- The sidebar's search filter still hides non-matching requests and still shows folders regardless of match.
- "Duplicate" on a request still produces a full copy (headers/body/auth intact) — proves `CollectionsSidebar.handleDuplicate` was correctly left on full `getCollection`.

- [ ] **Step 5: Final commit (if manual testing surfaced fixups)**

If Step 4 required any fixup changes, stage them and invoke the `dev-workflow-skills:1-git-commit` skill to generate and create the commit. If no fixups were needed, this step is a no-op — the branch is already fully committed from Tasks 1-4.
