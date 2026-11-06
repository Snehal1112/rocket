# Collections & requests

A Rocket collection is a tree of folders and requests stored as
[OpenCollection](https://github.com/Snehal1112/rocket) YAML files on disk —
human-readable and git-diffable, not a proprietary database. Nothing about a
collection requires Rocket's own cloud, because there isn't one: open the
folder in any editor and the files make sense on their own.

## Multiple protocols, one tree

A single collection can mix:

- **HTTP** requests
- **GraphQL** requests, with schema introspection and subscriptions
- **gRPC** requests (built on the `tonic`/`prost` Rust crates — a real
  implementation, not a UI-only stub)
- **WebSocket** connections (via `tokio-tungstenite`)

You don't need a separate tool per protocol, and a folder can hold any
combination of the four.

## Workspaces

A workspace can hold several collections at once, and a collection can be
**embedded** (its files live inside the workspace folder) or **external**
(referenced from somewhere else on disk — useful when a collection is shared
across more than one workspace, or lives in its own repository).

## A collection's Overview tab

Opening a collection shows four tabs:

- **Overview** — a method breakdown bar (how many GET/POST/PUT/DELETE/PATCH
  requests the collection holds, as counts and percentages), the collection's
  default headers, and a searchable request list grouped by folder.
- **Authorization** — the collection-level auth every request inherits unless
  it overrides it.
- **Variables** — collection-scoped `{{variable}}` values, separate from an
  environment's.
- **Documentation** — a Markdown page with an Edit/Preview toggle, stored with
  the collection so notes about the API travel with it, not in a separate wiki.

![A collection's Overview tab with its method breakdown and rendered Documentation](/screenshots/collection-documentation.png)

## The editor

- **Multi-tab**, with split panes so you can compare two requests, or a
  request and its response, side by side.
- **Auto-save** — there's no explicit save step to forget.
- Keyboard-driven navigation between tabs and panes.
- A saved HTTP response can be kept as a named **example**, nested under the
  request in the sidebar — see [Response examples](/guide/response-examples).

## Inheritance

Folders can carry their own headers, auth, variables and scripts — see
[Folder settings](/guide/folder-settings) — and requests inherit from the
folder (and the folder from its parent) unless they override it explicitly.
