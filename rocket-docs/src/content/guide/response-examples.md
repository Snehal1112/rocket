# Response examples

A sent request's response can be saved as a named, reusable **example** —
useful for documenting what a real response looks like, or for replaying it
later without re-hitting a live API.

## Saving one

Send an HTTP request, then press **Save as example** in the response toolbar.
Give the example a name (unique per request). It's saved to the request's
`.yml` file under an `examples:` key and appears nested under the request in
the sidebar.

The button is disabled with a specific reason when it can't be used yet: no
response sent yet, the request itself isn't saved, or you're in a **Try** tab
(see below) — a temporary tab's response was never meant to be saved.

![A saved response example, open as its own read-only tab with a Try button](/screenshots/response-example.png)

A few things are called out in the save dialog when they apply: unsaved
changes on the request mean the example captures those unsaved values;
disabled params/headers aren't included, since an example describes what was
actually sent; and a header that looks like it holds a secret (named something
like `authorization`, `cookie`, `token`, `secret`, `password` or `api key`) is
saved as written on disk but **masked automatically the moment it's committed
to git** — see [Git integration](/guide/git-integration).

Examples currently cover HTTP requests only — GraphQL, gRPC and WebSocket
don't have this yet — and a saved example is a snapshot of the request as it
was at save time, not a live link back to the request's current state.

## Opening and trying one

Clicking a saved example opens it as a **read-only** tab showing the captured
request and response. Press **Try** to open a normal, editable request tab
seeded from the example — method, URL, params, headers and body come from the
example, while auth, scripts, assertions and settings come from the parent
request.

A Try tab is explicitly temporary: it's marked with a **Temporary** badge, its
Save button is disabled, closing it never prompts to save, and nothing it does
can write back to the parent request or the example it came from. It's a
sandbox for experimenting with "what if I changed this value" without any risk
of modifying the real saved request.

## Limits

- A response body over roughly 512 KB, or binary, isn't saved — the dialog
  says so rather than silently truncating it into something invalid.
- Example names are capped at 120 characters.
- Try tabs don't survive an app restart.
