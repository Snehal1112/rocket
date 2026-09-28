# Flow Node Properties Panel — Plan Index

**Spec:** `docs/superpowers/specs/2026-09-29-flow-node-properties-panel-design.md` (issue #26).
**Branch:** `worktree-flow-phase2-branching`. This builds on Flow Phase 2 (`FlowNodeActionsContext`, `updateNodeKind` and the If/Switch nodes).
**Scope:** frontend only, with no Rust or IPC changes.

Each plan has at most three tasks and leaves the app working when it ends.

| # | Plan | Tasks | Delivers |
|---|---|---|---|
| 01 | [Panel foundation](2026-09-29-flow-node-properties-plan-01-panel-foundation.md) | 3 | Pure helpers, the panel with label and Input editing, the ⋮ button on every node, and selection wiring in FlowPane. |
| 02 | [Request node editing](2026-09-29-flow-node-properties-plan-02-request-editing.md) | 3 | Inline request editor, Saved request picker and "Open request", and source switching with Convert to inline. |

## Cross-plan interface contract

These names are produced by plan 01 and consumed by plan 02. Neither plan may rename them.

- `src/lib/flow-node-edits.ts`:
  - `savedToInline(request: Request): InlineConversion`, where `InlineConversion = { inline: InlineRequestData; dropped: string[] }`
  - `inlineHasContent(request: InlineRequestData): boolean`
  - `indexWiresOutOfRange(edges: FlowEdge[], nodeId: string, headerCount: number): FlowEdge[]`
  - `requestEntriesOf(folder: Folder): SavedRequestEntry[]`, where `SavedRequestEntry = { path: string; name: string; method: string }`
- `src/components/flow/properties/LabelField.tsx`: `LabelField({ value, onChange })`.
- `src/components/flow/properties/NodePropertiesPanel.tsx`: `NodePropertiesPanel({ node, onChange, onClose })`. Plan 02 adds the `edges` and `collection` props.
- `src/components/flow/nodes/FlowNodeActionsContext.tsx`: `FlowNodeActions.openProperties(nodeId: string): void`.
- `FlowCanvas` props: `selectedNodeIds?: ReadonlySet<string>` and `onSelectedNodeIdsChange?: (ids: ReadonlySet<string>) => void`.
- Test ids: `node-properties-panel`, `input-value-readonly`, `saved-request-path`.
