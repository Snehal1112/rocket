# Runner data: `{{column}}` variables from the data row

## Goal

A request sent inside a data-driven pass fills `{{column}}` placeholders from the current row of the data file, so a CSV or JSON file can drive URLs, headers and bodies without a script.

## Overview

Roadmap item R-84. The Runner tab and the backend runner already run one pass per row and give every send the row as `ExecuteRequestInput.iteration` (`crates/rocket-app/src/execution_service.rs:149`). Only scripts see it, through `rok.runner.iterationData`. A user who wants `https://api.test/users/{{id}}` must write a pre-request script that calls `rok.setVar`. This item makes the placeholder work directly.

## Current state

| Piece | Where | Today |
|---|---|---|
| Row type | `IterationInfo { index, total, data: BTreeMap<String, String> }`, `crates/rocket-scripting/src/context.rs:50` | Read by scripts only. |
| Layers | `VariableContext::layers()`, `crates/rocket-environment/src/context.rs:~108`: global_env, collection, external_secrets, env, folder, request, runtime | No row layer. This is the only place the order is written. |
| Send path | `resolve_request_parts`, `execution_service.rs:~1775` builds scopes then `expanded_send` (`:455`) | The row is not read. |
| Variable map for script-started sends | `send_variable_map_pending`, `execution_service.rs:1292` (used by `rok.grpc.run`) | The row is not read. |
| Nested runs | `inherit_iteration`, `execution_service/script_host.rs:95` | A nested `rok.runRequest` keeps the caller's row. |
| Script variable scopes | `var_ctx` at `execution_service.rs:~2787` | Per-scope maps read by `rok.getVar` and friends. |

Gap: the row cannot fill a placeholder.

## Non-goals

- `rok.getVar` or any script API returning a column (scripts keep `rok.runner.iterationData`).
- Marking columns secret or masking them.
- Editor highlighting or autocomplete of column names.
- Row filters, delays, data files in load tests or Flow (R-85), runner parity (R-86).

## Design

Flow: row (`input.iteration.data`) -> filtered column map -> new `data` layer of `VariableContext` -> `expanded_send` -> request parts.

### 1. A `data` layer in `rocket-environment`

`VariableContext` gets `pub data: HashMap<String, String>`. `layers()` returns eight layers, lowest first: global_env, collection, external_secrets, env, folder, **data**, request, runtime. `flatten` and `flatten_with_process_env` read `layers()`. Nested expansion (`is_expandable` in `nested.rs`) keeps its own list, which does not include `data`: row values are used literally. Precedence is the user's decision: the row beats folder, environment and collection variables; a request variable and a runtime variable beat the row.

### 2. A column filter in `rocket-app`

`fn iteration_columns(info: &IterationInfo) -> HashMap<String, String>` copies the row and drops every key that starts with `process.env.`, so a column cannot replace a host environment value. Other names, dots included, are kept as they are. The data file loader is unchanged.

### 3. Fill the layer on every send

Where `build_variable_scopes` is followed by `expanded_send` for a send (`resolve_request_parts`, the second expansion after pre-request scripts, and `send_variable_map_pending`), set `scopes.data = iteration_columns(info)` when `input.iteration` is `Some`. The builder finds every such call with `rg "expanded_send\("` and treats a send path that skips the layer as a defect. The layer is for placeholder expansion only. `rok.interpolate` resolves against `variables.flatten()` (`crates/rocket-infra/src/scripting/ops/rok.rs:207`), so the row must stay out of the variables handed to the script engine (`ScriptInputState`, built from `var_ctx` at `:~2787`). The second expansion (`second_pass_values_recording`, `:489-513`) compares `state.var_ctx.flatten()` with `templates.first_raw` and passes changed names to `expand_nested_recording` as `extra`, which is laid on top of every layer (`nested.rs:104,147`). The columns must therefore never go through `extra`. Store them on `PhaseState` (for example `state.row_columns`). `second_pass_values_recording` expands from a copy of `var_ctx` with `data` set to those columns, and `first_raw` (`:2810`) is built from the same copy, so both sides of the compare see the same layers and a shadowed name is not counted as changed. No `rok.*` function (`getVar`, `interpolate`, the scope getters) returns a column.

Example: row `{id: "7"}`, collection variable `id=1`, URL `https://x/{{id}}` -> `https://x/7`. With a request variable `id=9` the URL is `https://x/9`.

### 4. Docs

Add the layer to `docs/superpowers/specs/opencollection-spec-reference.md` section 6 as a Rocket extension (not in the OpenCollection schema), update `docs/features/runner-iteration-data.md`, add a manual check, update `crates/rocket-environment/CLAUDE.md`, and mark R-84 done.

## Error handling and edge cases

| Case | Behaviour |
|---|---|
| Column missing in the row | The placeholder stays unresolved, as for any unknown variable. |
| Empty cell | Fills with the empty string. |
| Column named `process.env.X` | Not offered as a variable. `rok.runner.iterationData.get` still returns it. |
| Same name in request and row | The request variable wins. |
| Same name as a runtime variable | The runtime variable wins. |
| Env value that mentions a column (`{{a}}`) | The column fills it, because expansion reads the merged layers. The column value itself is not expanded. |
| Send without a pass (`iteration` is `None`) | No change. |
| Column value contains `{{x}}` | Used literally, like a runtime value. `is_expandable` (`crates/rocket-environment/src/nested.rs:145-160`) gets `if ctx.data.contains_key(name) { return false; }` after the `request` check and before folder and env, because it returns true for a name in folder or env even when the row won the merge. Tests pin both a row-only name and a row name that shadows an env variable. |
| Column name equals a secret variable name (`secret_keys`) | The row value is added to the masked values, so history and the Console mask it. |
| Column name equals a RocketVault key (`alias.secretName`) | The column replaces it in top-level resolution, as an environment variable already does. This is intended. |
| Column name with spaces or odd characters | Kept as written. It matches only a placeholder spelled the same. |

## Testing

`rocket-environment` (`cargo test -j4 -p rocket-environment`):
- `data_beats_folder_env_and_collection`
- `request_beats_data`
- `runtime_beats_data`

`rocket-app` (`cargo test -j4 -p rocket-app execution_service`):
- `row_column_fills_a_url_placeholder`
- `row_column_overrides_an_environment_variable`
- `request_variable_beats_a_row_column`
- `process_env_columns_are_not_variables`
- `second_expansion_after_scripts_keeps_the_row` (env `id=1`, row `id=7`, a pre-request script sets an unrelated variable, the sent URL ends in `/7`)
- `second_pass_keeps_request_variable_over_row` (request variable `id=9`, row `id=7`, a pre-request script sets an unrelated variable, the sent URL ends in `/9`)
- `column_overriding_an_env_variable_is_not_expanded` (row `id={{other}}`, env `id=1`)
- `rok_get_var_does_not_return_a_column`
- `rok_interpolate_does_not_return_a_column`
- `column_value_with_placeholder_is_not_expanded`
- `column_named_like_a_secret_variable_is_masked`
- `send_without_iteration_is_unchanged`
- `nested_run_request_fills_from_the_inherited_row`
- `grpc_run_fills_from_the_row` (script host test)

Commands: `bun run tsc --noEmit` is not needed, there is no frontend change.

## Files changed

- `crates/rocket-environment/src/context.rs` (layer, tests)
- `crates/rocket-app/src/execution_service.rs` (`iteration_columns`, calls, tests)
- `crates/rocket-app/src/execution_service/script_host.rs` (grpc map, test)
- docs: spec reference section 6, `docs/features/runner-iteration-data.md`, `docs/manual-tests/runner-iteration-data.md`, `crates/rocket-environment/CLAUDE.md`, `docs/roadmaps/roadmap.md`

## Constraints

- DDD: the layer lives in the domain crate, the filter and wiring in `rocket-app`. No new trait, no file reads in `rocket-app`.
- Tests use `.expect("reason")`, never the unwrap call.
- Persistence is untouched, so there is no compatibility concern.
- Read `docs/superpowers/specs/opencollection-spec-reference.md` first.

## Acceptance criteria

- A column fills a `{{column}}` placeholder in a URL, a header and a body during a pass.
- The row beats folder, environment and collection variables.
- A request variable and a runtime variable beat the row.
- Columns named `process.env.*` never become variables.
- A send without a pass behaves as before.
- A nested `rok.runRequest` and `rok.grpc.run` fill from the same row.
- Scripts cannot read a column through `rok.getVar` or `rok.interpolate`.
- A column value is not expanded, and a column named like a secret variable is masked.
- Docs and the roadmap row are updated.

## Out of scope

Column highlighting in editors, secret columns, filters and delays (R-85), runner parity (R-86).

## Follow-ups

- Editor support: highlight and complete `{{column}}` names while a data file is chosen.

## Decisions

- Where does the row sit in the variable order? Options: under request variables and above folder/env, just under runtime, lowest scope. Chosen: under request variables, above folder/env. Decided by: user
- Can scripts read a column as a variable? Options: no, only `{{col}}`; also `rok.getVar`; as runtime vars. Chosen: only `{{col}}`. Decided by: user
- Which sends fill from the row? Options: every send in the pass; runner sends only; Runner tab only. Chosen: every send in the pass. Decided by: user
- How are reserved `process.env.*` column names handled? Options: skip them; reject the file; allow. Chosen: skip them. Decided by: user
- Are column values expanded when they contain `{{x}}`? Options: literal like runtime values; expanded like env values. Chosen: literal. Decided by: user
- A column shares its name with a secret variable: what happens to the value? Options: mask it; skip the column; leave it unmasked. Chosen: mask it. Decided by: user
- Are other column values masked in History, Console and logged URLs? Options: only secret-named columns; every column; a per-file secret flag. Chosen: only secret-named columns, documented in the feature doc so a user knows to name a sensitive column like a secret variable. Decided by: user
