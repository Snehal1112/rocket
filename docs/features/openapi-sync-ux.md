# OpenAPI sync tab

A collection that was imported from a spec link has its own `openapi-sync` tab. Open it from the collection menu or from the count badge on the sidebar row. The tab is not restored after a restart.

## Header and chip

The header shows the collection name, a status chip, the spec link, the last check and the interval. The chip states, first match wins: Needs approval, Check failed, N updates, Checks off, Connected. The badge, the toast and the chip use the same waiting count.

## Overview

Four counts: Endpoints, In sync, Changed by you, Spec updates waiting. A banner leads to the review when updates wait. The spec version comes from the stored applied copy.

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
- When the plan changes, an item whose content changed goes back to its default. An item whose content did not change keeps your choice, so a check or a skip never ticks again something you unticked. A plan change closes an open confirm dialog.

## Your changes

It lists requests you changed, added or deleted compared with the last synced spec. A Modified row has **Reset to spec**, and the group has **Reset all to spec (n)**. A request you reset shows in **Reset by you** with **Restore my version** and **Discard backup**.

## Local files

Besides `openapi-source.yml`, `openapi-baseline.json`, `openapi-local.json` and the pending copy, a collection keeps two more files under `.rocket/`, both git-ignored:

- `openapi-applied.spec` is the spec as of the last sync. The import writes it and each finished apply replaces it. View spec and Reset read it. A check heals a missing copy by fetching once more, so an older import can show "updates waiting" once.
- `openapi-reset-backups.json` holds the version of a request from before a reset. A sync that applies or deletes that request drops its backup.

## View spec

The header has a **View spec** button. It lists the operations of the newest spec the source accepts (the pending copy counts only when the source holds its hash) in tag groups, at most 500 rows at once with **Show more (N)**. It reads local files only, so it works offline.

The page header lists the spec's **Servers** (at most 10). An expanded operation shows its parameters, **Request body** (each media type with its schema as an indented text tree), **Responses** (status, description, and a **Show schema** button per media type) and **Security** (scheme names, or "None."). A schema tree is cut at depth 6 and 200 nodes and says so, a reference back to a schema that is already open shows "Same as <name>", at most 20 enum values are shown, and examples and defaults are never sent. Descriptions are plain text.

## Reset and Restore

- Reset puts the URL, query and path parameters, headers, body (form fields too) and auth back to the last synced spec, as Bruno does. The name, method, description, scripts, tests, assertions and docs are kept, so a rename is never undone. A row whose only changes are the name, method or description has no Reset button and is not counted in Reset all.
- A confirm dialog says which groups are replaced. A backup of your version is kept until the next sync.
- Restore writes the backup's URL, parameters, headers, body and auth back. It also brings back the backup's name, method and description, but only while they still match the last synced spec (an older build's reset had put the spec's values back); a value you changed since is kept. It finds the request by uid, never by path, so a new request at the old path is not touched.
- Reset is refused for sync data from an older version (no trusted baseline), while an apply runs, and for a request that changed on disk.

## Connect to an OpenAPI spec

The collection menu has **Connect to an OpenAPI spec** for a collection without a source. A collection with only GraphQL, gRPC or WebSocket requests does not offer it.

- Step one takes the link, an optional token (GitHub links only), the folder layout and **Check for spec updates** (on by default, every 5 minutes like the import).
- Step two is the Initial sync review. Matching uses the method and the normalised path, where `:id`, `{id}` and `{{var}}` are the same segment. Matched rows link, Suggested rows (a name match only) start as Keep, In spec only rows are added, Yours only rows stay, and tangled groups have no default and block **Connect** until picked.
- GraphQL and gRPC requests never match.
- Confirm checks that the collection and the spec are still what the preview showed, writes everything in one file transaction, and only then approves the typed link, sets the interval and stores the token. A failure in those last steps is a warning ("Checks are not enabled yet." or "The token was not saved.") and never undoes the connect.
- A linked or added request gets a `baseUrl` variable when the collection has none.


## Connection settings

The header button (and the menu item and the Overview link) opens **Connection settings**. It has:

- **Status.** When the spec was last checked and whether that went well, with a Check now button. Without approval the button is disabled. A failed check shows the fixed sentence, never the raw message.
- **Spec link.** Change link moves the collection to another link without disconnecting. It fetches the new spec with the typed token only, shows how many operations match, and keeps the baseline, skips, backups and interval. A move away from a GitHub repo gets a fresh source id so the old repo's token is never sent to the new one.
- **Check frequency.** For a GitHub link without a token, five-minute checks show a hint about GitHub's 60 requests an hour, with an Add token link.
- **Access.** The GitHub token, kept in the keychain.

## Next steps on a failed check

A failed check shows a button under the alert: **Add access token** (not found or unauthorized on a GitHub link, opens Connection settings at the token field) or **Check now** (network, status, rate limit, too many redirects). An unknown code, a non-GitHub link or a missing approval shows no button. Every Check now trigger of a collection shares one running check.

The Spec updates empty state says when the spec was last checked and offers Check now. The Your changes empty state explains how changes appear.

## Values in Your changes

Show changes on a modified row lists, for each changed field, the version last synced and yours (500 characters at most). Credentials are masked: the values of `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie` and `X-Api-Key` headers, URL user info, and query or form values whose name is exactly one of `api_key`, `apikey`, `api-key`, `key`, `token`, `access_token`, `refresh_token`, `client_secret`, `secret` or `password` (any letter case). A New item in Spec updates shows a preview: method, URL, the names of enabled headers and query parameters, whether it has a body, and the auth kind. Values of headers and query parameters are not sent. **Open request** opens the request tab (or focuses it); a missing file says so.

## Delete requests

In Your changes, each row of **Added by you** has **Delete**, and the group has **Delete all (N)**. In Spec updates, the detail of a removed request that you edited has **Delete this request**. Each asks first: `Delete "METHOD url"? This removes the file from the collection. The sync page cannot undo it, git can.` (for several: `Delete N requests?`). A request the spec still has, or that the spec removed and you did not edit, cannot be deleted here. The page names each request by its uid and path, and the backend refuses it when the file on disk is another one now. Deleting a spec-removed request also drops its operation, baseline entry and reset backup, so the removal is not reported again. It is one file transaction, it refuses while an apply runs, and it never uses the network.

## Skipped items

The "N skipped, hidden from this list." line opens the Skipped list. Each row has **Un-skip**, and **Un-skip all** takes every skip back. Un-skip refreshes the waiting count and refuses while an apply runs. A skip whose item changed again is back in the review list and is not listed here.

## Spec files

A spec can be a file on this computer instead of a link. Pick it in the import dialog (File mode, **Keep in sync with this file**, on by default; it approves the file for this machine and turns on checks every 5 minutes), in Connect, or in Change link (the Link / File switch and **Choose spec file**). The source file stores the path relative to the collection folder when the file is inside the collection or beside it (`../api.yaml`), and the absolute path otherwise. A source with a file is written as `version: 2`, so older Rocket builds refuse it, and link sources stay version 1.

- Approval is the gate, as for a link: a cloned or moved collection shows **Needs approval** with the full resolved path (`file:///home/me/specs/api.yaml`) and reads nothing until it is approved on this machine.
- A check reads the file and compares its SHA-256 with the last one. A rewrite with the same bytes is no update. A file has no validators, no redirects, no token and no backoff.
- A missing file fails with "The spec file was not found on this computer." and changes nothing. The alert offers **Choose another file**, which opens Connection settings at the Change link form. An unreadable file, a directory, a device, or a file over 25 MB also fails with a fixed sentence that names no path.
- A stored path is refused (approval fails with a fixed sentence and nothing is read) when it has more than one leading `..`, a `..` after a folder, a network share (`//server/share`) or a NUL.
- The `$ref` files of a spec file are not loaded, with one note.
- Disconnect and Change link never write the spec file.

## Localhost links

A spec served by a dev server can be imported and followed over plain http: `http://localhost:PORT/...`, `http://127.0.0.1:PORT/...` or `http://[::1]:PORT/...`. Any other http link is refused (look-alike hosts such as `localhost.evil.com`, private addresses and links with user info too). A link with a secret-like query name is refused as for https. A redirect must still be https and not local, a token is never sent over http, and the app proxy is not used for a localhost link. A stored localhost link must be in its normalised form (for example `http://127.0.0.1:8080/x`, not `http://2130706433:8080/x`), or it is refused. `$ref` files of a localhost spec, including relative ones, are not followed, so a dev-server spec that splits into files shows those refs as not followed. The link is approved on this machine like any other.

## Notifications

A count badge shows on the collection row. A toast "N spec updates" with Review shows once per pending spec version. The ids that were shown are kept in `localStorage` under `rocket.openapi-sync.notified` (newest 200).

## Local-only read commands

`openapi_sync_overview`, `openapi_sync_changes` and `openapi_sync_badges` read local files only. They never fetch the network or the keychain.

## Known limits

- Counts can be stale after an edit in another pane, until the tab remounts or the next check.
- The tab is not restored after a restart.
- A damaged backups file fails the whole Your changes view until it is removed.
- Reset all (n) counts only the rows it can reset: rows with a key and a changed reset field.
- Open reset confirm dialogs are not closed when a refetch removes their row.
- A tangled group can only be kept, with the spec operations added or skipped. Link them by hand afterwards.
- Scripts, tests and assertions are not compared, so they are never reset.
- The values in Your changes are not shown for a collection whose baseline is missing or from an older converter.
- Values inside a request body are shown as they are, without masking.
- JSON arrays and top-level array bodies are merged as one value, and numbers that are not canonical (such as `1.0` or `1e3`) are merged as text, so a rebuild never changes them.
- An urlencoded body is merged field by field, whether the baseline holds it as text or as entries. A body with an empty piece (a doubled or trailing `&`) or a repeated field name is merged as one value. The importer writes urlencoded fields as entries, as the collection file keeps them.
- Reset restores the last synced version. A spec change that waits stays in the review list, and a later sync still offers it.
- Regrouping existing folders is not supported: a request keeps its folder when the spec changes its tag or path.
- A tag or path segment named `node_modules`, `.git`, `environments` or `mocks` is not used as a folder name. The request goes to the collection root, as in Bruno.
- A skip for an operation that left the spec stays in the local file until it is un-skipped.

## Manual checks

1. Import from a link.
2. Change the spec and check again.
3. See the badge and the toast (once per version).
4. Open the tab and check the preselection (no Removed item ticked).
5. Accept all with a conflict shows the overwrite warning.
6. Confirm and sync, then see the badge clear.
7. Edit a request in another pane and check that the counts catch up after remount or a check.
8. Reach the badge with the keyboard (Tab, then Enter or Space).
9. Reset a modified request, then Restore it, then reset another and Discard its backup.
10. Reset, then sync: the backup of an applied request is gone.
11. Connect a hand-made collection: Preview, check the default choices, Connect, then Check now.
12. Open View spec with the network off.
13. Open Change link, paste another spec link, read the counts, then Move. Check that Spec updates shows the differences.
14. On a GitHub link, move to a link on another host: the token field is gone, and the old token is not sent.
15. Edit a request header, open Your changes, Show changes: the value shows; add `Authorization: Bearer abc` and check it shows as a mask.
16. Click Open request on a modified row, then on another: the tab opens, then focuses.
17. Break the link (wrong path), Check now: the alert offers Add access token on a GitHub link, Check now on a network failure.
18. With nothing waiting, the Spec updates empty state shows the last checked time and a Check now button.
19. Skip an item, open the skipped line, Un-skip it, and see it return to the list and the badge count rise.
20. Open Connection settings: the status row, the five-minute hint on a GitHub link without a token, and the title reads Connection settings from the menu, the button and the Overview link.
21. Run a local dev server that serves an OpenAPI file, import `http://localhost:PORT/openapi.json`, approve it, then Check now. Try `http://example.com/openapi.json`: it is refused.
22. Import a spec file with Keep in sync on, then edit the file and Check now: the update shows in Spec updates and the header shows a Local file badge.
23. Rewrite the file with the same content: Check now reports no changes.
24. Rename the file, Check now: the alert says the file was not found and offers Choose another file, which opens the Change link form.
25. Copy the collection folder to another place: the sync tab shows Needs approval with the full path, and nothing is read until you approve.
26. In Connect and Change link, switch to File, choose a spec, preview it, and check that there is no token field.
27. Add a request of your own, open Your changes, press Delete, confirm: the file is gone from the sidebar. Press Delete all with two added requests.
28. Edit a request, remove its operation from the spec, check, then open it in Spec updates and press Delete this request.
29. Edit one field of an urlencoded request body, change another field in the spec, check and sync: both changes survive.
30. Rename a synced request and change its URL, then Reset it: the URL goes back, the new name stays. A request with only a new name has no Reset button.
