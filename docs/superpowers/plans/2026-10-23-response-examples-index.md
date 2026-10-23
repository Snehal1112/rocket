# Response examples: plan index

Spec: `docs/superpowers/specs/2026-10-23-response-examples-design.md` (approved 2026-10-23).

Run the plans in this order. Each one ends with working, tested software and can merge on its own.

| # | Plan | Tasks | Depends on |
|---|---|---|---|
| 1 | `2026-10-23-response-examples-1-masking.md` | 2 | none |
| 2 | `2026-10-23-response-examples-2-backend.md` | 2 | none (Plan 1 can run first or after) |
| 3 | `2026-10-23-response-examples-3-frontend.md` | 3 | Plan 2 |

Why this order: Plan 1 closes the commit-masking gaps before the UI starts writing real responses into `.yml` files. Plan 2 gives the UI its commands. Plan 3 is the user-visible part.

Shared facts every plan relies on (verified in the code on 2026-10-23):

- Domain type `HttpRequestExample { name, description?, request?, response? }` lives in `crates/rocket-shared/src/action.rs`. `request` and `response` are opaque `serde_yaml::Value`. It already has `#[serde(rename_all = "camelCase")]`.
- On disk (OpenCollection shape, `crates/rocket-infra/src/oc/http.rs`):
  - `request`: `url`, `method`, `headers: [{name, value}]`, `params: [{name, value, type: query|path}]`, `body: {type, data}`. `data` is a string for `json`, `text`, `xml` and `sparql`, and a list of `{name, value}` for `form-urlencoded`.
  - `response`: `status`, `statusText`, `headers: [{name, value}]`, `body: {type: json|text|xml|html|binary, data: string}`.
- Commit masking is `mask_examples` in `crates/rocket-git/src/sanitize.rs`. It reads exactly those keys.
- `save_request` in `crates/rocket-infra/src/fs_collection/requests.rs` keeps the on-disk examples whenever the incoming request has none, so examples need their own commands.
