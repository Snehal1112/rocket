# Runner data column variables Implementation Plan

> **Spec:** docs/superpowers/specs/2026-11-14-runner-data-column-variables-design.md
> **Executor:** the factory driver runs this plan with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** A request sent inside a data-driven pass fills `{{column}}` from the current row, below the request and runtime variables and above folder, environment and collection variables.

**Architecture:** A new `data` layer in `VariableContext` (domain crate). `rocket-app` fills it from `input.iteration` through one helper on every send path that calls `expanded_send`. No frontend change.

**Tech Stack:** Rust, `cargo test -j4 -p <crate>`.

## File Structure

- `crates/rocket-environment/src/context.rs`: `data` field, `layers()` of eight, tests.
- `crates/rocket-app/src/execution_service.rs`: `iteration_columns`, the calls, tests.
- `crates/rocket-app/src/execution_service/script_host.rs`: the `rok.grpc.run` map, test.
- Docs: `docs/superpowers/specs/opencollection-spec-reference.md` (section 6), `docs/features/runner-iteration-data.md`, `docs/manual-tests/runner-iteration-data.md`, `crates/rocket-environment/CLAUDE.md`, `docs/roadmaps/roadmap.md`.

## Review Focus

1. A column named `process.env.HOME` must not change `{{process.env.HOME}}`. Test: `process_env_columns_are_not_variables`.
2. A pre-request script that sets a variable triggers the second expansion, and the row must still fill. Test: `second_expansion_after_scripts_keeps_the_row`.
3. `rok.getVar('id')` must not return the column `id`. Test: `rok_get_var_does_not_return_a_column`.
4. A send without a pass must be unchanged. Test: `send_without_iteration_is_unchanged`.
5. A request variable of the same name must beat the column. Test: `request_variable_beats_a_row_column`.

---

### Task 1: The `data` layer

**Interfaces:**
- Produces: `VariableContext.data: HashMap<String, String>`; `layers()` returns `[&HashMap<String, String>; 8]` in the order global_env, collection, external_secrets, env, folder, data, request, runtime.

- [ ] **Step 1: Read** `docs/superpowers/specs/opencollection-spec-reference.md` section 6, `crates/rocket-environment/CLAUDE.md`, and `crates/rocket-environment/src/context.rs` (`layers`, the tests from `env_beats_collection`).
- [ ] **Step 2: Write the failing tests** in `context.rs`, built like `folder_beats_env`:

```rust
#[test]
fn data_beats_folder_env_and_collection() {
    let ctx = VariableContext {
        collection: m(&[("k", "col")]),
        env: m(&[("k", "env")]),
        folder: m(&[("k", "folder")]),
        data: m(&[("k", "row")]),
        ..Default::default()
    };
    assert_eq!(ctx.flatten().get("k").map(String::as_str), Some("row"));
}
```

Add `request_beats_data` (request `req`, data `row`, expects `req`) and `runtime_beats_data` (runtime `run`, data `row`, expects `run`).
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-environment data_beats` and expect `no field named 'data'`.
- [ ] **Step 4: Implement** the field and put `&self.data` between `&self.folder` and `&self.request` in `layers()`; update the comment on the insertion order in `flatten`. Fix every struct literal in the workspace that names all fields (find with `cargo check -j4 -p rocket-app --tests`).
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-environment` and expect all pass.

### Task 2: Fill the layer on every send

**Interfaces:**
- Consumes: `VariableContext.data` (Task 1); `IterationInfo.data: BTreeMap<String, String>` (`rocket_scripting`).
- Produces: `pub(crate) fn iteration_columns(info: &rocket_scripting::IterationInfo) -> std::collections::HashMap<String, String>` in `execution_service.rs`. It drops keys that start with `process.env.` and copies the rest unchanged.

- [ ] **Step 1: Read** the reference doc line (`docs/superpowers/specs/opencollection-spec-reference.md`), and in `execution_service.rs`: `expanded_send` (`:455`), `second_pass_values` (`:489`), `resolve_request_parts` (`:1763`), `send_variable_map_pending` (`:1292`), the `var_ctx` build (search `let mut var_ctx`), and the test helpers `sample_input`, `build_svc_with_script`, `contexts_for_all_phases`. List every `expanded_send(` call with `rg "expanded_send\("`.
- [ ] **Step 2: Write the failing tests** in the `execution_service.rs` test module, reusing `sample_input` and the helper that already asserts the sent URL (find it with `rg "sent_url|captured_url|last_request" crates/rocket-app/src/execution_service.rs`). Each sets `input.iteration = Some(IterationInfo { index: 0, total: 1, data: BTreeMap::from([("id".into(), "7".into())]) })` and the URL `https://x.test/{{id}}`:
  - `row_column_fills_a_url_placeholder`: sent URL is `https://x.test/7`.
  - `row_column_overrides_an_environment_variable`: env variable `id=1` with the same input, sent URL still `.../7`.
  - `request_variable_beats_a_row_column`: request variable `id=9`, sent URL `.../9`.
  - `process_env_columns_are_not_variables`: row key `process.env.ROCKET_TEST_COL` = `x`; unit test of `iteration_columns` asserts the key is absent and `id` is present.
  - `second_expansion_after_scripts_keeps_the_row`: pre-request script sets a runtime variable, then the sent URL is still `.../7`.
  - `rok_get_var_does_not_return_a_column`: a post-response script logs `rok.getVar('id')`; the Console line shows `undefined`.
  - `send_without_iteration_is_unchanged`: no iteration, URL keeps `{{id}}` unresolved.
  - `nested_run_request_fills_from_the_inherited_row`: a nested send (see the `inherit_iteration` tests in `script_host_tests.rs`) sends `.../7`.
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app execution_service row_column` and expect failure `cannot find function 'iteration_columns'` or assertion `.../{{id}}` vs `.../7`.
- [ ] **Step 4: Implement** `iteration_columns`, and set `scopes.data` (and `var_ctx.data`) from `input.iteration` right after each `build_variable_scopes` result that feeds `expanded_send`, `second_pass_values` or the script state. Check every `rok.get*Var` getter in `rocket-infra` or `rocket-app` against `layers()`: if `getVar` reads `flatten()`, route it through a flatten that leaves `data` out (add `flatten_without_data` in `context.rs` with its own test) so the sixth test passes.
- [ ] **Step 5: Add** `grpc_run_fills_from_the_row` in `script_host_tests.rs`, built like the existing `grpc_fills_variables_from_scopes_and_runtime_vars`: a saved gRPC request with `{{id}}` in its URL or message, row `id=7`, and assert the fake executor saw `7`. Implement in `send_variable_map_pending`.
- [ ] **Step 6: Run** `cargo test -j4 -p rocket-app execution_service` and `cargo test -j4 -p rocket-app script_host`; expect all pass. Run `cargo check -j4` for `src-tauri`.

### Task 3: Docs and roadmap

- [ ] **Step 1: Read** the reference doc section 6 and `docs/features/runner-iteration-data.md`.
- [ ] **Step 2: Edit** section 6 of the reference doc: add the layer between folders and request variables as a Rocket extension, with the `process.env.` rule. Edit the feature doc (a new short section "Columns as variables", and remove "Scripts read the row today" style claims that say no variables). Add manual check 23 to `docs/manual-tests/runner-iteration-data.md`: a CSV with `id`, URL `https://httpbin.org/anything/{{id}}`, the sent URL per pass, and a request variable `id` winning. Add the layer to `crates/rocket-environment/CLAUDE.md`. Mark R-84 `done` in `docs/roadmaps/roadmap.md` and add a follow-up row for editor support, numbered after the highest id.
- [ ] **Step 3: Verify** with `rg "R-84" docs/roadmaps/roadmap.md` and check each fact against the code. Expected: the row says done and all links resolve.

Next plan: none
