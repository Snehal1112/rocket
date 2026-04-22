# Collection Overview Tab — Two-Column Layout

## Goal

Refactor the Collection Overview tab's `overview` section into a two-column layout matching the Workspace Overview pattern: left column for primary content (method breakdown, default headers, requests), right column for a Documentation panel identical to the Workspace Overview's documentation card.

## Layout

```
┌─────────────────────────────────┬──────────────────────┐
│  LEFT (flex-1, scrollable)      │  RIGHT (320px fixed)  │
│                                 │                       │
│  MethodBreakdown card           │  Documentation card   │
│  Default Headers card           │  (Edit / Preview)     │
│  Requests card (with search)    │                       │
│  Tags card (read-only)          │                       │
└─────────────────────────────────┴──────────────────────┘
```

The two columns are separated by `border-r border-border`, same as `WorkspaceOverviewTab`.

## Left Column

- **No description field** — removed entirely (Documentation panel replaces it)
- **MethodBreakdown** — rendered as-is, no changes to the component
- **Default Headers** — rendered as-is using `HeadersEditor` component + save button, wrapped in a `Card`
- **Requests** — rendered as-is using `RequestList` component (which already includes search), wrapped in a `Card`
- **Tags** — read-only tag list moved from the old Tags tab into a `Card` here, using the existing `TagsList` component
- All four sections scroll together inside a `ScrollArea`

## Right Column — Documentation Panel

Identical to `WorkspaceOverviewTab`'s right column, implemented via the upgraded `MarkdownEditor` component:

- `Card` wrapper with `flex-1 flex flex-col overflow-hidden`
- `CardHeader`: `FileText` icon + "Documentation" label on left; `Tabs` (Edit / Preview) on right
- `CardContent`:
  - **Edit mode:** `textarea` — monospace, `bg-transparent`, `border-none`, auto-saves on blur; footer with "Markdown supported · saves on blur" hint + Save button with loading/success states
  - **Preview mode:** `ReactMarkdown` with `remarkGfm`; empty state with FileText icon + "Add Documentation" button that switches to edit mode
- Uses existing `readme` state field (already persisted to disk via `saveCollectionSettings`)
- `isDocDirty` / `saveDocState` / `triggerSaveDoc` follow the same pattern as workspace

## Tabs — Removed and Replaced

- **Readme tab** — removed. The Documentation panel in the right column serves this purpose using the same `readme` field.
- **Tags tab** — removed. Tags moved into the left column as a read-only card.
- **New `documentation` section** — added to `CollectionSection` type and `TABS` array as a dedicated tab that renders a full-page `MarkdownEditor` (same as the old Readme tab, same `readme` field). This gives users a full-screen editing experience when they need it.

## CollectionSection Type

Old: `'overview' | 'auth' | 'variables' | 'readme' | 'tags'`
New: `'overview' | 'auth' | 'variables' | 'documentation'`

The `validSections` guard in `CollectionOverviewTab` already falls back to `'overview'` for unknown stored values, so users with `'readme'` or `'tags'` persisted will land on Overview automatically — no migration needed.

## Data

No backend changes. `readme` field on the collection settings already exists and is loaded/saved by `CollectionOverviewTab`. Both the Overview Documentation panel and the Documentation tab read/write the same `readme` state.

## Component Reuse

`MarkdownEditor` (`src/components/collections/MarkdownEditor.tsx`) is upgraded to the Documentation card style (Card wrapper, shadcn Tabs, monospace textarea, save-on-blur footer, empty state). It replaces:
- The inline Documentation block in `WorkspaceOverviewTab` (~90 lines removed)
- The right-column Documentation panel in `CollectionOverviewTab` overview section
- The full-page Documentation tab in `CollectionOverviewTab`

## Constraints

- `description` field: remove from the overview section only (it remains in the save payload in case other code reads it — just stop rendering the textarea)
- `MethodBreakdown`, `HeadersEditor`, `RequestList`, `TagsList` components: zero changes
- Outer container changes from `<ScrollArea className='flex-1'>` to `<div className='flex h-full overflow-hidden'>` with two child columns
- `max-w-3xl mx-auto` constraint on the left column content is removed — left column uses full available width with `p-5 flex flex-col gap-5`
