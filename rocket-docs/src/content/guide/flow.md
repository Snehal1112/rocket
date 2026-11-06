# Flow — visual workflows

Flow is a canvas for wiring saved requests together into a workflow —
chaining a login, a lookup and a cleanup call, or modeling a multi-step
business process, without writing a top-level script to drive it.

## Nodes

A flow is built from saved requests connected through:

- **Input** — parameters a run starts with
- **If** / **Switch** — conditional branching
- **Transform** — reshape data between steps
- **Auth** — handle an auth step inline
- **Wait-for-callback** — pause for an external event before continuing
- **Output** — what the run produces

A node can also carry a **custom config** step (for shared headers or values
feeding several branches downstream) and a request node's own response is
available to later nodes as structured JSON, so a Transform node can reshape
exactly the fields the next step needs.

## Running and reviewing

The canvas toolbar has **Add node**, **Tidy** (auto-layout), search, undo/redo,
and **Run**/**Stop**/**Save**/**Export**. Run the whole flow, or just part of
it, and inspect each step's trace afterward — a finished run lights up every
executed node green and shows "Run completed in `N`s". Every run is saved to
history, so you can go back and compare one run against another.

![A completed Flow run — every node lit up green, each step's response shown inline](/screenshots/flow-run.png)

## Flows are files

A flow is a plain YAML file inside the collection — git-diffable like
everything else Rocket stores, not a proprietary, opaque format you're locked
into.

## Flows follow your requests

Flow tracks the requests it references. Move or rename a request inside the
same collection, and every flow pointing at it rewrites automatically. A move
to a different collection, or a delete, is **reported** instead — Rocket never
lets a flow silently point at nothing.

## Live variable preview

The flow editor resolves `{{variable}}` live, across every scope that could
supply it — dynamic, process, global, collection, vault, and the active
environment — right there in the editor, with secrets shown masked the same
way they are everywhere else.
