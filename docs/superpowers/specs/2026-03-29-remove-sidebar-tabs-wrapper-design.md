# Design: Remove Redundant Tabs Wrapper from CollectionsSidebar

**Date:** 2026-03-29
**Status:** Approved

## Problem

After removing the Git sidebar tab (plan-02), the `<Tabs>` component in `CollectionsSidebar.tsx` wraps a single "Collections" tab. The tab header is now visual noise — there is nothing to switch between.

## Scope

One file, ~10 lines removed:

- `src/components/layout/CollectionsSidebar.tsx`

## Design

Remove the `<Tabs>`, `<TabsList>`, `<TabsTrigger>`, and `<TabsContent>` wrapper elements. The content inside (Select dropdown for Collections/History view, search bar, collection tree, history panel, delete dialog) is preserved as-is, directly inside the existing outer `<div>`.

Remove the `Tabs`/`TabsList`/`TabsTrigger`/`TabsContent` imports since they are no longer used.

### Before

```
<div className="h-full flex flex-col ...">
  <Tabs>
    <TabsList>
      <TabsTrigger value="collections">Collections</TabsTrigger>
    </TabsList>
    <TabsContent value="collections" className="flex-1 flex flex-col ...">
      ... sidebar content ...
    </TabsContent>
  </Tabs>
  <AlertDialog>...</AlertDialog>
</div>
```

### After

```
<div className="h-full flex flex-col ...">
  <div className="flex-1 flex flex-col overflow-hidden">
    ... sidebar content ...
  </div>
  <AlertDialog>...</AlertDialog>
</div>
```

The `flex-1 flex flex-col overflow-hidden` classes from `TabsContent` transfer to a plain `<div>` to preserve the same layout behavior.

## Testing

- `yarn tsc --noEmit` must pass.
- Visual check: sidebar renders identically, minus the "Collections" tab header bar.
