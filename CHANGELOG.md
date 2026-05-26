# Changelog

## [0.8.0](https://github.com/Snehal1112/rocket/compare/v0.7.0...v0.8.0) (2026-05-26)

### Features

* apply Lora serif font to markdown documentation preview ([a2a3885](https://github.com/Snehal1112/rocket/commit/a2a3885266293977fb56cf49c1b13a88c9844278))
* **body:** add Form URL Encoded option to body mode selector ([d2d9e34](https://github.com/Snehal1112/rocket/commit/d2d9e3465264d6f9e94cafb7e3474cea2831c79c))
* **body:** add formurlencoded to BodyMode and BodyState types ([29826ac](https://github.com/Snehal1112/rocket/commit/29826ac2ad6af578cf88eb3fdfdae8544ed075a7))
* **body:** render KeyValueEditor for formurlencoded body mode ([21ddaae](https://github.com/Snehal1112/rocket/commit/21ddaae19779356ab77769525a91732a7c17fb4a))
* **frontend:** implement Scripts and Vars tabs with Monaco editors, add Tests panel and console wiring ([1d8d100](https://github.com/Snehal1112/rocket/commit/1d8d100c17b2e7e9239688fc47db82903f283bea))
* replace IBM Plex Sans/Mono with Plus Jakarta Sans and JetBrains Mono ([6199be0](https://github.com/Snehal1112/rocket/commit/6199be0941767d6466d2b685d8f5fc46b8dae0a2))
* **scripts:** add phase and onEditorReady props to MonacoWrapper ([dc495ae](https://github.com/Snehal1112/rocket/commit/dc495aecbc1915361a85fe63e743101b503bb947))
* **scripts:** add rok-types snippet data and phase-scoped type definitions ([a93cc27](https://github.com/Snehal1112/rocket/commit/a93cc275afdf38319f7fea02252b6633d25f8025))
* **scripts:** add ScriptSnippetSidebar component ([1019bd9](https://github.com/Snehal1112/rocket/commit/1019bd96311c76094ea96368a772f66e306c0880))
* **scripts:** delegate expect() to bundled Chai for full API parity ([7f29524](https://github.com/Snehal1112/rocket/commit/7f29524d12006189143bfdcded30cc925f008d56))
* **scripts:** expand snippets sidebar with full Chai matcher set ([9c99f1b](https://github.com/Snehal1112/rocket/commit/9c99f1ba3e2e6c861a4587d64597e206f988b8c3))
* **scripts:** replace ChaiAssertion stub with full @types/chai ([75a9252](https://github.com/Snehal1112/rocket/commit/75a92526089da03eac067ec11f5f2cfd7182d2c6))
* **scripts:** wire ScriptSnippetSidebar into ScriptsTab ([741c15e](https://github.com/Snehal1112/rocket/commit/741c15eb5cdfb94d3ca302048b968d8d57b8f683))

### Bug Fixes

* **body:** send formData instead of content for formurlencoded mode ([6ddb4d2](https://github.com/Snehal1112/rocket/commit/6ddb4d2ed13762d737af61b87f9b0dc4ce04d02c))
* **body:** use toApiBody in auto-save to persist formData fields ([0280d48](https://github.com/Snehal1112/rocket/commit/0280d489c3945f74faad4756b0c0701a80472e75))
* **body:** use toApiBody in save button and save-to-collection dialog ([5899885](https://github.com/Snehal1112/rocket/commit/5899885fb50e9daaf6b1dee622bd9b81c580c1d9))
* explicitly set font-mono on prose-doc pre to prevent Lora inheritance ([af70c06](https://github.com/Snehal1112/rocket/commit/af70c069eb24a70e014c5b499eed7edcb541590c))
* **scripts:** add below, above, within, an to expect().to.be chain in bootstrap ([9a7b5f6](https://github.com/Snehal1112/rocket/commit/9a7b5f6974ae4401ec6500be215a109f12b4687d))
* **scripts:** extract handleEditorReady, fix null guard in handleInsert ([6afb019](https://github.com/Snehal1112/rocket/commit/6afb019fc09ff41e0650f9376885aad72ac66e63))
* **scripts:** focus editor after no-cursor snippet insert ([a701689](https://github.com/Snehal1112/rocket/commit/a701689b21382e91e1821e232600314a8fba8e19))
* **scripts:** move require() definition before expect() Chai delegation ([a55c3ec](https://github.com/Snehal1112/rocket/commit/a55c3ece5e5cd7a794aaa25c539391137cd12c3f))
* **scripts:** replace dynamic monaco import with static to eliminate stale-closure race ([86fefef](https://github.com/Snehal1112/rocket/commit/86fefef340bd7a5b003d02f366675ccdfb290ab8))
* **scripts:** use per-phase addExtraLib key to prevent global IntelliSense conflicts ([369dfb9](https://github.com/Snehal1112/rocket/commit/369dfb9f06fe3b79aac87da2c64b9f3d675c52f5))

## [0.7.0](https://github.com/Snehal1112/rocket/compare/v0.6.10...v0.7.0) (2026-05-21)

### Features

* **frontend:** add AssertionEntry type and assertions to Request IPC interface ([588ffbf](https://github.com/Snehal1112/rocket/commit/588ffbf05a1a34a1d574dde1bd4c26ec8ee9f8f6))
* **frontend:** add assertions to RequestState and mapping utilities ([fadf71c](https://github.com/Snehal1112/rocket/commit/fadf71c8314f8e479302db18823e8b50f1a403e6))
* **frontend:** add AssertionsTab component and wire into RequestPanel ([003ae1c](https://github.com/Snehal1112/rocket/commit/003ae1c2dd88d6f119de2fadfdee4bdeb173b05d))
* **frontend:** ScriptsTab, TestsPanel, script fields in RequestState/ResponseState ([4cd71e7](https://github.com/Snehal1112/rocket/commit/4cd71e7648656997b5b094943b33352a9edd7e02))
* **frontend:** wire assertions through save, auto-save, and execute paths ([9fd295c](https://github.com/Snehal1112/rocket/commit/9fd295cee148c5dc40fa6203493cc07ac572bc27))
* **http:** add max_redirects option to RequestOptions ([13e51b5](https://github.com/Snehal1112/rocket/commit/13e51b587cfbac5fdd3d0417be6566e1cd62a39b))
* **load-test:** add max_requests cap and improve chart time formatting ([fa6ce54](https://github.com/Snehal1112/rocket/commit/fa6ce54e9c7644d98315bbbf6916b24ece84a974))
* **request:** add overflow menu and reorder tabs by priority ([b5c9a17](https://github.com/Snehal1112/rocket/commit/b5c9a17a18847bf585ece5599f523f81d03bf660))
* **rocket-app:** 3-phase script pipeline + DomainEvent script variants ([e356888](https://github.com/Snehal1112/rocket/commit/e3568885d9a1a74e7c966bc1ae5cc8b8badc83ae))
* **rocket-app:** add assertion_evaluator with full Bruno operator set ([6cf09f1](https://github.com/Snehal1112/rocket/commit/6cf09f1bf2453cbc0e94ce967d71ce4c5ee9e82f))
* **rocket-app:** wire evaluate_assertions into execution pipeline ([222a880](https://github.com/Snehal1112/rocket/commit/222a8809cf2857fba079d74e9e64ef4cf48587dc))
* **rocket-infra:** add OpState structs and JS bootstrap script ([e92f01c](https://github.com/Snehal1112/rocket/commit/e92f01c0e848fdb3495a713699e6c26892a4a87e))
* **rocket-infra:** console tests + embedded JS module bundles + require() wiring ([fafb61e](https://github.com/Snehal1112/rocket/commit/fafb61e3a15a419e1156e17ff9d98188b83e839d))
* **rocket-infra:** DenoScriptEngine skeleton — runtime lifecycle, test ops, smoke tests ([cbabede](https://github.com/Snehal1112/rocket/commit/cbabede11c45680f2957beea8ee78e563813532f))
* **rocket-infra:** implement rok.* ops and integration tests ([e8056b9](https://github.com/Snehal1112/rocket/commit/e8056b92eb83c4f6b660463c2fd1a071211a815a))
* **rocket-infra:** req/res ops with phase guards + integration tests ([150b6cb](https://github.com/Snehal1112/rocket/commit/150b6cbf60f476a659f93f899b91b32ceb80fc4a))
* **rocket-infra:** scaffold scripting module + deno_core dependency ([a5d64b3](https://github.com/Snehal1112/rocket/commit/a5d64b303176b1f05c3ed2aa0a3b99ba8895e1af))
* **rocket-scripting:** add ScriptContext, convenience constructors, CLAUDE.md ([e530bce](https://github.com/Snehal1112/rocket/commit/e530bce67ce22c828ddb63ad1bafd0e1b31128e7))
* **rocket-scripting:** add ScriptContext, convenience constructors, CLAUDE.md ([867d0f2](https://github.com/Snehal1112/rocket/commit/867d0f2fd91cd99bf58b301dc10bcd9fabec1091))
* **rocket-scripting:** add ScriptResult and all output types ([027a6e3](https://github.com/Snehal1112/rocket/commit/027a6e3ebe9af338d7f7f3ec3f00be7b47d9929b))
* **rocket-scripting:** add ScriptResult and all output types ([8d0778e](https://github.com/Snehal1112/rocket/commit/8d0778e3f880124fa3aea66a04c3a850b305b692))
* **rocket-scripting:** scaffold crate, ScriptPhase, ScriptEngine trait ([2da27b3](https://github.com/Snehal1112/rocket/commit/2da27b3846b55f6d1ac693cd7ddacad6cc5f192c))
* **rocket-scripting:** scaffold crate, ScriptPhase, ScriptEngine trait ([3973cc3](https://github.com/Snehal1112/rocket/commit/3973cc39691479a27fb3411060804020178102b3))
* **scripting:** add script/test console entry types and pretty-print objects ([ad88743](https://github.com/Snehal1112/rocket/commit/ad88743b5e7905416b130fb87326f33b7aba1745))
* **src-tauri:** wire DenoScriptEngine, extend IPC types, add TS script types ([e328fd1](https://github.com/Snehal1112/rocket/commit/e328fd12f3c03ca743ab765c096fc0d2a38a8b8f))

### Bug Fixes

* **assertions:** remove horizontal scroll from Assertions tab ([1d51e22](https://github.com/Snehal1112/rocket/commit/1d51e22adf774377dc1fbc54a1cbb1dc9a3940da))
* **build:** remove cdylib crate-type to fix V8 TPOFF32 hidden-symbol link error ([2ccf98a](https://github.com/Snehal1112/rocket/commit/2ccf98ac7147c4bac1c52d3cd7cd0a1929bf9352))
* **build:** use gcc linker to resolve V8 TPOFF32 TLS relocation in cdylib ([a7c22f9](https://github.com/Snehal1112/rocket/commit/a7c22f9c00927a39807709ce9b126216ed57446b))
* **contracts:** update ContractCard test to match ContractParties behavior ([c109703](https://github.com/Snehal1112/rocket/commit/c1097032b51492c346d368a01b3dff95aa9ed55c))
* **frontend:** align tests field name with Rust IPC contract ([e6b0b7d](https://github.com/Snehal1112/rocket/commit/e6b0b7d4fe4963d4f508aeba605c8fd3bb263c88))
* **frontend:** hydrate script fields when opening a saved request ([d3f9025](https://github.com/Snehal1112/rocket/commit/d3f90256272f10e85d19e42c9c0928af392e5f2d))
* **frontend:** persist script fields on save and auto-save ([5938d9c](https://github.com/Snehal1112/rocket/commit/5938d9c56eff3260fcc22a71150fcfc6a8103225))
* **hooks:** scope unwrap guard to new_string/content only ([f4b1546](https://github.com/Snehal1112/rocket/commit/f4b1546522ee2e756c687c5537dd362ddf1a0301))
* resolve biome failures and stale curl paste handler ([7325972](https://github.com/Snehal1112/rocket/commit/73259723472a4309ece1c41220f461a1d148ec64))
* **rocket-infra:** strip trailing newlines from scripts before YAML write ([4ffa0c1](https://github.com/Snehal1112/rocket/commit/4ffa0c13b40a53734b431624c13512329adaf58d))
* **rocket-scripting:** use workspace deps, fix doc typo, complete Display tests ([4af7e01](https://github.com/Snehal1112/rocket/commit/4af7e01faf6c5c0698cebab7df9e492f28fe2b7b))
* **variables:** preserve request vars on save_request ([35b4602](https://github.com/Snehal1112/rocket/commit/35b460226d5bb9fd035c7e52cd19b444e971fc72))
* **variables:** show count badge on request open ([8dabce1](https://github.com/Snehal1112/rocket/commit/8dabce1152a11f52feb18cec4f80077288fcbd88))

## [0.6.10](https://github.com/Snehal1112/rocket/compare/v0.6.9...v0.6.10) (2026-05-14)

### Features

* add initialName/initialEmail/confirmLabel props to GitIdentityDialog ([88c1203](https://github.com/Snehal1112/rocket/commit/88c1203aabffeca8f1700d55eee66918e42382d3))
* add Rocket verification checks and safety hooks ([c22ed77](https://github.com/Snehal1112/rocket/commit/c22ed7726e75c7f22f28dbf7547aea341c5ac8d3))
* add workspaceId param to saveGitCredentials and loadGitCredentials ([e532c40](https://github.com/Snehal1112/rocket/commit/e532c4020918403c25a8befd78076b5fb76743e1))
* **contracts:** add ConsumerTree component ([31a83f2](https://github.com/Snehal1112/rocket/commit/31a83f2780660092826346e8da2fd47f738baa80))
* **contracts:** enhance ContractStatusChip and MiniChangelog components with improved status rendering and styles ([6440cc0](https://github.com/Snehal1112/rocket/commit/6440cc053edf991d10c2427f31219074b47bbc5e))
* enhance Changelog components with responsive styling and overflow handling ([dd66214](https://github.com/Snehal1112/rocket/commit/dd66214a188c0c5ede95220aca6ebd5636c00eaa))
* **git:** add git_get_identity and git_set_identity Tauri commands ([422bca3](https://github.com/Snehal1112/rocket/commit/422bca3043448810fbc72f8aca08211979ecd1ff))
* **git:** add gitGetIdentity and gitSetIdentity TypeScript wrappers ([0050be0](https://github.com/Snehal1112/rocket/commit/0050be08cf24a14d5a5f7e082426d81f61063bdb))
* **git:** prompt for author identity before committing when git config is unset ([f969f7b](https://github.com/Snehal1112/rocket/commit/f969f7be162d29aecc9a3eedf3c9d49072be22d6))
* implement per-workspace SSH key selection and update credential handling ([8d85e3f](https://github.com/Snehal1112/rocket/commit/8d85e3f646684b5ee6777a9815c785b954c1a7dc))
* list all ~/.ssh/ key files in credentials dialog so correct key is always visible ([295f91f](https://github.com/Snehal1112/rocket/commit/295f91f0c9c7dbddd282b01d6adb6c3a7148cc68))
* pass workspaceId to credential save/load in GitCredentialsDialog ([a965ab5](https://github.com/Snehal1112/rocket/commit/a965ab569a67265b3b28a1db1beb0daa31fb08d9))
* reload workspace-scoped credentials on setCollection ([7a3de84](https://github.com/Snehal1112/rocket/commit/7a3de84942b33f52a404835ea87ec13e45c18a64))
* render identity setup dialog in GitPanel after SSH key change ([3bcdeb5](https://github.com/Snehal1112/rocket/commit/3bcdeb59a3d4410cf54f7058e09a82a2b6563219))
* scope git keychain entry per workspace id ([a041414](https://github.com/Snehal1112/rocket/commit/a041414c931c122792c9a112af7b4e28b4dbfad7))
* trigger identity setup dialog after SSH key credential change ([f7ec68a](https://github.com/Snehal1112/rocket/commit/f7ec68afcceeadf13bb9426931ee54afb633b9e5))

### Bug Fixes

* close credentials dialog synchronously and auto-retry on SSH auth errors ([9b73f99](https://github.com/Snehal1112/rocket/commit/9b73f99510f36841a614d8a8649c362e5483e5bc))
* **contracts:** remove duplicate changelog append from recompute_drift ([b473bf5](https://github.com/Snehal1112/rocket/commit/b473bf5fd4d16cf49d75e98f2149da4530ee7dba))
* **git:** remove hardcoded identity fallback from pull merge-commit path ([8cb661b](https://github.com/Snehal1112/rocket/commit/8cb661bb6206682ee489fca7704a5567e1252309))
* **git:** remove hardcoded RocketAPI User fallback signature ([7ed53ae](https://github.com/Snehal1112/rocket/commit/7ed53aebb1c385de595255bb21370b051ee2cd5f))
* **git:** set git config identity in setup_repo() helpers for tests ([02aef5b](https://github.com/Snehal1112/rocket/commit/02aef5bed90b01ed02e30611686af4227fd6ed5a))
* **git:** set identity in push_fails_with_non_fast_forward test helper ([245b258](https://github.com/Snehal1112/rocket/commit/245b25859963c29531c0bd457cf09d37b59a7bc5))
* **test:** narrow FileStatus.status to GitStatusKind in git-store test ([2208c3b](https://github.com/Snehal1112/rocket/commit/2208c3b8b4678cb8b6e8ac16dd014c3d36e6c9d1))
* **ui:** opaque overlay fallback for WebKitGTK where backdrop-filter is unsupported ([5da5d25](https://github.com/Snehal1112/rocket/commit/5da5d25d2385d7674c62b71761ab68954657b6d7))
* **ui:** use overflow:clip on Linux body/[#root](https://github.com/Snehal1112/rocket/issues/root) so fixed portals are not clipped by WebKitGTK ([478da63](https://github.com/Snehal1112/rocket/commit/478da635688aa827ce66a8581139e0e823b872f9))
* use async IIFE in setCredentials and guard activatePendingCredentials ([219c093](https://github.com/Snehal1112/rocket/commit/219c0939fe5fbc9a1f6cdd9919d584d53c1b26a2))

## [0.6.9](https://github.com/Snehal1112/rocket/compare/v0.6.8...v0.6.9) (2026-05-11)

### Features

* add auth_to_scheme helper for OpenAPI export ([cf0bf41](https://github.com/Snehal1112/rocket/commit/cf0bf41df01d1046c299feb36844814818e4c4ba))
* add contract state machine documentation and export functionality ([9e49927](https://github.com/Snehal1112/rocket/commit/9e49927eb91e7c19c698c11d413f8aae0838cbec))
* add extract_server_and_path helper for OpenAPI export ([586111c](https://github.com/Snehal1112/rocket/commit/586111c11315e60d2c62221b87453084a441eb27))
* add infer_content_type_and_example helper for OpenAPI export ([7d4a650](https://github.com/Snehal1112/rocket/commit/7d4a650938d90def2319461a5814ba5abebeaf44))
* add private OpenAPI serde structs for contract export ([87154f5](https://github.com/Snehal1112/rocket/commit/87154f5ea0bae0f0399b4b233926b5d72452dd5d))
* add tag_from_request_path helper for OpenAPI export ([107f955](https://github.com/Snehal1112/rocket/commit/107f955bb5ef66887f362ee0b39d9d4b75103a55))
* **changelog:** enhance ChangelogEntry with request method, HTTP path, and author fields ([f45deb5](https://github.com/Snehal1112/rocket/commit/f45deb5b2694bcb859b6be78d1f198c2343faafe))
* **contract:** add ContractParty, PartyKind, ContractPolicy, BreakingChangePolicy ([2b5462b](https://github.com/Snehal1112/rocket/commit/2b5462bc8525a4266ee5a0539787c446a4557200))
* **contract:** add Draft/Drift/Breach/InReview/Paused status variants; is_breaking on ChangelogEntry ([f31d749](https://github.com/Snehal1112/rocket/commit/f31d7490c709ca9bb61a1df85b84163f6a14b3d5))
* **contract:** all lifecycle Tauri commands + tauri-api.ts wrappers ([e919684](https://github.com/Snehal1112/rocket/commit/e9196840ff356e1398e52fca40320c36c8a426bb))
* **contract:** policy-aware diff_signature — is_breaking computed per BreakingChangePolicy ([4ed46c2](https://github.com/Snehal1112/rocket/commit/4ed46c28ffc959557df17337bc095f70b4376c50))
* **contract:** publish/pause/resume/renew service methods + Tauri commands ([e7d1834](https://github.com/Snehal1112/rocket/commit/e7d18343bf7cc018d02dedfc7af2073db9dc070a))
* **contract:** re-export new domain types from contract module ([aa5c755](https://github.com/Snehal1112/rocket/commit/aa5c7558451e955130e0ea2bd67de1373efe9ee1))
* **contract:** recompute_drift_for_collection — drift engine in ContractService ([835b245](https://github.com/Snehal1112/rocket/commit/835b24540fb78f897f59bef9be5f23da5338c537))
* **contracts:** ⌘L global hotkey; j/k DOM focus via forwardRef; Enter opens focused card ([0c8af9d](https://github.com/Snehal1112/rocket/commit/0c8af9d8690fff88237826fc24660f6aa50949cf))
* **contracts:** add archive/unarchive Tauri commands + Archived to IPC DTO ([5eacf10](https://github.com/Snehal1112/rocket/commit/5eacf100df24131a44b7449e65dbf1bc3d88b293))
* **contracts:** add Archived status + Archive/Unarchive state machine events ([5070393](https://github.com/Snehal1112/rocket/commit/5070393a998722c9ffea4a9100622ae2cd5c3d72))
* **contracts:** add archived status to TypeScript types, tauri-api, and status labels ([dd0d76e](https://github.com/Snehal1112/rocket/commit/dd0d76e80fce49388e58122895d01d58fab8a724))
* **contracts:** add changelog drawer leaf components ([1b43d94](https://github.com/Snehal1112/rocket/commit/1b43d94daa5c7a4377c1fb400c459aba9e52fcd4))
* **contracts:** add changelog drawer shell ([16d2886](https://github.com/Snehal1112/rocket/commit/16d2886f55245b8f6f9556c7944b1c3e6431e07c))
* **contracts:** add changelog drawer types and store slice ([4cc74ee](https://github.com/Snehal1112/rocket/commit/4cc74ee5aa30e7033e73b39eaa3abc581347137e))
* **contracts:** add ContractDiffTab type and isContractDiffTab guard ([824c8c1](https://github.com/Snehal1112/rocket/commit/824c8c1d6b4591910c414a6fe39263faa15eaab8))
* **contracts:** add review button for breached contracts in PrimaryAction component ([b362f38](https://github.com/Snehal1112/rocket/commit/b362f38ae6341fe1e56f4c62006c7bcf848ce3f2))
* **contracts:** archive UI — status chip, filter chip, group render, unarchive CTA ([2b8ed19](https://github.com/Snehal1112/rocket/commit/2b8ed191941596e3ca616d8fd84424b631409f55))
* **contracts:** archive/unarchive store actions; archived group in selectors; exclude from 'all' filter ([e9f1bba](https://github.com/Snehal1112/rocket/commit/e9f1bba3edd32490be66c0a74ae4c12f4c743b5e))
* **contracts:** auto-transition expiry in list_contracts (ExpiryLapsed / ExpiringSoon) ([d84547c](https://github.com/Snehal1112/rocket/commit/d84547ccb1d6b7d810aceaf1be90d7cdf8993031))
* **contracts:** changelog drawer polish - telemetry, a11y, csv export ([e6300dc](https://github.com/Snehal1112/rocket/commit/e6300dcfe4ab4f8ad6f604136b14b410582a3bd5))
* **contracts:** ContractCard — two-column layout, 7 status modifiers, sub-components wired; rename PartyPill.role→partyRole to avoid ARIA collision ([3b24199](https://github.com/Snehal1112/rocket/commit/3b24199f975e61b80e74d6ef8384a742c3332035))
* **contracts:** ContractContextMenu — 10 items, status-aware, delete AlertDialog ([a7b1390](https://github.com/Snehal1112/rocket/commit/a7b1390f75d08b9a7873b8be915029a6a2e3a6c9))
* **contracts:** ContractsStatusItem — separators before+after, count+drift+breach; wired into StatusBar ([629f9a7](https://github.com/Snehal1112/rocket/commit/629f9a7986e141c4a475cd780e27ad32016b9f2b))
* **contracts:** ContractsTab — summary row, filter bar, grouped cards, last-sync, error banner, hotkey stubs ([6c3e7b4](https://github.com/Snehal1112/rocket/commit/6c3e7b40db7835f84ff3083b7a98196e29340c12))
* **contracts:** ContractStatusChip (8 variants incl. expiring_in_30_days + a11y) + ChangeChip + 4 tests ([0e5cfce](https://github.com/Snehal1112/rocket/commit/0e5cfce13f5ff8bcc74e0c1b6024121dea527197))
* **contracts:** create ContractDiffPane component ([6c571be](https://github.com/Snehal1112/rocket/commit/6c571bee67f441ff1a4025d24c43a44ec472fcd0))
* **contracts:** design tokens — warning-soft, success, destructive-soft, method colors, pulseRed; expose METHOD_COLOR_VARS from colors.ts ([d306229](https://github.com/Snehal1112/rocket/commit/d3062298b5ffe0110bf0f34f022902b457045b58))
* **contracts:** EditorGroup routes to new ContractsTab (SP5-SP7 architecture) via collectionRoot ([98bf74e](https://github.com/Snehal1112/rocket/commit/98bf74ef06fe760f65719c170dc7ac33312e97c4))
* **contracts:** export_contract_openapi Rust command — generates OpenAPI 3.0 YAML stub + TS wrapper ([b477eec](https://github.com/Snehal1112/rocket/commit/b477eecc96b155c9eaf598e0a891a458fe800d27))
* **contracts:** implement accept_drift functionality to handle contract drift and re-signing ([3bc319d](https://github.com/Snehal1112/rocket/commit/3bc319d4b3a3340167d2caa314ade4771e634b45))
* **contracts:** MetaItem + PrimaryAction internal sub-components ([59e1c08](https://github.com/Snehal1112/rocket/commit/59e1c08fa8781edbe9038a9a896a7798f444c3ec))
* **contracts:** MiniChangelog (entry.at + entry.summary), ContractCardSkeleton, GroupHeader, EmptyState; add date-fns + Skeleton ui component ([c1be6ba](https://github.com/Snehal1112/rocket/commit/c1be6ba355f98ddb6a8e5fe11ba97866704933a0))
* **contracts:** NewContractModal — full form, validation, toast on publish/draft, recomputeDrift ([d92812c](https://github.com/Snehal1112/rocket/commit/d92812c83a7519126d6c60be45f56255231303d9))
* **contracts:** open ContractDiffPane tab for review_diff/open/view_changelog actions ([2b53728](https://github.com/Snehal1112/rocket/commit/2b53728861c3098151f9faaa39f0c96cf655337d))
* **contracts:** PartyAvatar, PartyPill, ScopeTag (uses rel_path scope fields) ([c4e408a](https://github.com/Snehal1112/rocket/commit/c4e408a048c8795102497f0f3917d326e8b8c357))
* **contracts:** render ContractDiffPane in EditorGroup ([484414b](https://github.com/Snehal1112/rocket/commit/484414bfd2efd440e0ecc6029c67f2bebafaf876))
* **contracts:** request row contract dot — compliant/drift/breach, role=img a11y, rel_path scope matching ([6d8b4af](https://github.com/Snehal1112/rocket/commit/6d8b4af5fec0a2276a92962e832210589d1ce338))
* **contracts:** restore upstream changes post-merge - archive/unarchive, ContractDiffTab, rich meta row ([32ff18a](https://github.com/Snehal1112/rocket/commit/32ff18aa8670cc316c9caf20264db7fe804a8ea7))
* **contracts:** scope folder/request dropdowns populated from collection tree ([5ff4171](https://github.com/Snehal1112/rocket/commit/5ff417183db35dee359fb7550b94cac8cac38267))
* **contracts:** sidebar lock pin with lockPinLabel — opens ContractsTab, drift/breach icons ([a37e8b1](https://github.com/Snehal1112/rocket/commit/a37e8b19e16e4eb1c51fca5df35e8ca3f4af818d))
* **contracts:** snapshot.ts (6 tests), drift.ts (10 tests), avatarColor.ts, statusMachine.ts (7 tests) ([6f71197](https://github.com/Snehal1112/rocket/commit/6f71197c01e733174d37307df56d1997894afc61))
* **contracts:** SP5-02 — ContractsSummaryRow (+add/−rem/~mod trend), useContractsFilter (6 tests), ContractsFilterBar ([a1f3247](https://github.com/Snehal1112/rocket/commit/a1f3247ed9627dff07aa9310d680356e12a436f4))
* **contracts:** SP8-04 export OpenAPI save dialog + context menu wire ([5840753](https://github.com/Snehal1112/rocket/commit/5840753a681f48697bb120441bde9d07de759d39))
* **contracts:** stable useContractDrift (ref-based debounce, collection-changed event) + barrel index.ts ([bd1f269](https://github.com/Snehal1112/rocket/commit/bd1f2691cfe3675d907231b022c1050ed657302b))
* **contracts:** table-view 'coming soon' toast; effectiveAt past-date warning in modal ([719d131](https://github.com/Snehal1112/rocket/commit/719d131f0e35e4254b1ad4bd919ac7750aec1e8f))
* **contract:** state machine — pure transition() function with 13 test cases ([b36f89e](https://github.com/Snehal1112/rocket/commit/b36f89ecbdf6d6c820d450422573e4562be4f52a))
* **contracts:** telemetry — card_action, tab_opened, filter_used, empty_state_cta; CollectionName / Contracts breadcrumb ([2a0cea8](https://github.com/Snehal1112/rocket/commit/2a0cea8715445fad2d1de635d4669e00ce2abd06))
* **contracts:** telemetry — contracts.created, published, draft_saved in NewContractModal ([75afdc4](https://github.com/Snehal1112/rocket/commit/75afdc486d86b4ae1b1ad606da97a91a310e053d))
* **contracts:** telemetry — status_changed (7 actions), drift_detected (elapsedMsSinceSigned) ([414443d](https://github.com/Snehal1112/rocket/commit/414443d2dda106d196f1987554363eaa6115b4cf))
* **contracts:** TypeScript type definitions — Party, ChangeKind, full Contract shape ([2caefcb](https://github.com/Snehal1112/rocket/commit/2caefcb9864d46e0cf5626cc7e73b93c4d378a53))
* **contracts:** VersionTag + StatusSubline internal sub-components ([aae4616](https://github.com/Snehal1112/rocket/commit/aae46163c953545160b5bbaf97f42904a8673b89))
* **contracts:** wire changelog drawer filters and search ([524c1de](https://github.com/Snehal1112/rocket/commit/524c1deef674b849f64192462e10874423db733a))
* **contracts:** wire changelog drawer trigger surfaces ([3fe8f94](https://github.com/Snehal1112/rocket/commit/3fe8f945350be0a92666d6f96ec0bdf4be5c4249))
* **contracts:** wire ContractContextMenu into ContractCard — right-click + MoreHorizontal button ([a4f1343](https://github.com/Snehal1112/rocket/commit/a4f1343e47fc759a5210c53c039659ce2f19ab49))
* **contracts:** Zustand store (contractsSlice, contractsActions, contractsSelectors + 6 tests), useContracts + useContractDrift hooks ([72b6d0b](https://github.com/Snehal1112/rocket/commit/72b6d0bbbced2f117a89c742f2beb5c02ec4a50c))
* **contract:** update attach_contract — ContractParty consumers, publish_immediately, derive collection_name from root ([66da38a](https://github.com/Snehal1112/rocket/commit/66da38acde5775a1c7a2def25b5529808905c4cc))
* **contract:** update Contract struct — ContractParty, Vec<consumers>, stored status, policy, drift counts ([8710044](https://github.com/Snehal1112/rocket/commit/8710044c56f26b8a7784345119716e91d565b978))
* full OpenAPI 3.0.3 export with metadata, parameters, body, auth, and security schemes ([f7dd288](https://github.com/Snehal1112/rocket/commit/f7dd2886d1011a39e3ab9d0a2c7bc16fcd335294))

### Bug Fixes

* **components:** enhance accessibility and improve workspace import handling ([01e7a49](https://github.com/Snehal1112/rocket/commit/01e7a49ea5cd46967ef1511d03fa3c34f4914836))
* **contract:** endpoint_count post-merge total; consolidate impl blocks; add snapshot test ([c9f74fb](https://github.com/Snehal1112/rocket/commit/c9f74fbf3234285cbcaa6907e7cdbaebe0902bda))
* **contracts:** align ContractScope with IPC (rel_path, 'request' singular); add IPC→domain changelog adapter; fix warning CSS tokens in both light and dark mode ([809b0e7](https://github.com/Snehal1112/rocket/commit/809b0e726b96aee71f89375fbfe9c93111eca01e))
* **contracts:** changelog drawer close animation and a11y fixes ([3ba51c0](https://github.com/Snehal1112/rocket/commit/3ba51c0e8877ee7c48f366ce7e81566a67dbd5ae))
* **contracts:** ContractsStatusItem uses shadcn Button instead of raw <button> ([b1c638a](https://github.com/Snehal1112/rocket/commit/b1c638af4eaaf198167c4330aa4b3fe4dbf0a09d))
* **contracts:** ContractsTab selects Zustand actions individually instead of full store ([ce1aade](https://github.com/Snehal1112/rocket/commit/ce1aade20df15e91080e805ad057870fda41bfa7))
* **contracts:** expiryDate null in old YAML no longer panics — deserialise Option<NaiveDate> not NaiveDate ([24d3d1a](https://github.com/Snehal1112/rocket/commit/24d3d1a44d7c9fb2476740bcc7d10e29038637e8))
* **contracts:** ignore tracking errors in contract status change handlers ([4d721d4](https://github.com/Snehal1112/rocket/commit/4d721d4432f692e846191caa75348c62bea0239f))
* **contracts:** kebab ("...") button uses DropdownMenu so left-click opens action menu ([e7bba03](https://github.com/Snehal1112/rocket/commit/e7bba0356103da9e98fa481bb49bc93eefa5f42b))
* **contracts:** load changelog after listContracts so Recent Changes panel populates ([47e7d93](https://github.com/Snehal1112/rocket/commit/47e7d93eae862ab0aa80923e46c0dcb91b4e472a))
* **contracts:** lock pin span not button — fixes button-in-button HTML violation ([3b944fd](https://github.com/Snehal1112/rocket/commit/3b944fd742367129c06ffe2984c20b294821cc1e))
* **contracts:** re-export Contract from contractsSlice per plan intent ([8d720ce](https://github.com/Snehal1112/rocket/commit/8d720ce119bede01367d76d9e1612c90fcc64a4c))
* **contracts:** recompute_drift skips disk write when counts unchanged — breaks file-watcher loop ([364532c](https://github.com/Snehal1112/rocket/commit/364532c29eefcc989dc160119c4783be7ab1c369))
* **contracts:** register 11 missing contract Tauri commands; fix filename/MIME/error pattern in export ([366e402](https://github.com/Snehal1112/rocket/commit/366e40216ba15580e4f800f139be04e3408c5199))
* **contracts:** remove unused driftCount from StatusSubline Pick type ([b3516c3](https://github.com/Snehal1112/rocket/commit/b3516c302470cfca327b75d8351c0f2344d1f757))
* **contracts:** replace raw table with shadcn Table in ContractDiffPane ([f8e178d](https://github.com/Snehal1112/rocket/commit/f8e178dd72615868137ee8753f10d86051a550b3))
* **contracts:** resign from Drift/Breach uses StatusEvent::Resign not Publish ([2c9583a](https://github.com/Snehal1112/rocket/commit/2c9583afaa45a86e8e529a83a90c5189e7de086c))
* **contracts:** restore data-more-trigger attribute on MoreHorizontal button ([88394d3](https://github.com/Snehal1112/rocket/commit/88394d3d0926fd44d77dbf7f552a7a07adf60e9b))
* **contracts:** scope dropdown validation, cancellable effect, stale list reset ([0956755](https://github.com/Snehal1112/rocket/commit/09567558eb4e5dc94c7f6babeaf3d2b2c0f6e436))
* **contracts:** serialize ExpiringIn30Days as expiring_in_30_days in IPC DTO to match TypeScript ([41c3f2b](https://github.com/Snehal1112/rocket/commit/41c3f2bc25ec0b93679665f3e38f8f9df6e33708))
* **contracts:** stable EMPTY_IDS sentinel fixes infinite re-render in sidebar ([ac49750](https://github.com/Snehal1112/rocket/commit/ac4975027f3989f994b8d45abbcf479bf291d9bc))
* **contracts:** UI/UX fixes — chip labels, MiniChangelog card, PartyPill kind, StatusSubline, VersionTag draft suffix ([ca3ed50](https://github.com/Snehal1112/rocket/commit/ca3ed5080362f4c1b625f469bdf5725d215f3307))
* **contracts:** wire accept_drift to resign; stub review_diff/open_review/remind_reviewers ([4fe09d6](https://github.com/Snehal1112/rocket/commit/4fe09d60915e3ccf75994f26aa969b14c68b1e11))
* **contracts:** wire Edit button to open NewContractModal in edit mode ([c258de0](https://github.com/Snehal1112/rocket/commit/c258de0aab5cbbb25865a21c466cab9c58361771))
* **contracts:** wrap lock pin Tooltip in TooltipProvider ([54fbd53](https://github.com/Snehal1112/rocket/commit/54fbd531b11d769eb91a0236fb957c428f7ba1b4))
* **contract:** UpdateContractInput accepts ContractParty/Vec<ContractParty> and ContractPolicy ([e3c1b04](https://github.com/Snehal1112/rocket/commit/e3c1b04220ea1245bb086fd4214f11e02404de91))
* **frontend:** update TypeScript contract types — ContractParty, ContractPolicy, Contract, ChangelogEntry, AttachContractInput, UpdateContractInput ([c655838](https://github.com/Snehal1112/rocket/commit/c655838b70960fb313738e07809ff07fd052cb2b))
* remove leftover conflict marker in BreadcrumbBar.tsx ([ed74b1b](https://github.com/Snehal1112/rocket/commit/ed74b1bf6dc5cb62c9b81d1963b5516d43f0e5e7))

## [0.6.8](https://github.com/Snehal1112/rocket/compare/v0.6.7...v0.6.8) (2026-05-06)

### Bug Fixes

* **auth:** persist enabled state for OAuth2 additional parameters ([cbdfdba](https://github.com/Snehal1112/rocket/commit/cbdfdbab4bbf9cd9c75cbab389cda151fdf3449c))
* **collection:** self-heal request files missing uid field on read ([9dd8262](https://github.com/Snehal1112/rocket/commit/9dd82624e6305919a716bf06bb00389092ce3371))

## [0.6.7](https://github.com/Snehal1112/rocket/compare/v0.6.6...v0.6.7) (2026-05-05)

### Features

* **api:** importPostmanCollection + importPostmanEnvironment bindings ([2786ff7](https://github.com/Snehal1112/rocket/commit/2786ff7d5e4d0fcecc7218960ae7388a95523cc9))
* **contract:** diff full key-value fields, body content, and auth detail ([e5e81a9](https://github.com/Snehal1112/rocket/commit/e5e81a912aea1b010ede7e0d33266f7e55c7123e))
* **contract:** expand snapshot to capture full key-value fields and auth detail ([ad7bcb1](https://github.com/Snehal1112/rocket/commit/ad7bcb17108c2bf27e3bffc20bf925dcf15ee228))
* **contract:** update RequestSignatureSnapshot type with full field data ([7c5ef8d](https://github.com/Snehal1112/rocket/commit/7c5ef8d2c4f06b7703325c7aba28c7fe70c7972d))
* **import:** import_postman_collection + import_postman_environment on ImportService ([ac3827a](https://github.com/Snehal1112/rocket/commit/ac3827a07c6971e0efab286114162b2e33c41a2b))
* **import:** postman AST structs + error variants + serde_json dep ([0b7d65b](https://github.com/Snehal1112/rocket/commit/0b7d65b9f2b05f8aa5b4b8cfadd41f1270c8d1f8))
* **import:** postman converter — auth, headers, params, body, variables ([db4ffd2](https://github.com/Snehal1112/rocket/commit/db4ffd2d6f72323b8228231cab806400f1cc3570))
* **import:** postman JSON parser + environment parser ([6a2c717](https://github.com/Snehal1112/rocket/commit/6a2c717fffcbd6dcd27895004f4f4e39a27e489c))
* **import:** postman request item converter ([6a038f3](https://github.com/Snehal1112/rocket/commit/6a038f3d8fd2c6b843cfde559342a476a95953c8))
* **infra:** add yaml_io module with read_dir_yaml, read_dir_by_mtime, delete_if_exists ([2e0a4c7](https://github.com/Snehal1112/rocket/commit/2e0a4c70f16b1a332aee2f39e03549d80a2d20b1))
* **infra:** implement performance improvements by adding get_summaries, replacing serde_json::Value with concrete types ([dbbb591](https://github.com/Snehal1112/rocket/commit/dbbb59155cd5388686c28b79165d4a6f0884c58e))
* **tauri:** expose get_collection_summaries command wired to get_summaries ([8189bf0](https://github.com/Snehal1112/rocket/commit/8189bf0cfac38b986e1a60c47d75802ec51b1d3b))
* **tauri:** register import_postman_collection and import_postman_environment commands ([4bcb831](https://github.com/Snehal1112/rocket/commit/4bcb8310372bb5cda8a26b512baded3609efc707))
* **ui:** ImportDialog source toggle — Bruno / Postman with env file picker ([6068519](https://github.com/Snehal1112/rocket/commit/606851984a650691ec5292d59c3fa9df56e4a6ae))

### Bug Fixes

* **collections:** use on-disk filename for contract snapshot key ([56247b4](https://github.com/Snehal1112/rocket/commit/56247b4a1862b731ff05b404b981c7f417f6d672))
* **contract-tab:** pre-load changelogs for all contracts on list view ([ea526a5](https://github.com/Snehal1112/rocket/commit/ea526a566d6d68048d3c80740daedf474cd57cb6))
* **contract-tab:** stabilise changelogs selector with useShallow ([0b88081](https://github.com/Snehal1112/rocket/commit/0b8808196eb0aaabf9a9bdd8b9ef751cebfa87e3))
* **contract-tab:** stabilize contracts selector and skip loaded changelogs ([ab92fa1](https://github.com/Snehal1112/rocket/commit/ab92fa1b527c42c627ec8d02a7c9c8790adc7e86))
* **contract:** add serde(default) to headers, query_params, auth_detail for backward compat ([32a4872](https://github.com/Snehal1112/rocket/commit/32a48725375982851cf9640ff6f7a2db0318f4d8))
* **contract:** document duplicate-key invariant, add missing diff tests, minor cleanup ([398a289](https://github.com/Snehal1112/rocket/commit/398a289878529479d13ea495dcb1091df950e440))
* **contract:** OAuth2 auth detail, binary body file_path, PartialEq on ContractSnapshot ([1b3072e](https://github.com/Snehal1112/rocket/commit/1b3072ea9c456523c3a26d8fc79f3871926c5120))
* **contract:** refresh changelog count on file watcher events ([0b48f29](https://github.com/Snehal1112/rocket/commit/0b48f294e403fa76adf4c4bea9f857ccd1f7f4b3))
* **contract:** remove unused variable in form_field diff test ([9481e04](https://github.com/Snehal1112/rocket/commit/9481e046d7b47e587b92576b5d012b77a42fee95))
* **contract:** truncate ApiKey secret in auth_detail; rename SnapshotKeyValue to avoid type collision ([6f7635c](https://github.com/Snehal1112/rocket/commit/6f7635c3d5502c1228fa2980fe7d63cb34577b30))
* **contract:** truncate long document filenames in ContractCard ([4535a57](https://github.com/Snehal1112/rocket/commit/4535a5734bf67cfdd13724436fd3ff44e6d378e7))
* **contract:** update struct literals for expanded RequestSignatureSnapshot ([d73e43f](https://github.com/Snehal1112/rocket/commit/d73e43f2722c594638b2c3a2e61799af48efd09d))
* **frontend:** revert get_collection to full load; add summary type to CollectionItem union with guards ([e82dde9](https://github.com/Snehal1112/rocket/commit/e82dde925c7b1f905d0915574940fdd1a3f4f549))
* **import:** suppress dead_code warns, clean unused re-exports ([c9f34bc](https://github.com/Snehal1112/rocket/commit/c9f34bcda9a9390911436f90025dd1188e218b1a))
* **infra:** acquire per-collection mutex in rename_request to prevent RMW race ([d8591f7](https://github.com/Snehal1112/rocket/commit/d8591f7121b61afabf2b8cae81416568205743be))
* **infra:** add per-collection Arc<Mutex> map to serialize RMW ops in save_settings, save_folder_variables, save_request_variables, move_item ([4576e9c](https://github.com/Snehal1112/rocket/commit/4576e9cabe1d06d3fcb14e03177e29f8c449cf04))
* **infra:** add per-collection Arc<Mutex> map to serialize RMW ops; add new_standalone() for call sites without shared locks ([aed9e7a](https://github.com/Snehal1112/rocket/commit/aed9e7a7ed5cbd9a78c81f5673e654b4ec235079))
* **infra:** align FsEnvironmentRepo::list with warn-on-corrupt pattern from yaml_io ([9f8add7](https://github.com/Snehal1112/rocket/commit/9f8add7d657bae28f69e7d4c3151d74e058de906))
* **infra:** complete Phase 1 safety refactor with poison recovery, per-collection locking, and transactional migration ([5a38ecf](https://github.com/Snehal1112/rocket/commit/5a38ecf336738402927e11318facabcbab217884))
* **infra:** delete .json only after atomic_write_bulk succeeds in migration ([86f1191](https://github.com/Snehal1112/rocket/commit/86f11912ab1a5da7aafd25eaa0e24baba4adee77))
* **infra:** extend collection RMW lock to save_request and reorder_items ([96f7988](https://github.com/Snehal1112/rocket/commit/96f7988cd91a587a3c3d4ea8f7bfec6f572c3399))
* **infra:** filter Summary items in folder_to_oc_folder/collection_to_oc_collection instead of emitting empty folder ([74a149f](https://github.com/Snehal1112/rocket/commit/74a149f768eb5aff802c719f0bc0ec273fe7f340))
* **infra:** harden atomic_write — sync_all, parent-dir fsync, pid+nanos collision-resistant suffix ([5436795](https://github.com/Snehal1112/rocket/commit/54367959e0887515ac397e68ae808af6b3f8c91b))
* **infra:** include .uid files in legacy backup; add snapshot_backs_up_uid_files test ([3567790](https://github.com/Snehal1112/rocket/commit/35677901ada46e4bfe2a16a3aaa83b1792c7c247))
* **infra:** reject symlinks in delete/delete_folder/build_folder_tree/migration; validate_name on all public methods ([ed64cd0](https://github.com/Snehal1112/rocket/commit/ed64cd0b222b49f0b4636aa1eef82d428994444e))
* **infra:** remove needless borrow and clone_on_copy clippy warnings ([2c04f5a](https://github.com/Snehal1112/rocket/commit/2c04f5a86f354fccda75b1dc70f3ebe66db2e8c4))
* **infra:** replace expect(lock poisoned) with into_inner() recovery in audit, watcher, and shared-path repo ([2d10258](https://github.com/Snehal1112/rocket/commit/2d102582689ad147458714f31e827a02f541338e))
* **infra:** skip corrupt JSONL lines in audit log read instead of aborting ([fdc6d13](https://github.com/Snehal1112/rocket/commit/fdc6d1370ec48a52bfbbc1e4735e898beab93fbd))
* **infra:** suppress expected unused-import warnings on pub(crate) re-exports; remove two genuinely unused imports in tests ([bf87ba4](https://github.com/Snehal1112/rocket/commit/bf87ba4f5c1599339bce9d66b3017bb317699ea6))
* **infra:** surface interrupted migration error in get(); wire is_migration_interrupted ([a563353](https://github.com/Snehal1112/rocket/commit/a563353ef25947374029c8b5b77590b71edc1a90))
* **infra:** switch workspace_config, migration, and template writes to atomic_write ([0244041](https://github.com/Snehal1112/rocket/commit/02440418d4178ed91d0c5b2d58b689a8ee6a0ef7))
* **infra:** transactional migration — sentinel file + .legacy_backup snapshot; expose is_migration_interrupted ([e1114f1](https://github.com/Snehal1112/rocket/commit/e1114f1a524f5b6824c6f443bf584ce4afbe8c9b))
* **infra:** validate old_name in rename; propagate I/O errors from reject_symlink ([92cf676](https://github.com/Snehal1112/rocket/commit/92cf676b74f06441f87346433240d04a27f2ccfa))
* **infra:** warn on corrupt YAML in read_dir_yaml instead of silently skipping ([c49bd56](https://github.com/Snehal1112/rocket/commit/c49bd56b6d619659b5c6c05476a5a1f7206ba7c1))
* **infra:** warn on missing uid instead of silently minting in OC deserialization ([9f20068](https://github.com/Snehal1112/rocket/commit/9f2006841aa597bfaf55aaaab996a7fc5ca2935a))

### Performance Improvements

* **collection:** replace variables Value with CollectionVariable ([29746f8](https://github.com/Snehal1112/rocket/commit/29746f8cc5a823d21ba11217bf2598552763c581))
* **environment:** replace client_certificates serde_json::Value with ClientCertificate ([178d867](https://github.com/Snehal1112/rocket/commit/178d8678563ef24e2f76acf92e42e1844301f343))
* **infra:** add atomic_write_bulk with batched parent-dir fsync; use in migration ([9649355](https://github.com/Snehal1112/rocket/commit/9649355f99e88dbf0c3b0bcb85f87f3395503966))
* **infra:** add get_summaries for lightweight sidebar tree ([508cfc5](https://github.com/Snehal1112/rocket/commit/508cfc533e3a91b3897c8f2725fc551350bfdf54))
* **infra:** early-return for root requests in get_folder_chain_variables; add tracing span ([5a53784](https://github.com/Snehal1112/rocket/commit/5a537847e91460f6a0bac7fcbb1b39a00075ea39))
* **infra:** list() sorts history by mtime before parsing, stopping early once limit is reached ([1b057a8](https://github.com/Snehal1112/rocket/commit/1b057a8d0eed88b70741446ea76dcf49c2ed522a))
* **infra:** parse folder.yml once per folder in build_folder_tree, eliminating P3 double-parse ([1c28c37](https://github.com/Snehal1112/rocket/commit/1c28c3796348be6fef6f0e3f5e245488142c9d2d))
* **infra:** replace into_owned() in sort comparator with borrowed to_str() lookup ([602255b](https://github.com/Snehal1112/rocket/commit/602255bfc6dc000a95261d74f9b8bca4d9949637))
* **infra:** take &Request in request_to_oc_http_request, eliminating deep-clone on save ([e33601f](https://github.com/Snehal1112/rocket/commit/e33601f6488bfcffe2f8b9cb02ef5644764480b3))
* **shared:** use serde_yaml::Value for HttpRequestExample snapshots ([6c1086d](https://github.com/Snehal1112/rocket/commit/6c1086db01bbc618b9f44cd50ff9fcb5e7d61f0f))
* **tauri:** use get_summaries for sidebar collection loads ([4d34a04](https://github.com/Snehal1112/rocket/commit/4d34a0433d951885d4dbada884575d57417e2782))

## [0.6.6](https://github.com/Snehal1112/rocket/compare/v0.6.5...v0.6.6) (2026-05-02)

### Features

* add workspace and environment query and mutation hooks ([2be8518](https://github.com/Snehal1112/rocket/commit/2be85181b2c00d33096a0adaa26fefb888295970))
* App.tsx reads layout state from store, persists and restores on startup ([724100a](https://github.com/Snehal1112/rocket/commit/724100afbac80fd403242de1439142544b98fba5))
* extend layout-store with sidebar width, console open/height ([095fc98](https://github.com/Snehal1112/rocket/commit/095fc982c0b117d141b6e1bc73177af8fa8a92a8))
* **frontend:** add LiveDashboard, charts, RequestLogTable, ExportMenu ([ccec8af](https://github.com/Snehal1112/rocket/commit/ccec8afd48338efd2904862c8708b388fd9a5d44))
* **frontend:** add LoadTestConfigV2, LoadTestProgressEvent, TimeSeriesPoint, RequestLogEntry types and API functions ([3b5e891](https://github.com/Snehal1112/rocket/commit/3b5e891c09ab76ffce69b10fdc021602a21c95cc))
* **frontend:** add LoadTestTab shell and PhaseBuilder ([75c9fad](https://github.com/Snehal1112/rocket/commit/75c9fad24cb944992c8a09f379529c4441a6c767))
* **frontend:** add useLoadTestStore with Tauri event streaming and export ([954bece](https://github.com/Snehal1112/rocket/commit/954beceae460014543a1c8f10912d5133eb93067))
* **frontend:** wire LoadTestTab into RequestPanel section tabs ([a548ab1](https://github.com/Snehal1112/rocket/commit/a548ab19df5a0cb449e920262554e766294bcdfa))
* **git:** add get_default_ssh_key_path, save/load_git_credentials commands ([0186505](https://github.com/Snehal1112/rocket/commit/0186505fc95ee77952b846ee29399fd255270543))
* **git:** add getDefaultSshKeyPath, saveGitCredentials, loadGitCredentials API wrappers ([6c9f7c4](https://github.com/Snehal1112/rocket/commit/6c9f7c4f411f730395c66027cadd3813baf3d4a7))
* **git:** auto-load persisted credentials from OS keychain on collection open ([4f915c8](https://github.com/Snehal1112/rocket/commit/4f915c8510dd8dcd9777cdbeb452909c82646367))
* **git:** register get_default_ssh_key_path, save/load_git_credentials in Tauri handler ([336b39f](https://github.com/Snehal1112/rocket/commit/336b39fa3baea70ba038b5ecd5066825e7e39f2e))
* **git:** replace SSH key path input with WCAG AAA file picker and keychain persistence ([a5e899e](https://github.com/Snehal1112/rocket/commit/a5e899e91ab60660a69eac53d2f8779646c49d5e))
* install TanStack Query and wire QueryClientProvider ([4b0f3f5](https://github.com/Snehal1112/rocket/commit/4b0f3f57029b2389d84a16d1f82234c1e0a36cac))
* **load-test:** enhanced load testing with phases, streaming UI, and throughput-target mode ([876de41](https://github.com/Snehal1112/rocket/commit/876de415f55b6ac3c6652cbc9fa38d3c11956f63))
* **load-test:** redesign PhaseBuilder layout ([02fc011](https://github.com/Snehal1112/rocket/commit/02fc0110d17812daef3252dc9723a228a2f1c505))
* **load-test:** resizable per-request log panel ([9b15fa1](https://github.com/Snehal1112/rocket/commit/9b15fa15397a928aee3a56ef294475ceeddc483f))
* **load-test:** resizable sidebar with drag handle, default 260px, persisted to localStorage ([d56464e](https://github.com/Snehal1112/rocket/commit/d56464e869a38f4874537fb07e9988ca54462726))
* **load-test:** route simple mode through v2 streaming for live request log ([d74102e](https://github.com/Snehal1112/rocket/commit/d74102ed0d5fdbdf9eb4f583813826a1487c5cfb))
* persist sidebarWidth, isConsoleOpen, consoleHeight in UiState ([d6686df](https://github.com/Snehal1112/rocket/commit/d6686dfedab8184293db56ee55290de8d53fa87c))
* **rocket-app:** add ExportService (HTML/CSV/JSON/PDF) ([f13a0af](https://github.com/Snehal1112/rocket/commit/f13a0afe8cc3055fd1a87ed1d4a4ec32411bc2eb))
* **rocket-app:** add LoadTestService with variable resolution ([6da6861](https://github.com/Snehal1112/rocket/commit/6da686124eec52ae2513b7065c84c64a17c81365))
* **rocket-http:** add phase-based load test types ([47ba30b](https://github.com/Snehal1112/rocket/commit/47ba30b0e690958f4f97cc11cb6ff1f0707d8a4c))
* **rocket-http:** add RingBuffer and PhaseScheduler ([fabd22c](https://github.com/Snehal1112/rocket/commit/fabd22c64bc1781fd83550788c94fae8898db5aa))
* **rocket-http:** implement run_load_test_v2 with PhaseScheduler, RingBuffer, and event emission ([e15a70d](https://github.com/Snehal1112/rocket/commit/e15a70dc88a6b7833547445d699f57caf68e473a))
* **save:** include encodeUrl, maxRedirects, tags in auto-save and manual save payloads ([f0e5a88](https://github.com/Snehal1112/rocket/commit/f0e5a88436db429f5096e94defe35f6b1e687fc0))
* **tauri:** add run_load_test_v2_command and export_load_test commands ([380b7d2](https://github.com/Snehal1112/rocket/commit/380b7d22bf3876b2dd8e1f5ca7d62d7d1166b186))
* **types:** fix ApiRequestSettings IPC bridge; add encodeUrl/maxRedirects/tags to frontend types ([aa8098c](https://github.com/Snehal1112/rocket/commit/aa8098c3fdc7adcb9506ebb37bf629ef566805e5))
* **ui:** enhance window decorations for Linux and Windows with focus handling ([1f9f7d2](https://github.com/Snehal1112/rocket/commit/1f9f7d2e97676367d99a31a54cae9365e43641db))
* **ui:** redesign Settings tab in RequestPanel ([f0a001f](https://github.com/Snehal1112/rocket/commit/f0a001f480d9a023d6193063a5a5f371b23f8af2))

### Bug Fixes

* **breadcrumb:** call switchCollection from breadcrumb collection pickers ([f75971f](https://github.com/Snehal1112/rocket/commit/f75971f74b9fc82a6ad2311c3b73615ce47879cc))
* **CollectionOverviewTab:** remove unnecessary dependency from useEffect for scroll event ([0d6e85f](https://github.com/Snehal1112/rocket/commit/0d6e85fd92eba4d8a1311d132830910fac82c427))
* **editor:** re-register Monaco themes on app theme toggle ([d828da1](https://github.com/Snehal1112/rocket/commit/d828da1a80ce95c9b6286031a1ee2299e39d0d26))
* **git:** auto-size credentials dialog width to fit SSH key path content ([4368619](https://github.com/Snehal1112/rocket/commit/4368619fdf9dbb8cb6e22dbb706ad8b46e191c84))
* **git:** use DomainError in keychain commands, add From conversions for GitCredentialsPayload ([0423ef1](https://github.com/Snehal1112/rocket/commit/0423ef13528075b5449d245232eb3d7ef176b85e))
* **load-test:** apply code review fixes to main ([87f9dc0](https://github.com/Snehal1112/rocket/commit/87f9dc0f683fb29c26bb08502f353cc15633f5ab))
* **load-test:** cap drain deadline and use dynamic safety timeout ([5bb3718](https://github.com/Snehal1112/rocket/commit/5bb3718c24164c791309191ce03083311fcb03d9))
* **load-test:** improve formatting for PhaseTarget type and error message in load test store ([d63dc33](https://github.com/Snehal1112/rocket/commit/d63dc338ffeed5258e6a02348f2bbf34be1409a7))
* **load-test:** show request log live during simple mode run and stop at totalRequests ([602a21c](https://github.com/Snehal1112/rocket/commit/602a21c4e4f9fd031045634b4b03a185311bfc34))
* **load-test:** use default Button variant for Run load test to match VS Code 2026 theme ([e4c142c](https://github.com/Snehal1112/rocket/commit/e4c142c9e7e0f6538ea150c7cbb1ec020a974736))
* **load-test:** use PascalCase serde for PhaseKind; skip timeout when 0 ([bb65f28](https://github.com/Snehal1112/rocket/commit/bb65f286583acceee2e3c73665f52eeb34785734))
* **load-test:** use stable IDs for phase list React keys ([7a4204c](https://github.com/Snehal1112/rocket/commit/7a4204c8acb1f2d66a06caf6c6eaec0b1da5702e))
* **rocket-app:** RFC 4180 CSV quoting; add PDF passthrough test ([b83a9fc](https://github.com/Snehal1112/rocket/commit/b83a9fc63f41fbcd1fc8b866de2eccd9a8173798))
* **ui:** align input field backgrounds across themes ([a90d024](https://github.com/Snehal1112/rocket/commit/a90d024ca0efaca1f02e90d5cffb8eb834e3255b))
* **ui:** ensure minimum height for overflow handling in various components ([d2cd7d1](https://github.com/Snehal1112/rocket/commit/d2cd7d18b6e44d2a828995289789fda3c4f3bfed))
* **ui:** remove Card wrapper and redundant borders from env tab right panel ([a16d393](https://github.com/Snehal1112/rocket/commit/a16d39338c80d318d2b356b198481a9c9c136205))
* **ui:** remove double-border and button-bar separator from env tab left panel ([fd391bd](https://github.com/Snehal1112/rocket/commit/fd391bd360bcb828978ad6b54607b7683b368e04))
* **ui:** replace custom button-as-checkbox with shadcn Checkbox in env tab ([ff5b3c3](https://github.com/Snehal1112/rocket/commit/ff5b3c366017cba9c9180ec0231206490fadc50b))
* **ui:** restore footer border-t to border-border/40 in env tab ([555207a](https://github.com/Snehal1112/rocket/commit/555207a3b5efd99d8f9e000b24df39dc91634e23))
* **ui:** update styles for select component and adjust text sizes in OAuth2 token display ([bd17128](https://github.com/Snehal1112/rocket/commit/bd17128f91bf9bd21be3ee376604dc17b42f37f7))
* **ui:** use CSS grid for equal-width key/value fields in env tab ([07c4bc1](https://github.com/Snehal1112/rocket/commit/07c4bc1ef5f78e1723a4a97b4f26381ff6b9dd8b))
* **workspace:** call switchWorkspace from breadcrumb workspace picker ([c0550c2](https://github.com/Snehal1112/rocket/commit/c0550c2b875b10775de4623d5329c8de77432a7c))

### Performance Improvements

* **frontend:** memoize chart data; wire ConcurrencyChart into LiveDashboard ([cfc4882](https://github.com/Snehal1112/rocket/commit/cfc48821c81713394f1990c5d752b91db6966280))

## [0.6.5](https://github.com/Snehal1112/rocket/compare/v0.6.4...v0.6.5) (2026-04-27)

### Features

* add MarkdownRenderer component with syntax highlighting support ([62422a7](https://github.com/Snehal1112/rocket/commit/62422a76c802b57b5e091c0c0e79beeb03fd6dad))
* **CollectionNode:** auto-expand active collection in pane store ([f93b46a](https://github.com/Snehal1112/rocket/commit/f93b46a8c75fc1f0b992dde6d985a2b3ae433a60))
* **panes:** add breadcrumb sibling picker popovers for all segment types ([7112a4c](https://github.com/Snehal1112/rocket/commit/7112a4cf96ef0d63b6c90b10d878b6f3cd4e63eb))
* **panes:** add BreadcrumbBar component with per-tab segment derivation ([6d732c7](https://github.com/Snehal1112/rocket/commit/6d732c7ce1dbf89e819ed5bafa337d39e2c3af4a))
* **panes:** mount BreadcrumbBar in EditorGroup below tab strip ([6b492ef](https://github.com/Snehal1112/rocket/commit/6b492efbad59fabd86e72bdc10b721c72716e756))
* **theme:** add breadcrumb CSS vars from VSCode 2026 tokens ([967bc1b](https://github.com/Snehal1112/rocket/commit/967bc1b8eb932457861d5b520ba580e92b23b775))
* **theme:** add button/checkbox/dropdown/badge/scrollbar CSS vars from VSCode 2026 ([b1f6125](https://github.com/Snehal1112/rocket/commit/b1f6125f70f82b450207c1a69f0bb62da8454507))
* **theme:** add editor/diff CSS vars from VSCode 2026 ([1a8e179](https://github.com/Snehal1112/rocket/commit/1a8e179155aa292ad7a97c605f201a82b24bf456))
* **theme:** add list/panel/sidebar/titlebar CSS vars from VSCode 2026 ([07013f3](https://github.com/Snehal1112/rocket/commit/07013f36c26cfe77be4ac9bc6a6c02fe6ed2c67c))
* **theme:** add notification and chart CSS vars from VSCode 2026 ([59d263f](https://github.com/Snehal1112/rocket/commit/59d263f494cd71ac4dc00156735bd8ca73143191))
* **theme:** add VSCode 2026 chart palette and verify CodeMirror warning token ([e25530a](https://github.com/Snehal1112/rocket/commit/e25530a37a0bcc44bd27b24a2e965c8b84fa1405))
* **theme:** implement VSCode 2026 tokens for list, panel, layout, editor, and diff components ([d7b764c](https://github.com/Snehal1112/rocket/commit/d7b764cd752d62bf23b10e91f5cb267e83241f12))
* **theme:** update dropdown and context menu styles to use new CSS vars ([5e2c488](https://github.com/Snehal1112/rocket/commit/5e2c488e2f01b72a276e1e3d603bdb76c58d026e))
* **theme:** wire button/checkbox/dropdown/select/badge to VSCode 2026 tokens ([53ffcb1](https://github.com/Snehal1112/rocket/commit/53ffcb170dcff4f179f9d4c23500e6b453c31e3e))
* **theme:** wire CodeMirror badge colors and VisualDiffView to CSS vars ([73dbe5e](https://github.com/Snehal1112/rocket/commit/73dbe5e58de8272774bc3a54b283f33b1d2db739))
* **theme:** wire Monaco editor theme to VSCode 2026 CSS vars ([c0c78d0](https://github.com/Snehal1112/rocket/commit/c0c78d0fc5bd564d699c7c2b86f6c37c5af951e3))
* **theme:** wire panel/sidebar/titlebar to VSCode 2026 tokens ([614633c](https://github.com/Snehal1112/rocket/commit/614633cd57abfc242acf7f6f5da330344f93d726))
* **theme:** wire tree list item states to VSCode 2026 list tokens ([5470b44](https://github.com/Snehal1112/rocket/commit/5470b44e9b4d9787b37b23a1a2c5521dc0e08833))

### Bug Fixes

* **cm6:** replace hardcoded rgb() variable-token colors with VSCode 2026 palette ([a400790](https://github.com/Snehal1112/rocket/commit/a400790ce0a760c5c93eaddc16f1396ba08abd10)), closes [#b69500](https://github.com/Snehal1112/rocket/issues/b69500) [#587c0c](https://github.com/Snehal1112/rocket/issues/587c0c) [#0069cc](https://github.com/Snehal1112/rocket/issues/0069cc) [#652d90](https://github.com/Snehal1112/rocket/issues/652d90) [#606060](https://github.com/Snehal1112/rocket/issues/606060) [#e5ba7d](https://github.com/Snehal1112/rocket/issues/e5ba7d) [#73c991](https://github.com/Snehal1112/rocket/issues/73c991) [#3994bc](https://github.com/Snehal1112/rocket/issues/3994bc) [#ad80d7](https://github.com/Snehal1112/rocket/issues/ad80d7) [#8c8c8c](https://github.com/Snehal1112/rocket/issues/8c8c8c)
* **panes:** replace raw button elements with shadcn Button in BreadcrumbBar ([6082206](https://github.com/Snehal1112/rocket/commit/60822063f07fead97bfd81017cffdb1a90268734))
* **panes:** stabilise nav memo, fix folder picker items, uid keys, auto-width popover ([e38a636](https://github.com/Snehal1112/rocket/commit/e38a636a7b621eb631dd57d2aead4f177ec71663))
* **panes:** tighten BreadcrumbBar types, key, ARIA landmark and exhaustiveness ([7489457](https://github.com/Snehal1112/rocket/commit/7489457a54fdce33e9204c5c73a69f58fb770e20))
* **panes:** unique composite key and exhaustive workspace section switch ([7a39b1c](https://github.com/Snehal1112/rocket/commit/7a39b1c8341e901c7ee96569054d977fa872d15b))
* **theme:** add sidebar elevation shadow matching VSCode 2026 Light ([a22a0a1](https://github.com/Snehal1112/rocket/commit/a22a0a138a1cb3505603b70566d01dd03894808f))
* **theme:** apply off-white light theme — reduce glare on card-heavy layout ([95c96ab](https://github.com/Snehal1112/rocket/commit/95c96ab7a5e99f10b647c6c6c3ae1ff2839fe7c5)), closes [#eef0f4](https://github.com/Snehal1112/rocket/issues/eef0f4) [#fafafd](https://github.com/Snehal1112/rocket/issues/fafafd) [#f6f7fa](https://github.com/Snehal1112/rocket/issues/f6f7fa) [#ffffff](https://github.com/Snehal1112/rocket/issues/ffffff) [#f0f2f6](https://github.com/Snehal1112/rocket/issues/f0f2f6) [#fafafd](https://github.com/Snehal1112/rocket/issues/fafafd) [FAFAFD/#FFFFFF](https://github.com/FAFAFD/rocket/issues/FFFFFF)
* **theme:** calibrate VSCode 2026 light/dark surface hierarchy and border tiers ([e86b41e](https://github.com/Snehal1112/rocket/commit/e86b41ec9e9571d0bb6944cf7c1209ffb3790044)), closes [#e6e8ea](https://github.com/Snehal1112/rocket/issues/e6e8ea) [#c7c7c7](https://github.com/Snehal1112/rocket/issues/c7c7c7) [#e0e0e0](https://github.com/Snehal1112/rocket/issues/e0e0e0) [#c8c8c8](https://github.com/Snehal1112/rocket/issues/c8c8c8) [#fff](https://github.com/Snehal1112/rocket/issues/fff) [#fafafd](https://github.com/Snehal1112/rocket/issues/fafafd) [#202122](https://github.com/Snehal1112/rocket/issues/202122) [#181a1b](https://github.com/Snehal1112/rocket/issues/181a1b) [#2d2e2f](https://github.com/Snehal1112/rocket/issues/2d2e2f)
* **theme:** remove conflicting text-popover-foreground from dropdown content ([058f468](https://github.com/Snehal1112/rocket/commit/058f468a70530534288cb41bce14c26e2a2210ff))
* **theme:** replace color-mix calls in dark CodeMirror theme with CSS vars ([204b816](https://github.com/Snehal1112/rocket/commit/204b816a57cf9f7504844ea567d6ef61639c359f))
* **theme:** surface separation + VSCode 2026 Monaco themes ([9ef5c4a](https://github.com/Snehal1112/rocket/commit/9ef5c4a90cd2fc0686064c23a7f7ddbc506a7788)), closes [#fff](https://github.com/Snehal1112/rocket/issues/fff) [#fafafd](https://github.com/Snehal1112/rocket/issues/fafafd) [#d8d8d8](https://github.com/Snehal1112/rocket/issues/d8d8d8) [#f1f2f3](https://github.com/Snehal1112/rocket/issues/f1f2f3) [#d8d8d8](https://github.com/Snehal1112/rocket/issues/d8d8d8) [#e2e3e5](https://github.com/Snehal1112/rocket/issues/e2e3e5) [#116329](https://github.com/Snehal1112/rocket/issues/116329) [#0a3069](https://github.com/Snehal1112/rocket/issues/0a3069) [#cf222e](https://github.com/Snehal1112/rocket/issues/cf222e) [#8250df](https://github.com/Snehal1112/rocket/issues/8250df) [#FFFFFF](https://github.com/Snehal1112/rocket/issues/FFFFFF) [#7ee787](https://github.com/Snehal1112/rocket/issues/7ee787) [#a5d6ff](https://github.com/Snehal1112/rocket/issues/a5d6ff) [#ff7b72](https://github.com/Snehal1112/rocket/issues/ff7b72) [#d2a8ff](https://github.com/Snehal1112/rocket/issues/d2a8ff) [#121314](https://github.com/Snehal1112/rocket/issues/121314)
* **theme:** use rgba for dark button-secondary-hover token to preserve opacity ([51217e2](https://github.com/Snehal1112/rocket/commit/51217e287912258bdadcb6f6c661500493456fe8))
* **ui:** add missing space-y-4 wrapper in variables tab ([0131820](https://github.com/Snehal1112/rocket/commit/0131820fe442db94b8b8a24a1a258823f83d804a))
* **ui:** move biome-ignore comment to cover key={idx} in EnvironmentDialog ([ff79846](https://github.com/Snehal1112/rocket/commit/ff798462a0734ecac6b780170d0d9962dd9a68c7))
* **ui:** replace auto-save with explicit save button in EnvironmentDialog ([73c6c2e](https://github.com/Snehal1112/rocket/commit/73c6c2ec739cbe72b34fcc653dfe77a0d0655a68))
* **ui:** replace auto-save with explicit save button in WorkspaceEnvironmentsTab ([7e9212a](https://github.com/Snehal1112/rocket/commit/7e9212acc900b78af2924c3802991ae77ff9ca7b))
* **ui:** wrap collection variables tab in Card/CardContent ([1ab5abb](https://github.com/Snehal1112/rocket/commit/1ab5abb5604bc0fa9e7197ab8a895ee0fbc6a597))
* **ui:** wrap path and query params in Card layout with section headers ([7c2cbf5](https://github.com/Snehal1112/rocket/commit/7c2cbf5a295478f6a55af10bee9fb8d0d6ef8131))
* **ui:** wrap request settings section in Card/CardContent with ScrollArea ([c6494c9](https://github.com/Snehal1112/rocket/commit/c6494c9cb9560fa3f71b425f864be68bd641a36f))
* **ux:** elevate collection header when overview left panel is scrolled ([45b5e7f](https://github.com/Snehal1112/rocket/commit/45b5e7f2e82ae175635404b2f851a61090041a80))

## [0.6.4](https://github.com/Snehal1112/rocket/compare/v0.6.3...v0.6.4) (2026-04-25)

### Features

* **collection-overview:** two-column overview, Documentation tab, docs field, remove Readme/Tags tabs ([dce8859](https://github.com/Snehal1112/rocket/commit/dce8859d375377bfc5286dc5ac000423f462f750))
* **git:** add always-visible key button to change credentials in landing panel ([130f1f6](https://github.com/Snehal1112/rocket/commit/130f1f63b57ab8ab1fd42b7ae33ff419e526864d))
* **git:** add commit drill-down — click commit in log to view per-file diffs ([dc65087](https://github.com/Snehal1112/rocket/commit/dc65087f0d39a5bccd8405388a35b1b00500698b))
* **git:** add confirmation dialog before discard-all in file list ([dc639bc](https://github.com/Snehal1112/rocket/commit/dc639bc85c9dc8a8d265233fb12e189b7e9cb8cc))
* **git:** add Ctrl/Cmd+Shift+G keyboard shortcut to open git panel ([384ffd4](https://github.com/Snehal1112/rocket/commit/384ffd490cb128f8e8d957dfbc8c8718b85296c5))
* **git:** add hideStageToggle prop to DiffViewer and DiffHeader components ([6851a57](https://github.com/Snehal1112/rocket/commit/6851a5765962384d59abcb40384aaf17afad1514))
* **git:** show 'No files staged' hint in commit form when nothing is staged ([d797236](https://github.com/Snehal1112/rocket/commit/d797236d0b289a46e83074707a2eb9c0cc543597))
* **layout:** add useLayoutStore Zustand store ([63a5ef1](https://github.com/Snehal1112/rocket/commit/63a5ef1f2afa07250c2324b99c303daebfab1597))
* **layout:** persist requestLayout to ui-state.yml on change and restore on startup ([8772043](https://github.com/Snehal1112/rocket/commit/8772043b973f688116d4ae8e359f1028af9d6ec5))
* **markdown-editor:** upgrade to Documentation card style with optional save/mode props ([97fd820](https://github.com/Snehal1112/rocket/commit/97fd8200c4c0a5affdcaf181024dc186edb3c005))
* **oauth2:** add force_reauth field to OAuth2GetTokenRequest and ResolvedOAuth2Config ([0de008a](https://github.com/Snehal1112/rocket/commit/0de008ac01b6ea961654261edbefc66fc873e4a6))
* **oauth2:** add forceReauth field to OAuth2 frontend types ([310f5de](https://github.com/Snehal1112/rocket/commit/310f5de655cbd7ff53bdc5478e801127e75636dd))
* **oauth2:** inject prompt=login and forceReauth on Clear Cache for auth code flow ([734ded2](https://github.com/Snehal1112/rocket/commit/734ded2e4147d82ec4b2fe7f9b1cd7c8fc5dec78))
* **oauth2:** thread force_reauth through auth_code_flow; clear webview session on Linux ([08bb94c](https://github.com/Snehal1112/rocket/commit/08bb94cc7557c6f14a074f220d4378747115e669))
* **request-panel:** implement side-by-side layout mode ([cb25f59](https://github.com/Snehal1112/rocket/commit/cb25f59ced9fd9723f893ab194cf6c839c3f52b2))
* **status-bar:** add layout toggle button ([3a60fe7](https://github.com/Snehal1112/rocket/commit/3a60fe723a21562a2243444dcd7547442afa84a2))
* **types:** update CollectionSection — remove readme/tags, add documentation ([9b00859](https://github.com/Snehal1112/rocket/commit/9b00859a445490154c726d8635a515f062a219e2))
* **ui-state:** add layout_direction field to UiState ([8343dac](https://github.com/Snehal1112/rocket/commit/8343dac7b39c2cf1efb5afba50f3a7aa13335cfd))
* **ui-state:** add layoutDirection to UiState TS interface ([32cceec](https://github.com/Snehal1112/rocket/commit/32cceecfc9be03bc378421916728958e92211338))
* **ui:** add scroll elevation shadow to collection header tab bar ([a5ac416](https://github.com/Snehal1112/rocket/commit/a5ac416fde178da5442a8041fbeaddfbe25b45ec))

### Bug Fixes

* **api:** update CollectionSettings interface — replace description/readme with docs ([66f8f15](https://github.com/Snehal1112/rocket/commit/66f8f1510a7243c5ddbf87cd929b8fd297e10a1b))
* **auth:** preserve auth fields when switching auth type, use Card in Authorization tab ([2446c53](https://github.com/Snehal1112/rocket/commit/2446c53e0b09c22c486baf820a5132bc11641c97))
* **collection-overview:** complete h-full height chain so two-column flex layout resolves correctly ([390997c](https://github.com/Snehal1112/rocket/commit/390997c8a86e7f40cae57a7955574f9f52b6272c))
* **collection-overview:** constrain ScrollArea content to viewport width with w-full ([5d8d262](https://github.com/Snehal1112/rocket/commit/5d8d262d05e2a18b022541d6abdad9d038581beb))
* **collection-overview:** equal-width two-column layout — both columns flex-1 min-w-0 ([2d130f6](https://github.com/Snehal1112/rocket/commit/2d130f6265619de6cc1e9881cb7d9c0e851e1cce))
* **collection-overview:** fix card clipping — remove redundant overflow-hidden, use flex-1 on ScrollArea ([fffb338](https://github.com/Snehal1112/rocket/commit/fffb33868f380f96f8721dad5d0b669fcad04519))
* **collection-overview:** replace ScrollArea with overflow-y-auto in left column — Radix display:table wrapper broke flex width ([4a87344](https://github.com/Snehal1112/rocket/commit/4a873445c5d0c2023f723d598fd312d11651a6a7))
* **collection-overview:** use unified saveSettings for docs — prevents partial overwrite ([b75fab5](https://github.com/Snehal1112/rocket/commit/b75fab51226a5d6d9efe69e49ef0eea7b9ade305))
* **collection:** remove unused import in docs roundtrip test ([f4b94a1](https://github.com/Snehal1112/rocket/commit/f4b94a1feb0011abcdf3a9dd4a738bc7942410dd))
* **collection:** store documentation in spec-compliant docs field, remove non-standard readme ([87964a2](https://github.com/Snehal1112/rocket/commit/87964a2d29654e40b477b82cc692f33ac0dd83f8))
* **collection:** update rocket-app tests and CLAUDE.md for docs field migration ([c96ecfc](https://github.com/Snehal1112/rocket/commit/c96ecfc954aef09f99697aca8c7187a8ab7047fe))
* **contract:** use bg-card for contract panel surfaces ([bf53154](https://github.com/Snehal1112/rocket/commit/bf5315414fb55c277886b250166e95285fb4bcae))
* **git:** add HEAD rollback and TYPECHANGE flags to switch_branch dirty guard ([b6c26bf](https://github.com/Snehal1112/rocket/commit/b6c26bf044079e2287e318e4d9c752426e01ab7f))
* **git:** await mergeBranch in BranchSelector and surface errors inline ([3296a58](https://github.com/Snehal1112/rocket/commit/3296a58252cc1b0ee223ed93cc20cb677baf8ef0))
* **git:** expand ~ in SSH key path; default credentials dialog to sshKey ([e082f54](https://github.com/Snehal1112/rocket/commit/e082f54727726ae439e3a56cb0c53d3efda9d239))
* **git:** format commitDiff display for improved readability ([f5e3ca7](https://github.com/Snehal1112/rocket/commit/f5e3ca7c9c65cb115e9415b9c1cc46cfc34d9668))
* **git:** merge_branch writes index and returns Conflict error on merge conflicts ([38b9c7e](https://github.com/Snehal1112/rocket/commit/38b9c7e99371483e7f371e728db1109b7cbc7bce))
* **git:** populate files_changed in commit() and log() via diff_tree_to_tree ([5cc9458](https://github.com/Snehal1112/rocket/commit/5cc9458156b2e0f6265d9bb89c80cc47b28d8da0))
* **git:** push uses configured upstream refspec instead of hardcoded same-name ([0053188](https://github.com/Snehal1112/rocket/commit/00531883bd62aefcafd9b9f25b9eea9b2f74da78))
* **git:** skip stash pop when pull produces merge conflicts in stash-and-pull flow ([13b1c71](https://github.com/Snehal1112/rocket/commit/13b1c71e6f506ba3999bd2af2c1c757001d8ba0c))
* **git:** surface ConflictResolver errors, fix stale closure, refresh log after commit ([be11e42](https://github.com/Snehal1112/rocket/commit/be11e42003e494d7807e907d3bc3b7e4301cdc9f))
* **git:** switch_branch refuses when working tree has uncommitted changes ([d6ec4cc](https://github.com/Snehal1112/rocket/commit/d6ec4cc80f2dc7ddf45b90eb88a2800f3b81ccbf))
* **markdown-editor:** use shadcn Textarea, reset mock between tests ([e9644bc](https://github.com/Snehal1112/rocket/commit/e9644bc5d911ff5a1ff02cb4832a961c30c9da81))
* **oauth2:** deduplicate prompt param, tighten dep array, document Linux async race ([9f4e463](https://github.com/Snehal1112/rocket/commit/9f4e463ff896b038ec93b9a5a18484133c4d34c2))
* **oauth2:** enhance error handling for authorization URL and resolve variables in OAuth2 requests ([7261485](https://github.com/Snehal1112/rocket/commit/7261485953d5daaef835c84d225512721cde9cc3))
* **oauth2:** only set forceReauth on Clear Cache for authorization_code grant ([8a00fb9](https://github.com/Snehal1112/rocket/commit/8a00fb9c09066a76e092bdd31f5516475be338df))
* **oauth2:** use webkit2gtk re-exports for glib/gio and correct trait name ([a8bfda6](https://github.com/Snehal1112/rocket/commit/a8bfda61189147d045aafdb7bc238d1dcf5d714d))
* **response:** use bg-card for response viewer toolbars ([07882cf](https://github.com/Snehal1112/rocket/commit/07882cf09c197721e9d1453e15675ed09210552e))
* **secret-mask:** include $ in var regex so dynamic vars stay visible in secret fields ([6c1148f](https://github.com/Snehal1112/rocket/commit/6c1148fa882a8d7ff28d20f6606aa9d60c270bcb))
* **secret-mask:** keep partial {{ opener visible so autocomplete activates in secret fields; add cm-var-dynamic theme color ([b4d8193](https://github.com/Snehal1112/rocket/commit/b4d8193c41d4af63935c365edcf22f05c7c158d1))
* **sidebar:** align folder font size and workspace card bg with design system ([9e0fbb9](https://github.com/Snehal1112/rocket/commit/9e0fbb9a4bab330857efaa44e7c0db87182e3882))
* **tabs:** use bg-card for active tab elevation after layer swap ([2cf0622](https://github.com/Snehal1112/rocket/commit/2cf0622b2b15baee7e49ac16e3d3f084d8c676c7))
* **theme:** swap --background and --card roles for correct layer depth ([abd720f](https://github.com/Snehal1112/rocket/commit/abd720fafa29152d8ebd9485a05097300baa858c)), closes [#181818](https://github.com/Snehal1112/rocket/issues/181818) [#1F1F1F](https://github.com/Snehal1112/rocket/issues/1F1F1F) [#F8F8F8](https://github.com/Snehal1112/rocket/issues/F8F8F8) [#FFFFFF](https://github.com/Snehal1112/rocket/issues/FFFFFF)
* **ui-state:** restore collection tabs and active collection after reload ([92e310d](https://github.com/Snehal1112/rocket/commit/92e310d6cd780f52e0adf648667e6fc1ab066de8))
* **ui:** use absolute overlay gradient for scroll elevation instead of inset shadow ([0cbbd58](https://github.com/Snehal1112/rocket/commit/0cbbd586c3bafe8911240236518db60ecae1f412))
* **ui:** use bg-card for audit log, dialog, and alert-dialog surfaces ([690d563](https://github.com/Snehal1112/rocket/commit/690d56309f4fae9df0c46722d3c926554fbbb08a))
* **ui:** use drop-shadow filter to bypass overflow-hidden clip on scroll elevation ([f96b89c](https://github.com/Snehal1112/rocket/commit/f96b89c234cedd00f8ea1fac6e77ce0d61ed221b))
* **ui:** use inset shadow on scroll container for elevation — bypasses overflow-hidden clipping ([84486ad](https://github.com/Snehal1112/rocket/commit/84486ad5e399c99ae7d628596b795a25c94a7262))
* **variable-highlight:** seed context before EditorView creation; add variableContext to collection auth editors ([48d763f](https://github.com/Snehal1112/rocket/commit/48d763ff78d6e8af091b0d4b137e0cf4d5e4f022))
* **variables:** resolve dynamic vars ($name) in all SingleLineEditor inputs ([8058547](https://github.com/Snehal1112/rocket/commit/80585474cf2aecefc4055a8b4382f9f9fde8f2a2))

### Reverts

* Revert "chore(release): v0.6.4" ([daf386b](https://github.com/Snehal1112/rocket/commit/daf386bb6e27990b0e45acf710d3d1a986dfffce))

## [0.6.3](https://github.com/Snehal1112/rocket/compare/v0.6.2...v0.6.3) (2026-04-22)

### Features

* **ui:** add icons to CollectionNode context menu and dropdown items ([d22d957](https://github.com/Snehal1112/rocket/commit/d22d957649afae6748ce1a793733a3bff8c32d18))
* **ui:** add icons to FolderNode context menu items ([2035b76](https://github.com/Snehal1112/rocket/commit/2035b7664378c6c0c5403b9454f73d57b058e1b8))
* **ui:** add icons to GitStashSection stash action dropdown ([0de7cd0](https://github.com/Snehal1112/rocket/commit/0de7cd09224555b69056fb208fa839963f0dafad))
* **ui:** add icons to RequestNode menus ([765d8b2](https://github.com/Snehal1112/rocket/commit/765d8b20b9c6e1e8b1f35df43499e0c071d3cbc7))
* **ui:** add icons to TabBar context menu items ([0c636aa](https://github.com/Snehal1112/rocket/commit/0c636aafb393f5449d7a291fc08b13d332f65bef))
* **ui:** add icons to WorkspaceSwitcher per-workspace dropdown items ([d441e43](https://github.com/Snehal1112/rocket/commit/d441e438c89b0087346f13b3813fdcb6bc7ddce0))

### Bug Fixes

* **oauth2:** default to system browser on macOS and Windows for auth code flow ([e8165c3](https://github.com/Snehal1112/rocket/commit/e8165c343db0a310921322a5a65cd6884e35505d))
* **oauth2:** disable auto-fetch when clearing token cache ([f108bde](https://github.com/Snehal1112/rocket/commit/f108bde9c5a8dafa6a2f84200314b6ad4e107c30))

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
