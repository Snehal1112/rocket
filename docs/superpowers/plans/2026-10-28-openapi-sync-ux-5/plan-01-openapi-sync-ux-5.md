# OpenAPI sync UX 5: View spec detail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** View spec shows, for each operation, the request body schema, response schemas, required security and the spec's servers, so it is a real reference like Bruno's viewer (today it shows parameter names and response codes only).

**Architecture:** `rocket-import` builds a bounded `SchemaView` tree from the parsed spec (references resolved, cycles cut, no examples). `spec_view` carries it. The viewer renders it as an indented plain-text tree.

**Tech Stack:** Rust (`rocket-import`, `rocket-app`, `src-tauri`), React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 8); parity rows "View spec".

## File Structure

| File | Change |
|---|---|
| `crates/rocket-import/src/openapi/sync/{spec.rs,schema_view.rs (new)}` | `SchemaView`, `BodyView`, `SpecOperationView` fields, `ParsedSpec::servers_list` |
| `crates/rocket-app/src/{openapi_sync_view.rs,openapi_sync_spec_view.rs,openapi_sync_spec_view_tests.rs}` | Carry the new fields on `SpecOpRow` and `SpecView` |
| `src-tauri/src/commands/openapi_sync.rs` | DTOs |
| `src/lib/tauri-api.ts`, `src/components/collections/openapi-sync/{OpenApiSpecView.tsx,SchemaTree.tsx (new)}` | Types and rendering |

## Review Focus

1. A self-referencing schema (`Node` holding `Node[]`) must not loop or blow up. Pinned by `a_cyclic_schema_is_cut_with_a_reference_name`.
2. A huge schema must stay bounded (depth 6, 200 nodes) and say it was cut. Pinned by `a_large_schema_is_capped_and_marked`.
3. Examples and defaults must not be exposed (they can hold secrets). Pinned by `examples_and_defaults_are_not_in_the_view`.
4. Descriptions render as plain text. Pinned by `schema_text_is_rendered_as_plain_text`.
5. Opening View spec still never fetches. Pinned by the existing `never_touches_the_network`, kept green.

---

### Task 1: Schema read model

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Interfaces:**
- Consumes: `ParsedSpec` (`spec.rs`), `operation_views()`, the converter's schema resolution (`converter/openapi.rs:170 sample_json` shows how `$ref`, `allOf`, `oneOf` are walked), `SpecOperationView`.
- Produces: `SchemaView { kind: String, format: Option<String>, description: Option<String>, required: bool, nullable: bool, enum_values: Vec<String>, ref_name: Option<String>, properties: Vec<(String, SchemaView)>, items: Option<Box<SchemaView>>, variants: Vec<SchemaView>, cut: bool }` (`kind` is `object`, `array`, `string`, `integer`, `number`, `boolean`, `null` or `any`; `enum_values` holds at most 20; `variants` is for `oneOf`/`anyOf`, `allOf` is merged into `properties`); `BodyView { content_type: String, required: bool, schema: Option<SchemaView> }`; new fields `SpecOperationView.request_body: Option<BodyView>`, `SpecResponseView.bodies: Vec<BodyView>`, `SpecOperationView.security: Vec<String>` (scheme names; empty means none); `ParsedSpec::server_urls(&self) -> Vec<String>` (at most 10). Limits: depth 6, 200 nodes per schema; a cut node has `cut: true`.

- [ ] **Step 1: Read** `spec.rs` (`operation_views`, `SpecResponseView`) and how the converter resolves refs.
- [ ] **Step 2: Write the failing tests** (`schema_view.rs`): `an_object_schema_lists_properties_with_required_flags`, `an_array_schema_has_items`, `allof_is_merged_into_properties`, `oneof_lists_variants`, `a_cyclic_schema_is_cut_with_a_reference_name`, `a_large_schema_is_capped_and_marked`, `examples_and_defaults_are_not_in_the_view`, `enum_values_are_capped_at_20`, `a_request_body_lists_each_content_type`, `responses_carry_their_bodies`, `security_lists_scheme_names`, `server_urls_are_listed_up_to_10`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-import schema_view`. Expected: module not found.
- [ ] **Step 4: Implement** the builder; keep `doc()` `pub(crate)` and expose only the view types (re-export like `ParsedSpec`).
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-import openapi`. Expected: green.

### Task 2: Service and DTOs

**Interfaces:**
- Consumes: Task 1; `SpecOpRow`, `SpecView` (`openapi_sync_view.rs`), `spec_view` (`openapi_sync_spec_view.rs`), the DTOs of `openapi_sync_spec_view`.
- Produces: `SpecOpRow.request_body`, `.response_bodies` (per status), `.security`; `SpecView.servers: Vec<String>`; DTO keys `requestBody`, `contentType`, `properties` as `[{ name, schema }]`, `refName`, `enumValues`, `cut`, `servers`, `security`.

- [ ] **Step 1: Read** the existing spec view tests and DTO test.
- [ ] **Step 2: Write the failing tests:** `spec_view_carries_the_request_body_and_responses`, `spec_view_carries_servers_and_security`, `an_old_pending_copy_without_bodies_still_loads`, `schema_dto_uses_camel_case_keys`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app spec_view`, `cargo test -j4 -p rocket schema_dto`.
- [ ] **Step 4: Implement** the fields and mapping.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app spec_view`, `cargo check -j4`. Expected: green.

### Task 3: Viewer rendering

**Interfaces:**
- Consumes: Task 2 types in `tauri-api.ts` (`OpenApiSchemaView`, `OpenApiBodyView`), `OpenApiSpecView.tsx` (expanded row), `Badge`, `Collapsible`.
- Produces: `SchemaTree({ schema, name, depth }: { schema: OpenApiSchemaView; name?: string; depth?: number })` rendering `name: type (format)`, `required`, `nullable`, enum values, description as plain text, children indented, "Shown partly, the schema is large." when `cut`, and "Same as <refName>" for a cycle; the expanded operation shows **Request body** (content type, schema), **Responses** (status, description, content types, schema on demand), **Security** (scheme names or "None"); the page header lists **Servers**.

- [ ] **Step 1: Read** `OpenApiSpecView.tsx` and its tests.
- [ ] **Step 2: Write the failing tests:** `an_expanded_operation_shows_the_request_body_schema`, `a_response_schema_opens_on_demand`, `schema_text_is_rendered_as_plain_text`, `a_cut_schema_says_it_is_partial`, `a_cycle_shows_same_as_reference`, `security_none_is_shown`, `servers_are_listed_in_the_header`, `an_operation_without_a_body_shows_no_body_section`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSpecView SchemaTree`.
- [ ] **Step 4: Implement** the tree and sections.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

Next plan: docs/superpowers/plans/2026-10-28-openapi-sync-ux-5/plan-02-openapi-sync-ux-5.md
