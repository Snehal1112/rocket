# Flow Phase 3 — Transform (Script) Node — Design Spec

**Date:** 2026-09-30
**Status:** Implemented (plans 01-05 in `docs/superpowers/plans/flow-phase3-transform-node/`). Manual check of the real script engine is pending, see the plan 05 hand-off.
**Tracks:** GitHub issue #32
**Builds on:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md` (Phase 1, which deferred this work) and `docs/superpowers/specs/2026-09-28-flow-phase2-branching-design.md` (If/Switch routing nodes, whose single-input pattern this node copies).

**Out of scope:**
- Multiple named inputs or multiple named outputs. The node has one `input` and one `result`.
- Access to the `rok` API (`rok.setVar`, `rok.getVar`, and so on). The script only sees `response`.
- Timeout and memory limits beyond what the existing script engine already applies.
- Click-to-edit of an existing wire's expression, and any change to wire expressions.
- Loops and parallel execution.

---

## 1. Background

A wire can carry a one-expression script (`WireScriptDialog`), but that script belongs to one edge. To reuse a reshaping step across several consumers, the user retypes it on every wire.

Phase 3 adds a first-class **Transform** node. It takes one upstream value, runs a script body over it, and exposes the returned value as its output. Any number of downstream nodes can wire to that output.

Relevant current code (verified 2026-09-30 at `3c311e7e`):

| Area | Location |
|---|---|
| Node kinds | `crates/rocket-flow/src/node.rs` (`FlowNodeKind`) |
| Handle names | `crates/rocket-flow/src/handle.rs` |
| Validation | `crates/rocket-flow/src/validate.rs` (`check_routing_inputs`, `is_routing`) |
| Script wrapper | `flow_script` in `crates/rocket-app/src/flow_execution_service.rs`; accepts one expression or a function body with `return`, and the script reads `response` |
| Route evaluator | `evaluate_flow_route_expression` (same file), the model for the new evaluator |
| Executor | `execute_node` in the same file: `If` at :1154, `Input` at :1029, `ExecutedNode` (`output`, `chosen_exit`, `reported_value`) |
| Upstream to script | `captured_output_response_json`, which wraps a `CapturedOutput::Value` as a 200 response whose `body` is the value |
| Events | `FlowStepCompleted` in `crates/rocket-shared/src/events.rs` already has `value: Option<String>` (used by Input and Output) |
| IPC | `FlowNodeKindDto` in `src-tauri/src/commands/flow.rs` |
| Frontend | `src/components/flow/nodes/*`, `NodePalette.tsx`, `properties/NodePropertiesPanel.tsx`, `properties/LastRunTab.tsx`, `src/lib/flow-wiring.ts`, `src/lib/flow-handles.ts`, `src/lib/tauri-api.ts` |

## 2. Goals

- A user can add a Transform node from the palette, wire one node's output into its `input`, write a multi-line script that returns a value, and wire its `result` into downstream nodes.
- Several downstream nodes can wire to the same Transform output.
- When a Flow runs, the Transform node executes as a normal step, and Last run shows the returned value and any `console.log` output.
- A Transform node is skipped, with the Phase 2 skip reasons, when its branch is not taken or its upstream failed.
- Flow files without Transform nodes load, run and re-save byte-identically.

## 3. Non-goals

No new Tauri commands. No new execution path or sandbox: the node runs through the same script engine as wire and routing expressions. No change to OpenCollection files.

## 4. Chosen approach

Three approaches were considered:

1. **Mirror If/Switch (chosen).** A new `FlowNodeKind::Transform` with one `input` and one `result` handle. It runs through the existing `flow_script` path and stores its result as `CapturedOutput::Value`. It reuses the input validation, the fate decision, the events and the sandbox.
2. Evaluate through the wire-expression entry point with no new evaluator. Rejected: object results need their own encoding, so it grows into approach 1.
3. A separate script service with its own sandbox options. Rejected: the issue asks to reuse the existing sandbox, and this is more than the feature needs.

## 5. Data model (`rocket-flow`)

```rust
Transform {
    label: String,
    /// One expression or a function body that returns a value. It reads `response`.
    script: String,
}
```

- Serialized under the existing `kind` tag (`Transform`). `DEFAULT_SCRIPT` is `return response.body;`.
- `handle.rs` needs no new names: the node uses `INPUT` for its incoming edge and `RESULT` for its output.
- Nothing is added to Phase 1, If or Switch files, so existing flows round-trip unchanged.

## 6. Validation

A new rule, V-T, mirrors V1 (`check_routing_inputs`):

1. A Transform node has exactly one incoming edge, and it targets `input`.
2. Its `script` is not blank.
3. Edges leaving it use the `result` exit only. The existing `source_handle` check already rejects other names once `Transform` is known to have only `result`.
4. A `trigger` edge into a Transform node is rejected, because rule 1 allows one incoming edge and it must target `input`. This matches If and Switch.

`topological_sort` and cycle detection are unchanged.

## 7. Execution

`execute_node` gains a `Transform` arm:

1. Take the single upstream captured output, as `single_input` does for If.
2. Add `evaluate_flow_transform_script`. It follows `evaluate_flow_route_expression`, calls `flow_script` with `FlowCoercion::Required`, which guards against an `undefined` result (error `script returned no value`) and accepts any other value, including `null`, and returns the JSON value plus console entries.
3. Convert the result to a string:
   - A string is used as is.
   - A number or boolean becomes its text.
   - An object or array becomes compact JSON.
   - `null` becomes `"null"`.
   - `undefined` (no `return`) fails the node with "script returned no value". This is stricter than If/Switch, because a transform with no output is almost always a bug.
4. Return `ExecutedNode` with `output: CapturedOutput::Value(VariableValue::simple(result))`, `chosen_exit: handle::RESULT`, and `reported_value: Some(result)`.
5. A thrown error or syntax error fails the node. Dependents skip as "upstream failed", using the Phase 2 fate rules unchanged.
6. Console entries are added to the step's logs, as for If nodes.

`decide_fate` and `is_live` need no change: the node is a plain single-exit node.

Downstream wires read the value through `captured_output_response_json`: the returned text is the `response.body`. The script engine's `res.getBody()` parses JSON text, so an object result is read directly (`response.body.plan`), and number or boolean text arrives as a number or boolean.

## 8. Events and IPC

- `FlowStepCompleted.value` is already `Option<String>`. It is now also filled for Transform nodes, through `reported_value`. Any cap the executor already applies to reported values applies here too.
- `FlowNodeKindDto` gains a `Transform { label, script }` variant with `From` impls in both directions. Persistence structs get no camelCase rename.
- No new events and no new commands.

## 9. Frontend

- **`TransformNode.tsx`** in `src/components/flow/nodes/`: one target handle (`input`), one source handle (`result`), the label, a one-line preview of the script, and the shared status caption and menu wiring. The script is not edited on the node.
- **Registration:** the node type map in `FlowCanvas.tsx`, and a `NodePalette.tsx` entry with the default script.
- **Wiring rules:** `flow-wiring.ts` treats Transform like a routing node for input rules (one incoming wire, to `input`) and like a normal node for its `result` exit.
- **Properties panel:** a `TransformNodeEditor` with the label field and a Monaco editor for the script. The Monaco setup follows the existing Scripts-tab and `WireScriptDialog` conventions, and the editor is sized to fit the panel. Edits update the node kind through the same `onChange` path as other editors.
- **Last run:** shows the returned value and the logs, the way it already does for Output and Input nodes.
- Icons come from `lucide-react`, and every control is a shadcn/ui primitive.

## 10. Errors

| Situation | Result |
|---|---|
| Script throws or has a syntax error | Node fails with the error text, dependents skipped as upstream failed |
| Script returns nothing | Node fails: "script returned no value" |
| Blank script or wrong wiring | Rejected at save and at run start by validation |
| Upstream branch not taken | Node skipped with `branch_not_taken` |

## 11. Testing

- **`rocket-flow`:** round trip of a Transform node; a Phase 1 flow re-saves byte-identically; V-T cases (zero, two and wrongly targeted incoming edges, blank script).
- **`rocket-app`:** transform results for string, number, object, `null`, no return and a thrown error; console logs captured; fan-out to two consumers; a Transform after a not-taken branch is skipped; a Transform after a failed node is skipped as upstream failed.
- **`src-tauri`:** DTO round trip for `Transform`.
- **Frontend (Vitest):** node rendering and handles, palette entry, panel editor edits, wiring rules, Last run value.
- Verification: `cargo check -j4`, targeted `cargo test -j4 -p rocket-flow` and `-p rocket-app flow`, `yarn tsc --noEmit`, `yarn check`, `yarn test flow`.

## 12. Risks

- A Monaco editor in the properties panel must keep its size and focus while the panel re-renders. It should be built and checked first.
- Objects are stored as JSON text. Wires see them parsed, because `res.getBody()` parses JSON text, but a returned string that happens to be valid JSON (such as `"123"`) is parsed too. A typed value is a possible later step.
