# OpenAPI sync tab

A collection that was imported from a spec link has its own `openapi-sync` tab. Open it from the collection menu or from the count badge on the sidebar row. The tab is not restored after a restart.

## Header and chip

The header shows the collection name, a status chip, the spec link, the last check and the interval. The chip states, first match wins: Needs approval, Check failed, N updates, Checks off, Connected. The badge, the toast and the chip use the same waiting count.

## Overview

Four counts: Endpoints, In sync, Changed by you, Spec updates waiting. A banner leads to the review when updates wait. The spec version shows "-" until the stored applied copy lands (series 2).

## Spec updates

- On open, every New item, every Changed item without a conflict and every variable change is selected.
- A Changed item with a conflict starts unselected and keeps your version.
- A Removed item is never preselected. A Removed item you edited can never be deleted by a bulk action.
- Accept all selects New and Changed items and variable changes, and takes the spec on every conflict field. It first warns "N of your edits will be overwritten" when there are conflicts.
- Skip all asks first, then skips every request until the spec changes. Variable changes cannot be skipped and stay waiting.
- Keep current clears the selection and sends nothing.
- Select matches selects the New and Changed items that match the search. It never selects a Removed item.
- Skip selected skips only the selected requests.
- Confirm and sync is disabled at 0 selected. It opens the confirm dialog, then the result dialog.
- The selection resets to the defaults whenever the plan changes. Choices made inside one plan are kept.

## Your changes

Read-only. It lists requests you changed, added or deleted compared with the last synced spec.

## Notifications

A count badge shows on the collection row. A toast "N spec updates" with Review shows once per pending spec version. The ids that were shown are kept in `localStorage` under `rocket.openapi-sync.notified` (newest 200).

## Local-only read commands

`openapi_sync_overview`, `openapi_sync_changes` and `openapi_sync_badges` read local files only. They never fetch the network or the keychain.

## Known limits

- Counts can be stale after an edit in another pane, until the tab remounts or the next check.
- The spec version shows "-" until series 2.
- The tab is not restored after a restart.
- Series 2 is pending: View spec, Reset to spec, Connect an existing collection.

## Manual checks

1. Import from a link.
2. Change the spec and check again.
3. See the badge and the toast (once per version).
4. Open the tab and check the preselection (no Removed item ticked).
5. Accept all with a conflict shows the overwrite warning.
6. Confirm and sync, then see the badge clear.
7. Edit a request in another pane and check that the counts catch up after remount or a check.
8. Reach the badge with the keyboard (Tab, then Enter or Space).
