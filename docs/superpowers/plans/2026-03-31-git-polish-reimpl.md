# Git Polish & Collection Tabs Reimplementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reimplement the git polish work: collection readme & tags tabs, icon standardization, push/pull commit counts on buttons, ahead_behind fallback fix, clone dialog bugs, and cloned workspace activation.

**Architecture:** Six independent workstreams — Rust `CollectionSettings` readme field, two new React components (MarkdownEditor, TagsList), icon class normalization across 22 files, git landing panel button labels, `ahead_behind()` Rust fix with fallback to `refs/remotes/origin/<branch>`, and three clone dialog bug fixes.

**Tech Stack:** Rust (rocket-collection, rocket-git, rocket-infra, rocket-app), React 19 + TypeScript 5.8, Tailwind, Lucide icons, Zustand.

---

### Task 1: Add `readme` field to `CollectionSettings` (Rust)

**Files:**
- Modify: `crates/rocket-collection/src/settings.rs:22-38`
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs:453-486` (get_settings construction)
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs:560` (save_settings — persist readme)
- Modify: `crates/rocket-infra/src/oc_conversions.rs:1169-1200` (OC conversion)
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs:820-863` (tests with explicit fields)
- Modify: `crates/rocket-app/src/execution_service.rs:518-523` (test construction)

- [ ] **Step 1: Add `readme` field to `CollectionSettings`**

In `crates/rocket-collection/src/settings.rs`, add after the `description` field:

```rust
/// Optional markdown readme for this collection.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub readme: Option<String>,
```

- [ ] **Step 2: Fix all explicit `CollectionSettings` constructions in `fs_collection_repo.rs`**

In `crates/rocket-infra/src/fs_collection_repo.rs`, update the two `get_settings` construction sites.

At line ~453 (with request defaults), add `readme: None,` after `description: oc.docs,`:

```rust
Ok(CollectionSettings {
    description: oc.docs,
    readme: None,
    auth: defaults.auth.map(rocket_shared::types::Auth::from),
    // ... rest unchanged
})
```

At line ~483 (description-only fallback) — no change needed because it uses `..CollectionSettings::default()`.

Update the three test construction sites:

`settings_roundtrip` (~line 820):
```rust
let original = rocket_collection::CollectionSettings {
    description: None,
    readme: None,
    auth: Some(Auth::Bearer { token: "tok_abc".into() }),
    headers: vec![Header::new("X-Tenant", "acme")],
    variables: vec![],
};
```

`settings_file_not_counted_as_request` (~line 839):
```rust
let settings = rocket_collection::CollectionSettings {
    description: None,
    readme: None,
    auth: Some(Auth::None),
    headers: vec![],
    variables: vec![],
};
```

`settings_stored_in_opencollection_yml` (~line 858):
```rust
let settings = CollectionSettings {
    description: Some("My API docs".into()),
    readme: None,
    auth: Some(Auth::Bearer { token: "tok".into() }),
    headers: vec![Header::new("X-Tenant", "acme")],
    variables: vec![],
};
```

- [ ] **Step 3: Fix `oc_conversions.rs` construction site**

In `crates/rocket-infra/src/oc_conversions.rs` at line ~1170, add `readme: None,`:

```rust
CollectionSettings {
    description: oc.docs,
    readme: None,
    auth: defaults.auth.map(Auth::from),
    // ... rest unchanged
}
```

The `..CollectionSettings::default()` fallback at line ~1196 needs no change.

- [ ] **Step 4: Fix `execution_service.rs` test construction**

In `crates/rocket-app/src/execution_service.rs` at line ~518:

```rust
let settings = CollectionSettings {
    description: None,
    readme: None,
    auth: Some(Auth::Bearer { token: "col_tok".into() }),
    headers: vec![],
    variables: vec![],
};
```

- [ ] **Step 5: Persist readme in `save_settings`**

In `crates/rocket-infra/src/fs_collection_repo.rs`, the `save_settings` method already stores `oc.docs = settings.description.clone()` at line ~560. The `readme` field lives in `CollectionSettings` and serializes via serde — it will roundtrip through the Tauri IPC automatically. No OC YAML mapping needed since readme is collection-level metadata stored via the settings struct (which serializes to `opencollection.yml` indirectly through the existing `docs` field pattern).

Actually, we need to store it. Add after `oc.docs = settings.description.clone();` (~line 560):

We don't have an `OcCollection.readme` field, so store it in the extensions field or as a separate approach. Simpler: the readme is a standalone markdown string that goes through CollectionSettings serde. Since `save_settings` serializes to OcCollection YAML format, we need a place to put it.

Best approach: Store readme as a separate file `README.md` in the collection directory, not in `opencollection.yml`. This matches how real projects work.

Actually, let's keep it simple — store it directly on `CollectionSettings` which serializes with serde. The `save_settings` and `get_settings` methods convert between `CollectionSettings` and `OcCollection`. We'll store readme as a new top-level field on `OcCollection`.

Add to `crates/rocket-infra/src/opencollection.rs` struct `OcCollection` (after `docs` field ~line 941):

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub readme: Option<String>,
```

Then in `save_settings` after `oc.docs = settings.description.clone();`:

```rust
oc.readme = settings.readme.clone();
```

And in `get_settings` at line ~453 construction:

```rust
readme: oc.readme.clone(),
```

And at line ~483 (description-only):

```rust
Ok(CollectionSettings {
    description: oc.docs,
    readme: oc.readme,
    ..CollectionSettings::default()
})
```

And in `oc_conversions.rs` at ~1170:

```rust
readme: oc.readme.clone(),
```

And at ~1196:

```rust
CollectionSettings {
    description: oc.docs,
    readme: oc.readme,
    ..CollectionSettings::default()
}
```

- [ ] **Step 6: Run Rust checks**

Run: `cargo check -p rocket-collection && cargo check -p rocket-infra && cargo check -p rocket-app`
Expected: No errors.

- [ ] **Step 7: Run Rust tests**

Run: `cargo test -p rocket-infra -- settings && cargo test -p rocket-app`
Expected: All pass.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-collection/src/settings.rs crates/rocket-infra/src/fs_collection_repo.rs crates/rocket-infra/src/oc_conversions.rs crates/rocket-infra/src/opencollection.rs crates/rocket-app/src/execution_service.rs
git commit -m "$(cat <<'EOF'
feat(collection): add readme field to CollectionSettings

Add optional markdown readme to collection settings, persisted via
opencollection.yml. Fix all manual construction sites.
EOF
)"
```

---

### Task 2: Add `tags` to Request TypeScript interface and extend CollectionSection

**Files:**
- Modify: `src/lib/tauri-api.ts:82-91`
- Modify: `src/lib/tauri-api.ts:65-70`
- Modify: `src/types/pane-types.ts:34`

- [ ] **Step 1: Add `tags` to Request interface**

In `src/lib/tauri-api.ts`, update the `Request` interface (~line 82):

```typescript
export interface Request {
  uid: string;
  name: string;
  method: HttpMethod;
  url: string;
  headers: Header[];
  body?: Body;
  auth: Auth;
  fileName?: string;
  tags?: string[];
}
```

- [ ] **Step 2: Add `readme` to CollectionSettings interface**

In `src/lib/tauri-api.ts`, update `CollectionSettings` (~line 65):

```typescript
export interface CollectionSettings {
  description?: string;
  readme?: string;
  auth?: Auth;
  headers: Header[];
  variables: CollectionVariable[];
}
```

- [ ] **Step 3: Extend CollectionSection type**

In `src/types/pane-types.ts` (~line 34):

```typescript
export type CollectionSection = 'overview' | 'auth' | 'variables' | 'readme' | 'tags';
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/tauri-api.ts src/types/pane-types.ts
git commit -m "$(cat <<'EOF'
feat(frontend): add readme/tags to TypeScript interfaces
EOF
)"
```

---

### Task 3: Create MarkdownEditor component

**Files:**
- Create: `src/components/collections/MarkdownEditor.tsx`

- [ ] **Step 1: Install react-markdown**

Run: `yarn add react-markdown`

- [ ] **Step 2: Create MarkdownEditor component**

Create `src/components/collections/MarkdownEditor.tsx`:

```tsx
import { useState } from 'react';
import ReactMarkdown from 'react-markdown';
import { Textarea } from '@/components/ui/textarea';

interface MarkdownEditorProps {
  value: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
}

export function MarkdownEditor({ value, onChange, onBlur }: MarkdownEditorProps) {
  const [mode, setMode] = useState<'edit' | 'preview'>('preview');

  return (
    <div className="space-y-2">
      <div className="flex gap-1">
        <button
          type="button"
          className={`px-2 py-0.5 text-xs rounded ${
            mode === 'edit'
              ? 'bg-muted text-foreground'
              : 'text-muted-foreground hover:text-foreground'
          }`}
          onClick={() => setMode('edit')}
        >
          Edit
        </button>
        <button
          type="button"
          className={`px-2 py-0.5 text-xs rounded ${
            mode === 'preview'
              ? 'bg-muted text-foreground'
              : 'text-muted-foreground hover:text-foreground'
          }`}
          onClick={() => setMode('preview')}
        >
          Preview
        </button>
      </div>
      {mode === 'edit' ? (
        <Textarea
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onBlur={onBlur}
          placeholder="Write markdown here..."
          className="min-h-[200px] font-mono text-sm"
        />
      ) : (
        <div className="prose prose-sm dark:prose-invert max-w-none min-h-[200px] p-3 border rounded-md">
          {value ? (
            <ReactMarkdown>{value}</ReactMarkdown>
          ) : (
            <p className="text-muted-foreground text-sm italic">No readme yet.</p>
          )}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: No errors related to MarkdownEditor.

- [ ] **Step 4: Commit**

```bash
git add src/components/collections/MarkdownEditor.tsx package.json yarn.lock
git commit -m "$(cat <<'EOF'
feat(frontend): add MarkdownEditor component with edit/preview toggle
EOF
)"
```

---

### Task 4: Create TagsList component

**Files:**
- Create: `src/components/collections/TagsList.tsx`

- [ ] **Step 1: Create TagsList component**

Create `src/components/collections/TagsList.tsx`:

```tsx
import { Tag } from 'lucide-react';
import type { Collection, CollectionItem } from '@/lib/tauri-api';

interface TagCount {
  tag: string;
  count: number;
}

function collectTags(items: CollectionItem[]): Map<string, number> {
  const counts = new Map<string, number>();

  for (const item of items) {
    if (item.type === 'request' && item.tags) {
      for (const tag of item.tags) {
        counts.set(tag, (counts.get(tag) || 0) + 1);
      }
    }
    if (item.type === 'folder' && item.items) {
      const sub = collectTags(item.items);
      for (const [tag, count] of sub) {
        counts.set(tag, (counts.get(tag) || 0) + count);
      }
    }
  }

  return counts;
}

interface TagsListProps {
  collection: Collection | null;
}

export function TagsList({ collection }: TagsListProps) {
  if (!collection) return null;

  const tagMap = collectTags(collection.root.items);
  const tags: TagCount[] = Array.from(tagMap.entries())
    .map(([tag, count]) => ({ tag, count }))
    .sort((a, b) => b.count - a.count || a.tag.localeCompare(b.tag));

  if (tags.length === 0) {
    return (
      <p className="text-sm text-muted-foreground italic py-4 text-center">
        No tags found. Add tags to requests to see them here.
      </p>
    );
  }

  return (
    <div className="flex flex-wrap gap-2 py-2">
      {tags.map(({ tag, count }) => (
        <span
          key={tag}
          className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-muted text-sm"
        >
          <Tag className="h-3.5 w-3.5 text-muted-foreground" />
          {tag}
          <span className="text-xs text-muted-foreground">({count})</span>
        </span>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Check the CollectionItem type**

Verify that `CollectionItem` from `tauri-api.ts` has the correct shape. It should have `type`, `tags` (on request items), and `items` (on folder items). If the discriminated union doesn't include `tags`, update it:

In `src/lib/tauri-api.ts`, the `CollectionItem` type should support `tags` on request items. Check and add if missing — the Rust `Request` already has `tags: Vec<String>` so it serializes through IPC.

- [ ] **Step 3: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/collections/TagsList.tsx
git commit -m "$(cat <<'EOF'
feat(frontend): add TagsList component with recursive tag aggregation
EOF
)"
```

---

### Task 5: Wire Readme and Tags into CollectionOverviewTab

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx`

- [ ] **Step 1: Read current CollectionOverviewTab**

Read `src/components/collections/CollectionOverviewTab.tsx` in full to understand the TABS constant (line ~163) and the tab rendering structure.

- [ ] **Step 2: Add imports**

Add at the top of the file:

```typescript
import { MarkdownEditor } from '@/components/collections/MarkdownEditor';
import { TagsList } from '@/components/collections/TagsList';
```

- [ ] **Step 3: Extend TABS constant**

Find the `TABS` constant and add the two new tabs:

```typescript
const TABS = [
  { key: 'overview', label: 'Overview' },
  { key: 'auth', label: 'Authorization' },
  { key: 'variables', label: 'Variables' },
  { key: 'readme', label: 'Readme' },
  { key: 'tags', label: 'Tags' },
] as const;
```

- [ ] **Step 4: Add readme state**

Near the other state declarations (description, auth, headers, variables), add:

```typescript
const [readme, setReadme] = useState('');
```

In the `useEffect` that loads settings, add after `setDescription(s.description ?? '')`:

```typescript
setReadme(s.readme ?? '');
```

- [ ] **Step 5: Include readme in saveSettings**

In the `saveSettings` callback, add `readme: readme || undefined` to the settings object:

```typescript
await saveCollectionSettings(collectionName, {
  auth: authStateToApi(auth),
  headers: headers.filter((h) => h.key).map((h) => ({
    key: h.key,
    value: h.value,
    enabled: h.enabled,
  })),
  description: description || undefined,
  readme: readme || undefined,
  variables,
} as any);
```

- [ ] **Step 6: Add readme and tags tab content panels**

In the tab content rendering section, add cases for 'readme' and 'tags':

For readme tab content:

```tsx
{activeTab === 'readme' && (
  <div className="p-4 space-y-4">
    <MarkdownEditor
      value={readme}
      onChange={setReadme}
      onBlur={saveSettings}
    />
    <div className="flex justify-end">
      <Button size="sm" onClick={saveSettings}>Save</Button>
    </div>
  </div>
)}
```

For tags tab content:

```tsx
{activeTab === 'tags' && (
  <div className="p-4">
    <TagsList collection={collection} />
  </div>
)}
```

- [ ] **Step 7: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: No errors.

- [ ] **Step 8: Commit**

```bash
git add src/components/collections/CollectionOverviewTab.tsx
git commit -m "$(cat <<'EOF'
feat(frontend): wire readme and tags tabs into CollectionOverviewTab
EOF
)"
```

---

### Task 6: Fix `ahead_behind` to fall back to `refs/remotes/origin/<branch>`

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs:168-200`

- [ ] **Step 1: Update the `ahead_behind` function**

Replace the `ahead_behind` function at line ~168 with:

```rust
/// Compute how many commits the local branch is ahead/behind the upstream.
fn ahead_behind(repo: &Repository) -> (usize, usize) {
    let head = match repo.head() {
        Ok(r) => r,
        Err(_) => return (0, 0),
    };

    let local_oid = match head.target() {
        Some(oid) => oid,
        None => return (0, 0),
    };

    let branch_name = head.shorthand().unwrap_or("main");

    let branch = match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(b) => b,
        Err(_) => return (0, 0),
    };

    // Try the configured upstream first.
    let upstream_oid = branch
        .upstream()
        .ok()
        .and_then(|u| u.get().target())
        // Fall back to refs/remotes/origin/<branch> when no upstream is configured.
        .or_else(|| {
            let refname = format!("refs/remotes/origin/{}", branch_name);
            repo.find_reference(&refname).ok().and_then(|r| r.target())
        });

    match upstream_oid {
        Some(oid) => repo.graph_ahead_behind(local_oid, oid).unwrap_or((0, 0)),
        None => (0, 0),
    }
}
```

- [ ] **Step 2: Add test for ahead_behind with remote ref fallback**

Add at the bottom of the `#[cfg(test)]` module (before the closing `}`):

```rust
#[test]
fn status_ahead_behind_with_remote() {
    let (_dir, path) = setup_repo();
    let repo = Repository::open(&path).unwrap();
    let sig = git2::Signature::now("Test", "test@test.com").unwrap();

    // Create a bare remote to push to.
    let remote_dir = TempDir::new().unwrap();
    let remote_path = remote_dir.path().to_string_lossy().to_string();
    Repository::init_bare(&remote_path).unwrap();

    // Add the bare repo as "origin" and push main.
    let mut remote = repo.remote("origin", &remote_path).unwrap();
    remote
        .push(&["refs/heads/main:refs/heads/main"], None)
        .unwrap();

    // Make one more local commit (ahead by 1).
    fs::write(Path::new(&path).join("extra.txt"), "extra").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("extra.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(
        Some("refs/heads/main"),
        &sig,
        &sig,
        "second",
        &tree,
        &[&head_commit],
    )
    .unwrap();

    // No upstream tracking configured — falls back to refs/remotes/origin/main.
    let svc = Git2Service::new();
    let status = svc.status(&path).unwrap();
    assert_eq!(status.ahead, 1, "should be 1 commit ahead");
    assert_eq!(status.behind, 0, "should be 0 commits behind");
}
```

- [ ] **Step 3: Run the test**

Run: `cargo test -p rocket-git status_ahead_behind_with_remote`
Expected: PASS.

- [ ] **Step 4: Run all git tests**

Run: `cargo test -p rocket-git`
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "$(cat <<'EOF'
fix(git): ahead_behind falls back to refs/remotes/origin/<branch>

When no upstream tracking is configured, look up the remote ref
directly so push/pull counts work after clone or manual remote add.
EOF
)"
```

---

### Task 7: Show commit counts on Push/Pull buttons

**Files:**
- Modify: `src/components/git/GitLandingPanel.tsx:154-169`

- [ ] **Step 1: Update Pull button text**

In `src/components/git/GitLandingPanel.tsx`, find the Pull button (~line 154). Change the button label from `Pull` to show the count:

```tsx
<Button variant="outline" size="sm" onClick={handlePull} disabled={pulling}>
  {pulling ? (
    <Loader2 className="h-3.5 w-3.5 animate-spin" />
  ) : (
    <ArrowDown className="h-3.5 w-3.5" />
  )}
  Pull{behind > 0 ? ` ↓${behind}` : ''}
</Button>
```

- [ ] **Step 2: Update Push button text**

Find the Push button (~line 162). Change the button label:

```tsx
<Button variant="outline" size="sm" onClick={handlePush} disabled={pushing}>
  {pushing ? (
    <Loader2 className="h-3.5 w-3.5 animate-spin" />
  ) : (
    <ArrowUp className="h-3.5 w-3.5" />
  )}
  Push{ahead > 0 ? ` ↑${ahead}` : ''}
</Button>
```

- [ ] **Step 3: Commit**

```bash
git add src/components/git/GitLandingPanel.tsx
git commit -m "$(cat <<'EOF'
feat(frontend): show commit counts on Push/Pull buttons
EOF
)"
```

---

### Task 8: Fix Clone Dialog bugs (3 fixes)

**Files:**
- Modify: `src/components/git/GitPanel.tsx:73-100` (clone dialog in non-repo block)
- Modify: `src/components/git/GitCloneDialog.tsx:84-91` (switchWorkspace after open)

- [ ] **Step 1: Move GitCloneDialog and GitCredentialsDialog into the non-repo block**

The problem: When `isRepo` is false, the component returns early (line 73-99) so the dialogs rendered at lines 164-166 are never reached.

In `src/components/git/GitPanel.tsx`, update the non-repo early-return block to include the dialogs:

```tsx
if (!isRepo) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 h-full px-4 text-center">
      <p className="text-sm text-muted-foreground">
        This collection is not a Git repository.
      </p>
      <div className="flex gap-2">
        <Button
          variant="outline"
          size="sm"
          onClick={async () => {
            await gitInit(collectionPath);
            await checkAndLoad(collectionPath);
          }}
        >
          Initialize Git
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setShowCloneDialog(true)}
        >
          Clone Repository
        </Button>
      </div>
      {showCredentialsDialog && <GitCredentialsDialog />}
      <GitCloneDialog open={showCloneDialog} onOpenChange={setShowCloneDialog} />
    </div>
  );
}
```

- [ ] **Step 2: Fix `handleOpen` in GitCloneDialog to call `switchWorkspace`**

In `src/components/git/GitCloneDialog.tsx`, update `handleOpen` (~line 84):

```tsx
const handleOpen = async (collectionPath: string) => {
  try {
    await useWorkspaceStore.getState().openWorkspaceFromDisk(collectionPath);
    // Refresh workspace list to pick up the new workspace.
    const store = useWorkspaceStore.getState();
    await store.loadWorkspaces();
    // Find the newly opened workspace and switch to it.
    const workspaces = useWorkspaceStore.getState().workspaces;
    const newWs = workspaces.find((w) => w.path === collectionPath);
    if (newWs) {
      await useWorkspaceStore.getState().switchWorkspace(newWs.id);
    }
    onOpenChange(false);
  } catch (e) {
    setError(String(e));
  }
};
```

Wait — `openWorkspaceFromDisk` returns `invoke<Workspace>`. But the store wrapper discards it. Let's fix the store to return the workspace.

- [ ] **Step 3: Update `openWorkspaceFromDisk` in workspace-store to return Workspace**

In `src/stores/workspace-store.ts` at line 72, update to capture and return:

```typescript
openWorkspaceFromDisk: async (path) => {
  const ws = await apiOpenFromDisk(path);
  return ws;
},
```

Also update the interface at line 36:

```typescript
openWorkspaceFromDisk: (path: string) => Promise<Workspace>;
```

Wait, the current interface says `Promise<void>`. Update to `Promise<Workspace>`:

In `src/stores/workspace-store.ts`, change the interface (~line 36):

```typescript
openWorkspaceFromDisk: (path: string) => Promise<Workspace>;
```

And the implementation (~line 72):

```typescript
openWorkspaceFromDisk: async (path) => {
  const ws = await apiOpenFromDisk(path);
  return ws;
},
```

- [ ] **Step 4: Simplify GitCloneDialog handleOpen using returned workspace**

In `src/components/git/GitCloneDialog.tsx`, update `handleOpen`. The `openWorkspaceFromDisk` Tauri command internally fires a `workspace-created` event (which adds it to the store) and then we switch to it:

```tsx
const handleOpen = async (collectionPath: string) => {
  try {
    const ws = await useWorkspaceStore.getState().openWorkspaceFromDisk(collectionPath);
    await useWorkspaceStore.getState().switchWorkspace(ws.id);
    onOpenChange(false);
  } catch (e) {
    setError(String(e));
  }
};
```

- [ ] **Step 5: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: No errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/git/GitPanel.tsx src/components/git/GitCloneDialog.tsx src/stores/workspace-store.ts
git commit -m "$(cat <<'EOF'
fix(frontend): clone dialog bugs — dialogs in non-repo block, workspace activation

- Move GitCloneDialog and GitCredentialsDialog into the non-repo
  early-return so they render when no repo is initialized.
- openWorkspaceFromDisk now returns the Workspace object.
- After clone, call switchWorkspace to activate the new workspace.
EOF
)"
```

---

### Task 9: Icon Standardization (22 files)

**Files:**
- Modify: `src/components/panes/TabItem.tsx`
- Modify: `src/components/panes/TabBar.tsx`
- Modify: `src/components/collections/CollectionNode.tsx`
- Modify: `src/components/collections/FolderNode.tsx`
- Modify: `src/components/collections/RequestNode.tsx`
- Modify: `src/components/collections/CollectionVariablesEditor.tsx`
- Modify: `src/components/layout/CollectionsSidebar.tsx`
- Modify: `src/components/layout/ConsolePanel.tsx`
- Modify: `src/components/layout/EnvironmentSwitcher.tsx`
- Modify: `src/components/layout/GitToolbarButton.tsx`
- Modify: `src/components/layout/StatusBar.tsx`
- Modify: `src/components/request/AuthEditor.tsx`
- Modify: `src/components/request/KeyValueEditor.tsx`
- Modify: `src/components/response/ResponseBodyViewer.tsx`
- Modify: `src/components/response/ResponseHeadersTable.tsx`
- Modify: `src/components/workspace/WorkspaceEnvironmentsTab.tsx`
- Modify: `src/components/workspace/WorkspaceOverviewTab.tsx`
- Modify: `src/components/workspace/CreateWorkspaceDialog.tsx`
- Modify: `src/components/environments/EnvironmentDialog.tsx`

**Rules:**
- All navigation/action/structural icons: `h-3.5 w-3.5`
- All structural icons: `text-muted-foreground`
- Exceptions: Keep `text-primary` on intentional accent icons (Layers collection icon), keep `text-emerald-500` on success Check icons, keep `text-amber-500` on warning icons
- Context menus MoreHorizontal dots stay `h-3 w-3` (they're smaller by design)
- The large decorative GitBranch (`h-12 w-12`) stays as-is
- "Add row" Plus icons in tables stay `h-3 w-3 mr-1` (smaller inline context)

- [ ] **Step 1: TabItem.tsx**

Update icon classNames:
- `GitBranch` (git tab icon, line ~47): `h-3 w-3` → `h-3.5 w-3.5` (keep `shrink-0 text-muted-foreground`)
- `Folder` (line ~55): `h-3 w-3` → `h-3.5 w-3.5` (keep `shrink-0 text-primary`)
- `X` close button (line ~71): `h-3 w-3` → `h-3.5 w-3.5`

- [ ] **Step 2: TabBar.tsx**

Update:
- `PanelRight` (line ~106): `size-4 mr-2` → `h-3.5 w-3.5 mr-2`
- `PanelBottom` (line ~109): `size-4 mr-2` → `h-3.5 w-3.5 mr-2`

- [ ] **Step 3: CollectionNode.tsx**

Update:
- `ChevronDown/ChevronRight` (lines ~229, ~234): `h-4 w-4` → `h-3.5 w-3.5` (keep `flex-none text-muted-foreground`)

- [ ] **Step 4: FolderNode.tsx**

Update:
- `FolderOpen/Folder` (line ~100): `h-4 w-4` → `h-3.5 w-3.5` (keep `shrink-0 text-muted-foreground`)

- [ ] **Step 5: CollectionsSidebar.tsx**

Update:
- `LayoutDashboard` (line ~36 usage): `h-4 w-4` → `h-3.5 w-3.5` (keep `shrink-0 text-muted-foreground mr-2`)

- [ ] **Step 6: GitToolbarButton.tsx**

Update:
- `GitBranch` (line ~34): `h-4 w-4` → `h-3.5 w-3.5 text-muted-foreground`

- [ ] **Step 7: StatusBar.tsx**

Update:
- `Terminal` (line ~23): `h-3 w-3` → `h-3.5 w-3.5 text-muted-foreground`

- [ ] **Step 8: AuthEditor.tsx**

Update:
- `User` (line ~221): `h-4 w-4 text-muted-foreground` → `h-3.5 w-3.5 text-muted-foreground`
- `Lock` (line ~235): `h-4 w-4 text-muted-foreground` → `h-3.5 w-3.5 text-muted-foreground`
- `Key` (line ~255): `h-4 w-4 text-muted-foreground` → `h-3.5 w-3.5 text-muted-foreground`
- `ChevronDown/ChevronRight` (lines ~410, ~412): `h-3 w-3` → `h-3.5 w-3.5`

- [ ] **Step 9: KeyValueEditor.tsx**

Update:
- `X` (line ~75): `h-4 w-4` → `h-3.5 w-3.5`

- [ ] **Step 10: ResponseBodyViewer.tsx**

Update:
- `Copy` (line ~174): `h-3 w-3` → `h-3.5 w-3.5`
- `Check` (line ~172): `h-3 w-3 text-emerald-500` → `h-3.5 w-3.5 text-emerald-500`
- `Clock` (line ~128): `h-3 w-3` → `h-3.5 w-3.5 text-muted-foreground`
- `FileText` (line ~135): `h-3 w-3` → `h-3.5 w-3.5 text-muted-foreground`

- [ ] **Step 11: ResponseHeadersTable.tsx**

Update:
- `Copy` (line ~91): `h-3 w-3` → `h-3.5 w-3.5`
- `Check` (line ~89): `h-3 w-3 text-emerald-500` → `h-3.5 w-3.5 text-emerald-500`

- [ ] **Step 12: WorkspaceEnvironmentsTab.tsx**

Update:
- `Check` (line ~216): `h-3 w-3` → `h-3.5 w-3.5`
- `Eye` (line ~247): `h-3 w-3` → `h-3.5 w-3.5 text-muted-foreground`
- `EyeOff` (line ~245): `h-3 w-3` → `h-3.5 w-3.5 text-muted-foreground`
- `X` (line ~259): `h-3 w-3` → `h-3.5 w-3.5`

- [ ] **Step 13: WorkspaceOverviewTab.tsx**

Update:
- `Plus` (line ~121): `mr-2 h-4 w-4` → `h-3.5 w-3.5 mr-2`

- [ ] **Step 14: CreateWorkspaceDialog.tsx**

Update:
- `FolderOpen` (line ~107): `h-4 w-4 mr-1.5` → `h-3.5 w-3.5 mr-1.5`

- [ ] **Step 15: EnvironmentDialog.tsx**

Update:
- `EyeOff` (line ~201): `h-3 w-3` → `h-3.5 w-3.5 text-muted-foreground`
- `Eye` (line ~203): `h-3 w-3` → `h-3.5 w-3.5 text-muted-foreground`
- `X` (line ~212): `h-3 w-3` → `h-3.5 w-3.5`

- [ ] **Step 16: ConsolePanel.tsx**

Update:
- `ChevronDown/ChevronRight` (lines ~127, ~128): keep `h-3 w-3` (inline tree indicator — acceptable at this size)
- `Trash2` (line ~108): `h-3 w-3 mr-1` → `h-3.5 w-3.5 mr-1 text-muted-foreground`

- [ ] **Step 17: EnvironmentSwitcher.tsx**

Update:
- `ChevronDown` (line ~45): `h-3 w-3 text-muted-foreground` → `h-3.5 w-3.5 text-muted-foreground`

- [ ] **Step 18: Verify frontend builds**

Run: `yarn tsc --noEmit`
Expected: No errors.

- [ ] **Step 19: Commit**

```bash
git add src/components/panes/TabItem.tsx src/components/panes/TabBar.tsx \
  src/components/collections/CollectionNode.tsx src/components/collections/FolderNode.tsx \
  src/components/collections/RequestNode.tsx src/components/collections/CollectionVariablesEditor.tsx \
  src/components/layout/CollectionsSidebar.tsx src/components/layout/ConsolePanel.tsx \
  src/components/layout/EnvironmentSwitcher.tsx src/components/layout/GitToolbarButton.tsx \
  src/components/layout/StatusBar.tsx src/components/request/AuthEditor.tsx \
  src/components/request/KeyValueEditor.tsx src/components/response/ResponseBodyViewer.tsx \
  src/components/response/ResponseHeadersTable.tsx src/components/workspace/WorkspaceEnvironmentsTab.tsx \
  src/components/workspace/WorkspaceOverviewTab.tsx src/components/workspace/CreateWorkspaceDialog.tsx \
  src/components/environments/EnvironmentDialog.tsx
git commit -m "$(cat <<'EOF'
style(frontend): standardize icons to h-3.5 w-3.5 and text-muted-foreground
EOF
)"
```

---

## Dependency Graph

```
Task 1 (Rust readme) ──→ Task 2 (TS interfaces) ──→ Task 5 (wire into overview tab)
                                                  ╲
Task 3 (MarkdownEditor) ──────────────────────────→ Task 5
Task 4 (TagsList) ─────────────────────────────────→ Task 5

Task 6 (ahead_behind fix) ─── independent
Task 7 (push/pull counts) ─── independent (but benefits from Task 6)
Task 8 (clone bugs) ────────── independent
Task 9 (icon standardization) ─ independent
```

**Parallel groups:**
- Group A: Tasks 1, 3, 4, 6, 7, 8, 9 (all independent)
- Group B: Task 2 (depends on Task 1)
- Group C: Task 5 (depends on Tasks 2, 3, 4)
