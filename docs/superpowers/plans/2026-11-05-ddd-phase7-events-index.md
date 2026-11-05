# DomainEvent split (D-74, D-71): plan index

Spec: `docs/superpowers/specs/2026-11-05-domain-event-split-design.md`. One plan per row, at most three tasks each, run in a git worktree, merged to local `main` only after a fresh `opus` review and the user's go-ahead. Rows after plan 0 are written when the previous plan is done.

| Plan | Scope | File | Status |
|---|---|---|---|
| 0 | `PublishedEvent` port, `EventPublisher` takes `&dyn PublishedEvent`, bus without `channel_for` | `2026-11-05-ddd-phase7-events-plan-0-publisher-port.md` | done, merged |
| 1 | `HistoryEvent`, `LoadTestEvent` (+ `load_test.rs` types), `GrpcEvent` (+ `grpc.rs` types), loud recorders | `2026-11-05-ddd-phase7-events-plan-1-history-load-test-grpc.md` | done, merged |
| 2 | `EnvironmentEvent`, `WorkspaceEvent` | `2026-11-05-ddd-phase7-events-plan-2-environment-workspace.md` | done, merged |
| 3 | `FlowEvent`, `GitEvent` | `2026-11-05-ddd-phase7-events-plan-3-flow-git.md` | done, merged |
| 4 | `AcpEvent` (+ `acp.rs` types) | `2026-11-05-ddd-phase7-events-plan-4-acp.md` | done, merged |
| 5 | `OpenApiSyncEvent`, `ExecutionEvent` | `2026-11-05-ddd-phase7-events-plan-5-openapi-execution.md` | done, merged |
| 6 | `CollectionEvent`, `RecentWrites`, file watcher; deletes the then-empty `DomainEvent` and `domain_event_port.rs` | `2026-11-05-ddd-phase7-events-plan-6-collection.md` | written |
| 7 | Move the helper types left in `rocket-shared/src/events.rs` (Flow DTOs, WebSocket and GraphQL payload enums) to their contexts; final docs | | not written |

A context with many call sites may take two plans.
