# Flow Node Properties Panel — Design

**Issue:** #26 (Flow: no UI to edit node properties after creation).
**Date:** 2026-09-29.
**Branch:** `worktree-flow-phase2-branching`. This builds on Flow Phase 2 (#31), which is not merged yet.
**Scope:** frontend only. No Rust or IPC changes.

## 1. Problem and goal

Flow nodes cannot be edited after creation. The ⋮ icon on a Request node has no handler. To fix a typo in an inline URL, change an Input value, or point a node at a different saved request, you have to delete the node and rewire every edge.

**Goal:** every node kind can be corrected in place, without touching its edges.

**Users:** people who build flows from both saved and inline Request nodes, about equally.

**Success criteria:**
- Every field listed in §4 can be edited through a panel next to the canvas.
- Edits keep the node id, so every wire stays attached.
- The flow never writes to a shared collection request file.
- The Phase 2 focus bug class cannot come back: typing or pressing Backspace in an editor field never deletes a node.

## 2. Decisions taken during brainstorming

| Question | Decision |
|---|---|
| Where editing lives | **A docked properties panel** to the right of the canvas. A modal Dialog/Sheet and on-card editing were rejected (see §9). |
| Editing a Saved Request node | The flow can **repoint** the node and **Convert to inline**. The request's own content is edited in its request tab, opened with "Open request". The flow never writes the `.yml`. |
| Apply model | **Edits apply live.** There is no Save/Cancel. Each edit marks the flow tab unsaved, like any other canvas edit. |
| Palette placeholders | Keep the placeholders. The panel opens on the new node right away, instead of a prompt asking for a label. |
| Switching a Request node's source | Only through the two explicit actions in §5.1. There are no free-flipping tabs. |

## 3. Architecture

### 3.1 Layout
`FlowPane` renders the canvas and a `NodePropertiesPanel` side by side, split by the shadcn `resizable` handle. The panel starts about 360px wide. It is mounted only while exactly one node is selected.

### 3.2 Opening and closing
The panel opens when:
- a single node is selected by clicking it;
- the ⋮ button on a node header is clicked (every node kind gets one; the Request node's existing icon becomes a real shadcn `Button`);
- a node is added from the palette (it becomes the selection).

The panel closes when:
- the ✕ in its header is clicked;
- empty canvas is clicked;
- more than one node is selected;
- the node is deleted.

### 3.3 State
The selected node id and the open state live in `FlowPane` as UI state. They are not persisted. The panel always reads the node live from `tab.nodes`, so it never holds a copy that can go stale.

### 3.4 Applying edits
Each field change calls the existing Phase 2 path `updateNodeKind(nodeId, kind)`. That path is one store update and reads the latest tab state. Node edits never add, remove or rewrite edges.

### 3.5 Components
`NodePropertiesPanel` picks an editor by `kind.kind`:
- `RequestNodeEditor`, which contains `SavedSourceEditor` and `InlineSourceEditor`;
- `InputNodeEditor`;
- `LabelOnlyEditor`, used for Output, If and Switch.

Editors take `(node, onChange)` props and never touch the store directly, so each one can be tested on its own.

### 3.6 Keyboard and focus
React Flow listens for its delete key on `document`, not on the canvas, and skips it only for input, select and textarea targets, contenteditable elements and anything inside a `nokey` element. The panel always has a node selected, so its root carries `nokey` and `tabIndex={-1}`. Any portalled popover or select content the panel opens carries `nokey` too, because portals escape the panel root. When a palette add opens the panel, focus moves to the Label field, so the next Backspace edits text. Regression tests guard this (§8).

## 4. Editor contents

**All kinds.** A **Label** field (shadcn `Input`) sits at the top. Editing it updates the node header live. The on-card label editing for If/Switch stays as it is.

**Request node, Saved source:**
- Shows the request path, for example `auth/login.yml`.
- **Choose request…** opens a shadcn `Popover` with a filter `Input` and a list of this collection's requests, built from `getCollection` each time it opens. Picking one sets `requestPath`.
- **Open request** opens the request in a normal request tab.
- **Convert to inline** follows §5.1.

**Request node, Inline source:**
- **Method:** a shadcn `Select` with GET, POST, PUT, PATCH, DELETE, HEAD and OPTIONS.
- **URL:** a `SingleLineEditor`, so `{{vars}}` are highlighted.
- **Headers:** a table with name and value on each row, both `SingleLineEditor`. Each row has a remove button (lucide `X`), and **Add header** (lucide `Plus`) adds a row. There is no enabled toggle, because `InlineHeader` is `{ name, value }`.
- **Body:** a Monaco editor. Empty means `body: null`.
- **Use a saved request…** follows §5.1.

**Input node:**
- **Value:** a `SingleLineEditor`.
- A stored value that is not a string is shown read-only, with a note. The panel never writes it.

**Output, If and Switch nodes:** only the label. If and Switch also show a note that the condition, value and cases are edited on the node itself.

## 5. Edge cases and error handling

### 5.1 Switching the source of a Request node
**Saved to Inline, "Convert to inline":**
- Loads the request with `getRequest(collection, requestPath)` and maps it with `savedToInline` (§6).
- Before applying, an in-panel confirmation lists what will be dropped: auth, scripts, tests, assertions, non-raw body modes and disabled headers.
- Nothing changes until you confirm.
- The saved file is never modified.

**Inline to Saved, "Use a saved request…":**
- Opens the same picker as Choose request.
- If the inline request has any content (a URL, any header, or a body), an in-panel confirmation says those fields will be discarded.

In both directions the node keeps its id, so its edges stay attached. The confirmations are in-panel UI, not `window.confirm`.

### 5.2 Wires that target edited fields
- **Headers wired by name** (`headers[Name].value`): renaming or removing the header is safe. At run time the backend adds a missing named header (`apply_header_override`, `crates/rocket-app/src/flow_execution_service.rs:302-318`).
- **Headers wired by index** (`headers[N].value`): if N is at least the current header count, the panel shows a warning next to the headers table, because that wire will fail at run time with "header index N out of range".
- **URL, body and value wires** replace the whole field, so edits do not affect them.

### 5.3 Saved request problems
- A missing or unreadable `requestPath` during Convert: the panel shows the error inline and changes nothing.
- The request list fails to load: the picker shows an error row with a retry button.

### 5.4 Selection and runs
- If the node is deleted while its panel is open, the panel closes.
- Editing during a run is allowed. The run already executes the flow as it was loaded when it started.

## 6. Pure helpers

The helpers go in `src/lib/flow-node-edits.ts`, next to Phase 2's `flow-graph-edits.ts`.

- **`savedToInline(request: Request): { inline: InlineRequestData; dropped: string[] }`**
  - Copies `method`, `url`, the enabled headers as `{ name, value }`, and a raw text body (JSON, XML or text).
  - Names in `dropped` every non-empty item it cannot carry: auth other than inherit/none, pre/post scripts, tests, assertions, a form or file body, and disabled headers.
- **`inlineHasContent(request: InlineRequestData): boolean`** decides whether §5.1 must ask for confirmation.
- **`indexWiresOutOfRange(edges: FlowEdge[], nodeId: string, headerCount: number): FlowEdge[]`** returns the index-targeted header wires into the node whose index is at least `headerCount`.

## 7. Out of scope

- Undo/redo. The flow tab has none today.
- Dragging a request onto an existing node to swap its target.
- Auth, query params, scripts or body modes on inline requests. The inline model has none of these, and extending it is a backend change.
- Editing a saved request's file from the flow.
- Prompting for a label when a node is created.

## 8. Testing

**Unit tests:** `savedToInline` covers raw, form and file bodies, disabled headers, auth, and scripts, tests and assertions. `inlineHasContent` and `indexWiresOutOfRange` are also unit-tested.

**Editor components** (Testing Library, with real `userEvent` clicks and typing):
- Each label, method, URL, header add/rename/remove and body edit produces exactly one `onChange` with the expected `kind`.
- Convert to inline shows the dropped list and applies nothing before you confirm.
- Use a saved request… asks for confirmation only when the inline request has content.
- A non-string Input value is read-only.
- A missing saved request shows an inline error and changes nothing.

**Panel wiring:**
- Single selection opens the right editor.
- Clicking empty canvas, clicking ✕, selecting several nodes, or deleting the node closes the panel.
- Adding a node from the palette opens the panel on it.
- ⋮ opens the panel for every node kind.
- A panel edit updates the card, marks the tab unsaved, and leaves the edges unchanged.

**Focus regression test:** click the panel's URL field, then press Backspace. The text changes and the node still exists.

**Mocks:** `SingleLineEditor` and Monaco are mocked as plain inputs, as existing flow tests do. `getRequest` and `getCollection` are mocked through `@/lib/tauri-api`.

**Checks:** `yarn test --run flow`, `yarn tsc --noEmit`, `yarn check`. There are no Rust changes.

**Manual check:** in `yarn tauri dev`, confirm real CodeMirror/Monaco focus in the panel and panel resizing.

## 9. Alternatives rejected

**Modal Dialog or Sheet:**
- It hides or dims the canvas while editing.
- Monaco inside modals is the known WebKitGTK compositing trouble spot on this machine.
- It feels heavy for quick relabels.

**Editing on the card, as If/Switch do:**
- A Monaco body breaks under React Flow's zoom transform.
- A header table grows the card and moves the wire handles.
- Every extra editable control on the canvas widens the focus and delete-key bug surface.
- The graph stops being readable.
