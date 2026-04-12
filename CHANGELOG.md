# Changelog

## [0.7.0](https://github.com/Snehal1112/rocket/compare/v0.5.0...v0.7.0) (2026-04-12)

### Features

* **contract-tab:** add ContractTab pane type and openContractTab store action ([c27e48f](https://github.com/Snehal1112/rocket/commit/c27e48fa7453e0bf188cae7bed6f6e59782f3785))
* **contract-tab:** ChangelogSummaryBar and ChangelogTable ([55049cf](https://github.com/Snehal1112/rocket/commit/55049cfd9e2e695b610d5751d7a6e1dedc0fefea))
* **contract-tab:** ContractCard and ContractEmptyState components ([4a8dc56](https://github.com/Snehal1112/rocket/commit/4a8dc56d55723f5d73ee3d912419cae474047758))
* **contract-tab:** ContractForm controlled component ([7bf3f15](https://github.com/Snehal1112/rocket/commit/7bf3f15cba84261afd677a3174c09e1bd40cae4e))
* **contract-tab:** ContractLivePreview and ContractTabTopBar ([921865e](https://github.com/Snehal1112/rocket/commit/921865e4eef86027ecc9942a21c00a45359f8b54))
* **contract-tab:** ContractTab root component with list/create/edit/changelog views ([6c3c9ef](https://github.com/Snehal1112/rocket/commit/6c3c9effb6b90438f89a958fb85a6c4a7c82cada))
* **contract-tab:** delete AttachContractDialog and ContractPanel — fully replaced by ContractTab ([b3216cf](https://github.com/Snehal1112/rocket/commit/b3216cf54d779d7527188e23a33480578650fe72))
* **contract-tab:** FileLock badge on sidebar items and tab improvements ([7fb24b3](https://github.com/Snehal1112/rocket/commit/7fb24b35cc81cba4ea9807cffd13d7c83e5a6079))
* **contract-tab:** implement ContractTab UI, sidebar integration, and cleanup ([254f1e7](https://github.com/Snehal1112/rocket/commit/254f1e7f75ffba78a4e7db1a11ae8717849ce927))
* **contract-tab:** sidebar context menu entry + ContractBadge opens tab ([ac0b13f](https://github.com/Snehal1112/rocket/commit/ac0b13f2229530d7835efed48184293aac5f93ac))
* **contract-tab:** wire ContractTab into EditorGroup routing ([2beabb4](https://github.com/Snehal1112/rocket/commit/2beabb4d423dcb50f499df793d1235f6db954bd6))
* **contract:** domain types, snapshot, changelog, diff_signature ([2f4ae14](https://github.com/Snehal1112/rocket/commit/2f4ae1462feaca5d844b912d46e33aef4b0c1b07))
* **contract:** frontend store, UI, and tests for Contract Lock ([a85239f](https://github.com/Snehal1112/rocket/commit/a85239fd8e464ee6a222c4de3ba95801fd81c04f))
* **contract:** FsContractRepo, ContractService, save hook with Model B seam ([99502ce](https://github.com/Snehal1112/rocket/commit/99502ceaf276f602d5d49fbd75d80debefc10050))
* **contract:** Tauri IPC commands + save hook wired ([cb55868](https://github.com/Snehal1112/rocket/commit/cb55868ef35e1dc65c2e60f65add40fa08de73b1))
* **frontend:** add delay-between-requests input and failure breakdown types ([5a8a74c](https://github.com/Snehal1112/rocket/commit/5a8a74cbf4d65665c53c15ac5593c4fc1c5770d6))
* **frontend:** pass collection and environment context to run_load_test_command ([d4415a6](https://github.com/Snehal1112/rocket/commit/d4415a6e2f44b3d133d9d01da9513b1fb1d55ee7))
* **frontend:** show transport/status failure breakdown in LoadTestDialog ([397bc3a](https://github.com/Snehal1112/rocket/commit/397bc3a8fc2b4153a47498c32e0a9cdc3ba2f48e))
* **logging:** migrate to tracing and stream backend logs to console ([905629c](https://github.com/Snehal1112/rocket/commit/905629c24ad19580051917e428ec7c95c29c28ad))
* **rocket-app:** add run_load_test to RequestExecutionService with variable resolution ([130f05d](https://github.com/Snehal1112/rocket/commit/130f05de8210bebca878a07660bdd9f914b9bd1f))
* **rocket-http:** add interval_ms to LoadTestConfig ([fa0ed0d](https://github.com/Snehal1112/rocket/commit/fa0ed0db48de2811641a8dbf7bce2d9377e3ead8))
* **tauri:** route run_load_test_command through RequestExecutionService ([85e5e7a](https://github.com/Snehal1112/rocket/commit/85e5e7aef7191e6a9e344c8107a44ef57f64effe))

### Bug Fixes

* **contract:** keep ContractScope.rel_path as snake_case on wire ([52da6b0](https://github.com/Snehal1112/rocket/commit/52da6b0d3a23f8c2fdc7f9e58917c0e155c69541))
* **frontend:** clamp intervalMs to >= 0 in LoadTestDialog ([e893098](https://github.com/Snehal1112/rocket/commit/e89309837f5512779e3a2c531a7fdcdf38cb361a))
* **frontend:** support hyphenated variable names in resolveWithContext ([6046cd0](https://github.com/Snehal1112/rocket/commit/6046cd0801a051256d6268b98df008041d4fa063))
* **rocket-http:** classify load test outcomes by HTTP status ([4437ef9](https://github.com/Snehal1112/rocket/commit/4437ef9ec4d6da71735c04ffce3831ca6b717b11))

### Performance Improvements

* **rocket-infra:** cache reqwest::Client in ReqwestExecutor ([1192af8](https://github.com/Snehal1112/rocket/commit/1192af888409e079a530fc91100a6bb54057c152))

### Reverts

* Revert "Release 0.6.0" ([1dff254](https://github.com/Snehal1112/rocket/commit/1dff254688dde2b569e749d039d728bd939e3ec8))

* Revert "Release 0.6.0" (1dff254)
* Release 0.6.0 (06da0f5)
* Refactor code structure for improved readability and maintainability (157e933)

All notable changes to this project will be documented in this file.

## [0.1.0] - 2026-03-24

### Added
- Full DDD architecture with 7 Rust crates (shared, collection, environment, history, http, app, infra)
- Tauri desktop app with React + TypeScript frontend
- Collection management with CRUD, folders, and filesystem persistence
- Request builder with URL bar, method selector, tabbed sections (params, headers, body, auth)
- Query and path parameter editors with bidirectional URL sync
- Body editor with Monaco integration (JSON, XML, form-data, binary file upload)
- Auth support: Basic, Bearer, API Key, OAuth 2.0 (client_credentials, password, authorization_code), AWS Signature v4
- Collection settings with auth and headers inheritance
- Environment management with variable resolution in URLs, headers, body, and auth
- History panel with search and method/status filters
- Tab system with split panes, context menus, keyboard shortcuts (Cmd+T, Cmd+W, Cmd+Enter)
- Response viewer with JSON pretty-print, HTML preview, and headers table
- Splash screen with Lottie rocket animation and liftoff dismissal
- Cmd+S save-draft-to-collection dialog
- Debounced auto-save for collection-owned requests
- Inline rename and delete confirmation for sidebar items
- Right-click context menus and hover action icons for sidebar nodes
- shadcn/ui component library with custom theme

### Fixed
- Path traversal vulnerability in filesystem collection repository
- Auth credentials preserved correctly on auto-save
- OAuth2 token response validated before storing
- CSP policy tightened (removed external CDN from script-src)
- Monaco editor lazy-loaded to reduce initial bundle size

### Security
- Path traversal prevention with canonicalization checks on all filesystem operations
- Sandboxed iframe for untrusted response HTML rendering
- CSP restricts script sources to self + unsafe-eval (required for Monaco workers)
