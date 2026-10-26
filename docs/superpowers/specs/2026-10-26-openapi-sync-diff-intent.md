# OpenAPI sync diff: intent

Date: 2026-10-26. Branch: `openapi-sync-diff`. Part of: `2026-10-25-openapi-import-and-sync-design.md` (Series 2, Plan 5). Plan: `plans/2026-10-25-openapi-import-and-sync/plan-05-diff-and-merge.md`.

## Intent

A team imports an API spec into Rocket, then adds their own work on top: edited values, scripts, tests, docs, saved examples. The spec keeps changing. When it does, the user needs to know exactly what changed, and needs to take the spec's changes without losing any of their own work.

This feature is the part that answers one question: **given the spec as last synced, the user's requests as they are now, and the new spec, what would change, and what would stay as the user has it?** It produces a reviewable plan. It does not change anything itself.

The risk it exists to remove is quiet data loss. A sync that overwrites a user's edit, or deletes a request they changed, destroys trust in the whole sync feature on the first bad day. Everything here is built so that the safe answer is the default answer.

## Goals

- Compare three versions, not two: the spec as last synced (the baseline), the user's current request, and the new spec. Two-way compare cannot tell a user edit from a spec change.
- Work field by field. If the user edited a header and the spec changed the body, both apply. The user's value wins when both changed the same field.
- Sort every endpoint into New, Changed or Removed, and show each changed field with the baseline value, the user's value and the spec's value.
- Treat "removed from the spec" with care. An endpoint the user never touched can be deleted on approval. An endpoint the user edited is kept as a normal request that no longer follows the spec.
- Follow requests wherever the user puts them. A request the user moved, renamed or reordered still syncs. A request the user deleted is not brought back.
- Offer "possibly renamed" as a hint when one endpoint disappears and one appears that look like the same thing, so the user can keep their edits instead of losing them to a remove plus an add.
- Hide changes the user skipped until the spec changes that endpoint again.
- Be cautious when the baseline is missing or unreliable (a collection imported before this feature, a clone of a git repo, a changed converter). In that case nothing of the user's is overwritten and no removal is offered as safe.
- Handle specs of about 5,000 operations without slowdowns.

## Non-goals

- Writing anything to the user's requests. Applying approved changes is Plan 6.
- Fetching the spec or deciding when to check. That is Plans 4 and 7.
- The review screen. That is Plans 8 and 9.
- Merging two different specs into one collection, or syncing several spec versions of one API.
- Merging array contents inside a JSON body. An array is treated as one value.
- Keeping the key order or whitespace of a JSON body when a spec change is merged into it. If nothing is taken from the spec, the user's text is untouched.

## Anti-goals (a failure even if it technically works)

- A sync that overwrites or deletes something the user wrote without their approval.
- Treating a request as unedited when in doubt. When the answer is unknown, the request counts as edited.
- A converter update that makes every request look edited, or every endpoint look changed, with no real change in the spec.
- A skipped change that comes back on every check, or a skip that hides a later real change.
- A rename hint applied without the user's confirmation, or a hint offered when the match is ambiguous.
- Slow planning on a large spec (anything that scans all requests once per endpoint).

## What the user decided

- Follow Bruno's behaviour: user edits, scripts, tests, assertions, docs and examples are kept; merge per field; the user wins on conflict.
- Keep an applied copy of the last synced version. (Refined by the designer, see below.)
- The spec copies live in `.rocket/` and are git-ignored there.
- "Possibly renamed" is wanted; the user left the shape of it to the designer.

## Calls made by the designer (open to objection)

- **The applied copy is a snapshot of converted requests, one per operation, not the spec text.** One spec file cannot record that operation A moved to the new version while operation B stayed. The rule "a skipped change does not move the baseline" needs a per-operation baseline.
- **A skipped change is hidden, not listed.** The spec says both "stays in the list" and "stays hidden until the spec's version differs". The later, more specific rule wins. The plan reports how many are hidden.
- **Rename hints are two rules.** A changed `operationId` on the same method and path; a changed path on the same method and summary. A pair is offered only when it is the single candidate on both sides. A changed `operationId` that keeps the same key already matches and needs no hint.
- **"Edited" ignores `uid`, `file_name` and `seq`.** The first two are identity. `seq` cannot be rebuilt after import and reordering is not editing. Every other field counts, including scripts and examples.
- **The converter has a version number.** If it differs from the one that wrote the baseline, the plan goes cautious instead of reporting a mass edit.
- **A request the user deleted is not recreated** when the spec changes it.
- **Auth:** same type keeps the user's values; a different type takes the spec's.

## How we will know it works

- A user edits a header, a JSON body key and a script on an imported request. The spec changes a different body key and adds a new endpoint. The plan shows one Changed item with the body key taken, the header and script untouched, and one New item.
- The spec removes a param the user changed. The plan shows a conflict, not a silent delete.
- A collection imported before this feature gets a plan that overwrites nothing and offers no deletion.
- Moving a request to another folder, then syncing, still matches it.
- A 5,000-operation spec plans in well under a second.

## Related

- Carry-forward items for later plans: `plans/2026-10-25-openapi-import-and-sync/00-plan-index.md` ("Carry-forward from the Plan 4 final review", "Series 2 decisions").
- Roadmap: R-70 in `docs/roadmaps/roadmap.md`.
