# Flow Properties Panel Tabs — Settings · Last run · Wires — Design Spec

**Date:** 2026-09-30
**Status:** Approved in brainstorming — pending written-spec review
**Builds on:** `docs/superpowers/specs/2026-09-29-flow-node-properties-panel-design.md` (the panel, #26), `2026-09-29-flow-request-debug-mode-design.md` (masked request/response records), `2026-09-29-flow-async-poll-callback-design.md` (Repeat until, Wait for callback).
**Branch:** `worktree-flow-properties-tabs`, based on `worktree-flow-phase2-branching` at `26a93d7a`.

**Out of scope:**
- Editing If/Switch conditions or cases in the panel. They stay edited on the node (decision 3).
- Adding, deleting or retargeting wires from the panel (decision 2).
- Keeping results of more than the last run.
- Editing a saved request's file from the flow (unchanged from #26).
- The header wire editor change (separate branch `worktree-flow-header-wire-editor`).

---

## 1. Background

The panel (#26) was designed for configuration only. Today:

| Node | Panel content |
|---|---|
| Output | Label only |
| If / Switch | Label and "edited on the node itself" |
| Saved Request | Label, Debug mode, Repeat until, "Source: saved request", the file path |
| Inline Request | Label, switches, method/URL/headers/body editors |
| Input | Label, value |

The frontend already holds, per node, the last run's status and detail (`FlowTab.nodeStatus`, `FlowTab.nodeDetail`, `src/types/pane-types.ts:177-178`), and the flow's wires (`tab.edges`). The panel receives none of it (`FlowPane.tsx:462-477` passes only `node`, `edges`, `collection` and callbacks). Script logs and debug records go only to the Console (`FlowPane.tsx:365-392`). A save error's full text is only toasted (`FlowPane.tsx` `handleSave`). Saved Request cards always show `SAVED · 0 set · —`, because `RequestNode`'s `method`, `headerCount` and `bodyPreview` props are never filled (`RequestNode.tsx:23-48`, `FlowCanvas.tsx:84-110`).

## 2. Decisions made in brainstorming

1. The panel gets three tabs: **Settings · Last run · Wires**.
2. The Wires tab lets you view wires and jump: a wire opens its script dialog, a node name selects that node. No other wire editing.
3. Settings adds read-only details. If and Switch are still edited on the node.
4. Last run always shows a Request's response and the request as sent, masked and size-capped. This needs a backend change.

## 3. Layout (frontend)

- The panel header is unchanged: type, label, Delete, Close.
- Below it, shadcn `Tabs` with `Settings`, `Last run`, `Wires`. The selected tab is kept in `FlowPane` state (not per node, not persisted), so moving between nodes keeps the tab.
- `FlowPane` passes the panel: `status = tab.nodeStatus[id]`, `detail = tab.nodeDetail?.[id]`, `nodes = tab.nodes`, `edges = tab.edges`, `saveError` (section 4.3), `onEditWire(edgeId)` (the existing wire dialog opener) and `onSelectNode(nodeId)`.
- Focus rules from #26 stay: the panel root keeps `nokey`, so Backspace in any panel control never deletes the node; actions that unmount a focused control call `refocusPanel()`.

## 4. Settings tab

### 4.1 Per node

| Node | Adds (read-only) | Stays editable |
|---|---|---|
| Saved Request | Method badge, URL, headers (sensitive header values masked, see 4.2), auth type, body preview (first 20 lines, monospace) | Label, Debug mode, Repeat until, Choose request / Open request / Convert to inline |
| Inline Request | — | as today |
| If | Condition in a monospace block, note "Edit on the node." | Label |
| Switch | Value, then each case as `label = matches`, note "Edit on the node." | Label |
| Output | "Shows `<script>` from **<source label>**", or "No value wire." | Label |
| Input | — | as today |
| Wait for callback | — | its editor (P2) |

### 4.2 Loading a saved request

- A hook `useSavedRequestPreview(collection, requestPath)` calls the existing `getRequest` once per `(collection, requestPath)` and caches the result in a small module-level map, invalidated when the flow's collection reloads.
- Masking follows the debug record's rules: a header whose name is sensitive shows the same redaction marker the debug record uses (`REDACTED` in `crates/rocket-app/src/redaction.rs`). The sensitive-name list is `is_sensitive_header` in that file. The frontend reuses an existing TS equivalent if one exists; otherwise it mirrors the list in one small helper with a comment pointing to the Rust source. Auth is shown as its type only (as `sensitive_auth_label` does), never its secret. `{{var}}` placeholders are shown as text, never resolved.
- A load failure shows "Could not load request: <error>" in the section; the rest of Settings still works.

### 4.3 Save errors

`FlowPane` keeps the full save error message next to `cycleNodeIds`. When the selected node is in `cycleNodeIds`, Settings starts with a red box with that message. The box clears when a save succeeds.

### 4.4 Card fix

`FlowCanvas.toRfNodes` fills `method`, `headerCount` (enabled headers) and `bodyPreview` for Saved Request nodes from the same preview cache, so cards show real data instead of `SAVED · 0 set · —`. While loading, the card keeps today's fallback.

## 5. Last run tab

### 5.1 Backend: `exchange`

- `FlowStepResult` and `DomainEvent::FlowStepCompleted` gain `#[serde(default, skip_serializing_if = "Option::is_none")] exchange: Option<FlowDebugRequest>` (the existing masked record type in `crates/rocket-shared/src/events.rs:61`).
- It is set for every Request node that got as far as sending (plain, polled: last attempt) and for a Wait for callback node that accepted a call (the call as the response, `method`/`url` from the call). It uses `build_debug_request` with the same masking.
- Response bodies over **262 144 bytes** are cut to that size at a UTF-8 boundary, and the record's response gains `truncated: bool` (`#[serde(default)]`).
- `debug_request` stays tied to Debug mode and keeps feeding the Console. Debug mode now means "also log to the Console".
- Input nodes report their resolved value in `FlowStepResult.value` (today only Output does).
- Old payloads load unchanged: every new field is optional.

### 5.2 Frontend storage

- `FlowNodeDetail` gains `exchange?: FlowDebugRequest` and `logs?: FlowLogEntry[]`.
- `detailFromEvent` and `detailFromStep` map them (event fields are snake_case, summary fields camelCase, as today).
- Details are still reset when a run starts, so nothing from an older run lingers.

### 5.3 Content, top to bottom

1. **Status line:** badge (Success, Failed, Skipped, Not taken, Running + progress text), duration, status code, attempts when polled.
2. **Error:** full message, selectable. Skipped nodes explain why: "An earlier node failed." or "Its branch was not taken."
3. **By node:**
   - Request / Wait for callback: **Response** — headers table and body in a read-only Monaco (JSON pretty-printed when it parses, Copy button, "Truncated at 256 KB" when cut). Then a collapsible **Request as sent** — method, URL, headers, body.
   - If / Switch: "Took: `true`" or the chosen case's label.
   - Output / Input: the full value in a read-only monospace block with Copy.
4. **Logs:** this node's script console lines, if any.
5. **Never ran:** "Not run yet. Run the flow to see results here."

## 6. Wires tab

- **Incoming:** one row per edge into the node — the target field shown as the node would name it (`URL`, header name, `Body`, `Run when`, `Value`, `Input`), an arrow from the **source node's label** with the exit (`result` omitted, otherwise `true`, `false`, the case label or `default`), and a one-line preview of the script (`(no script)` for Run when).
- **Outgoing:** grouped by exit; each row is **target node label · field**.
- A row with a script opens the existing "Value from source" dialog (`onEditWire`). Run when rows are not clickable.
- A node label is a link button that selects that node (`onSelectNode`); the panel then shows that node, on the same tab.
- After a run, edges the canvas shows as not taken (`flowExits.edgeRunState`) are faded with "not taken".
- Empty: "No wires. Drag from a dot on the canvas to connect nodes."

## 7. Error handling

| Situation | Result |
|---|---|
| Saved request file missing or unreadable | Settings shows "Could not load request: …"; card keeps its fallback |
| Response body over 256 KB | Cut, with "Truncated at 256 KB" |
| Non-text response body | Shown as returned text; no binary viewer |
| Node deleted while selected | Panel closes, as today |
| Wire's source node missing (should not happen after validation) | Row shows "(missing node)" and is not clickable |

## 8. Testing

**Rust:** `exchange` set for plain, polled (last attempt) and Wait nodes; masked like debug; truncated at 262 144 bytes on a UTF-8 boundary with `truncated: true`; absent for Input/If/Output; old payloads without `exchange`/`truncated` deserialize; Input steps report `value`; Debug mode still controls `debug_request` only.

**Frontend:** tabs render and keep the selected tab across nodes; Settings shows saved-request details with masked secrets and handles a load error; If/Switch/Output details; the save-error box; cards show method, header count and body preview for Saved requests; Last run for each node kind, error text, skip reasons, truncation note, logs, never-ran state; Wires incoming/outgoing grouping, click opens the dialog, node link selects, not-taken fading, empty state; Backspace inside every tab never deletes the node.

## 9. Implementation plan split

Plans of at most three tasks each, under `docs/superpowers/plans/flow-properties-tabs/`:

1. Backend `exchange` + truncation + Input value; frontend types and `FlowNodeDetail` storage.
2. Tabs shell, panel props from `FlowPane`, Settings details (saved request preview hook, If/Switch/Output, save-error box), card fix.
3. Last run tab.
4. Wires tab.

## 10. Acceptance criteria

- Clicking any node shows a useful Settings tab; a saved Request shows its method, URL, headers (masked), auth type and body preview, and its card shows the same summary.
- After a run, Last run shows each Request's response and the request as sent, without Debug mode.
- The Wires tab lists every wire in and out, opens a wire's script, and jumps between nodes.
- Old flows and old IPC payloads work unchanged.
- `cargo check`, targeted `cargo test -j4 -p <crate>`, `yarn tsc --noEmit`, `yarn check` and `yarn test src/components/flow src/lib src/stores` pass.
