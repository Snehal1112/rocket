# Variable preview in flow editors

The flow properties panel resolves `{{variable}}` for display with `useCollectionVariableContext(collection)`.
`FlowVariableScope` provides the map to the Input and Request editors; `SingleLineEditor` shows it with
`variableContext`, `readOnlyVariables` (the click popover cannot save) and `hoverPreview` (the hover tooltip).

## What the preview shows

Scopes: dynamic, process, global, collection, vault (always masked) and the active environment of the
flow's collection. A secret entry shows `●●●●`; its value is never put in the DOM.

## What it cannot show

- Inline requests inherit no folder or request variables (the backend gives them none).
- A saved request gets its own folder and request variables at run time. The editor cannot know them.
- Runtime values and wire values exist only during a run.
- Environment `extends` is stored but not resolved anywhere, so inherited variables are unresolved here
  and at run time alike.

## Why the click popover is read-only here

`useVariableCommit` saves to `useEnvStore.activeCollection`, which can differ from the flow's collection.
Do not remove `readOnlyVariables` from a flow editor without changing that hook.
