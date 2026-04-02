# Manage Remotes Dialog — Remove Scrollbars + Wider Width

**Date:** 2026-04-02  
**Status:** Approved

## Problem

The Manage Remotes dialog (`GitRemotesDialog`) shows a horizontal scrollbar, degrading the user experience. The root cause: setting `overflow-y: auto` on any element implicitly sets `overflow-x: auto` too (CSS spec), so any horizontal overflow also triggers a scrollbar.

## Solution

One change to one element: the `<DialogContent>` opening tag in `GitRemotesDialog.tsx`.

### Before
```
className="sm:max-w-md max-h-[85vh] overflow-y-auto"
```

### After
```
className="sm:max-w-lg overflow-x-hidden overflow-y-auto [&::-webkit-scrollbar]:hidden [scrollbar-width:none]"
```

### What each change does

| Change | Reason |
|---|---|
| `sm:max-w-md` → `sm:max-w-lg` | Widens dialog from 448px to 512px, giving URLs more horizontal room |
| `overflow-x-hidden` | Kills horizontal scrollbar at the root |
| `[&::-webkit-scrollbar]:hidden` | Hides vertical scrollbar track in Chrome/WebKit |
| `[scrollbar-width:none]` | Hides vertical scrollbar in Firefox |
| Remove `max-h-[85vh]` | No longer needed — the dialog sizes naturally to its content (1–5 remotes is well within viewport height) |

## File Affected

- `src/components/git/GitRemotesDialog.tsx` — line 57 (`<DialogContent` opening tag)

## Trade-offs Considered

- **Approach A (overflow-hidden):** Would silently clip content with many remotes.
- **Approach B (hide scrollbar visually) ← chosen:** Keeps content reachable via trackpad/keyboard; never shows a scrollbar track.
- **Approach C (remove all overflow constraints):** Fragile on small screens with many remotes.
