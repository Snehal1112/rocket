# Design: App Version in Status Bar

**Date:** 2026-04-10
**Status:** Approved

## Goal

Display the Rocket app version (`v0.4.0`) as a plain text label in the bottom-right corner of the status bar.

## Scope

Single file change: `src/components/layout/StatusBar.tsx`.

## Approach

Use Tauri's runtime API (`getVersion()` from `@tauri-apps/api/app`) to read the version. This always reflects the actual bundled app version from `tauri.conf.json`, which is the source of truth for Tauri distributions.

`@tauri-apps/api` is already a project dependency — no new packages required.

## Implementation Details

**Version state:**
- `useState<string | null>(null)` — starts as null, populated after the async call resolves.
- `useEffect` on mount calls `getVersion()` and sets state on success.
- No loading indicator. The label simply does not render until the value is available (resolves near-instantly on Tauri startup).
- No error handling needed; this API cannot fail in a running Tauri app.

**Layout:**
- The status bar `<div>` is already a flex row (`flex items-center gap-1.5`).
- The version `<span>` uses `ml-auto` to push it to the far right.
- Existing left-side controls (theme toggle, console button) are unaffected.

**Styling:**
- `text-2xs text-muted-foreground` — matches the muted small-text style used for the console entry count badge.
- Displays as `v{version}` (e.g., `v0.4.0`).

## Non-Goals

- No click handler or "About" dialog.
- No build-time version injection.
- No changes to other components or files.
