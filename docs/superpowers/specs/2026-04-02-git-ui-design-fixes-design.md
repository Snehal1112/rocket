# Git UI Design Fixes

**Date:** 2026-04-02  
**Status:** Approved  
**Scope:** All Git UI components — dialogs and main panel layout

## Overview

Several design issues affect the Git UI: dialogs overflow when long text is entered, the commit form is single-line only, stash messages have no overflow handling, and the AlertDialog footer with three buttons can look cramped. This spec covers a consistent fix pass across all affected components.

## Affected Files

| File | Issue |
|------|-------|
| `src/components/git/GitRemotesDialog.tsx` | URL overflow, no min-w-0, no viewport cap |
| `src/components/git/GitCloneDialog.tsx` | Long error strings overflow horizontally |
| `src/components/git/GitCommitForm.tsx` | Single-line input, needs multi-line textarea |
| `src/components/git/GitLandingPanel.tsx` | AlertDialog footer 3-button layout |
| `src/components/git/GitStashSection.tsx` | Stash messages truncate with no tooltip |

## Section 1: Dialog Overflow and Text Handling

### GitRemotesDialog

**Display row (normal state):**
- Remote name: add `max-w-[100px] truncate` to prevent long names pushing URL off screen.
- Remote URL: keep `truncate flex-1` but add `min-w-0` to the span and wrap it in a Shadcn `Tooltip` so hovering shows the full URL.

**Edit row (editing a URL):**
- Remote name span: add `max-w-[100px] truncate` — same as display row.
- Outer row div: add `min-w-0` to allow the `flex-1` input to properly shrink.
- No other changes to the edit row.

**Add row (adding a new remote):**
- Both `Input` elements: add `min-w-0` to prevent minimum content width from forcing the row wider than the dialog.
- Layout (name `flex-[2]`, URL `flex-[5]`, Add button `shrink-0`) stays the same.

**Delete confirmation row:**
- No changes needed — already uses `flex-1` on text and `shrink-0` on buttons.

**Dialog viewport cap:**
- `DialogContent`: add `max-h-[85vh] overflow-y-auto` so the dialog caps at 85% of viewport height if an extreme number of remotes are added. The dialog grows naturally for normal use; this is only a safety bound.

### GitCloneDialog

**Error text:**
- Add `break-words` to the error paragraph so long Rust error strings (which often contain full file paths and URLs) wrap within the dialog width rather than overflowing horizontally.

### GitCredentialsDialog

No changes needed. Inputs handle long values internally; the `sm:max-w-sm` dialog is appropriate for this form.

## Section 2: GitCommitForm Textarea

**Replace `Input` with `Textarea`:**
- Import Shadcn `Textarea` component (`@/components/ui/textarea`).
- Use `className="text-sm min-h-[60px] resize-none"` — starts at ~3 lines, no manual resize.
- Change the commit keyboard shortcut from `Enter` → `Ctrl+Enter` / `Cmd+Enter` (since plain Enter must now insert newlines).
- The `onKeyDown` handler: detect `e.key === 'Enter' && (e.ctrlKey || e.metaKey)` to trigger commit.
- Placeholder text: `"Commit message... (Ctrl+Enter to commit)"` to hint the new shortcut.
- The "Commit Changes" button below stays full-width and unchanged.

## Section 3: AlertDialog Footer and Stash Tooltip

### GitLandingPanel — AlertDialog Footer

Both `AlertDialog` instances (stash dialog and fetch-before-push dialog) have three buttons in their footer: Cancel + two action buttons. The Shadcn `AlertDialogFooter` renders inline on `sm:` breakpoints. Since this is a desktop app in a resizable panel, narrow panel widths can cause the buttons to overflow or clip.

**Fix:**
- Add `flex-wrap gap-2` to each `AlertDialogFooter` so the three buttons wrap to a second line gracefully on narrow windows rather than overflowing.

### GitStashSection — Stash Message Truncation

Stash messages use `truncate` but have no tooltip, making long messages unreadable.

**Fix:**
- Wrap the stash message `span` in a Shadcn `Tooltip` (same pattern as remote URLs) so hovering shows the full message.
- Keep `truncate` for the display text.

## Patterns Applied Consistently

These patterns are used uniformly across all fixes:

| Pattern | Usage |
|---------|-------|
| `min-w-0` on flex children | Prevents inputs/spans from forcing parent wider than container |
| `max-w-[100px] truncate` | Remote names and other short identifiers that must not push layout |
| `truncate` + Shadcn `Tooltip` | Any long text that is display-only (URLs, stash messages) |
| `break-words` | Error strings that may contain long unbreakable tokens |
| `max-h-[85vh] overflow-y-auto` | Dialog viewport safety cap |
| `flex-wrap gap-2` | Multi-button footers in constrained-width contexts |

## Out of Scope

- `GitCommitLog`: commit messages already use `truncate`; author names are short. No changes needed.
- `BranchSelector`: already has `truncate flex-1` on the collection name. No changes needed.
- `GitFileList`: not reviewed in this pass — file path display is a separate concern.
- Keyboard shortcut documentation or onboarding hints beyond the placeholder text change.
