# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

The `rocket-flow` crate is a pure domain crate in the Rocket HTTP client
workspace. It owns the `Flow`/`FlowNode`/`FlowEdge` entities (a visual,
node-canvas API workflow definition — see
`docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`)
and the `FlowRepository` trait. It has no I/O — the filesystem implementation
lives in `rocket-infra` (`FsFlowRepo`).

## Commands

```bash
# Check this crate
cargo check -p rocket-flow -j4

# Run all tests in this crate
cargo test -p rocket-flow -j4
```

## Architecture

### Module Map

| Module | Responsibility |
|---|---|
| `handle.rs` | Exit/input handle names (`result`, `true`, `false`, `default`, `input`, `trigger`, `auth`, `case:<id>`) |
| `node.rs` | `FlowNodeKind` (Request/Input/Output/If/Switch/WaitForCallback/Transform/Auth), `SwitchCase`, `RepeatUntil`, `RequestSource`, `InlineRequestData`, `InlineHeader`, `NodePosition` |
| `flow.rs` | `FlowNode`, `FlowEdge`, `Flow` aggregate, `FlowRepository` trait. `list_entries` returns corrupt files as entries with an `error`; its default wraps `list`. |
| `graph.rs` | `topological_sort`, `reachable_from`, `FlowGraphError` |
| `validate.rs` | `validate` — `topological_sort` plus structural rules V1–V17 (spec §7, listed in the file header), and `is_request_wire_field` |
| `lint.rs` | Non-blocking lint tier: `validate_with_warnings(flow, ctx)`, `graph_error_lints`, `FlowLint`, `LintSeverity`, `LintContext` |
| `request_refs.rs` | Rewrites Saved request paths in a flow after a move (whole path segments, folder prefix) and finds flows that use a path |

### Key Design Points

- No cross-domain-crate dependencies — `RequestSource::Saved` references a
  collection request by plain `String` path, not by importing
  `rocket-collection`'s types. `RequestSource::Inline` embeds its own minimal
  `InlineRequestData`, not `rocket-http::HttpRequest`.
- `FlowEdge.target_field` is a plain string path ("url", "headers[1].value",
  "body"), not a fixed enum, so new wireable fields on a node type never
  require a schema change here.
- `FlowEdge.expression` is a JS/jsonq expression evaluated against the source
  node's captured output at run time (`rocket-app`, Plan 05) — this crate
  does not evaluate it, only carries it as data.
- `FlowEdge.source_handle` names the exit an edge leaves from. It defaults
  to "result" and is omitted on disk when "result", so Phase 1 files
  re-save byte-identically.
- Not part of the OpenCollection schema (`additionalProperties: false` does
  not apply) — this is a Rocket-only extension format.

### Dependencies

- `rocket-shared` — `DomainResult`, `VariableValue`
- `serde` — serialization

## Auth node

`FlowNodeKind::Auth { label, auth, apply_to_inherit }` authenticates once per
run. It has no inputs and one `result` exit. `auth` is any `rocket_shared`
`Auth` except `none` and `inherit`; only configuration is stored, never a
token. Validation (`validate.rs`): no wires into an Auth node; an `auth` wire
(`handle::AUTH`) must go from an Auth node into a Request node; and
`check_auth_nodes` (V13) requires a concrete auth type on every Auth node and
at most one Auth node with `apply_to_inherit = true`. `check_single_auth_wire` (V14) allows at most one `auth` wire into a Request.
V13 and V14 are labelled in the code; the other wire rules are inline
`check_edges` closures.
Secrets typed literally into an Auth node (client secret, password, token)
are persisted in plaintext in the flow yml, as with collection auth; use
`{{vars}}` or RocketVault references.

## Hard wire and graph rules

After V1-V15, `validate` rejects: V16 (an edge rule) a wire into a Request node whose `target_field` is not `url`, `body`, `trigger`, `auth` or `headers[<index or name>].value`. `is_request_wire_field` is the one spelling of that list, and a `rocket-app` test pins it against `apply_wired_overrides`. Other node kinds keep their own field rules (V1-V4); Output fields are not policed. `rocket-flow` cannot read a saved request, so a `body` wire into a saved form or binary body is still only caught at run time. Inline bodies are always JSON text, so they cannot hit that case. `topological_sort` rejects, in order, duplicate node ids, duplicate edge ids (`FlowGraphError::DuplicateEdge`) and edges to unknown nodes. V17 (`check_output_wires`) requires exactly one non-`trigger` wire into every Output node (spec section 6). It runs last, and `OUTPUT_WIRE_TAIL` is shared with `lint.rs` so the lint sentence can drop the label. The client rule in `src/lib/flow-issues.ts` (`output-no-value`, `output-many-values`) uses the same wording.

## Lint tier (`lint.rs`)

`validate_with_warnings(flow, ctx)` warns about a graph that runs but may not
do what its author meant. It never blocks save or run, works on invalid
graphs too, and is linear in graph size (one `GraphIndex` per call). Rules:
`exit_without_edge` (If exits and Switch cases with no wire, one lint per
node), `switch_without_default`, `no_path_to_output` (only when the flow has
an Output; an Auth node with `apply_to_inherit` is exempt),
`auth_no_effect` (an Auth node with no `auth` wire that applies to no Request:
not applying, or applying while every Request has an `auth` wire or explicit
saved auth; a saved request the context cannot read counts as taking it),
`auth_wire_overrides_auth` (an `auth` wire into a saved request whose own auth
is explicit; the wire wins at run time and replaces it; one lint per wire, with
`edge_id`), `callback_not_wired` (a saved request sends `{{callback.<name>}}`
but has no path to the Wait named `<name>`, the V15 direction; inline requests
stay with V15; scripts and auth fields are not scanned).
`graph_error_lints` turns a `validate` failure into `invalid_graph` error
lints, one per named node or wire. Output order is fixed: node file order,
then rule order. Messages name nodes by label and never quote values,
expressions, match values or auth fields. `LintContext` is the seam for
facts that need I/O (saved requests, known variables): the app layer answers
yes, no or `None`, and `None` never warns. The methods in use are
`saved_request_exists` (F-21), `variable_is_known` (F-22),
`saved_request_auth_is_explicit` (F-24) and `saved_request_callbacks` (F-25).
The app layer computes the saved-request facts in `rocket-app`
`flow_lint_facts.rs`; effective auth is `runtime_auth` else `auth`, and
disabled fields are not scanned.
