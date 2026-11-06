# Folder settings

Clicking a folder in the sidebar opens a **Folder Settings** tab with six
sub-tabs: **Headers**, **Script**, **Test**, **Vars**, **Auth** and **Docs**.
Everything set here is stored in that folder's `folder.yml` and applies to
every request below it when it runs — the chevron still just expands and
collapses the folder; opening the settings tab is a separate click (or the
folder's context menu → Settings).

## Headers

The tab header summarizes the folder's size ("12 requests, 3 subfolders", or
"Empty folder"). Below it, a read-only **inherited headers** list shows what
the folder receives from the collection and its parent folders, each with a
source chip — and a row is marked "Overridden here" if this folder sets an
enabled header with the exact same name.

## Inheritance

Everything set on a folder is inherited by everything beneath it — subfolders
and requests alike — unless something further down the tree overrides it
explicitly. That's how a collection can define auth or a base set of headers
once, at the folder level, instead of repeating it on every request. Setting a
folder's own auth to **Inherit** is the same as leaving it unset.

## Known gaps worth knowing about

- **gRPC requests don't inherit folder settings** — a folder's headers/auth
  don't apply to a gRPC request inside it.
- **Folder-level scripts run pre-request only.** A folder's post-response
  script behavior is more limited than a request's own.
- **Folder OAuth2 has no interactive token step.** A grant type other than
  client credentials (authorization code, password, implicit) with no cached
  token sends the request with no Authorization header at all, silently —
  worth testing explicitly if a folder relies on OAuth2.
- Header name matching is case-sensitive on the backend but case-insensitive
  on the way a request is sent from the UI — in the rare case a folder header
  and a request header differ only in case, both can end up on the wire.

## Why it's a separate file

Keeping folder settings in their own `folder.yml` keeps them git-diffable and
reviewable on their own, the same way the rest of a Rocket collection is —
see [Collections & requests](/guide/collections-and-requests) for the overall
storage model.
