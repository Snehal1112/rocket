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
| `flow.rs` | `FlowNode`, `FlowEdge`, `Flow` aggregate, `FlowRepository` trait |
| `graph.rs` | `topological_sort`, `reachable_from`, `FlowGraphError` |
| `validate.rs` | `validate` — `topological_sort` plus structural rules V1–V15 (spec §7, listed in the file header) |

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
