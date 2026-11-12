# rok.grpc Plan 03: Typings and Docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Editor IntelliSense for `rok.grpc`, a feature doc, a manual test, updated crate guides, parity notes and the roadmap.

**Architecture:** Docs and one typings file. No runtime change.

**Tech Stack:** TypeScript (Monaco typings), Markdown.

**Spec:** `docs/superpowers/specs/2026-11-12-rok-grpc-design.md`. Plans 01 and 02 must be merged first.

## Global Constraints

- Docs match the code as merged, not the plan. Check each fact (limits, member names, error prefixes, Developer-mode text) against the code before writing it.
- Say in the feature doc and the parity notes that Bruno's gRPC scripting API could not be checked, so the names are Rocket-native.
- Roadmap: R-05 becomes `done` with the commit range. Add follow-ups as new rows at the end of section 1, numbering from the next free id after R-87: interactive streaming from scripts, `rok.runRequest` support for gRPC items, server reflection from scripts, gRPC calls in Flow nodes, aligning names with Bruno.
- Plan-index rulings 5 and 6 apply. No emojis.

## Review Focus

- The typings must make `await rok.grpc.run(...)` return a typed result, and flag a call with both `message` and `messages`.
- The manual test must say which server to run (the greeter fixture) and what to see.

---

### Task 1: Typings

**Files:**
- Modify: `src/components/editor/rok-types.ts`
- Test: the existing typings test file for `rok-types.ts` (find it with `rg -l "rok-types" src --glob "*.test.*"`; add a case there)

**Interfaces:**
- Produces: `rok.grpc.run(path: string, options?: GrpcScriptOptions): Promise<GrpcScriptResult>` and `rok.grpc.call(options: GrpcAdHocOptions): Promise<GrpcScriptResult>`, each with a callback overload `(err, result)`. `GrpcScriptResult` has `status: { code: number; name: string; message: string }`, `message?: unknown`, `messages: unknown[]`, `headers: Record<string, string>`, `trailers: Record<string, string>`, `truncated: boolean`. Options as in the spec. Doc comments state Developer mode, the caps and the non-interactive streaming model.

- [ ] **Step 1: Write a failing test** that the typings text contains `grpc:` under `rok`, the members `run` and `call`, and the result members.
- [ ] **Step 2: Run** `bun run test rok-types` and expect failure.
- [ ] **Step 3: Implement** the typings.
- [ ] **Step 4: Run** `bun run test rok-types`, `bun run tsc --noEmit`, `bun run check`. Expect pass.
- [ ] **Step 5: Stage** explicit paths. Do not commit.

---

### Task 2: Docs, manual test and roadmap

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `docs/features/rok-grpc.md`, `docs/manual-tests/rok-grpc.md`
- Modify: `crates/rocket-grpc/CLAUDE.md`, `crates/rocket-app/CLAUDE.md`, `crates/rocket-scripting/CLAUDE.md`, `crates/rocket-infra/CLAUDE.md`, `.claude/docs-index.md`, `docs/features/rok-api-parity-notes.md` (Part E row), `docs/roadmaps/roadmap.md`

**Interfaces:**
- Consumes: the merged code and plan 01 and 02 reports.

- [ ] **Step 1: Write `docs/features/rok-grpc.md`**: the API, the result shape, limits table, errors, trust rules, how streaming works (non-interactive), a short example per method type, and the follow-ups.
- [ ] **Step 2: Write `docs/manual-tests/rok-grpc.md`**: numbered checks against the greeter fixture server: saved unary, ad hoc unary, server stream, client stream, bidirectional, Safe mode refusal, blocked host, proto outside the collection, a cap (a server that streams past 1000), the time limit, the callback form, a secret variable in a saved request.
- [ ] **Step 3: Update** the four crate guides (new types and the new method in one or two lines each), `.claude/docs-index.md` (one line), the Part E row of the parity notes (implemented, Bruno unverified), and the roadmap (R-05 done with the commit range, new follow-up rows).
- [ ] **Step 4: Verify** every link with `rg` and every fact against the code. Report any mismatch.
- [ ] **Step 5: Stage** explicit paths. Do not commit. Write the report to `.superpowers/sdd/03-typings-and-docs/task-2-report.md`.
