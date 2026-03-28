# Variable-Aware URL Input Design (Postman-like Hover Edit)

## Context
The request URL currently accepts `{{variable}}` placeholders and substitutes them at send time, but variables are not visually highlighted inline and cannot be edited from URL hover interactions like Postman.

## Goal
Add Postman-like inline variable highlighting and hover-edit in the URL input, without adding a second resolved-preview line.

## Confirmed Behavior
- URL input keeps template text (e.g., `https://{{host}}/users`).
- Variables are highlighted inline in the same input row.
- Hovering a variable opens a quick editor popover.
- Save precedence:
  1. update active environment variable if it exists
  2. else update collection variable if it exists
  3. else create variable in active environment if selected; otherwise collection vars

## Non-Goals
- Replacing request substitution logic at send time.
- ContentEditable-based URL field rewrite.
- Adding a separate resolved URL preview row.

## Architecture
1. Create `VariableAwareUrlInput` component:
   - controlled `value`, `onChange`
   - overlay token rendering for `{{var}}` highlights
   - hover anchor + edit popover

2. Build variable lookup from store:
   - `activeEnvironment.variables`
   - `collectionVariables`
   - environment wins on conflicts

3. Apply save updates through existing store APIs:
   - `saveEnvironment(collectionName, env)`
   - `saveCollectionVariables(collectionName, vars)`

4. Integrate in `RequestBuilder` URL section; keep submit/save/send flow unchanged.

## UX Details
- Token style variants:
  - env-backed token: positive/primary style
  - collection-backed token: neutral accent style
  - missing token: warning/destructive tint
- Hover editor:
  - variable name label
  - editable value field
  - Save/Cancel actions

## Validation
- `lint`, `test`, and `build` must pass.
- Manual checks:
  - highlight appears while typing and loading saved requests
  - hover edit updates correct variable source
  - unresolved tokens keep warning style

## Acceptance Criteria
- Variables in URL are highlighted inline with no second preview line.
- Hover edit updates env first, then collection fallback.
- Existing request send/save behavior remains unchanged.
