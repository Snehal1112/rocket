# Contract Check Plan 3: Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the provider's "Check with the team" flow and the consumer's toast and review dialog, plus docs and manual checks.

**Architecture:** API wrappers and a store slice carry the state; the event bridge listens on `network-contract-check`; two dialogs and one contract-view action render it. Diffs use `DiffViewer` with `hideStageToggle`.

**Tech Stack:** React, TypeScript, Zustand, shadcn/ui, lucide-react, Monaco via `DiffViewer`, Vitest, Bun.

**Spec:** `docs/superpowers/specs/2026-11-09-team-network-contract-check-design.md` (section 8, 10). Index: `00-plan-index.md`. Needs Plan 2.

## Global Constraints

- **Tasks 2 and 3 MUST invoke the `frontend-design:frontend-design` skill before writing any component**, then follow the hard rules: shadcn/ui primitives only (no raw button, input, dialog, select, form), lucide-react icons only, Monaco only for multi-line diffs, never fully destructure Zustand store state at the top of a component, Bun only.
- Fixed texts only; never render peer strings as markup. Peer strings (contract name, field) render as plain text.
- Texts: toast `"<name>" wants to check a change to "<contract>" (N changes)`; rows `Pass`, `Break`, `Not used`, `No answer yet`, `Declined`, `Busy`; buttons `Send result`, `Decline`; provider error `Check fewer changes at a time.`
- Event payload fields are snake_case, command args camelCase.
- Components stay under ~250 lines; split by responsibility. Match the style of `IncomingSyncToast.tsx`, `SyncReviewDialog.tsx`, `SyncConflictDiff.tsx`.

## Review Focus

- Offer expires while the dialog is open: dialog switches to a fixed "This check expired." and the buttons disable (Task 2 test).
- Two simultaneous offers: both toasts, one dialog at a time, no overwritten state (Task 1 and 2 tests).
- A contract name containing `<script>` or markdown: shown as plain text (Task 2 test).
- A teammate who never answers: row stays `No answer yet` after the provider closes and reopens the view (Task 3 test).
- Zero online paired teammates: the picker shows an empty state and the send button is disabled (Task 3 test).

---

### Task 1: API, store and event bridge

**Files:**
- Modify: `src/lib/tauri-api.ts` (types and wrappers after the network sync section ~l.4302-4640)
- Modify: `src/stores/network-store.ts` (a contract-check slice)
- Modify: `src/lib/network-event-bridge.ts` (listener, toast, expiry)
- Create: `src/lib/network-contract-check-utils.ts` (verdict labels, toast ids, text builders)
- Test: `src/lib/__tests__/tauri-api.network.test.ts` (extend), `src/stores/__tests__/network-store.contract-check.test.ts`, `src/lib/__tests__/network-contract-check-utils.test.ts`, bridge test next to the existing bridge tests

**Interfaces:**
- Produces:

```ts
export type ContractVerdict = 'pass' | 'break' | 'not_used';
export type ContractCheckOutcome = 'answered' | 'declined' | 'busy';
export interface ContractCheckRowDto { index: number; verdict: ContractVerdict }
export type NetworkContractCheckEvent =
  | { type: 'offerReceived'; check_id: string; device_id: string; contract: string; change_count: number }
  | { type: 'resultReceived'; check_id: string; device_id: string; outcome: ContractCheckOutcome; rows: ContractCheckRowDto[] }
  | { type: 'expired'; check_id: string };
export const startContractCheck: (collectionRoot: string, contractId: string, deviceIds: string[]) => Promise<ContractCheckStarted>;
export const reviewContractCheck: (checkId: string) => Promise<ContractCheckReview>;
export const replyContractCheck: (checkId: string) => Promise<void>;
export const declineContractCheck: (checkId: string) => Promise<void>;
export const onNetworkContractCheck: (handler: (e: NetworkContractCheckEvent) => void) => Promise<UnlistenFn>;
// store slice: contractChecksIncoming, contractCheckResults (by checkId/deviceId), dialogContractCheckId,
// addIncomingContractCheck, removeIncomingContractCheck, setContractCheckResult, openContractCheckDialog, closeContractCheckDialog
```

- [ ] **Step 1: Write failing tests**: wrapper arg names (`collectionRoot`, `contractId`, `deviceIds`, `checkId`); store: add/remove incoming, result keeps `No answer yet` until set, expired removes incoming and marks result `expired`, two offers coexist; utils: `verdictLabel('not_used') === 'Not used'`, toast text for 1 and many changes; bridge: `offerReceived` shows a toast, `resultReceived` updates the store, `expired` clears.
- [ ] **Step 2: Run** `bun run test contract-check` — expect FAIL.
- [ ] **Step 3: Implement** the wrappers, slice (narrow selectors only), bridge cases and utils.
- [ ] **Step 4: Run** `bun run test contract-check`, `bun run tsc --noEmit`, `bun run check` — expect PASS.
- [ ] **Step 5: Commit** `feat(network): add contract check api and store`.

### Task 2: Consumer toast and review dialog

> **Invoke `frontend-design:frontend-design` first.**

**Files:**
- Create: `src/components/network/IncomingContractCheckToast.tsx`, `ContractCheckReviewDialog.tsx`, `ContractCheckChangeDiff.tsx` (before and after request diff through `DiffViewer`)
- Modify: `src/components/network/NetworkPanel.tsx` (mount the dialog), `src/lib/network-event-bridge.ts` (toast action opens the dialog)
- Test: `src/components/network/__tests__/ContractCheckReviewDialog.test.tsx`, `IncomingContractCheckToast.test.tsx`

**Interfaces:**
- Consumes: Task 1 API and store; `reviewContractCheck` returns per change `{ index, field, changeType, breaking, matches: string[], verdict }`.

- [ ] **Step 1: Write failing tests**: toast shows the fixed text and a Review action; dialog lists per change the local matches, verdict badge and the before and after diff; `Send result` calls `replyContractCheck` once and closes; `Decline` calls `declineContractCheck`; buttons disable after `expired`; `<script>` in the contract name is plain text; keyboard: Escape closes without replying.
- [ ] **Step 2: Run** `bun run test ContractCheck` — expect FAIL.
- [ ] **Step 3: Implement** the three components (shadcn `Dialog`, `Badge`, `Button`, `ScrollArea`; lucide icons such as `ShieldCheck`, `ShieldAlert`, `CircleSlash`). Visual direction comes from the design skill; the colors use the existing theme tokens.
- [ ] **Step 4: Run** `bun run test network`, `bun run tsc --noEmit`, `bun run check` — expect PASS.
- [ ] **Step 5: Commit** `feat(network): review incoming contract checks`.

### Task 3: Provider view, docs and manual checks

> **Invoke `frontend-design:frontend-design` first.**

**Files:**
- Create: `src/components/contracts/ContractCheckAction.tsx` (button and node picker), `ContractCheckResults.tsx` (per-teammate rows beside the diff)
- Modify: `src/components/contracts/ContractDiffPane.tsx` (mount the action and the results)
- Modify: `docs/features/team-network.md` (contract check section), `docs/manual-tests/team-network.md` (new section 13), `docs/roadmaps/roadmap.md` (R-83 row), `.claude/docs-index.md` if its team-network line needs the new topic, `.claude/sprint-plan.md` (move contract check from "next" to built)
- Test: `src/components/contracts/__tests__/ContractCheckAction.test.tsx`, `ContractCheckResults.test.tsx`

**Interfaces:**
- Consumes: Task 1 API and store; `listNetworkNodes`-style online paired nodes selector already in `network-store.ts`.

- [ ] **Step 1: Write failing tests**: action disabled with a tooltip text when the contract has no changes; picker lists only online paired nodes and shows an empty state with the send button disabled; `Check fewer changes at a time.` is shown on that backend error; results show Pass, Break with the field, Not used, No answer yet, Declined, Busy; closing and reopening keeps `No answer yet`.
- [ ] **Step 2: Run** `bun run test ContractCheck` — expect FAIL.
- [ ] **Step 3: Implement** the components with shadcn `Popover`/`Command` or `Dialog`, `Checkbox`, `Badge`, lucide icons. The preview reuses `DiffViewer` with `hideStageToggle`. Write manual checks for section 13 (about 12): offer, review, reply, decline, busy (4th offer), expiry, older build, wrong-workspace, secret hidden, nothing written to files, audit rows, no match all `not_used`.
- [ ] **Step 4: Run** `bun run test network`, `bun run test contracts`, `bun run tsc --noEmit`, `bun run check` — expect PASS.
- [ ] **Step 5: Commit** `feat(network): check contract changes with the team`, then a separate `docs(network): add contract check manual checks`.
