# Contract Collaboration Enhancements

Status: ideas and direction only. No spec or code yet.
Date: 2026-10-07.

## Constraints (decided)

- Local first. All contract data lives on disk in the workspace, as files.
- Sharing happens only through git. No Rocket server, no accounts, no hosted
  state, no push notifications.
- Git work uses the `git2` crate through `rocket-git`. Never shell out to the
  `git` CLI.
- Persistence stays backward compatible: new fields are optional with defaults.
  camelCase renames apply to IPC DTOs only, never to persistence structs.

## What exists today

Read this from the docs only. The code was not re-checked, so verify before
building on it.

- Lifecycle states: Draft, Active, Drift, Breach, InReview, Paused,
  ExpiringIn30Days, Expired, Archived. See `docs/contract-state-machine.md`.
- Specs under `docs/superpowers/specs/`: contract lock, lock enhancement,
  tab UI, diff pane, DTO/persistence split, lifecycle gaps, OpenAPI export.
- Pitch pain points the feature should answer: informal contracts, drift,
  breaking changes found late, no ownership or dependency visibility
  (`docs/svp-api-collaboration-pitch.md`).

## Enhancements, ranked

### 1. Provider and consumer roles

- Already partly built: `Contract` has `provider` and `consumers` (both
  `ContractParty`), plus `version` and `expiry_date`
  (`crates/rocket-collection/src/contract/types.rs`). What is missing is the
  consumer pin file and the pre-publish impact list below.
- A contract names its provider (owner) and zero or more consumers.
- Stored as optional fields in the contract file, so older files still load.
- Git-only meaning: a consumer subscribes by committing a pin file that names
  the contract and the version it depends on.
- A provider change that would move a contract to Drift or Breach lists the
  known consumers before it is published.
- InReview maps to a branch or pull request on the git host. Rocket records the
  state; the git host does the approving.

### 2. Dependency map

- Link contracts to the flows, collections and requests that use them.
- Derive links locally from workspace files, for example a flow Request node
  that points at a saved request covered by a contract.
- View: "who depends on this contract" and "what contracts does this flow use".
- Link rule for v1: a contract covers a flow Request node when the node's
  `RequestSource::Saved { request_path }` falls inside the contract's `scope`
  (`Collection`, `Folder { rel_path }` or `Request { rel_path }`). Inline
  requests are never linked. Links are computed on open and never stored, so
  nothing new is committed and merges cannot conflict.
- Decided 2026-10-07: a flow covered by a contract in Breach or Expired only
  shows a warning before the run. It never blocks the run, whatever the
  contract's enforcement mode says. Revisit blocking later if teams ask for it.
- Related gap: a renamed or moved saved request leaves the flow pointing at the
  old path with no warning. The link scan can flag it.
- Open question: cross-repo consumers. Options are (a) consumers commit pin
  files into the provider repo, (b) the provider repo vendors a read-only copy
  of consumer pins, (c) single-repo workspaces only for v1.

### 3. Breaking-change check on a git diff

- Compare a contract at a base ref against HEAD using `git2`, and classify the
  change as compatible, drift or breaking. Reuse the diff pane logic.
- Show it in the app before commit and in a headless mode.
- Headless mode (CLI) exits non-zero on a breaking change, so a provider's CI
  can block a merge. No Rocket server is needed.

### 4. Version history from git

- Git history is the version history. Published versions are git tags.
- Changelog is generated from the diff between two tags.
- Consumers compare their pinned version with the latest tag.
- Deprecation windows reuse the Expiring and Expired states, with dates stored
  in the contract file.

### 5. Consumer-driven field subsets

- A consumer records only the fields it uses, in its pin file.
- A provider change that touches only unused fields is not a breach for that
  consumer.
- Most design work of the list. Do after 1 to 4.

### 6. Review notes in the repo

- Review comments and approver names live in a review file next to the contract
  and travel with the branch. Rocket shows them; the git host's pull request
  remains the real approval.
- Optional. Skip if the git host review is enough.

### 7. Changes since last sync (replaces notifications)

- No push alerts. After a pull or on workspace open, show a summary:
  "3 contracts changed since you last synced: 1 breaking, 1 drift, 1 compatible".
- Uses the git diff between the last seen commit and HEAD, stored locally.

### 8. Local verification runs (replaces scheduled server checks)

- "Verify all contracts" runs on demand against a chosen environment, using the
  existing run engine.
- Scheduled runs are done by the team's own CI calling the headless mode from
  item 3. Rocket ships no scheduler.

### 9. OpenAPI import

- OpenAPI export exists. Add import so a provider's spec becomes a contract and
  a consumer can start without recording requests.

## Suggested order

1. Roles (1) and dependency map (2), because they add the team model the rest
   needs.
2. Breaking-change check and headless mode (3).
3. Version history (4) and changes since last sync (7).
4. Verification runs (8) and OpenAPI import (9).
5. Field subsets (5) and review notes (6).

## Open questions

- Cross-repo consumers (see item 2). Needs a decision before the spec.
- Where pin files live: inside the consumer's collection folder, or a dedicated
  contracts folder at the workspace root.
- Merge conflicts: contract files are edited by several people, so file format
  should be line-oriented and stable to keep git diffs and merges clean.
- Secrets: contracts must never store secret values, since they are committed.

## Next step

Pick the scope for the first spec (suggested: items 1 and 2), then write it
under `docs/superpowers/specs/` and plan it with the `writing-plans` skill. Read
`docs/superpowers/specs/opencollection-spec-reference.md` first, since contract
persistence touches collection data.
