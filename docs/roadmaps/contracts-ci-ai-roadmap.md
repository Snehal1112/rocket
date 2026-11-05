# Contracts, CI and AI Roadmap (v1)

Status: direction only (v1). No spec or code yet.
Date: 2026-11-05.

Goal: a team knows that a change breaks another team before the change ships, and
a user gets real help when a request fails. Rocket stays local first.

Each project below is large. It gets its own brainstorm, spec and plan series
before any code is written. This file holds the high-level picture only. Refine
one project at a time and link its spec here.

## How to maintain this file

- Projects have stable IDs (`CA-1` to `CA-4`, sub-projects `CA-4a` and so on).
  Never renumber. Add new projects at the end.
- Status: `open`, `brainstorm`, `spec`, `planned`, `in-progress`, `done`,
  `dropped`.
- When a project gets a spec, add its path to the project's "Links" line.
- The master roadmap tracks each project as one row (R-79 to R-82, section 5).
- Plans hold at most 3 tasks each, with an index file.
- Tasks that touch `.yml`, collection or environment models, auth or variable
  resolution start with: "Before starting, read
  `docs/superpowers/specs/opencollection-spec-reference.md`."

## Constraints (carried over)

From `docs/features/contract-collaboration-enhancements.md`:

- Local first. Contract data lives on disk in the workspace, as files.
- Sharing happens only through git. No Rocket server, no accounts, no hosted
  state. A hosted index would revert this decision (see Open decisions).
- Git work uses `git2` through `rocket-git`. Never the `git` CLI.
- Persistence stays backward compatible. camelCase only on IPC DTOs.

New for this roadmap:

- The pass or fail result is always deterministic code. AI explains and
  suggests, it never decides whether a change is breaking.
- Every feature works with no AI configured. AI adds text on top.
- Secrets follow the existing masking rules (Console masking, example masking).
  Nothing unmasked leaves the machine or the CI job.

## What exists today

- Contract model: provider, consumers, version, expiry, scope, snapshot, diff,
  changelog and the lifecycle states (Draft, Active, Drift, Breach, InReview and
  more) in `crates/rocket-collection/src/contract/`.
- `ContractEnforcementMode` has `Informational`, `Warn` and `Block`. Only
  `Informational` is used (R-22).
- OpenAPI sync with a spec diff (R-70, `docs/features/openapi-sync-ux.md`).
- Collection runner in the app, no headless mode (R-15).
- ACP assistant with an MCP tool server inside the app (R-16). Tools include
  `list_collections`, `get_request`, `run_request`, `get_history`,
  `get_test_results` and `get_environment`.
- Request history, Console with secret masking, collection trust gate.

## Overview

| ID | Project | Depends on | Status |
|---|---|---|---|
| CA-1 | Headless `rocket` CLI: run collections and check contracts in CI | none | open |
| CA-2 | Contracts from source code: check a spec generated from the code | CA-1 | open |
| CA-3 | Consumer impact: who breaks, before the provider ships | CA-1 | open |
| CA-4 | AI layer: MCP outside the app, PR explainer, failure debugging | CA-1, CA-3 for 4b | open |
| CA-5 | Team network: AirDrop-style discovery and pairing, one node as hub | none | spec |

Order: CA-1, then CA-3, then CA-2, then CA-4. CA-4c (failure debugging in the
app) does not need the CLI and can start in parallel with CA-1.

---

## CA-1. Headless `rocket` CLI

Why: nothing can run in CI today. Every other project needs this.

What:

- A new binary crate that reuses `rocket-app` and `rocket-infra`. No Tauri.
- `rocket run <collection> [--folder] [--env ci] [--tags]`: runs requests with
  their scripts and assertions. Writes JUnit, JSON and a short text summary.
  Exit code is non-zero when a test fails.
- `rocket contract check --base <git ref>`: loads each contract at the base ref
  and at HEAD with `git2`, runs the existing diff, and classifies each change as
  compatible, drift or breaking. Output is a JSON report plus text.
- Enforcement: `Block` mode fails the job on a breaking change, `Warn` passes
  with a warning, `Informational` only reports. This closes R-22 for CI.
- Secrets in CI come from environment variables, not from the keychain.
- The trust gate applies: CI must opt in to running scripts of a collection.

Covers: R-15 (CI headless runs) and part of R-22.

Links: none yet.

## CA-2. Contracts from source code

Why: a provider can break a contract in code without anyone editing the
collection. The check must look at the code itself.

What:

- Most backend frameworks can generate OpenAPI from code (utoipa, springdoc,
  FastAPI, swag, NestJS). The provider's CI generates the spec from the PR
  branch.
- `rocket contract check --spec generated.yaml`: compares the generated spec
  with the signed contract snapshot. Reuses the OpenAPI sync diff.
- The report points at the endpoint and field that changed. Mapping a change
  back to a source file and line is a later step (it needs the framework's
  source map, or the AI in CA-4b).
- Ready-made CI snippets for GitHub Actions and GitLab CI.

Links: none yet.

## CA-3. Consumer impact before shipping

Why: the provider knows that a change is breaking (CA-1, CA-2), but not who it
breaks. The consumer teams should be named before the merge.

What:

- Consumer pin file: a consumer team commits a small file that names the
  contract, the version it depends on, and what it actually uses (endpoints,
  fields, status codes). Rocket can derive the "uses" list from the consumer's
  saved requests, assertions and scripts.
- The provider's CI reads all pins and joins them with the breaking-change
  report. Output example: "Removes `user.email`. Used by team Billing in
  `checkout/get-user.yml`, assertion on line 12."
- A provider change that moves a contract to Drift or Breach lists the known
  consumers in the app before commit too.

Two levels of checking:

1. Shape check (v1): compare the change with the pins. Fast, no server needed.
   It finds removed or renamed fields, changed types, removed endpoints.
2. Behaviour check (later): run the consumers' own collections against the
   provider's PR. See below.

### The behaviour check, explained

This is the consumer-driven contract idea that Pact made popular.

- In Pact, each consumer writes tests that record what it expects from the
  provider: "GET /users/1 returns 200 with an `email` string". The expectations
  are saved as a file. The provider's CI replays that file against the provider
  and fails when an expectation no longer holds. The consumer decides what
  matters, not the provider.
- Rocket teams already have these expectations: a consumer's collection holds
  the requests it sends and the assertions it checks. No separate Pact file is
  needed.
- The flow:
  1. A provider opens a PR.
  2. CI deploys the PR to a short-lived preview environment (a Railway PR
     environment, a Kubernetes namespace, or Docker Compose inside the job).
  3. CI fetches the consumer collections named in the pins (the requests tagged
     `contract`, not all of them).
  4. CI runs them with `rocket run --env preview` against the preview URL.
  5. A failed assertion is a consumer that will break. The PR gets the list.
- Why both checks: the shape check sees only the schema. The behaviour check
  also sees changed status codes, new validation rules, auth changes, changed
  defaults and ordering.
- Hard parts, to settle in the spec:
  - Test data: the preview needs seed data that the consumer requests expect.
  - Credentials for the preview environment.
  - Requests with side effects (POST, DELETE): only requests tagged as safe
    for contract runs are used.
  - Run time and flaky results.
  - Where consumer collections live (see "Cross-repo consumers" below).

Links: `docs/features/contract-collaboration-enhancements.md` (items 1 and 2).

## CA-4. AI layer

Why: make Rocket useful to AI tools outside the app, explain contract results
in plain language, and help a user find why a request failed.

The deterministic parts (CA-1 to CA-3) produce structured JSON. The AI layer
reads that JSON. It never changes a result.

### CA-4a. `rocket mcp`: Rocket's tools for any AI agent

- A stdio MCP server started from the CLI: `rocket mcp --workspace <path>`.
- Claude Code, Copilot, Cursor or any MCP client can connect to it from the
  backend source repo, not only from inside Rocket.
- Reuses the existing tools of the in-app MCP server (collections, requests,
  environments, history, test results).
- New contract tools:
  - `list_contracts`, `get_contract`.
  - `find_consumers`: who uses this endpoint or field (from CA-3 pins).
  - `check_change`: give a spec or a contract edit, get the compatible, drift
    or breaking report (from CA-1).
- Example: a developer asks their coding agent "can I rename `email` to
  `emailAddress`?". The agent calls `find_consumers` and `check_change` and
  answers with the teams that break, before any commit. This is the earliest
  point to catch a break.
- Safety: read-only by default. `run_request` needs a flag and passes the
  trust gate. Secrets are masked with the same rules as the Console.

### CA-4b. PR explainer

- A CI step reads the reports of CA-1, CA-2 and CA-3 and posts one PR comment.
- Without AI: a plain template comment (changes, consumers that break).
- With AI:
  - A short summary of the change in plain language.
  - Per consumer: what breaks and why.
  - A non-breaking alternative, for example "keep `email`, mark it deprecated,
    add `emailAddress`".
  - A draft migration note for the consumers and a changelog entry.
- Input to the AI is the report only: names, paths, fields. No request or
  response bodies, no secrets.
- Provider and key are configured in CI. The provider is not fixed (Claude by
  default, others allowed). Missing key means the template comment.

### CA-4c. Failure debugging in the app

- An "Explain this failure" action on a failed request or test.
- Rocket first builds a debug bundle with deterministic facts:
  - The request as it was sent, after variables were resolved. Secrets masked.
  - The response: status, headers, body (cut to a size limit), timings.
  - A diff against the last successful run of the same request from history:
    URL, headers, body shape, status, timing.
  - Variables that changed since that run, and which environment and scope
    each one came from.
  - Script errors and Console output of the run.
  - The contract the response breaks, if any, and which field.
- The assistant gets the bundle and answers in three parts: the likely cause,
  the evidence from the bundle, and the next step to try.
- Without AI, the bundle itself is shown as a "What changed" view. That alone
  answers many failures.
- Uses the existing ACP assistant and its MCP tools. New tool:
  `get_debug_bundle`.

### CA-4d. Link failures to backend traces

- Optional `traceparent` header (W3C Trace Context) on every request, set per
  collection or environment.
- History stores the trace id of each run.
- A link template in settings opens the trace in the team's tool (Jaeger,
  Grafana Tempo, Datadog and so on).
- Later: the agent fetches the backend logs for that trace through the
  observability tool's own MCP server and adds them to the CA-4c answer.

Links: none yet.

## CA-5. Team network (AirDrop-style)

Why: teams on the same network should find each other's Rocket and work
together without a git server or a cloud service.

What:

- MVP (spec written): every Rocket is a node. Nodes on the same network show
  in a Network panel with a search box. Pairing uses the same 6-digit code on
  both screens. Pairings last until removed. One node can pair with many
  teammates and act as the team's hub. "Add by address" covers VPN and
  Tailscale.
- Later, each with its own spec: sending collections, contracts and pins as
  sanitised snapshots (lands with zero trust), and contract checks between
  nodes. Git history over the hub stays a separate spike.
- This reverses "no Rocket server" for an opt-out local service only. No
  hosted server.

Links: `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`
(MVP). Recommendations for after the MVP are in its section 11.

---

## Open decisions

- Cross-repo consumers. How a provider's CI finds consumer pins and
  collections:
  (a) consumers commit pins into the provider repo,
  (b) the provider repo vendors a read-only copy of consumer pins,
  (c) single-repo workspaces only for v1.
  This decides how far CA-3 can reach.
- Shared view. Git-only means no org-wide "who depends on what" page. That
  needs a small hosted index and reverts the "no Rocket server" decision.
- AI in CI. Which provider and whose key. Default to Claude, allow others,
  always work without one.
- Preview environments for the behaviour check. Rocket does not deploy
  anything. It needs a URL from the team's own CI.

## Related

- `docs/features/contract-collaboration-enhancements.md`
- `docs/product/svp-api-collaboration-pitch.md`
- `docs/features/openapi-sync-ux.md`
- `docs/features/console-masking.md`, `docs/features/collection-trust.md`
- Master roadmap items R-15, R-22, R-24.
