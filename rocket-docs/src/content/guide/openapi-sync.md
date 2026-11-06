# OpenAPI Sync

A collection connected to an OpenAPI spec doesn't have to drift out of date.
A collection imported from a spec link (or connected to one afterward) has its
own **OpenAPI sync** tab, opened from the collection's menu or the sync count
badge on its sidebar row. A collection that *wasn't* imported from a spec can
still start tracking one afterward — right-click it in the sidebar and choose
**Connect to an OpenAPI spec**, right there alongside New Request and New
Folder.

![OpenAPI Sync overview — endpoint count, in-sync percentage, and spec updates waiting](/screenshots/openapi-sync-overview.png)

## Overview

Four numbers: **Endpoints**, **In sync with spec**, **Changed by you**, and
**Spec updates waiting**. The spec version shown is the one from the last
applied sync, not necessarily the very latest fetch.

## Reviewing a spec update

When updates are waiting, the **Spec updates** tab lists them grouped into
**Updated in spec**, **New in spec**, **Removed from spec**, and **Variables**.
Each row gets a two-button choice instead of a checkbox — the right button
takes the spec's version (**Update**/**Add**/**Delete**), the left button
keeps yours (**Keep current**/**Don't add**/**Keep**). A row with a
**Conflict** badge means you and the spec both changed the same field.

A **Keep my values** switch (on by default) changes what **Accept all** does:
with it on, every conflicting field keeps *your* edited value while
everything else takes the spec's update, with no warning. Turn it off and
Accept all takes the spec's value everywhere, warning first how many of your
edits will be overwritten. **Your changes** (a separate tab) lists what you've
modified, added or deleted relative to the last synced spec, each with its own
**Reset to spec** or **Restore my version**.

## View spec

**View spec** shows the full spec read from local files (works offline),
grouped by tag, with a search box and full parameter/request-body/response/
security detail per operation — useful for checking exactly what an endpoint
expects before wiring a request to it.

## Connection settings — the trust model

OpenAPI Sync treats the spec link or file as something that must be
**approved on this machine** before Rocket reads it automatically — a cloned
or copied collection starts in "Needs approval," not silently trusted.
**Connection settings** is where that approval and the automatic-check
behavior live:

![Connection settings — approval status, check frequency, and the trust/disconnect controls](/screenshots/openapi-sync-connection-settings.png)

- **Check for spec updates automatically**, with a **Check frequency**
  dropdown (for example every 5 minutes).
- **Change link** — move the collection to a different spec source without
  losing its sync history (skips, backups, interval).
- **Trust on this machine** → **Stop trusting this link**, which pauses
  background checks and token use until you approve the link again — a
  reversible step short of fully disconnecting.
- **Disconnect from the spec** — removes the sync link entirely. Your
  requests stay; only the tracking relationship is gone.

A GitHub-hosted spec can take an optional **access token** (for private repos
or to avoid rate limits), kept in the OS keychain, never written into the
collection.

## Spec files instead of links

A spec can also be a local file rather than a remote link — pick **File** in
the import dialog, Connect, or Change Link. Rocket then compares the file's
hash on each check rather than fetching a URL, so it works entirely offline
except for the initial choice.

## Known limits

- Counts can go briefly stale after an edit in another tab, catching up on the
  next check or tab remount.
- Values inside **Your changes** aren't masked for most fields — credentials
  in headers, cookies and known secret-named query params are, but a body
  field holding a secret-shaped value isn't automatically caught.
- Scripts, tests and assertions are never compared or reset by sync — only the
  URL, params, headers, body and auth are spec-tracked.

## Related

- [Import & export](/guide/import-and-export) — the initial import that most
  often starts an OpenAPI-synced collection.
- [Secrets & vault](/guide/secrets-and-vault) — where that GitHub token, and
  every other credential, actually lives.
