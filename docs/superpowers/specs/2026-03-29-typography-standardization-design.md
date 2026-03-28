# Design: Typography Standardization (Bruno-style)

**Date:** 2026-03-29
**Status:** Draft

## Problem

The app's typography is inconsistent across components. `text-xs` (12px) is used 494 times and dominates the UI, while `text-sm`/`text-base`/`text-lg` are underused. The same type of UI element (e.g., HTTP method badges, section headers, form labels) uses different sizes and weights in different components. This makes the app feel typographically flat and unprofessional compared to Bruno.

## Goal

Standardize font sizes and weights across all components to create a clean, consistent, Bruno-like visual hierarchy. Keep the existing Inter + JetBrains Mono font families (already correct).

## Typography Scale

Define a 5-tier hierarchy with clear rules for when each is used:

| Token | Size | Use case |
|---|---|---|
| `text-2xs` | 10px (0.625rem) | Micro badges, keyboard shortcuts, status counts |
| `text-xs` | 12px | Secondary labels, metadata, muted helper text |
| `text-sm` | 14px | **Default body text**, form labels, button labels, input text, tree items |
| `text-base` | 16px | Section headers, dialog titles |
| `text-lg` | 18px | Page-level headings (rare) |

### Key shift

The app currently treats `text-xs` as the default. **The new default is `text-sm` (14px).** This is the single biggest change — most form labels, input fields, tree items, and body text move from 12px to 14px.

## Font Weight Rules

| Weight | Token | Use case |
|---|---|---|
| 400 | `font-normal` | Body text, input values, descriptions |
| 500 | `font-medium` | Form labels, button text, active/selected states, tab labels |
| 600 | `font-semibold` | Section headers, dialog titles, emphasis |

`font-bold` (700) is removed from UI text. It is too heavy for a compact app and is only appropriate for marketing content.

## Component-Specific Changes

### Sidebar (CollectionsSidebar, CollectionNode, RequestNode)
- Tree item names: `text-sm` (was `text-xs`)
- Search input: `text-sm` (was `text-xs h-7`)
- Section labels (Collections/History): `text-xs font-medium uppercase tracking-wider` (stays)
- HTTP method in tree: `text-2xs font-semibold` (normalize — was mixed `font-bold`/`font-semibold`)

### Tab Bar (TabItem)
- Tab label: `text-sm` (was `text-xs`)
- HTTP method badge in tab: `text-2xs font-semibold` (normalize)

### WorkspaceToolbar (CollectionDropdown, SandboxPopover, WorkspaceToolbar)
- Dropdown button text: `text-sm` (was `text-xs`)
- Popover section headers: `text-xs font-medium uppercase tracking-wider` (stays — these are micro-labels)
- Collection list items: `text-sm` (was `text-sm`, stays)
- Popover descriptions: `text-xs` (stays — secondary text)

### Request Panel (RequestPanel, AuthEditor, headers/params editors)
- URL input: `text-sm` (stays)
- Tab triggers (Params, Headers, Body, Auth): `text-sm` (was `text-xs`)
- Form labels: `text-sm font-medium` (was `text-xs font-medium`)
- Form inputs: `text-sm` (was `text-xs`)
- Param count badges: `text-2xs` (stays)
- Error messages: `text-xs text-destructive` (was `text-2xs`)

### Response Panel (ResponseBodyViewer, ResponseHeadersTable)
- Tab triggers (Body, Headers, Cookies): `text-sm` (was `text-xs`)
- Table headers: `text-xs font-semibold` (stays — compact table context)
- Table values: `text-xs font-mono` (stays)
- Status badge: `text-sm font-semibold` (was `text-xs`)

### Git Components (GitTab, BranchSelector, GitCommitForm, etc.)
- Sub-tab triggers (Changes, Log, Stash): `text-sm` (was `text-xs`)
- Changed file count: `text-xs` (stays)
- Commit form labels: `text-sm font-medium` (was `text-xs`)
- File list items: `text-sm` (was `text-xs`)

### Dialogs (EnvironmentDialog, GitCloneDialog, CreateWorkspaceDialog, etc.)
- Dialog title: `text-base font-semibold` (standardize)
- Dialog description: `text-sm` (stays)
- Form labels inside dialogs: `text-sm font-medium` (was `text-xs`)
- Form inputs inside dialogs: `text-sm` (was `text-xs`)

### Status Bar
- Console button: `text-xs` (was `text-2xs`)
- Entry count badge: `text-2xs` (stays)

### Console Panel
- Log entries: `text-xs font-mono` (stays — dense log context)
- Timestamp: `text-2xs` (stays)

### History Panel
- List items: `text-sm` (was `text-xs`)
- Timestamp/metadata: `text-xs` (was `text-xs`, stays)

## What Does NOT Change

- Font families: Inter and JetBrains Mono stay.
- Monaco editor font size (13px) stays.
- The `text-2xs` custom token stays (10px for micro elements).
- Color system stays unchanged.
- Spacing/padding stays unchanged (font size change alone gives enough visual lift).

## Scope

~30 component files need class name updates. No new dependencies, no structural changes. Pure find-and-replace of Tailwind classes.

Files to modify (grouped by priority):

**High-traffic UI (do first):**
- `src/components/layout/CollectionsSidebar.tsx`
- `src/components/collections/CollectionNode.tsx`
- `src/components/collections/RequestNode.tsx`
- `src/components/panes/TabItem.tsx`
- `src/components/request/RequestPanel.tsx`
- `src/components/request/AuthEditor.tsx`
- `src/components/response/ResponseBodyViewer.tsx`
- `src/components/response/ResponseHeadersTable.tsx`

**Secondary UI:**
- `src/components/git/GitTab.tsx`
- `src/components/git/BranchSelector.tsx`
- `src/components/git/GitCommitForm.tsx`
- `src/components/git/GitChangedFiles.tsx`
- `src/components/git/GitStagedFiles.tsx`
- `src/components/git/GitCommitLog.tsx`
- `src/components/git/GitStashSection.tsx`
- `src/components/git/ConflictResolver.tsx`
- `src/components/git/DiffViewer.tsx`
- `src/components/git/GitCloneDialog.tsx`
- `src/components/git/GitCredentialsDialog.tsx`

**Toolbar & status:**
- `src/components/layout/WorkspaceToolbar.tsx`
- `src/components/layout/CollectionDropdown.tsx`
- `src/components/layout/SandboxPopover.tsx`
- `src/components/layout/StatusBar.tsx`
- `src/components/layout/ConsolePanel.tsx`

**Other components:**
- `src/components/environments/EnvironmentDialog.tsx`
- `src/components/collections/CollectionOverviewTab.tsx`
- `src/components/collections/CollectionVariablesEditor.tsx`
- `src/components/history/HistoryPanel.tsx`
- `src/components/workspace/*.tsx` (dialogs)

## Testing

- `yarn tsc --noEmit` — no errors.
- `yarn test` — all existing tests pass (no behavioral changes).
- Visual check: open the app and verify text is consistently sized, readable, and has clear hierarchy.
