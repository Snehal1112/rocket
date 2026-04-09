# Variable Source Navigation Design

## Goal

When the user clicks "Variables in request →" in the `VariableAwareUrlInput` popover, navigate to the correct UI location for the variable's source type, rather than always routing to a fixed section of the request panel.

## Problem

The current link always navigates to either the `params` or `variables` section of `RequestPanel`. For variables sourced from collection environments, global environments, or collection-level variables, the correct destination is outside the request panel entirely (a modal dialog or a workspace/collection tab). The link text is also always the same regardless of destination.

## Approved Approach

Single `onNavigateToSource` callback on `VariableAwareUrlInput`. The input component is dumb — it passes the resolved source type up. `RequestPanel` holds all routing logic in one handler.

---

## Navigation Map

| Source | Link text | Action |
|--------|-----------|--------|
| `pathParam` | `Params →` | Switch RequestPanel to `params` section |
| `request` | `Request Variables →` | Switch RequestPanel to `variables` section |
| `runtime` | `Request Variables →` | Switch RequestPanel to `variables` section |
| `environment` | `Collection Environments →` | Open `EnvironmentDialog` (collection env modal) |
| `global` | `Global Environments →` | Open workspace tab → Environments section |
| `collection` | `Collection Variables →` | Open collection tab → Variables section |
| `folder` | — | Link hidden (no direct navigation available) |
| `process` | — | Link hidden (read-only system vars) |

---

## Interface Changes

### `VariableAwareUrlInput`

**File:** `src/components/request/VariableAwareUrlInput.tsx`

Replace:
```tsx
onSwitchToSection?: (section: 'params' | 'variables') => void;
```

With:
```tsx
onNavigateToSource?: (source: VariableSource | 'pathParam') => void;
```

The link label is computed from the source type using a `navLinkLabel` helper:

```tsx
function navLinkLabel(source: VariableSource | 'pathParam'): string | null {
  switch (source) {
    case 'pathParam': return 'Params →';
    case 'request':
    case 'runtime':   return 'Request Variables →';
    case 'environment': return 'Collection Environments →';
    case 'global':    return 'Global Environments →';
    case 'collection': return 'Collection Variables →';
    default:          return null; // folder, process — link hidden
  }
}
```

The link is rendered only when `navLinkLabel(source)` returns a non-null string and `onNavigateToSource` is provided.

The `navSection` local variable is removed. The click handler calls `onNavigateToSource(source)` where source is `'pathParam'` for path params, or `scopeEntry.source` for variable tokens. If `scopeEntry` is undefined (unresolved variable with no known source), the link is hidden.

### `RequestPanel`

**File:** `src/components/request/RequestPanel.tsx`

Adds local state and a `handleNavigateToSource` callback:

```tsx
const [envDialogOpen, setEnvDialogOpen] = useState(false);

const handleNavigateToSource = useCallback(
  (source: VariableSource | 'pathParam') => {
    switch (source) {
      case 'pathParam':
        setActiveSection('params');
        break;
      case 'request':
      case 'runtime':
        setActiveSection('variables');
        break;
      case 'environment':
        setEnvDialogOpen(true);
        break;
      case 'global': {
        const wsId = useWorkspaceStore.getState().activeWorkspaceId;
        if (wsId) usePaneStore.getState().openWorkspaceTabs(wsId, 'environments');
        break;
      }
      case 'collection': {
        const collection = usePaneStore.getState().activeCollection;
        if (collection) usePaneStore.getState().openCollectionTab(collection, 'variables');
        break;
      }
    }
  },
  [],
);
```

Renders `<EnvironmentDialog open={envDialogOpen} onOpenChange={setEnvDialogOpen} />` alongside the existing request panel content.

Passes `onNavigateToSource={handleNavigateToSource}` to `VariableAwareUrlInput`.

---

## Store Changes

### `pane-store` — `openWorkspaceTabs`

**File:** `src/stores/pane-store.ts`

Add optional `section?: WorkspaceTabSection` parameter. When provided, the opened/focused workspace tab's `activeSection` is set to that value.

```tsx
openWorkspaceTabs(workspaceId: string, section?: WorkspaceTabSection): void
```

### `pane-store` — `openCollectionTab`

Add a new `openCollectionTab(collection: string, section: CollectionSection): void` action (or extend the existing tab-open path used by `CollectionNode`). Opens or focuses the collection tab with the given `activeSection`.

---

## Files

| File | Action |
|------|--------|
| `src/components/request/VariableAwareUrlInput.tsx` | Replace `onSwitchToSection` with `onNavigateToSource`; add `navLinkLabel` helper; update link render |
| `src/components/request/RequestPanel.tsx` | Add `handleNavigateToSource`; add `envDialogOpen` state; render `EnvironmentDialog`; update prop |
| `src/stores/pane-store.ts` | Add `section?` param to `openWorkspaceTabs`; add `openCollectionTab` action |

No backend changes. No new Tauri commands.

---

## Out of Scope

- Folder variable navigation (FolderVariablesPopover requires a sidebar folder path not available in scopedContext)
- Process variable navigation (system env vars are read-only, no editing UI)
- Highlighting the specific variable row after navigating (navigates to the panel, not to the specific row)
