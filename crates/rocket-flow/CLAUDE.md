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
| `handle.rs` | Exit/input handle names (`result`, `true`, `false`, `default`, `input`, `trigger`, `case:<id>`) |
| `node.rs` | `FlowNodeKind` (Request/Input/Output/If/Switch), `SwitchCase`, `RequestSource`, `InlineRequestData`, `InlineHeader`, `NodePosition` |
| `flow.rs` | `FlowNode`, `FlowEdge`, `Flow` aggregate, `FlowRepository` trait |
| `graph.rs` | `topological_sort`, `reachable_from`, `FlowGraphError` |
| `validate.rs` | `validate` — `topological_sort` plus structural rules V1–V8 (spec 2026-09-28 §7) |

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
