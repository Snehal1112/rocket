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
└─────────────────────────────────┴──────────────────────┘
```

The two columns are separated by `border-r border-border`, same as `WorkspaceOverviewTab`.

## Left Column

- **No description field** — removed entirely (documentation panel replaces it)
- **MethodBreakdown** — rendered as-is, no changes to the component
- **Default Headers** — rendered as-is using `HeadersEditor` component + save button, wrapped in a `Card`
- **Requests** — rendered as-is using `RequestList` component (which already includes search), wrapped in a `Card`
- All three sections scroll together inside a `ScrollArea`

## Right Column — Documentation Panel

Pixel-identical to `WorkspaceOverviewTab`'s right column:

- `Card` wrapper with `flex-1 flex flex-col overflow-hidden`
- `CardHeader`: `FileText` icon + "Documentation" label on left; `Tabs` (Edit / Preview) on right
- `CardContent`:
  - **Edit mode:** `textarea` — monospace, `bg-transparent`, `border-none`, auto-saves on blur; footer with "Markdown supported · saves on blur" hint + Save button with loading/success states
  - **Preview mode:** `ReactMarkdown` with `remarkGfm`; empty state with FileText icon + "Add Documentation" button that switches to edit mode
- Uses existing `readme` state field (already persisted to disk via `saveCollectionSettings`)
- `isDocDirty` / `saveDocState` / `triggerSaveDoc` follow the same pattern as workspace

## Data

No backend changes. `readme` field on the collection settings already exists and is loaded/saved by `CollectionOverviewTab`. The Documentation panel reads/writes the same `readme` state.

## Constraints

- `description` field: remove from the overview section only (it remains in the save payload in case other code reads it — just stop rendering the textarea)
- `MethodBreakdown`, `HeadersEditor`, `RequestList` components: zero changes
- Outer container changes from `<ScrollArea className='flex-1'>` to `<div className='flex h-full overflow-hidden'>` with two child columns
- `max-w-3xl mx-auto` constraint on the left column content is removed — left column uses full available width with `p-5 flex flex-col gap-5`
- The standalone `readme` tab section remains unchanged — it still works as a full-page editor when the user clicks the Readme tab
