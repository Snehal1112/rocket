# Design Spec: UI Design Pattern Consistency Fix

**Date:** 2026-04-26
**Scope:** All form/settings panels across the application
**Reference pattern:** Authorization tab in `src/components/collections/CollectionOverviewTab.tsx`

---

## Goal

Standardise every form/settings panel in the app to use the same structural pattern, save UX, and label styling as the Authorization tab. No new abstractions. No changes outside the four targeted areas.

---

## Canonical Pattern

Every affected panel adopts this exact structure:

```
ScrollArea (h-full)
└── div.p-6.max-w-3xl.mx-auto.space-y-6
    └── Card
        └── CardContent.space-y-4.p-4
            ├── [optional section header: icon + uppercase label]
            ├── label.text-sm.font-medium.text-muted-foreground → field
            └── div.flex.justify-end
                └── Button size='sm' (Save → Loader2 → Check + text-green-600)
```

- Save state driven by the existing `useSaveButton` hook.
- Labels always: `text-sm font-medium text-muted-foreground`.
- Save button always inside the same `Card` as the fields it saves.
- `max-w-3xl` applies to full-page form panels only (not split-panel layouts).

---

## Files In Scope

### 1. `src/components/collections/CollectionOverviewTab.tsx` — Variables tab (lines 619–651)

**What changes:** Wrap the existing `div.space-y-4` content (CollectionVariablesEditor + save button) in `Card > CardContent.space-y-4.p-4`.

**What stays:** All logic, `useSaveButton` wiring, `isDirty` tracking, and save button are already correct. Pure structural markup change.

---

### 2. `src/components/request/RequestPanel.tsx` — Settings section (lines 938–1023)

**What changes:**
- Replace `div.rounded-md.border.bg-muted/20.p-3` groups with `Card > CardContent.p-4` for both Security and Connection groups.
- Keep icon + uppercase section header style inside `CardContent`.
- Wrap both cards in `ScrollArea > div.p-6.max-w-3xl.mx-auto.space-y-4`.

**What stays:** `handleSettingsChange` behaviour is unchanged — it calls `updateRequest` (pane store in-memory update only; request settings are not independently persisted to disk). No save button is added here — settings changes are part of the tab's in-memory state, consistent with how all other request fields (URL, headers, body) work.

**All existing settings fields** (SSL toggle, follow-redirects, timeout input) and their label markup are unchanged.

---

### 3. `src/components/workspace/WorkspaceEnvironmentsTab.tsx`

**What changes:**
- Remove debounce ref (`debounceRef`), `persistEnv` callback, `savedAt` state, and `SavedPill` import.
- Variable edits (`updateVar`, `addVar`, `removeVar`) update local state + set `isDirty = true`.
- Add `useSaveButton` with a `saveSettings` async function (calls `updateEnvironment` directly).
- Replace the `SavedPill` slot in the column header row with nothing.
- Add save button (Save → Loader2 → Check) right-aligned in the existing footer row (`px-3 py-2 border-t`).
- The right panel variable editor area gets a `Card` wrapper around the column-headers + `ScrollArea` + footer. `max-w-3xl` is NOT applied (split-panel layout fills available width).

**What stays:** Left sidebar (env list, add/delete/rename), column headers markup, variable row markup, empty-state illustration.

---

### 4. `src/components/environments/EnvironmentDialog.tsx`

**What changes:** Identical to `WorkspaceEnvironmentsTab` changes above — remove debounce/SavedPill, add `useSaveButton` + explicit save button in footer, wrap right panel variable area in `Card`.

**What stays:** `Dialog`/`DialogContent`/`DialogHeader` outer shell, left sidebar, column headers, variable row markup, empty-state.

---

## What Does NOT Change

| Component | Reason |
|---|---|
| `AuthEditor.tsx` | Already uses `Card > CardContent.p-4`; child component — parent provides wrapper |
| `WorkspaceOverviewTab.tsx` | Two-column content layout with MarkdownEditor; not a settings form |
| `RequestPanel.tsx` non-settings sections | Params, Headers, Body, Auth, Variables, Docs have appropriate layouts |
| Collection Auth tab | This is the reference; untouched |
| Environment panel left sidebars | Navigation panels, not forms |

---

## Save State Wiring Summary

| Component | Before | After |
|---|---|---|
| CollectionOverviewTab Variables | `useSaveButton` already wired | No change to logic |
| RequestPanel Settings | Immediate in-memory update, no save button | Structural Card wrapper added; no save button (settings are in-memory tab state, not disk-persisted) |
| WorkspaceEnvironmentsTab | 400ms debounce + SavedPill | `useSaveButton` + explicit save button |
| EnvironmentDialog | 500ms debounce + SavedPill | `useSaveButton` + explicit save button |

---

## Non-Goals

- No new shared components or abstractions.
- No changes to Rust backend or Tauri IPC commands.
- No changes to field-level components (`SingleLineEditor`, `Input`, `Checkbox`, `Select`).
- No changes to the `useSaveButton` hook itself.
- No changes to any component outside the four listed above.
