# Getting started

Rocket is a native desktop API client for HTTP, GraphQL, gRPC and WebSocket — a
Postman/Insomnia-style workspace, but file-based, git-friendly, and local-first.
There is no cloud and no account: every collection, environment and request
lives on disk, in a format you can read and diff yourself.

## Install

Native builds are published for Linux (`.deb`, plus `.rpm`/`.AppImage`), macOS
(Apple Silicon `.dmg`) and Windows (`.exe`/`.msi`). Grab the latest release from
the [GitHub releases page](https://github.com/Snehal1112/rocket/releases) —
there's no sign-up step before you can open the app. Rocket also checks for a
newer release in the background after launch and shows a toast when one is
available, with Update, Later or Skip this version.

## Quickstart: your first request

1. **Open or create a workspace.** On first launch Rocket opens with a default
   workspace already selected. Click the workspace name at the top of the
   window to **switch** workspaces, start a **New workspace**, **Open
   workspace**, or **Import workspace**. Everything a workspace holds is
   stored under `~/.rocket-api/<workspace-name>/`.

   ![The workspace switcher, open, showing New/Open/Import workspace](/screenshots/workspace-switcher.png)
2. **Create or open a collection.** A workspace's Overview tab has three Quick
   Action buttons: **Create Collection**, **Open Collection**, and
   **Import Collection**. If you already have a Postman, Bruno, WSDL or
   OpenAPI source to start from, use Import — see
   [Import & export](/guide/import-and-export) for the exact steps. Otherwise,
   Create Collection makes an empty one, ready for your first request.

   ![A workspace's Overview tab, with its Quick Actions and collection list](/screenshots/workspace-overview.png)
3. **Add a request.** Right-click the collection (or a folder inside it) in
   the sidebar and choose **New Request**. The same menu has **New Folder**
   and **New Script** for organizing things further, and **Export...**,
   **Connect to an OpenAPI spec**, **Rename**, **Manage contracts** and
   **Delete** for everything else you can do to a collection from here. Give
   the request a name and pick a protocol — HTTP, GraphQL, gRPC and WebSocket
   all live in the same collection tree.

   ![Right-clicking a collection, with New Request and the rest of the collection menu](/screenshots/new-request-menu.png)
4. **Fill in the URL and send it.** Type the URL, add any headers, body or
   auth the API needs, then press **Send** (or Ctrl/Cmd+Enter). The response
   appears on the right — status, timing, body, and a **Tests** tab if the
   request has any assertions.

   ![Rocket's main request/response editor](/screenshots/main-editor.png)
5. **Save it.** Press the **Save** button (or Ctrl/Cmd+S) to write the new
   request to the collection on disk — that's the step that makes it a real
   file a teammate pulling the repo will see. From then on, further edits to
   that request auto-save as you go.

From here, an **environment** (see [Environments & variables](/guide/environments-and-variables))
lets you swap a request's URL and credentials between Local/Staging/Production
without editing the request itself — worth setting up as soon as you have more
than one request that needs it.

## Workspaces and collections

A workspace can hold several **collections**, each stored as OpenCollection
YAML files under `collections/<name>/` — and a collection can be embedded in
the workspace folder or referenced from elsewhere on disk (an external
collection).

## Documentation lives with the work

Every workspace and every collection has its own **Documentation** tab — a
Markdown page, with an Edit/Preview toggle, stored alongside the data it
describes rather than in a separate wiki. Until you've written anything, it
shows a plain "No documentation yet" state with an **Add Documentation**
button — nothing to configure, just start writing. A collection's Overview tab
also shows a method breakdown (how many GET/POST/DELETE/etc. requests it
holds) and its default headers.

## Where to go next

- [Collections & requests](/guide/collections-and-requests) — the tree, the
  protocols, and how collections are stored.
- [Environments & variables](/guide/environments-and-variables) —
  `{{variable}}` templating and secret masking.
- [Flow](/guide/flow) — wire saved requests into a workflow.
- [AI Assistant](/guide/ai-assistant) — an agent that reads your workspace and
  proposes changes, never a blind write.
