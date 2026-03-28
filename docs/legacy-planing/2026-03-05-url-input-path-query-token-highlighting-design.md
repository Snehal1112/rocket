# URL Input Path/Query Token Highlighting Design

**Date:** 2026-03-05
**Status:** Approved

## Problem
Rocket currently highlights only environment-style URL variables (`{{...}}`) in the URL input. Bruno-style path tokens (for example `:token`) and query token references are not highlighted inline, reducing parity and making missing values harder to detect.

Example:
- `{{BASE_URL}}/api/v3/network_invitations/:token/resend`
- `{{BASE_URL}}` highlights, `:token` does not.

## Goal
Highlight path and query tokens inside the URL input text (only), with resolved/missing visual status similar to existing variable highlighting.

## Non-Goals
- No highlighting changes in Params table rows.
- No request execution behavior change.
- No broad URL editor redesign.

## Selected Approach
Extend URL tokenization in `VariableAwareUrlInput` to include path and query tokens while preserving existing `{{...}}` behavior.

## Architecture
- Keep `VariableAwareUrlInput` as the single source for URL inline token rendering.
- Expand token model to represent token kinds:
  - environment/collection (`{{name}}`)
  - path token (`:name`)
  - query token candidates
- Resolve tokens via layered lookup:
  1. request `pathParams` / `queryParams`
  2. active environment variables
  3. collection variables
- Reuse existing token badge rendering pipeline and resolved/missing style semantics.

## UX Behavior
- Highlighting applies only inside URL input text.
- Existing `{{...}}` edit popover remains unchanged.
- `:token` and query token highlights show status (resolved/missing); no variable-save action from these token types.

## Data Flow
- `RequestBuilder` passes current `pathParams` and `queryParams` into `VariableAwareUrlInput`.
- URL input parses URL text into token spans and renders badges with status classes.

## Validation & Tests
- Add frontend tests to verify:
  1. `:token` path highlighting
  2. query token highlighting
  3. resolved vs missing classes
  4. existing `{{...}}` highlight/edit flow unchanged
- Confirm URL editing is unaffected and request send/save behavior remains unchanged.

## Success Criteria
1. `{{BASE_URL}}` and `:token` both highlight in URL input.
2. Missing path/query tokens are visually distinct from resolved tokens.
3. Behavior feels Bruno-parity for inline URL token visibility.

