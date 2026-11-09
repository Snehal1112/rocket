# Sprint Plan

Built from `docs/roadmaps/roadmap.md` on 2026-11-06. One sprint is one week, 3 to 5 items. The item IDs and statuses live in the roadmap; this file only orders them. Update both when an item ships.

## How the sprints are ordered

1. Close out work that is already built (merge, push, manual checks). This costs the least and removes risk.
2. Fix user-visible defects and small hygiene items.
3. Security and correctness.
4. Product features, grouped by area so the context stays loaded.
5. Large series (dedup, perf, flow, style, DDD) run in slices next to the feature sprints.

Rules that still apply: a plan has at most 3 tasks, tasks that touch `.yml`, collections, environments, auth or variable resolution start with the OpenCollection spec read, and an item that needs a spec is brainstormed first.

## Sprint 1: Ship what is built

| ID | Item | Done when |
|---|---|---|
| R-83 | Team network manual checks (`docs/manual-tests/team-network.md`) | Every check passed or ticketed. |
| R-72, R-71 | Settings shell and IPC ACL manual checks | Checks in their docs run. |
| R-21, R-53 | Run the full suites, push local main, confirm CI is green | Main pushed, CI green. |
| R-66, R-78, R-69/R-70 (OpenAPI), R-60 | Merge `feat/rok-r66`, `feat/rocketvault-trust-gate`, the `openapi-*` branches. Do the factory v3 dry run. | Branches merged or dropped. |

R-21 comes first: the later sprints build on a pushed main. R-59 (`cargo fmt` once) fits here as a separate commit.

## Sprint 2: Small defects and hygiene

| ID | Item |
|---|---|
| R-31 | Real PDF export, or rename the option. |
| R-41, R-43, R-40 | Raw `<button>` in `GitStashSection.tsx`; variable popover for folder and process sources; `backend-log` listener. |
| R-57 | Vitest 5 s timeout flakes (cap workers or raise `testTimeout`). |
| R-38, R-37 | Verify issue #25 (may already be fixed); `{{vars}}` in query params (#44). |

R-44 (delete the stale worktree directory) needs the user's confirmation first.

## Sprint 3: Security

| ID | Item |
|---|---|
| R-68 | Check every redirect hop against the request guard. |
| R-36 | Console shows secrets in plain text (#43). Check against F-02 and F-16 first. |
| R-06 | Static script scanner (trust gate is done). |
| R-07 | Configurable script timeout. |
| R-58 | Decision only: agent edits of folder auth. |

## Sprint 4: Auth and import

| ID | Item |
|---|---|
| R-27 then R-26 | Verify issue #20, then OAuth2 implicit grant, OIDC discovery, JWKS. |
| R-62, R-63 | Inherit parent secret bindings and certificates; inherited variables in assistant views. |
| R-30 | Insomnia import (brainstorm and spec). |
| R-65 | Export follow-ups (Postman round trip first). |

## Sprint 5: Rok API follow-ups

| ID | Item |
|---|---|
| R-67, R-69 (cookie follow-ups) | Part B review minors and cookie follow-ups. |
| R-04 | Part D: runner `iterationData` (needs the runner data-file feature, so it follows Sprint 6 if that slips). |
| R-05 | Part E: `rok.grpc.*` spec. |
| R-10, R-11 | Spec only, if time is left. |

## Sprint 6: Collection runner

| ID | Item |
|---|---|
| R-12 | "Run folder" and "Run collection" sidebar entries. |
| R-13 | Tag include and exclude filtering. |
| R-14 | Assertions in the runner and in load tests. |
| R-32 | Load test history and test-plan persistence (spec). |

## Sprint 7: Settings parity

| ID | Item |
|---|---|
| R-73 | Themes and Display (frontend only). |
| R-74 | General settings (needs a global preferences file and executor wiring). |
| R-75 | Global client certificates per domain. |
| R-77, R-76 | Cache and Beta decision; editable Keybindings (needs a shortcut registry). |

## Sprint 8: CLI and contracts

| ID | Item |
|---|---|
| R-79 | Headless `rocket` CLI: brainstorm and spec (covers R-15 and the CI part of R-22). |
| R-22 | Model B warn and block enforcement. |
| R-23 | `open_review`, `remind_reviewers`, `MarkBreaking`. |
| R-82 (CA-4c) | The part that can start without R-79. |

## Sprint 9: Performance

| ID | Item |
|---|---|
| R-34 | #92 measure-first baseline, then #93 (HTTP client cache) and #94 (tree-load double parse). |
| R-42 | Wire the orphaned OpenCollection regression fixtures into a test or CI step. |

## Sprint 10 and on: Series and larger specs

- R-33 dedup series (49 work packages, tiers #89 to #91): two or three packages per sprint beside other work.
- R-35 git integration audit backlog: needed before destructive git workflows.
- Team network: send and keep-in-sync are built (manual checks R-83 sections 11 and 12 open, nothing ships before they run). Next: contract-check spec between nodes (CA-5). Open follow-ups: unconfirmed-apply trade-off (unlink and resend), lost `Unpair` frame in `socket.rs`, stale waiting toast and Cancel after Apply, link quarantine startup warning.
- R-80, R-81, R-82 after R-79 and the cross-repo consumers decision.
- Flow (`flow-roadmap.md`), Rust style (`rust-style-roadmap.md`), DDD (`ddd-roadmap.md`) follow their own batches.

## Backlog (blocked or parked)

- R-55 events and IPC rework (parked by the user 2026-10-11).
- R-19 Claude PR review gate; R-20 RocketVault certificate source (needs an unpushed RocketVault branch).
- R-24 contract collaboration (ideas only); R-25 legacy contract retirement (product decision).
- R-08, R-09 optional follow-ups. R-15 is covered by R-79.
- Section 9 of the roadmap ("probably dropped") needs the user's confirmation.

## Roadmap issue to fix

`R-69` is used twice in `docs/roadmaps/roadmap.md`: for the cookie follow-ups (section 1) and for OpenAPI import (section 6). The file says never renumber, so the second one needs a new ID chosen by the user.
