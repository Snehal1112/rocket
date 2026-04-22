# Changelog

## [0.6.2](https://github.com/Snehal1112/rocket/compare/v0.6.1...v0.6.2) (2026-04-22)

### Features

* add 'dynamic' source type and resolution to url-variables ([12eee0a](https://github.com/Snehal1112/rocket/commit/12eee0a985b283a211b9346f45c90f1095eee084))
* add TypeScript dynamic variable registry (118 variables, faker.js) ([3eb78c2](https://github.com/Snehal1112/rocket/commit/3eb78c21f9821b3f347237fead2e058c07da44dd))
* **dynamic-vars:** add 118-variable generator registry ([632834c](https://github.com/Snehal1112/rocket/commit/632834cebe1075ee85565a6aeff2c22d1a027612))
* **git:** return fetch summary from git_fetch IPC command ([716955d](https://github.com/Snehal1112/rocket/commit/716955dc34e2671e50777e3f692d68434c35d84a))
* **git:** show skeleton while Git panel loads ([ca3ba30](https://github.com/Snehal1112/rocket/commit/ca3ba30e1cdad60b11c9f3702e12da3ae03c74b4))
* integrate dynamic variable resolution into resolver ($-prefix) ([609d145](https://github.com/Snehal1112/rocket/commit/609d14592cfbf68fa0d1f401850d95c621c30309))
* integrate dynamic variable resolution into variable-context.ts ([804ffef](https://github.com/Snehal1112/rocket/commit/804ffef144b737404637456a2e7bcfc816102872))
* **jwt:** add JWT decoder for OAuth2 ID token display ([c21c4d0](https://github.com/Snehal1112/rocket/commit/c21c4d0c81eef66b75bf72246977df3c2a2b3f5b))
* **oauth2:** add code exchange helper for auth_code flow ([9c95aeb](https://github.com/Snehal1112/rocket/commit/9c95aebbad367744306b6c26fa3a0370da8dd979))
* **oauth2:** add HTTP dispatch for direct grant flows ([af8a277](https://github.com/Snehal1112/rocket/commit/af8a277a0d33b5d15adcc985ca8dc3474dde73f0))
* **oauth2:** add id_token, AdditionalParam, URL/body helpers ([6bd6397](https://github.com/Snehal1112/rocket/commit/6bd6397b1fa8c0f83e4ecd4d8ceebf0e98de30a4))
* **oauth2:** add oauth2_decode_jwt command and OAuth2Service managed state ([907765a](https://github.com/Snehal1112/rocket/commit/907765a3c4fae67e6f62e16bc31fc994c80a1f15))
* **oauth2:** add OAuth2Service scaffold with resolve ([5c4fa20](https://github.com/Snehal1112/rocket/commit/5c4fa20021acbc7a125da1c455ef7515373f6f48))
* **oauth2:** auto-fetch and auto-refresh tokens before request execution ([1dda055](https://github.com/Snehal1112/rocket/commit/1dda055b8cfd529e7f1fa1578eebd5f370072b3a))
* **oauth2:** decode and display access token JWT payload ([97c8aaa](https://github.com/Snehal1112/rocket/commit/97c8aaa88769ec45a00703781e81562a5be1f258))
* **oauth2:** extend AuthState with PKCE, additional params, settings, JWT claims ([a5169f4](https://github.com/Snehal1112/rocket/commit/a5169f4402d3ea6b9b7bd7545ccfa2cce64cd02d))
* **oauth2:** mapping layer and Tauri API bindings for unified token commands ([478e19c](https://github.com/Snehal1112/rocket/commit/478e19c16438f27dd496a5a6940bddfe4894aaa6))
* **oauth2:** oauth2_get_token, oauth2_refresh_token commands with system browser support ([bafdcdc](https://github.com/Snehal1112/rocket/commit/bafdcdc4a98990dd71e48cfb67e337b4717b8fc3))
* **oauth2:** OAuth2AdditionalParams tabbed key-value editor ([0d127ab](https://github.com/Snehal1112/rocket/commit/0d127ab92c6d08b612f99316ffc41e39d02d3f63))
* **oauth2:** OAuth2AuthEditor orchestrator with Tauri command wiring ([7d1d2fa](https://github.com/Snehal1112/rocket/commit/7d1d2fa3955efe84db8d2ec8ddee7b26408556be))
* **oauth2:** OAuth2ConfigSection and OAuth2TokenSection components ([55e3e5a](https://github.com/Snehal1112/rocket/commit/55e3e5acb4c66a190df2bf5de0f655f3824ea52d))
* **oauth2:** OAuth2TokenDisplay, SettingsSection, AdvancedSection components ([4a4d613](https://github.com/Snehal1112/rocket/commit/4a4d61318dc90226521143516c6b6dc21c327ac0))
* render dynamic variables with cyan D badge in popover and Monaco ([c17007f](https://github.com/Snehal1112/rocket/commit/c17007fd8c3122d2cd1566c5e8e0f27bd1a999bb))

### Bug Fixes

* **a11y:** raise sidebar icon and badge contrast to WCAG AAA ([13a12b0](https://github.com/Snehal1112/rocket/commit/13a12b070e078ec1873ae506785239e0b6caaf79))
* **backend:** address important backend issues ([7a1b3c2](https://github.com/Snehal1112/rocket/commit/7a1b3c2688a63be547624866cdf92194c12a0792))
* **ci:** generate changelog once, not per matrix runner ([ce82522](https://github.com/Snehal1112/rocket/commit/ce825222fc2a4225c353dc8c7608815a4cc2fc4e))
* **console:** show auth headers in console request log ([513b3d8](https://github.com/Snehal1112/rocket/commit/513b3d837847ebe4fd2a657b2a28df7338fdfd77))
* **dynamic-vars:** correct alphaNumeric charset and bitcoin address length ([36cda65](https://github.com/Snehal1112/rocket/commit/36cda658efc74f6f07da1df99316207b536a683e))
* **editor:** eliminate theme flash on first Monaco diff/conflict view ([b0598b3](https://github.com/Snehal1112/rocket/commit/b0598b37b1ffe47e1549653611235fa56a60c6ba))
* **git:** handle directory paths in stage() and refresh status before stageAll ([50412d0](https://github.com/Snehal1112/rocket/commit/50412d08f5332e3f457b5e3a215a0617686b9271))
* **git:** recurse untracked dirs in status to prevent staging errors ([124984e](https://github.com/Snehal1112/rocket/commit/124984e98a4fb3212d97b4af9f6f5b2b18d9d0b2))
* **import:** map Bruno variable value to initial_value on collection import ([632e87c](https://github.com/Snehal1112/rocket/commit/632e87cee986e071c6b4fa24c94b6e27c2efc8a7))
* **infra:** atomic file writes to prevent data corruption on crash ([6712540](https://github.com/Snehal1112/rocket/commit/6712540475cd28b4b8818f3b7331b2f817e9dac9))
* **infra:** use create_new to eliminate TOCTOU race in save_request ([f7c2d93](https://github.com/Snehal1112/rocket/commit/f7c2d9377d354735b0d06cb6cd09d0d3aa6fadf4))
* **oauth2:** add missing use_system_browser to oc_conversions test fixture ([11e2d57](https://github.com/Snehal1112/rocket/commit/11e2d57130c72616cf979c20f130ea7e7ddeec2b))
* **oauth2:** always show Refresh button, disabled when no refresh token ([17eecc0](https://github.com/Snehal1112/rocket/commit/17eecc0e559a3717ef11ecdf5f7e42b03b180e69))
* **oauth2:** fix stale closure clobbering access token on fetch ([2bdb513](https://github.com/Snehal1112/rocket/commit/2bdb513b8a1e466603425c5f5f535c93af149848))
* **oauth2:** persist full OAuth2 config on save, not just access token ([cdffc53](https://github.com/Snehal1112/rocket/commit/cdffc53d3dbd3bcfbd6444e09252895127b277f7))
* **oauth2:** persist tokenSource selection across save/reload ([5280cf9](https://github.com/Snehal1112/rocket/commit/5280cf9313daeb80f509a5dab95d4f052cb3a485))
* **oauth2:** persist useSystemBrowser setting across save/reload ([ec839cc](https://github.com/Snehal1112/rocket/commit/ec839ccd11881eaa26cc830935b047732c250178))
* **oauth2:** restore accessTokenClaims from cache on collection tab reopen ([8d9c6de](https://github.com/Snehal1112/rocket/commit/8d9c6de69c5525164da22a1783f0ad88a802aa70))
* **oauth2:** restore missing token fields on collection tab reopen ([93f0ceb](https://github.com/Snehal1112/rocket/commit/93f0ceb579652b5dec3e6cb8fa16dacd734935e3))
* **params:** extract path params on request load and cURL import ([c99fbdc](https://github.com/Snehal1112/rocket/commit/c99fbdcdd02de89849159c9ece196339e578ed24))
* **security:** confine HTTP executor file reads to workspace directory ([f1820cf](https://github.com/Snehal1112/rocket/commit/f1820cfee41a0c0e8d728011a1682e4af4075664))
* **sidebar:** prevent layout shift on active tree item ([f694b00](https://github.com/Snehal1112/rocket/commit/f694b00d92c5d86a89d36ddf3747fa9ef2ae883d))
* **startup:** replace expect() panics with graceful error propagation ([c991553](https://github.com/Snehal1112/rocket/commit/c991553a738c0a673fb78495666af8ec969b0f0c))
* **vars:** open request variables dialog from Variables tab for saved requests ([9176fc3](https://github.com/Snehal1112/rocket/commit/9176fc399c1e9d99cb255fadde6aa5f99f404203))
* **vars:** open request variables in dismissible dialog on tooltip nav ([97ca33a](https://github.com/Snehal1112/rocket/commit/97ca33a3843f93b52647337e40ccc18990dadfac))
* **vars:** persist current variable value on save and restore on load ([b83678f](https://github.com/Snehal1112/rocket/commit/b83678f8549240e749583f668a89ceca796b5e4a))
* **vars:** persist initial and current values separately ([461ceda](https://github.com/Snehal1112/rocket/commit/461ceda6f70a66e9a5a04af47ab49126aa6db751))
* **workspace:** update active path before persisting on workspace switch ([5c7f4a2](https://github.com/Snehal1112/rocket/commit/5c7f4a20f874037d340c4f2a067bfa82317e7399))

### Performance Improvements

* **git:** eliminate redundant IPC calls on Git tab switch ([45ce1b9](https://github.com/Snehal1112/rocket/commit/45ce1b922776851b05e7b9fb551f5d504001d71c))

### Reverts

* **vars:** show request variables inline in tab, not in dialog ([1446120](https://github.com/Snehal1112/rocket/commit/14461209ecb35c60e7c7e5f5b04783f961d76560))

## [0.6.1](https://github.com/Snehal1112/rocket/compare/v0.5.0...v0.6.1) (2026-04-20)

### Features

* add Tailwind CSS typography plugin and update UI components with new illustrations ([cfc421b](https://github.com/Snehal1112/rocket/commit/cfc421b9d314d499aa19b60110f8e943e6fe6500))
* **audit:** add audit section to workspace tabs and sidebar ([8baf3a7](https://github.com/Snehal1112/rocket/commit/8baf3a763429577735b9c63455187cbcd8b78578))
* **audit:** add audit-store with events and profile state ([1ccab1f](https://github.com/Snehal1112/rocket/commit/1ccab1feda4b8443e6456230f5c2571f6797ae49))
* **audit:** add AuditEventRow component ([10b6181](https://github.com/Snehal1112/rocket/commit/10b618175a5db2797fbc64bda6074dc7cd2829aa))
* **audit:** add AuditLogRepository and SecurityAuditPublisher traits ([295c269](https://github.com/Snehal1112/rocket/commit/295c269e195f56372eef73008d32b4bfc00670a1))
* **audit:** add AuditLogTab viewer with filter and actions ([db17aee](https://github.com/Snehal1112/rocket/commit/db17aeef88eddd22c04956382328e2ad4d60dc7d))
* **audit:** add ComplianceProfile and ProfileRepository trait ([8eba38e](https://github.com/Snehal1112/rocket/commit/8eba38ecabd5aed04b25f500edbd3aed6a2dd2d4))
* **audit:** add ComplianceProfileDialog ([a3cb302](https://github.com/Snehal1112/rocket/commit/a3cb3028e16e45e183449b1c1039a00706a09c07))
* **audit:** add contract audit domain rules and agent documentation ([f6327a2](https://github.com/Snehal1112/rocket/commit/f6327a2188c78c7ebc7f8d060039566349745a94))
* **audit:** add control catalog for SOC 2, ISO 27001, ISO 42001, CSA STAR ([08d5045](https://github.com/Snehal1112/rocket/commit/08d5045d9eb673c3d77d0882fe66fa39814c45ca))
* **audit:** add ExportEvidenceDialog ([2a86dce](https://github.com/Snehal1112/rocket/commit/2a86dce6bf73b0d172ac1a1541c19f4cf5053ea3))
* **audit:** add FsAuditLogRepo append-only JSONL store ([fcce028](https://github.com/Snehal1112/rocket/commit/fcce0283e63067c084e5a583e4a5658deafd796f))
* **audit:** add FsComplianceProfileRepo YAML profile store ([d90f1f2](https://github.com/Snehal1112/rocket/commit/d90f1f298cb7df1ee964161f92f8e23327395e92))
* **audit:** add IPC commands for audit list, profile, and evidence export ([6bf0357](https://github.com/Snehal1112/rocket/commit/6bf03574570c42a998076729f60ab91dafcd37fa))
* **audit:** add SecurityAuditEvent and AuditEventKind ([2700400](https://github.com/Snehal1112/rocket/commit/270040028cc4bbaa4cee7af4f1e1419f1758a14a))
* **audit:** add SecurityAuditService with hash-chain append and profile filter ([9e4008a](https://github.com/Snehal1112/rocket/commit/9e4008a66d9f1c1fa36ea68eab2632d09dddb41b))
* **audit:** add SHA-256 hash chain helpers ([ee3b1db](https://github.com/Snehal1112/rocket/commit/ee3b1db90de11d0a62a37bca5ed57cde0a02db83))
* **audit:** add typed frontend wrappers for audit IPC ([f372c65](https://github.com/Snehal1112/rocket/commit/f372c653d780e1b4047915e0af3a100952392ec5))
* **audit:** emit security audit events from contract/collection/env/exec services ([2e6019c](https://github.com/Snehal1112/rocket/commit/2e6019cfc71c89d961f87aceef8bf063a375dae3))
* **audit:** wire SecurityAuditService and ServiceBackedAuditPublisher in src-tauri ([4768a88](https://github.com/Snehal1112/rocket/commit/4768a88f19e6624c134b9bdd83da9a0f7bc56d0c))
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
* **contract:** add multi-document attachments and PDF preview ([50e685c](https://github.com/Snehal1112/rocket/commit/50e685c24aeee384652d9d383e7280410ed6184c))
* **contract:** domain types, snapshot, changelog, diff_signature ([2f4ae14](https://github.com/Snehal1112/rocket/commit/2f4ae1462feaca5d844b912d46e33aef4b0c1b07))
* **contract:** frontend store, UI, and tests for Contract Lock ([a85239f](https://github.com/Snehal1112/rocket/commit/a85239fd8e464ee6a222c4de3ba95801fd81c04f))
* **contract:** FsContractRepo, ContractService, save hook with Model B seam ([99502ce](https://github.com/Snehal1112/rocket/commit/99502ceaf276f602d5d49fbd75d80debefc10050))
* **contract:** Tauri IPC commands + save hook wired ([cb55868](https://github.com/Snehal1112/rocket/commit/cb55868ef35e1dc65c2e60f65add40fa08de73b1))
* **editor:** barrel export for CM6 extensions ([826ac57](https://github.com/Snehal1112/rocket/commit/826ac57e8f64d412636b2a5549f3859dd4475d2c))
* **editor:** barrel export for SingleLineEditor module ([fac8596](https://github.com/Snehal1112/rocket/commit/fac8596aefa447378801fabc3f5e0b9685007a53))
* **editor:** CM6 theme matching shadcn Input + variable token colors ([7d326d7](https://github.com/Snehal1112/rocket/commit/7d326d701dcfc61bd9e30f6a7faa26c574f7ef14))
* **editor:** language detection — bodyMode/contentType/filePath to CM6 extensions ([bec1ffb](https://github.com/Snehal1112/rocket/commit/bec1ffb34c7945ee820a66f26054fc5cb114a5d4))
* **editor:** migrate AuthEditor to SingleLineEditor with isSecret for secret fields ([89395c6](https://github.com/Snehal1112/rocket/commit/89395c669cdf40b56b781d946214e03d0bcb7754))
* **editor:** migrate BodyEditor from MonacoWrapper to MultiLineEditor ([f99bccb](https://github.com/Snehal1112/rocket/commit/f99bccbd730c064222af5a9b173f0a09ffea577e))
* **editor:** migrate KeyValueEditor and PathParamsPanel to SingleLineEditor ([382a7a1](https://github.com/Snehal1112/rocket/commit/382a7a153e11418a8e3c146906626c6ed1fecf6f))
* **editor:** migrate ResponseBodyViewer from MonacoWrapper to MultiLineEditor ([69c14c8](https://github.com/Snehal1112/rocket/commit/69c14c8e1452317a7a6c0abb9a42444d18e175c2))
* **editor:** multi-line theme + variable hover tooltip extension ([1362742](https://github.com/Snehal1112/rocket/commit/136274297ff8eb7b2cce222e24c851b181fd8e4b))
* **editor:** MultiLineEditor — CM6 multi-line editor with language + variable support ([a1019a7](https://github.com/Snehal1112/rocket/commit/a1019a711422456410a3eb88b6be8cc9fcba762f))
* **editor:** SingleLineEditor React wrapper with CM6 + portal popover ([551f8f2](https://github.com/Snehal1112/rocket/commit/551f8f2b15b9b4e1ebada33d602a7f25bea7defc))
* **editor:** URL tokens + secret mask extensions wired into SingleLineEditor ([51ab2ea](https://github.com/Snehal1112/rocket/commit/51ab2ea0d8ea3edac4f633ebf8549cf0efef916a))
* **editor:** URL tokens extension — pathParam + query highlighting + curl paste ([6f26aa3](https://github.com/Snehal1112/rocket/commit/6f26aa3e3ec7849f1a6e4625196f19a8cea002cd))
* **editor:** variable autocomplete with scope badges and priority ranking ([02c4031](https://github.com/Snehal1112/rocket/commit/02c40311ded2dfc69e202f327fcd87806990efcb))
* **editor:** variable highlight ViewPlugin with scope-colored decorations ([7e2d051](https://github.com/Snehal1112/rocket/commit/7e2d051074973b6d6cf162a29e53027eaecd3e60))
* **editor:** variable popover ViewPlugin with CM6 tooltip state management ([c4c2849](https://github.com/Snehal1112/rocket/commit/c4c2849b0822646218e09052431bfb931d9a1812))
* **editor:** variable-context facet + single-line transaction filter ([0e4cf1d](https://github.com/Snehal1112/rocket/commit/0e4cf1d1f095291fbb887468ddb14bc57205ec82))
* **editor:** VariablePopover component for CM6 tooltip portal ([5c15e51](https://github.com/Snehal1112/rocket/commit/5c15e511e894d9d5c939e4a3fa9d867a7a68fca8))
* **frontend:** add delay-between-requests input and failure breakdown types ([5a8a74c](https://github.com/Snehal1112/rocket/commit/5a8a74cbf4d65665c53c15ac5593c4fc1c5770d6))
* **frontend:** pass collection and environment context to run_load_test_command ([d4415a6](https://github.com/Snehal1112/rocket/commit/d4415a6e2f44b3d133d9d01da9513b1fb1d55ee7))
* **frontend:** show transport/status failure breakdown in LoadTestDialog ([397bc3a](https://github.com/Snehal1112/rocket/commit/397bc3a8fc2b4153a47498c32e0a9cdc3ba2f48e))
* **git:** CM6DiffViewer — side-by-side diff using @codemirror/merge MergeView ([a08c3ee](https://github.com/Snehal1112/rocket/commit/a08c3eefb3c25d61d694f6202be6fb9fbb9c0583))
* **git:** migrate ConflictResolver from Monaco Editor to MultiLineEditor (CM6) ([40945c5](https://github.com/Snehal1112/rocket/commit/40945c5c3643e36052611d18f0afa9e41f4ea112))
* **git:** migrate DiffViewer from Monaco DiffEditor to CM6 MergeView ([0b5d131](https://github.com/Snehal1112/rocket/commit/0b5d131ac1db0e83c3af809ebc577c51306e1c7b))
* **input:** add caret helpers for contenteditable refactor ([12334c2](https://github.com/Snehal1112/rocket/commit/12334c2c4cec9c35593e95922468d72c34955554))
* **input:** add renderTokens and useContentEditableInput hook ([ed133ab](https://github.com/Snehal1112/rocket/commit/ed133abf0498a69a73cc2bb6171f059f40febdd5))
* **input:** add selectionchange guard to eject caret from badge spans ([4f18a9e](https://github.com/Snehal1112/rocket/commit/4f18a9e3383978baed7aad8b07fa563fb9c814b9))
* **input:** replace overlay with contenteditable in VariableAwareInput ([a72640c](https://github.com/Snehal1112/rocket/commit/a72640c08ead00e2264b3fd084dae6a576c6306c))
* **input:** replace overlay with contenteditable in VariableAwareUrlInput ([e7ed7c7](https://github.com/Snehal1112/rocket/commit/e7ed7c787f9f788c847bcc9a619e865ab9192701))
* **logging:** migrate to tracing and stream backend logs to console ([905629c](https://github.com/Snehal1112/rocket/commit/905629c24ad19580051917e428ec7c95c29c28ad))
* migrate URL bar from VariableAwareUrlInput to SingleLineEditor ([c33a483](https://github.com/Snehal1112/rocket/commit/c33a48398a6e6513567a15017cac48eaa4754800))
* **panes:** add split-pane usability improvements ([703fee7](https://github.com/Snehal1112/rocket/commit/703fee76d1e7fb628b01c58e6a5f74d991b8da07))
* **response:** enhance ResponseBodyViewer UX ([874f87b](https://github.com/Snehal1112/rocket/commit/874f87bcaa8d4ec01a39a6afe13c1c7481c465fc))
* **rocket-app:** add run_load_test to RequestExecutionService with variable resolution ([130f05d](https://github.com/Snehal1112/rocket/commit/130f05de8210bebca878a07660bdd9f914b9bd1f))
* **rocket-http:** add interval_ms to LoadTestConfig ([fa0ed0d](https://github.com/Snehal1112/rocket/commit/fa0ed0db48de2811641a8dbf7bce2d9377e3ead8))
* **tauri:** route run_load_test_command through RequestExecutionService ([85e5e7a](https://github.com/Snehal1112/rocket/commit/85e5e7aef7191e6a9e344c8107a44ef57f64effe))
* **typography:** apply monospace font to technical input fields ([887e018](https://github.com/Snehal1112/rocket/commit/887e018b3c129b6957fcc58fe259aad2d8451977))
* useVariableCommit hook — shared variable save logic ([fd08b42](https://github.com/Snehal1112/rocket/commit/fd08b4289437413427e63d94f2f4228f1218caa5))

### Bug Fixes

* **a11y:** apply WCAG 2.1 AAA accessibility across frontend ([222c419](https://github.com/Snehal1112/rocket/commit/222c419393c17b2c3ecd67669867cc2b9dbfad81))
* **audit:** associate framework label with checkbox via htmlFor ([6a2f7da](https://github.com/Snehal1112/rocket/commit/6a2f7dad1d2427c4cd4bae3fe01ac17d77e28ac6))
* **audit:** derive Ord on Framework for BTreeSet usage ([ead0fc4](https://github.com/Snehal1112/rocket/commit/ead0fc45c2b4a153147d649e7a18315898083efc))
* **build:** pin tauri-plugin-fs to 2.5 and raise Node heap limit ([8db3c9d](https://github.com/Snehal1112/rocket/commit/8db3c9d008c896bb2ef9db872feb613a0a24718d))
* **contract:** keep ContractScope.rel_path as snake_case on wire ([52da6b0](https://github.com/Snehal1112/rocket/commit/52da6b0d3a23f8c2fdc7f9e58917c0e155c69541))
* **editor:** add aria-label to VariablePopover dialog role ([3dcd732](https://github.com/Snehal1112/rocket/commit/3dcd7320d6009b54bc5dfee10510930fd3ddc61e))
* **editor:** add isSecret to API Key value and AWS Session Token fields ([6cf0ff0](https://github.com/Snehal1112/rocket/commit/6cf0ff05690b03f64bb8c703e1326635317b5858))
* **editor:** align create() signature with CM6 API and use exclusive pos < to boundary ([cc68687](https://github.com/Snehal1112/rocket/commit/cc68687f4c581ef0a8e66e898578768a14169587))
* **editor:** apply Biome format and import order fixes ([a5f7d6e](https://github.com/Snehal1112/rocket/commit/a5f7d6e6fa458a16e0f8f0b7e501f7d9c2a35856))
* **editor:** autocomplete filter prefix + apply insertion offsets ([36f96a3](https://github.com/Snehal1112/rocket/commit/36f96a35899cc7eb3a1d55fcc126aa7d13e30e35))
* **editor:** override CM6 base placeholder vertical-align so it centers in h-8 line ([6b5beea](https://github.com/Snehal1112/rocket/commit/6b5beea21db7add1be6c2c4c74121055b566020f))
* **editor:** recreate editor only on lang/readOnly change, not on every value update ([5264a42](https://github.com/Snehal1112/rocket/commit/5264a4255d674db37327f916488565deae6d475d))
* **editor:** render CM6 tooltips at document root so popover escapes overflow-hidden wrapper ([b2f036d](https://github.com/Snehal1112/rocket/commit/b2f036da576ea9856c2e4903ce2888df6239f56f))
* **editor:** strip nested input border in VariablePopover so corners match card ([322b742](https://github.com/Snehal1112/rocket/commit/322b74210347eee8491afca1f41e5bfa1dd76ba3))
* **editor:** use CSS var(--font-mono) in multiLineTheme instead of hardcoded font ([07c6a79](https://github.com/Snehal1112/rocket/commit/07c6a79330ae20d2183dc3189bf3a8b2a45a4209))
* **editor:** vertically center placeholder and inline content via flex on .cm-line ([7600064](https://github.com/Snehal1112/rocket/commit/7600064483783aeb8b44f63b4eab363b866464f0))
* **editor:** vertically center single-line content and trim var token vertical padding ([19b6dcd](https://github.com/Snehal1112/rocket/commit/19b6dcd96d5cbe79d894ec9021ede10251356586))
* **frontend:** clamp intervalMs to >= 0 in LoadTestDialog ([e893098](https://github.com/Snehal1112/rocket/commit/e89309837f5512779e3a2c531a7fdcdf38cb361a))
* **frontend:** support hyphenated variable names in resolveWithContext ([6046cd0](https://github.com/Snehal1112/rocket/commit/6046cd0801a051256d6268b98df008041d4fa063))
* **git:** wire real event bus and fix branch-switch UI refresh ([8d55a9d](https://github.com/Snehal1112/rocket/commit/8d55a9d586ce99aacf87c4e9bf8c91f1045e026b))
* **input:** resolve Biome lint and format violations from contenteditable refactor ([9a7947b](https://github.com/Snehal1112/rocket/commit/9a7947b461ec58d29ac1c95f121039260115f624))
* **rocket-http:** classify load test outcomes by HTTP status ([4437ef9](https://github.com/Snehal1112/rocket/commit/4437ef9ec4d6da71735c04ffce3831ca6b717b11))

### Performance Improvements

* **editor:** lazy-load Monaco and eliminate redundant IPC on tab switch ([aa1fc78](https://github.com/Snehal1112/rocket/commit/aa1fc781598ae577dcbf07f0b91e99a5fdf89e49))
* **rocket-infra:** cache reqwest::Client in ReqwestExecutor ([1192af8](https://github.com/Snehal1112/rocket/commit/1192af888409e079a530fc91100a6bb54057c152))

### Reverts

* Revert "Release 0.6.0" ([1dff254](https://github.com/Snehal1112/rocket/commit/1dff254688dde2b569e749d039d728bd939e3ec8))
* **editor:** restore Monaco for multi-line editors ([99275f0](https://github.com/Snehal1112/rocket/commit/99275f0eb7c2e772ba5e6c724cfe626290bf91f9))

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
