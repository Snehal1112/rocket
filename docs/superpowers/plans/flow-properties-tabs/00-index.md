# Flow Properties Panel Tabs — Plan Index

**Spec:** `docs/superpowers/specs/2026-09-30-flow-properties-panel-tabs-design.md`
**Branch / worktree:** `worktree-flow-properties-tabs` at `.claude/worktrees/flow-properties-tabs` (based on `worktree-flow-phase2-branching` at `f655dbd4`).

Four plans, at most three tasks each. Execute in order.

| # | File | Delivers |
|---|---|---|
| 01 | `01-exchange-and-storage.md` | Backend `exchange` (+ truncation), Input value; TS types; `FlowNodeDetail.exchange/logs`; sensitive-header TS helper |
| 02 | `02-tabs-and-settings.md` | Tabs shell and panel props from `FlowPane`; Settings details (saved-request preview hook, If/Switch/Output, save-error box); card fix |
| 03 | `03-last-run-tab.md` | Last run tab |
| 04 | `04-wires-tab.md` | Wires tab |

## Rules for every task

- Read `CLAUDE.md`, `.claude/frontend.md` and `.claude/rules/00-shortcuts.md` first.
- Tasks touching Rust flow step results, events or Tauri flow commands start with: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
- TDD: failing test first, watch it fail, minimal code, watch it pass.
- Cargo: always `-j4`, one crate (`cargo test -j4 -p rocket-app <filter>`); `cargo check -j4 -p rocket --tests` for src-tauri. Never the whole workspace suite.
- Frontend: `yarn test src/components/flow src/lib src/stores`, `yarn tsc --noEmit`, `yarn check`.
- shadcn/ui primitives only, lucide-react icons only, SingleLineEditor for single-line fields, Monaco for multi-line (read-only Monaco for bodies), Zustand narrow selectors.
- Commit every task with the `dev-workflow-skills:1-git-commit` skill (inline fallback mode for subagents: run the four git commands yourself, no subagent dispatch). Never `git stash`. Stage only your own files by path.
- Never write the literal panicking-unwrap call text anywhere (a write hook blocks it).

## Locked interface contract

### Plan 01 — backend

```rust
// crates/rocket-shared/src/events.rs — FlowDebugResponse gains:
#[serde(default, skip_serializing_if = "std::ops::Not::not")]
pub truncated: bool,

// FlowStepResult (crates/rocket-app/src/flow_execution_service.rs) and
// DomainEvent::FlowStepCompleted gain:
#[serde(default, skip_serializing_if = "Option::is_none")]
exchange: Option<FlowDebugRequest>,   // event: Option<Box<FlowDebugRequest>> like debug_request

// crates/rocket-app/src/flow_debug.rs
pub(crate) const EXCHANGE_BODY_LIMIT: usize = 262_144;
/// Cuts a response body to EXCHANGE_BODY_LIMIT bytes at a UTF-8 boundary.
pub(crate) fn cap_exchange(record: FlowDebugRequest) -> FlowDebugRequest;
```

- `exchange` is set for every Request node that sent (plain and polled — last attempt), and for a Wait for callback node that accepted a call (response = the call; `method` and `url` from the call). Built with `build_debug_request` (same masking), then `cap_exchange`.
- `debug_request` stays Debug-mode-only (Console).
- Plan 01 adds `pub(crate) fn callback_exchange(call: &ReceivedCall, duration_ms: u64, secret_values: &HashSet<String>) -> FlowDebugRequest` in `flow_debug.rs` (the call is the response; `url` is path plus query). `execute_node` and `run_repeat_until` gain `exchange: &mut Option<FlowDebugRequest>` right after `debug`; `wait_for_callback` gains it right after `logs`. The run loop keeps it in `node_exchange` and sets `FlowStepResult.exchange`.
- Input steps set `FlowStepResult.value` to their resolved value (today only Output does).

### Plan 01 — frontend types and storage

```ts
// src/lib/tauri-api.ts
// FlowDebugResponse gains: truncated?: boolean
// FlowStepResult gains: exchange?: FlowDebugRequest
// FlowStepCompletedEvent gains: exchange?: FlowDebugRequest   (event key is `exchange`)

// src/types/pane-types.ts — FlowNodeDetail gains:
exchange?: FlowDebugRequest;
logs?: FlowLogEntry[];

// src/lib/sensitive-headers.ts (new)
/** Mirrors is_sensitive_header in crates/rocket-app/src/redaction.rs. */
export function isSensitiveHeader(name: string): boolean;
export const REDACTED_VALUE: string; // same marker as REDACTED in redaction.rs
```

- `detailFromEvent` and `detailFromStep` (`src/components/flow/FlowToolbar.tsx`) map `exchange` and `logs`.

### Plan 02 — panel shell and Settings

```ts
// src/components/flow/properties/NodePropertiesPanel.tsx — new props (all required unless marked):
status: FlowNodeStatus;                 // tab.nodeStatus[id] ?? 'idle'
detail?: FlowNodeDetail;                // tab.nodeDetail?.[id]
nodes: FlowNode[];                      // tab.nodes
nodeStatus?: Record<string, FlowNodeStatus>;         // added by plan 04 (Wires tab: other nodes' run state)
nodeDetail?: Record<string, FlowNodeDetail>;         // added by plan 04
saveError?: string;                     // set when this node is in cycleNodeIds
activeTab: PanelTab;                    // controlled by FlowPane
onTabChange: (tab: PanelTab) => void;
onEditWire: (edgeId: string) => void;   // FlowPane's existing wire-dialog opener
onSelectNode: (nodeId: string) => void; // selects that node and shows it in the panel

export type PanelTab = 'settings' | 'last-run' | 'wires';

// src/lib/saved-request-preview.ts (new; non-React cache shared by panel and canvas)
export interface SavedRequestPreview {
  method: string;
  url: string;
  headers: { key: string; value: string; enabled: boolean }[]; // values already masked
  authType: string;          // 'none' | 'basic' | 'bearer' | …; never a secret
  bodyPreview: string | null; // first 20 lines
}
export type PreviewEntry =
  | { status: 'loading' }
  | { status: 'ready'; preview: SavedRequestPreview }
  | { status: 'error'; error: string };
export function toSavedRequestPreview(request: Request): SavedRequestPreview;
export function loadSavedRequestPreview(collection: string, path: string): void; // once per key
export function peekSavedRequestPreview(collection: string, path: string): PreviewEntry | undefined;
export function clearSavedRequestPreviewCache(collection?: string): void; // also on collection-changed
export function subscribeSavedRequestPreviews(listener: () => void): () => void;
export function getSavedRequestPreviewVersion(): number;

// src/components/flow/properties/useSavedRequestPreview.ts (new; hooks)
export function useSavedRequestPreview(collection: string, requestPath: string | null):
  { preview: SavedRequestPreview | null; error: string | null; loading: boolean };
export function useSavedRequestPreviews(collection: string | null, paths: string[]):
  Record<string, SavedRequestPreview>;
export { clearSavedRequestPreviewCache } from '@/lib/saved-request-preview';
```

- `FlowPane` keeps `saveErrorMessage: string | null` next to `cycleNodeIds`, set in `handleSave`'s catch, cleared on successful save, and `panelTab: PanelTab` (default `'settings'`). `openWireEditor(edgeId)` serves both the canvas and the panel.
- Plan 02 renders only the Settings `TabsTrigger`; plans 03 and 04 each add their trigger and `TabsContent` inside the same `Tabs` in `NodePropertiesPanel`.
- Card fix: `toRfNodes` gains a last parameter `savedPreviews: Record<string, SavedRequestPreview>` (from `useSavedRequestPreviews`) and spreads `method`, `headerCount` (enabled headers), `bodyPreview` for Saved Request nodes.

### Plan 03 — Last run tab

- `src/components/flow/properties/LastRunTab.tsx` (new): `{ node, status, detail }` props. Content per spec §5.3.

### Plan 04 — Wires tab

- `src/components/flow/properties/WiresTab.tsx` (new): `{ node, nodes, edges, nodeStatus, nodeDetail, onEditWire, onSelectNode }` props. Reuses `edgeRunState` from `src/components/flow/flowExits.ts` for "not taken". Content per spec §6.
