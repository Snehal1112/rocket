# Changelog

## [0.6.0](https://github.com/Snehal1112/rocket/compare/v0.5.0...v0.6.0) (2026-04-12)

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

## [0.5.0](https://github.com/Snehal1112/rocket/compare/v0.4.0...v0.5.0) (2026-04-10)

### Features

* **status-bar:** display app version in bottom-right corner ([5cfbd82](https://github.com/Snehal1112/rocket/commit/5cfbd82463481c184f312f4410ab765aed90384e))
* **variable-input:** wire scopedContext and onNavigateToSource from RequestPanel to all editors ([0ed70e6](https://github.com/Snehal1112/rocket/commit/0ed70e61c0cfe8e5753d497f046a21a3f5524092))
* **variable-input:** add variable decorations and hover provider to MonacoWrapper ([76b9ff9](https://github.com/Snehal1112/rocket/commit/76b9ff95aca64acc53b1be338a6f5b68c777e291))
* **variable-input:** wire VariableAwareInput through AuthEditor, PathParamsPanel, KeyValueEditor ([b5ae38b](https://github.com/Snehal1112/rocket/commit/b5ae38be6f7402d95949c54351d50840ae55947c))
* **variable-input:** add VariableAwareInput component with dual-layer overlay and popover ([8e13feb](https://github.com/Snehal1112/rocket/commit/8e13fbeb6053cbbbecc271b6e34b36d76a38dc56))
* **ux:** inline rename for global and collection environments ([857d7c6](https://github.com/Snehal1112/rocket/commit/857d7c6d7573b50733b3d97d71d1b41b14ff2652))
* **ux:** show collection and global env names in switcher trigger ([5e46a5d](https://github.com/Snehal1112/rocket/commit/5e46a5d7e5a416dad8fb1d48817c81fd56ed96d6))
* **ux:** add SavedPill component and useSaveButton hook for auto-save feedback ([47f0c87](https://github.com/Snehal1112/rocket/commit/47f0c871d733c42c7a8b09d9957908b852416f8e))
* **ux:** navigate to correct variable source location from URL input popup ([ede45b0](https://github.com/Snehal1112/rocket/commit/ede45b008aff21cbf8b653b188e38482d266846c))
* **ux:** disable save buttons until user makes changes ([b940318](https://github.com/Snehal1112/rocket/commit/b940318e584a2e70a173265f88f6c211e7980281))
* **git:** stash stats, discard untracked, multi-select stashes with batch actions ([4c71416](https://github.com/Snehal1112/rocket/commit/4c7141674ebeb03ca90032b992efe43726f19b56))
* **oauth2:** add verify_ssl field to OAuth2Settings ([56e7056](https://github.com/Snehal1112/rocket/commit/56e70565738700b23f89f07b3772c3031cde0c7d))
* **collection-auth:** add in-memory OAuth2 token store ([979b797](https://github.com/Snehal1112/rocket/commit/979b7977d67b3d109bbe509cde0f09413cee49b2))
* **ui:** redesign Settings, Docs, and Variables tab panels ([b8c973d](https://github.com/Snehal1112/rocket/commit/b8c973da4626f78a9426347bac1eecc72c06832d))
* **execution:** wire folder and request variable scopes via VariableContext ([72deba2](https://github.com/Snehal1112/rocket/commit/72deba202b6ff6a28bcc18cf445af3e78e766e2e))

### Bug Fixes

* **variable-input:** fix duplicate-variable popover and restore password masking ([3f65373](https://github.com/Snehal1112/rocket/commit/3f65373057102fef17b105ffa35497e9ee19c1a8))
* **ux:** only guard tab close when tab has unsaved edits ([09b801b](https://github.com/Snehal1112/rocket/commit/09b801b199bcac043e896ac6a78c1a813acd851d))
* **ux:** fix variable scope and navigation in URL input popup ([cf0b730](https://github.com/Snehal1112/rocket/commit/cf0b730590d134929241bbcf2b39962467ad032d))
* **collections:** use dir_name for folder paths to fix rename of existing folders ([b9be4f6](https://github.com/Snehal1112/rocket/commit/b9be4f6bc7fd3da23002440111a79ac84fd8e02e))
* **git:** resolve git init error for uninitialized store ([d8440dd](https://github.com/Snehal1112/rocket/commit/d8440dd706f308ee21af809a20f529defac16d6a))
* **request:** skip URL validation for template variable syntax ([42e6762](https://github.com/Snehal1112/rocket/commit/42e6762af4170917dce6de426bab29ef0c17db41))
* **request:** persist settings and docs when saving ([4e788cd](https://github.com/Snehal1112/rocket/commit/4e788cd78e09a4889e8fd829a153d18f49c15218))
* **infra:** persist initial_value for collection variables ([a850c26](https://github.com/Snehal1112/rocket/commit/a850c2698b35c6683dc46daa2c9cf71a49f630c0))
* prevent trailing ? on URLs and fix curl boolean flag parsing ([fdf2506](https://github.com/Snehal1112/rocket/commit/fdf2506cd11ba1cec65ec71e16eb50ca370262be))

## [0.4.0](https://github.com/Snehal1112/rocket/compare/v0.3.0...v0.4.0) (2026-04-06)

### Features

* **import:** redesign ImportBrunoDialog — drop zone, inline links, auto-detected type badge ([91efa27](https://github.com/Snehal1112/rocket/commit/91efa2730d026b31db081e9be6dc2283adb54bb8))
* **import:** add workspace import with nested collection support ([7419d02](https://github.com/Snehal1112/rocket/commit/7419d02f8914c487273368b1fadd2c55a1ca930b))
* **import:** replace import commands with import_bruno/zip and auto-detection ([88528bd](https://github.com/Snehal1112/rocket/commit/88528bd33220eca28bf7c6793a000c523ec66fd1))
* **import:** add ZIP extraction and BrunoFormat detection ([cbce939](https://github.com/Snehal1112/rocket/commit/cbce93932d485053b35d184443a51360c169b4b2))
* **import:** full ImportService — walks Bruno directory tree, writes via existing repos ([34de0cb](https://github.com/Snehal1112/rocket/commit/34de0cb5a0aa01a9e86e55479901d0624a777748))
* **import:** environment and request converters for Bruno format ([495459c](https://github.com/Snehal1112/rocket/commit/495459c4a2527909aea54a93dcd7c777917a6edb))
* **import:** .bru parser with lexer and AST types ([4beb01c](https://github.com/Snehal1112/rocket/commit/4beb01c1cf8e8e20112425b3c5dee3e3d9a227d3))
* **ui:** sort collection root and folder children folders-first ([78388bc](https://github.com/Snehal1112/rocket/commit/78388bc26d5fc9ff4bbc9a05829cb64f6a31cd6d))
* **ui:** add toast notifications and fix startup empty state ([4917440](https://github.com/Snehal1112/rocket/commit/4917440575a56e3339acb95f882b929dac5315b3))
* **env:** redesign environment switcher with tabbed UI ([a6e0e0b](https://github.com/Snehal1112/rocket/commit/a6e0e0b6cd681e6d7a8faf2bfd2719a5fed720ee))

### Bug Fixes

* **import:** prevent nested braces from truncating bru JSON bodies ([5943903](https://github.com/Snehal1112/rocket/commit/59439036ef0baf8ecaec2176963b3d65edfeeb5d))
* **import:** use ZIP filename as collection name for flat-root archives ([54faa15](https://github.com/Snehal1112/rocket/commit/54faa1557b0327793d9ef0e48d52103d8d5eb34e))
* **import:** strip ~ prefix from disabled keys ([bc395e2](https://github.com/Snehal1112/rocket/commit/bc395e2c2316a3c9396890ad0ec76751acfd1611))
* **ui:** fix workspace overview double fetch and move error ([bb3c872](https://github.com/Snehal1112/rocket/commit/bb3c87277d129b8492f2718e66aac0dfc62bdfa8))
* **git:** improve clone error handling and post-clone detection ([a417206](https://github.com/Snehal1112/rocket/commit/a417206a3a616587ae0c2745077838c722884a8c))
* **env:** resolve variable input focus loss and stale save ([6110432](https://github.com/Snehal1112/rocket/commit/61104323dcf86642d42e8212e97f43b3e4ae6f7c))

### Performance Improvements

* **ui:** memoize request editor onChange callbacks to reduce re-renders ([ddbfd40](https://github.com/Snehal1112/rocket/commit/ddbfd40535b1d11c2ba6c2333b2119b66c80f483))

## [0.3.0](https://github.com/Snehal1112/rocket/compare/v0.2.0...v0.3.0) (2026-04-02)

### Features

* **variables:** RequestVariablesPanel and Variables tab in request editor ([f635038](https://github.com/Snehal1112/rocket/commit/f6350386194cc4bb987be106e420eb90bd6b9aa7))
* **variables:** FolderVariablesPopover and folder context menu wiring ([8bc24a3](https://github.com/Snehal1112/rocket/commit/8bc24a3ba0eaf48d98f0c9438eaa0cbee44f9724))
* **variables:** split EnvironmentSwitcher into Global and Environment sections ([25d55b7](https://github.com/Snehal1112/rocket/commit/25d55b7b043fd6ced9df14cd975b8a7b245d4377))
* **variables:** scope-aware badge colours and secret masking in URL input ([8541c2a](https://github.com/Snehal1112/rocket/commit/8541c2a21e0bcc8db5693edd05dacefac3ec22ea))
* **variables:** full variable resolution pipeline with 7 scopes ([5ae0e39](https://github.com/Snehal1112/rocket/commit/5ae0e39ed8283158acc9fd5a5fca3eb30eb86dab))
* **variables:** buildVariableContext — merges all 7 scopes with correct priority ([ed1d24c](https://github.com/Snehal1112/rocket/commit/ed1d24c8af83188165f2597e3ee583fa88b53f26))
* **environment:** global env selection and process env Tauri commands ([2675537](https://github.com/Snehal1112/rocket/commit/2675537e7000c591c38bb6cb49ae544ad8c4e893))
* **environment:** reload envs on collection change, workspace switch, and startup ([80bc9b4](https://github.com/Snehal1112/rocket/commit/80bc9b4bacbb8822f6aa9d40722c6ac3327660c0))

### Bug Fixes

* **infra:** exclude environments/ from collection folder tree ([5351a8d](https://github.com/Snehal1112/rocket/commit/5351a8d692635aeed92e71bc0a426afe070bd140))
* **environment:** workspace-level env CRUD commands wired to global envs ([2c87e6b](https://github.com/Snehal1112/rocket/commit/2c87e6b07d4fd77c31ea6b9ebb3d2a92b6b171ee))
* **store:** wire loadProcessEnvVars and fetchGlobalEnv into app lifecycle ([d28f540](https://github.com/Snehal1112/rocket/commit/d28f54b6040da5c503dadd67ad49c5e2e9a020fe))

## [0.2.0](https://github.com/Snehal1112/rocket/compare/v0.1.0...v0.2.0) (2026-04-01)

### Features

* **git:** make git left panel resizable ([5894969](https://github.com/Snehal1112/rocket/commit/589496930607dc24f764b56e02b2c05a551b4b88))
* **git:** add back-to-overview breadcrumb header in git right panel ([7ccb443](https://github.com/Snehal1112/rocket/commit/7ccb44399da8158f9794b46ac4e949f1c308045f))
* **git:** add fetch-before-push safety prompt ([6c710df](https://github.com/Snehal1112/rocket/commit/6c710dfceee0b19b2b5bb5987802ba356ceb0c80))
* **git:** add auto-stash confirmation dialog before pull with dirty state ([391b51f](https://github.com/Snehal1112/rocket/commit/391b51f2729e7d05b62b8b47667658d52c31ab2d))
* **git:** add abort merge to store and ConflictResolver UI ([293015f](https://github.com/Snehal1112/rocket/commit/293015fa463e661a7ca6b170846500b3e8d51c08))
* **git:** add per-file stage/unstage/discard buttons to GitFileList ([589a0a7](https://github.com/Snehal1112/rocket/commit/589a0a7a8b2b14172def8098f18621592e3be057))
* **git:** wire GitPanel into EditorGroup for collection-level git tab ([acbb57b](https://github.com/Snehal1112/rocket/commit/acbb57b068f160d3a82fdb9a0b5e4b688644cbd7))
* **git:** add text/visual mode toggle to git diff viewer ([669b75c](https://github.com/Snehal1112/rocket/commit/669b75ccf02bb0803c8873d4c1cb261403e60112))
* **git:** add VisualDiffView component with request diff parser ([fb4fcfb](https://github.com/Snehal1112/rocket/commit/fb4fcfb3c45e279ad2f683655c620683f0d52d31))
* **git:** show remote branches in BranchSelector with auto-checkout ([24f255c](https://github.com/Snehal1112/rocket/commit/24f255c108e9213bf94963ffdf7aad09ded587ad))
* **git:** add checkout_remote_branch to create local tracking branch ([85c503c](https://github.com/Snehal1112/rocket/commit/85c503c009100e7f767b09993fdf78100bc68632))
* **collection:** add readme field to CollectionSettings ([c35f927](https://github.com/Snehal1112/rocket/commit/c35f927f9e7b68f8ecf2094a58be1c47b58f762b))
* **frontend:** enhance workspace overview to match Bruno's WorkspaceHome ([c681f80](https://github.com/Snehal1112/rocket/commit/c681f804d754a5da8c9220c19f3068648c9dd150))
* **frontend:** add MarkdownEditor component with edit/preview toggle ([743e887](https://github.com/Snehal1112/rocket/commit/743e88730f8fb8b202a363f5e0262269ffb22cb6))
* **frontend:** add TagsList component with recursive tag aggregation ([faf3ed8](https://github.com/Snehal1112/rocket/commit/faf3ed86b16eb34ffc4281a35fa5be1f3889618d))
* **frontend:** show commit counts on Push/Pull buttons ([12e3e9d](https://github.com/Snehal1112/rocket/commit/12e3e9d51d5061e10e1c0044c5ba11b329214e03))
* **frontend:** add Load Test button, LoadTestDialog, and load test runner ([d9760b1](https://github.com/Snehal1112/rocket/commit/d9760b1e3cfa4a6954449a8471e3d844c510b082))
* **frontend:** add SaveToCollectionDialog and Cmd+S routing for ephemeral tabs ([81c3d7f](https://github.com/Snehal1112/rocket/commit/81c3d7f27235dec96fb666b5adf0993712d8f252))
* **frontend:** add CreateRequestDialog for within-collection request creation ([60812cf](https://github.com/Snehal1112/rocket/commit/60812cf1b35e29323505e7c2642f92c001ea5566))
* **frontend:** add FilePlus sidebar button for workspace-level unsaved requests ([370d48c](https://github.com/Snehal1112/rocket/commit/370d48c7e6a3c72bed98db0186bbf111dc8fb7b1))
* **http:** add load test runner with concurrency control and percentile stats ([b558b94](https://github.com/Snehal1112/rocket/commit/b558b94e25b4a4709c3c6fef5aa4bf4d05152743))

### Bug Fixes

* **git:** stage deleted files using index.remove_path ([7cb1461](https://github.com/Snehal1112/rocket/commit/7cb146130c4552050d1ef7806d1f4e734c923e10))
* **git:** ahead_behind falls back to refs/remotes/origin/branch ([23b6297](https://github.com/Snehal1112/rocket/commit/23b6297ba2a9b79c06613b53ab70c0d9972ba4be))
* **frontend:** clone dialog bugs — dialogs in non-repo block, workspace activation ([faf3ed8](https://github.com/Snehal1112/rocket/commit/faf3ed86b16eb34ffc4281a35fa5be1f3889618d))
* **frontend:** use filename as React key for request nodes in tree ([3f7e8b7](https://github.com/Snehal1112/rocket/commit/3f7e8b75d45bc70b5e2b347cdfc92abd1a703470))
* **frontend:** Monaco theme sync and offline production loading ([3ae0657](https://github.com/Snehal1112/rocket/commit/3ae06578a9daf8cc09c8e59772f5e9663b51f1bd))
* **frontend:** preserve collection tab snapshots when entering workspace mode ([245397b](https://github.com/Snehal1112/rocket/commit/245397b44d585a44341c8d287ef44f6262bcd68d))
* **frontend:** pre-fill New Workspace dialog path with .rocket-api directory ([e6f3495](https://github.com/Snehal1112/rocket/commit/e6f3495792f4eb76db9f216133289f5921592c87))
* **shared:** add missing serde rename_all = camelCase to public types ([548a224](https://github.com/Snehal1112/rocket/commit/548a2243239f65a2ccd9340daefc90094098b647))

## [0.1.0](https://github.com/Snehal1112/rocket/releases/tag/v0.1.0) (2026-03-27)

### Features

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

### Bug Fixes

- Path traversal vulnerability in filesystem collection repository
- Auth credentials preserved correctly on auto-save
- OAuth2 token response validated before storing
- CSP policy tightened (removed external CDN from script-src)
- Monaco editor lazy-loaded to reduce initial bundle size

### Security

- Path traversal prevention with canonicalization checks on all filesystem operations
- Sandboxed iframe for untrusted response HTML rendering
- CSP restricts script sources to self + unsafe-eval (required for Monaco workers)
