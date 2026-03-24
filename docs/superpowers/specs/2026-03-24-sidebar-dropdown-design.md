# Sidebar Dropdown Redesign — Design Spec

**Date:** 2026-03-24
**Status:** Approved

## Problem

The Collections/History toggle in the sidebar uses shadcn Tabs which look like navigation tabs rather than a view selector. The "New Collection" button is buried below the search bar. There is no import button.

## Changes

All changes confined to `src/components/layout/CollectionsSidebar.tsx`. No new files.

### 1. Replace Tabs with shadcn Select

Remove `Tabs`, `TabsList`, `TabsTrigger`, `TabsContent` usage. Add a `view` state:

```tsx
const [view, setView] = useState<'collections' | 'history'>('collections');
```

Use shadcn `Select` component:

```tsx
<Select value={view} onValueChange={(v) => setView(v as 'collections' | 'history')}>
  <SelectTrigger className="h-8 flex-1 text-xs">
    <SelectValue />
  </SelectTrigger>
  <SelectContent>
    <SelectItem value="collections">Collections</SelectItem>
    <SelectItem value="history">History</SelectItem>
  </SelectContent>
</Select>
```

### 2. Add action icons to the right of the dropdown

A horizontal bar at the top of the sidebar:

```tsx
<div className="flex items-center gap-1 px-2 pt-2 pb-1">
  <Select ...>...</Select>
  {view === 'collections' && (
    <>
      <Button variant="ghost" size="icon" className="h-8 w-8 shrink-0" onClick={() => setIsCreating(true)} title="New Collection">
        <Plus className="h-4 w-4" />
      </Button>
      <Button variant="ghost" size="icon" className="h-8 w-8 shrink-0" onClick={handleImport} title="Import Collection">
        <Upload className="h-4 w-4" />
      </Button>
    </>
  )}
</div>
```

Icons are only visible when "Collections" is selected.

### 3. Import button behavior

On click, opens a Tauri file picker for `.json` files:

```tsx
import { open } from '@tauri-apps/plugin-dialog';

async function handleImport() {
  const file = await open({
    multiple: false,
    filters: [{ name: 'Collection', extensions: ['json'] }],
  });
  if (file) {
    console.log('Import file selected:', file);
  }
}
```

The actual import parsing is out of scope. The file picker is wired up and logs the path.

### 4. Remove "+ New Collection" button from below search

The `+ New Collection` ghost button below the search bar is removed. Its functionality moves to the `+` icon button in the top bar. The inline creation input still appears below the search bar when `isCreating` is true.

### 5. Conditional rendering replaces TabsContent

```tsx
{view === 'collections' ? (
  <div className="flex-1 flex flex-col overflow-hidden">
    {/* Search bar + collection tree */}
  </div>
) : (
  <div className="flex-1 overflow-hidden">
    <HistoryPanel />
  </div>
)}
```

## Imports to add

- `Select, SelectContent, SelectItem, SelectTrigger, SelectValue` from `@/components/ui/select`
- `Upload` from `lucide-react`
- `open` from `@tauri-apps/plugin-dialog`

## Imports to remove

- `Tabs, TabsList, TabsTrigger, TabsContent` from `@/components/ui/tabs`

## Files

- Modify: `src/components/layout/CollectionsSidebar.tsx`
