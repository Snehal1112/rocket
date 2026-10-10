# Collection Trust Gate Design

Date: 2026-10-11
Status: draft for review
Supersedes: Section 1 (trust model) of `docs/superpowers/specs/2026-10-07-js-script-security-design.md`.
The scanner (Section 2 of that draft) is unchanged and stays a separate later plan.

> Before implementing, read `docs/superpowers/specs/opencollection-spec-reference.md`. This work
> touches `opencollection.yml`, collection settings, variable resolution and collection IPC.

## 1. Problem

A collection's own `opencollection.yml` is git-shared, and today three of its keys grant power
directly:

| Key in `opencollection.yml` | Read at | Power it grants |
|---|---|---|
| `extensions.rocketapi.sandboxMode: developer` | `crates/rocket-infra/src/fs_collection/settings.rs:19-31` | Scripts get the Developer ops: unrestricted `fs.*` and `process.exec` (`crates/rocket-infra/src/scripting/engine.rs:448-452`), plus the host environment (`engine.rs:498-499` only clears it in Safe mode). |
| `extensions.rocketapi.scripts.additionalContextRoots` | `settings.rs:66-79` | Scripts may `require()` from those directories, Developer mode only (`crates/rocket-infra/src/scripting/local_modules.rs:39`). |
| `extensions.rocketapi.agentAutonomyEnabled: true` | `settings.rs:194-201` | The AI assistant may run the collection's requests (`crates/rocket-app/src/mcp_tool_service.rs:198-207`) and the session gets MCP servers (`crates/rocket-app/src/acp_session_service.rs:199-209`). |

A cloned or pulled collection can set any of them, and it takes effect on the next send or
session. The comment at `crates/rocket-collection/src/settings.rs:23-25` ("an imported collection
never silently inherits an elevated capability") is false today. The comment at `settings.rs:69-74`
already admits the gap for agent access.

A fourth power needs no setting at all: `{{process.env.NAME}}` resolves the host environment in any
collection, with no script. HTTP resolves it in the frontend (`src/lib/execute-request.ts:69-71`,
`244`, `590`; values from the `get_process_env_vars` command, `src-tauri/src/commands/environments.rs:171-174`).
WebSocket, GraphQL subscriptions and gRPC resolve it in the backend
(`crates/rocket-app/src/execution_service.rs:893-913`, called from
`crates/rocket-app/src/execution_service/websocket_resolution.rs:145` and `:206`, and
`src-tauri/src/commands/grpc.rs:57`). A cloned collection can put
`{{process.env.AWS_SECRET_ACCESS_KEY}}` in a header of a request to its own host. A pre-request
script can also read the resolved value through `req.getUrl()` or `req.getHeader()`.

## 2. User decision and goal

Per-collection confirmation. An elevated capability takes effect only after the user confirms it
for that collection on this computer. The confirmation lives in the user's own app data
(`~/.rocket-api/`), never in collection files, so a clone cannot grant itself anything.

The collection file value becomes a **request**. The trust store decides what is **granted**.
What runs is the **effective** value.

## 3. What is gated (question 1)

### In scope for v1

| Capability | Requested by | Granted unit | Effective when |
|---|---|---|---|
| `developer_mode` | `sandboxMode: developer` | a boolean | requested and granted |
| `context_roots` | `additionalContextRoots` list | the approved list of roots | effective Developer mode, and only the requested roots that are in the approved list |
| `agent_run` | `agentAutonomyEnabled: true` | a boolean | requested and granted |
| `process_env` | implicit, any `{{process.env.*}}` | a boolean | granted (no file request exists) |

`process_env` is in v1 (coordinator addendum). It is the cheapest exfiltration path, needs no
script and no setting, and the gate costs Bruno-style collections one click (Section 11).

Script access to the host environment (`rok.getProcessEnv`) is not a separate capability. It
follows effective Developer mode, as it does today (`engine.rs:498-499`).

### Checked and left out of v1

| Setting | Where | Why it is out |
|---|---|---|
| File and multipart bodies | `crates/rocket-infra/src/reqwest_executor.rs:116-170`, `245-306` | Already confined to the collection folder or the workspace. |
| `config.proxy`, `config.clientCertificates` in `opencollection.yml` | `crates/rocket-infra/src/oc/collection.rs:60-69` | Parsed for round-trip only, no consumer applies them. If either is ever honoured it must join this gate. |
| Client certificate paths in collection environments | `crates/rocket-infra/src/conversions/environment.rs:75` | The certificate is presented only to a matching domain, and the private key never leaves the machine. Low value to an attacker. |
| `requestGuardPolicy` in `workspace.yml` | `crates/rocket-infra/src/oc/workspace.rs:32-71` | Protective and off by default. A shared file can only turn off a guard the user opted into. Listed in Section 13. |
| AWS SigV4 profile name in request auth | `crates/rocket-infra/src/aws_profile.rs` | The signature is bound to the target host and cannot be replayed to AWS. The access key id is exposed, the secret is not. |
| Local `require()` inside the collection folder | `local_modules.rs` | Safe mode, collection root only. No extra power. |
| Script content changes | everywhere | Trust is a decision about a collection's authors, not each script (same ruling as the 2026-10-07 draft). The scanner is the later aid. |

## 4. Trust store (question 2)

### Location and format

`~/.rocket-api/trust.yml` (the draft's name, open item 1 resolved as proposed). It sits next to
`proxy.yml`, `secret_managers.yml` and `agent_configs.yml` (`src-tauri/src/lib.rs:254-257`, `397`,
`419`, `441`). Persistence struct, so snake_case keys and no `rename_all` (hard rule).

```yaml
version: 1
migrated: true                  # One-time grandfathering has run (Section 9).
migration_notice:               # Collections to list in the one-time notice. Cleared on dismiss.
  - /home/u/work/payments-api
collections:
  - root: /home/u/work/payments-api   # Canonical collection directory.
    uid: 01J9ZQ7K3V...                # opencollection.yml uid at grant time, absent if the file had none.
    developer_mode: true
    context_roots: ["../shared"]      # Approved, normalised entries.
    agent_run: false
    process_env: true
    source: user                      # user | created | migrated
    updated_at: 2026-10-11T09:12:00Z
```

### Key: canonical path plus uid

- **Canonical root path.** Collection names are not unique across workspaces, and the trust store
  is global, so the name is not a key. The canonical path (symlinks resolved) is chosen by the
  user's own checkout location, which a repository cannot pick.
- **Plus the `uid` from `opencollection.yml`.** A record matches only when both the path and the
  uid match (a record with no uid matches on path alone). This stops a different repository cloned
  into a deleted collection's folder from inheriting its grants. The uid is in the shared file, so
  it is a tripwire, not a secret: changing it fails closed (re-confirm), and copying another
  collection's uid still needs the same path.
- **Deviation from the draft.** The draft keyed on path only.

### Renames, moves, deletes

- Rename in Rocket (`crates/rocket-app/src/collection_service.rs:161-169`): re-key the record from
  the old identity to the new one. The user did it in the app, so the grant follows.
- Delete in Rocket (`collection_service.rs:146-159`): remove the record.
- Move or rename outside Rocket: the path no longer matches, so the collection is untrusted and
  shows the request banner (Section 7). This is intended, as in the draft. No fuzzy matching by uid.

### Writes and failure

- Atomic writes with the existing `rocket_infra::atomic_write` (used at
  `crates/rocket-infra/src/fs_collection/settings.rs:363`), under a store-level `Mutex` for
  read-modify-write.
- Read on every check. The file is small and each consumer already reads `opencollection.yml` per
  call. No cache, so another Rocket window's grant is seen at once.
- **Missing file.** Triggers the one-time migration (Section 9), then behaves as empty.
- **Corrupt file (parse error or unknown `version`).** Fail closed: every collection is untrusted,
  sends still work in Safe mode. The trust status reports `storeError`, and the UI shows it. The
  migration does **not** run again (the file exists). The first grant after a corruption moves the
  bad file to `trust.yml.corrupt-<unix-seconds>` and writes a fresh file with `migrated: true`.
- **Write failure on grant.** The grant command returns an error, the UI keeps the capability off.

## 5. Effective-value resolution (question 3)

### One pure function, one app helper

- Domain (pure, in `rocket-collection`, new module `trust.rs`):

  ```rust
  pub struct RequestedElevation { developer_mode: bool, context_roots: Vec<String>, agent_run: bool }
  impl RequestedElevation { pub fn from_settings(s: &CollectionSettings) -> Self }
  pub struct CollectionGrant { developer_mode: bool, context_roots: Vec<String>, agent_run: bool, process_env: bool, source: GrantSource }
  pub struct EffectiveCapabilities { sandbox_mode: SandboxMode, context_roots: Vec<String>, agent_run: bool, process_env: bool }
  pub fn resolve_effective(req: &RequestedElevation, grant: Option<&CollectionGrant>) -> EffectiveCapabilities
  pub fn normalize_root(raw: &str) -> String  // trim, strip trailing '/', drop leading "./"
  pub fn request_fingerprint(req: &RequestedElevation) -> String  // canonical text, no hashing dependency
  ```

- App (`rocket-app`, new module `collection_trust.rs`):

  ```rust
  pub fn effective_capabilities(repo: &dyn CollectionRepository, store: &dyn CollectionTrustStore,
                                collection: &str) -> EffectiveCapabilities
  ```

  It reads the identity and settings through the caller's own repository (so a workspace switch is
  respected, see `src-tauri/src/lib.rs:464-470`), looks up the grant and calls `resolve_effective`.
  **Any error** (unknown collection, unreadable identity, corrupt store) returns
  `EffectiveCapabilities::untrusted()`: Safe, no roots, no agent run, no process env.

### Rules

| Requested | Granted | Effective |
|---|---|---|
| Safe | anything | Safe. No trust needed. |
| Developer | `developer_mode: true` | Developer |
| Developer | missing or false | Safe |
| roots R, effective Developer | approved A | R ∩ A (by normalised string). New roots are dropped until approved. |
| roots R, effective Safe | anything | none (already true at `local_modules.rs:39`) |
| `agentAutonomyEnabled: false` | anything | off. The file can always turn power **down**. |
| `agentAutonomyEnabled: true` | `agent_run: true` | on |
| `agentAutonomyEnabled: true` | missing or false | off |
| (none) | `process_env: true` | host environment resolves |
| (none) | missing or false | `{{process.env.*}}` stays unresolved |

A grant survives the file turning a capability off. If a later pull turns it back on, the
already-approved capability is active again. This adds no exposure: while it was approved, a pull
could already change any script.

### All consumers switch to the effective value

| Consumer | Anchor | Change |
|---|---|---|
| Script phases (send, runner, flows) | `crates/rocket-app/src/execution_service.rs:1704-1727` | Mode and `additional_roots` from `effective_capabilities`, not from `settings`. Set `var_ctx.process_env` (`:1699`) only when the effective mode is Developer. |
| Backend `{{process.env}}` for WS, GraphQL subscriptions, gRPC | `execution_service.rs:893-913` | Fill `scopes.process_env` only when `effective.process_env`. The three callers need no change. |
| Agent run gate | `crates/rocket-app/src/mcp_tool_service.rs:198-207` | Check `effective.agent_run`. Two messages: "turn on ..." when not requested, "allowed in the collection file but not confirmed on this computer, confirm it in Agent permissions" when requested and not granted. |
| Outline and list `run_allowed` | `mcp_tool_service.rs:493-497`, `:528-532` | `effective.agent_run`. |
| Masked settings view | `crates/rocket-app/src/mcp_read_views.rs:196-209` | `MaskedSettings::from_settings(settings, run_allowed: bool)`. Callers `mcp_tool_service.rs:580` and `crates/rocket-app/src/mcp_tool_service/chips.rs:72` pass the effective value. Chip text at `crates/rocket-app/src/assistant_chip_text.rs:317` then reads correctly with no change. |
| ACP session MCP servers | `crates/rocket-app/src/acp_session_service.rs:199-209` | `effective.agent_run`. |
| Comments | `crates/rocket-collection/src/settings.rs:23-25`, `:69-76`; `crates/rocket-app/src/mcp_tool_service.rs:12-15` | Say the value is a request and the trust store decides. |

Guard test: a `rocket-app` unit test reads its own `src/**/*.rs` (via `CARGO_MANIFEST_DIR`) and
fails if `.sandbox_mode`, `.script_context_roots` or `.agent_autonomy_enabled` is read outside an
allow-list (`collection_trust.rs`, `collection_service.rs`, test modules, `test_doubles.rs`). This
keeps a new consumer from reading the raw request.

### Constructor default

`RequestExecutionService`, `McpToolService` and `AcpSessionService` take an
`Arc<dyn CollectionTrustStore>`. The default when none is wired is a deny-all store, so a missed
wiring fails closed. Tests that need Developer mode or agent run use an allow-all test double in
`crates/rocket-app/src/test_doubles.rs` (next to the settings doubles at `:279-310`).

## 6. Where writes happen: requests vs grants (question 4, backend half)

- **`save_collection_settings` never grants.** It is a full replace
  (`src-tauri/src/commands/collections.rs:350-357` to `collection_service.rs:452-462`), and every
  surface spreads the current settings into it, including a requested Developer mode from a pull.
  If a save granted, editing a header would approve a hostile request.
- **`save_collection_settings` no longer changes capability fields.** `CollectionService::save_settings`
  copies `sandbox_mode`, `script_context_roots` and `agent_autonomy_enabled` from the file on disk
  and ignores the incoming values. This also fixes a live bug: `buildSettingsForSave`
  (`src/lib/collection-settings-save.ts:14-19`) drops `agentAutonomyEnabled`, so saving the
  collection overview today silently turns agent run off.
- **One write path for capabilities:** `CollectionTrustService::set_capability(collection, cap, on)`.
  - On: write the grant first, then the file. If the file write fails, a grant without a request
    is harmless (effective stays off).
  - Off: revoke the grant first, then write the file. Fail closed in both orders.
  - `process_env` has no file field, so it only touches the store.
  - Context roots: `set_context_roots(collection, roots)` writes the list to the file and sets the
    approved list to the same normalised entries. No UI edits roots today; this is the API for one.
- **Approve without changing the file:** `grant_requested(collection, caps, expected_fingerprint)`.
  Used by the banner. It approves exactly the requested values the user was shown, and refuses
  with "This collection's settings changed. Review them again." when the current
  `request_fingerprint` differs (a pull between showing and clicking).
- **Revoke all:** `revoke(collection)` removes the record. The file is not touched.
- **Collections created in Rocket** (`collection_service.rs:137-144`, the only caller is
  `src-tauri/src/commands/collections.rs:129`): record `process_env: true`, `source: created`.
  They request nothing else, so nothing else is needed. Imports (`crates/rocket-import/src/importer.rs:586`),
  clones and linked folders get no record. This replaces the draft's open item 2.

## 7. UI (question 4, frontend half)

All new UI uses shadcn primitives (`alert`, `alert-dialog`, `badge`, `button`, `switch`, `table`,
`tooltip` in `src/components/ui/`) and `lucide-react` icons.

### Trust state on every surface

- New query hook `useCollectionTrust(collection)` over `get_collection_trust`, invalidated on the
  new `collection-trust-changed` event, on `collection-changed` (file watcher,
  `src/lib/tauri-api.ts:1356-1357`) and on settings saves. A pull or checkout therefore refreshes the
  banner with no git-specific hook.

### Sandbox popover (`src/components/layout/SandboxPopover.tsx`)

- The icon colour and the selected option follow the **effective** mode (`:31-38`).
- When Developer is requested and not granted, show a third state under the options: badge
  "Requested by this collection", text "This collection asks for Developer mode. It runs in Safe
  mode until you allow it on this computer.", and an "Allow on this computer..." button that opens
  the existing confirm dialog (`:232-259`).
- `setMode` (`:69-84`) calls `setCollectionCapability(collection, 'developerMode', on)` instead of
  `saveCollectionSettings`. Confirming in the dialog counts as the confirmation.
- The two raw `<button>` elements (`:128`, `:174`) break the shadcn rule. Replace them with
  `Button variant='ghost'` while the file is open.

### Agent toggle (`src/components/request/AgentAutonomyToggle.tsx`)

- `checked` follows `effective.agentRun` (`:46`).
- Requested but not granted: a line under the switch, "This collection's files turn this on. It is
  off until you allow it on this computer." Switching on opens the existing dialog (`:102-129`) and
  then calls `setCollectionCapability(collection, 'agentRun', true)`.
- `save` (`:59-74`) calls `setCollectionCapability` instead of `saveCollectionSettings`.
- `AssistantPermissionsPopover.tsx:37-46` needs no structural change, it renders this toggle.
  Its intro copy (`:29-31`) becomes "... Running requests needs the switch below, confirmed on this
  computer."

### Collection overview: trust section and banner

New `src/components/collections/CollectionTrustSection.tsx`, rendered in
`CollectionOverviewTab.tsx`. A `Table` with one row per capability:

| Capability | Requested by the collection | Allowed on this computer | Action |
|---|---|---|---|
| Developer mode (scripts get file and command access) | Yes / No | Yes / No | Allow... / Revoke |
| Extra script folders | listed roots, pending ones with a badge | approved roots | Allow... / Revoke |
| Agent may run requests | Yes / No | Yes / No | Allow... / Revoke |
| Host environment variables (`{{process.env.*}}`) | "Used by requests" when known | Yes / No | Allow... / Revoke |

Plus "Forget this collection's permissions" (`revokeCollectionTrust`).

New `src/components/collections/CollectionTrustBanner.tsx` (`Alert`, `ShieldAlert` icon), shown at
the top of the overview tab and of the request panel's Scripts tab when any capability is requested
and not granted:

> **This collection asks for more access than it has on this computer.**
> It asks for: Developer mode, agent request runs. Until you allow them, its scripts run in Safe
> mode and the agent cannot run its requests. Only allow this for a collection whose authors you
> trust. [Review...]

"Review..." opens an `AlertDialog` that lists each pending capability with a `Checkbox`, all
unchecked, and an "Allow selected" action that calls `grantRequestedCapabilities` with the
fingerprint from the query. A store error shows a destructive `Alert`: "Rocket could not read its
trust settings, so every collection runs with no extra access. Allowing a permission will reset
them."

### Process environment warning on send

Shared helper `warnIfProcessEnvWithheld(collection, texts)` in `src/lib/process-env-gate.ts`. Before
a send in any protocol, if the collection's `processEnv` is not granted and any raw request text
(URL, headers, params, body, auth fields) contains `{{process.env.`, it adds a console line and a
`sonner` toast: "Host environment variables are not allowed for this collection, so
`{{process.env.NAME}}` was sent unresolved. [Review]". The send still goes out (fail closed, never
block), matching the draft's "never blocks execution" ruling. The literal placeholder reaches the
server, which leaks only the variable name the collection already contains.

## 8. Detection of a changed request (question 5)

Minimal sound rule: **store the approved values, never an approval flag alone, and compute the
effective value on every use.**

- Developer mode and agent run are booleans, so the approved value is the request itself.
- Roots are approved one by one, so a pull that adds a root does not get it (R ∩ A), and the
  banner lists it as pending.
- A uid change fails closed for everything.
- The `expected_fingerprint` check on `grant_requested` covers the window between showing a
  request and approving it.
- **Deviation from the draft:** the draft stored one fingerprint over mode and roots, so adding a
  root silently turned off Developer mode. Per-capability values keep Developer mode working and
  only withhold the new root. The script baselines in the draft belong to the scanner and are left
  for that plan; the record has room for a later `script_baselines` field.

## 9. Migration for existing users

**Recommendation: grandfather once, at the first start of the new version, with a visible notice.**

- When `trust.yml` is missing, the app enumerates every collection of every registered workspace
  (the `<workspace>/collections` directories and the external collections listed in each
  `workspace.yml`), and records a grant equal to each collection's current request, plus
  `process_env: true`, with `source: migrated`. It writes `migrated: true`. A fresh install has no
  collections, so it only writes the empty file.
- Collections that kept Developer mode, extra roots or agent run are put in `migration_notice`.
  At start the frontend shows a one-time `AlertDialog`: "Rocket now asks before a collection gets
  extra access. These collections already had it and keep it: payments-api (Developer mode, agent
  runs), ... Review them in each collection's overview." Buttons "Review" (opens the first one) and
  "OK" (dismisses, clears the list). Host environment access is summarised as a count.

Why grandfather and not force re-confirmation:

- The hole is about **future** clones and pulls. Collections already on disk have already run with
  these powers, so revoking them now adds little protection against what already happened.
- Most existing grants were set by the user through the popover or toggle. Forcing a re-confirm of
  each, and losing `{{process.env}}` in every existing collection at once, would break working
  setups and train users to click "Allow" without reading.
- The notice makes every grandfathered high-power grant visible once, with a one-click review.

Cost: a hostile collection cloned before the upgrade keeps its powers. The notice names it.
Deleting `trust.yml` by hand re-runs the migration; that is the user's own data and out of scope.

## 10. Agent side (question 6)

- Run gate: `check_autonomy_enabled` (`mcp_tool_service.rs:198-207`) uses the effective value and
  the two messages from Section 5. It is still re-checked on every `run_request`.
- Session start: `acp_session_service.rs:199-209` gives MCP servers only when effective. A grant
  made mid-session takes effect on the next run (the run gate is re-checked), but MCP tools appear
  only in a new session, as today when the toggle changes.
- Outline, list and chips report the effective `run_allowed` (Section 5 table), so the agent never
  claims access it does not have.
- `src-tauri/src/mcp/tool_server.rs:1192-1216` and `src-tauri/src/mcp/stdio_bridge.rs:7` comments
  and test fixtures need the allow-all trust double.
- The agent cannot grant: no MCP tool writes the trust store, and proposals only write collection
  files, which are requests.

## 11. Process environment gate details

- **Backend command.** `get_process_env_vars(collection: Option<String>)`
  (`src-tauri/src/commands/environments.rs:171-174`, registered at `src-tauri/src/lib.rs:839`)
  returns an empty map when `collection` is given and `process_env` is not effective. With no
  collection (a scratch request, the environment dialog preview) it returns the full map, as today.
  The command becomes stateful (it needs the trust service and the collection repo).
- **Frontend wiring.**
  - `src/lib/tauri-api.ts:1544`: `getProcessEnvVars(collection?: string)`.
  - `src/lib/queries/environment-queries.ts:21`: `process: (collection: string | null) => [...]`,
    and `useProcessEnvVars(collection)` at `:54-60`. Invalidated with the trust query.
  - `src/lib/execute-request.ts:69-71`: becomes async, `fetchQuery` keyed by collection (today it
    reads the cache only and silently returns `{}` when not loaded). Callers `:244` and `:590`
    pass the collection.
  - Previews pass their collection: `src/components/request/RequestPanel.tsx:536`,
    `src/components/collections/CollectionOverviewTab.tsx:121`,
    `src/components/flow/properties/AuthNodeEditor.tsx:57`. `EnvironmentDialog.tsx:398` passes its
    collection when it edits a collection environment. Unresolved placeholders then show as
    unresolved in the highlighter (`src/lib/url-variables.ts:77`), matching what is sent.
- **Backend protocols.** Section 5 change at `execution_service.rs:893-913`.
- **Consistency.** Both paths read the same `effective.process_env`, from the same store, keyed by
  the same identity. The frontend receives an empty map, it does not decide.
- **Compatibility cost.** Collections created in Rocket and all collections present at upgrade keep
  working. A newly cloned or imported Bruno-style collection that uses `{{process.env.X}}` sends the
  literal placeholder until the user clicks "Allow" once; the toast says why and links to the
  review. No change to precedence (`crates/rocket-environment/src/context.rs:42-48`, a user variable
  named `process.env.X` still wins).

## 12. Backend and IPC (question 7)

### Placement (DDD)

| Layer | File | Contents |
|---|---|---|
| Domain | `crates/rocket-collection/src/trust.rs` (new), export in `crates/rocket-collection/src/lib.rs:1-40` | Types and `resolve_effective` (Section 5); `CollectionIdentity { canonical_root: PathBuf, uid: Option<String> }`; trait `CollectionTrustStore { load, grant_for(&CollectionIdentity), put, remove, rekey, migration_state, set_migrated, take_migration_notice }`. |
| Domain | `crates/rocket-collection/src/repository.rs:163` | New trait method `collection_identity(&self, name) -> DomainResult<CollectionIdentity>`, default `Err(NotFound)` so a double that lacks it fails closed. |
| Infra | `crates/rocket-infra/src/fs_collection/mod.rs:124` and `settings.rs` | `collection_identity`: canonicalise `collection_path(name)` and read `uid` from `opencollection.yml`. |
| Infra | `crates/rocket-infra/src/shared_path_collection_repo.rs:168` | Delegate. |
| Infra | `crates/rocket-infra/src/fs_trust_store.rs` (new), export in `lib.rs` | `FsCollectionTrustStore` over `trust.yml`: persistence structs (snake_case), `atomic_write`, mutex, corrupt-file handling. |
| App | `crates/rocket-app/src/collection_trust.rs` (new) | `effective_capabilities` helper; `CollectionTrustService { status, set_capability, set_context_roots, grant_requested, revoke, migrate_legacy, migration_notice, dismiss_notice }`. Publishes events. |
| App | `collection_service.rs:137-169`, `:452-462` | create, rename, delete hooks; capability fields preserved on save. |
| Shared | `crates/rocket-shared/src/events.rs:322` | `DomainEvent::CollectionTrustChanged { collection: String }`, mapped to the `collection-trust-changed` Tauri event. |
| Tauri | `src-tauri/src/commands/collection_trust.rs` (new), registered next to `src-tauri/src/lib.rs:821-822` | Thin commands. |
| Wiring | `src-tauri/src/lib.rs:254-257` (store at `data_dir.join("trust.yml")`), `:336`, `:349`, `:483-505`, `:561-582`, `:599`, `:706` | One `Arc<FsCollectionTrustStore>` shared by every service. Migration runs in setup before the commands are served. |

### Commands and DTOs (camelCase on DTOs only)

| Command | Input | Output |
|---|---|---|
| `get_collection_trust` | `collection` | `CollectionTrustDto` |
| `set_collection_capability` | `collection`, `capability: 'developerMode' \| 'agentRun' \| 'processEnv'`, `enabled` | `CollectionTrustDto` |
| `set_collection_context_roots` | `collection`, `roots: string[]` | `CollectionTrustDto` |
| `grant_requested_capabilities` | `collection`, `capabilities[]`, `expectedFingerprint` | `CollectionTrustDto` |
| `revoke_collection_trust` | `collection` | `CollectionTrustDto` |
| `get_trust_migration_notice` / `dismiss_trust_migration_notice` | none | `{ collections: [{ name, path, capabilities[] }] }` / none |
| `get_process_env_vars` (changed) | `collection?` | map |

```ts
interface CapabilityState { requested: boolean; granted: boolean; effective: boolean }
interface CollectionTrustDto {
  developerMode: CapabilityState;
  contextRoots: { requested: string[]; granted: string[]; effective: string[]; pending: string[] };
  agentRun: CapabilityState;
  processEnv: { granted: boolean };
  pending: boolean;               // Any capability requested and not granted.
  fingerprint: string;            // For grant_requested_capabilities.
  storeError: string | null;      // Set when trust.yml is corrupt.
}
```

`get_collection_settings` keeps returning the file's values (the requests). Errors map to stable
`DomainError` messages and never echo file contents.

## 13. Risks and what this does not protect (question 9)

- **Safe mode still has the network.** `rok.sendRequest` and `rok.runRequest` work in Safe mode
  (2026-10-07 addendum). An untrusted collection's script can send environment values, RocketVault
  values and response data to any host. Host environment variables are now withheld from it.
- **Requests send what they reference.** A pulled request can point at a new host while using
  `{{apiToken}}` from the user's environment. The gate does not inspect request content. The user
  sees the URL before sending; the scanner and request-guard work are the later aids.
- **Trusted means trusted.** After a Developer grant, any script change from a pull runs with full
  power. Roots under Developer mode add no power beyond Developer itself; gating them is defence in
  depth and keeps the user's mental model ("I approved these folders").
- **Path re-use.** A different repository cloned into the same folder with the same uid inherits
  grants. Copying a uid needs knowledge of the original collection and the user's exact path.
- **Grandfathered collections** keep their powers. The notice is the mitigation.
- **Local attacker.** Anyone who can write `~/.rocket-api/trust.yml` or run code as the user is out
  of scope.
- **`workspace.yml` from git** can turn off the opt-in request guard. Out of scope for v1, listed for
  the roadmap.
- **Multiple Rocket windows** writing `trust.yml` at the same moment: last writer wins. The store is
  re-read on each check, so the worst case is a lost grant (fail closed).
- **A missed consumer** would read the raw request. The guard test and the deny-all default reduce
  this.

## 14. Test plan

Domain (`rocket-collection`, pure):
- `resolve_effective` table: every row of Section 5, including "file turns power down", roots
  intersection, normalisation (`./a/`, `a`), and `untrusted()`.
- `request_fingerprint` changes when any requested value changes and is order-independent for roots.

Infra (`rocket-infra`, `tempfile`):
- Store round-trip, atomic write, missing file, corrupt file (fail closed, migration flag
  respected, quarantine on next write), unknown `version`.
- `collection_identity`: canonical path through a symlink, uid read, missing uid.
- Existing `fs_collection/settings.rs:378-591` tests stay green.

App (`rocket-app`, test doubles):
- A collection whose file requests Developer with no grant runs scripts in Safe mode with no roots
  and no host environment (adapt `execution_service.rs:9274-9296` and the roots test near `:9326`;
  add the granted counterpart). Same result for a plain send and the runner.
- `script_chain.rs:824-843`: Developer only with a grant.
- `build_variable_context_with_process_env`: no `process.env.*` keys without the grant.
- `run_request` refused with the "not confirmed on this computer" message when requested but not
  granted; allowed with the grant (adapt `mcp_tool_service.rs` tests at `:1259`, `:1802`, `:1967`,
  `:2071`, `:2167`). Outline, list, masked settings and chips report the effective value.
- ACP session: no MCP servers without the grant.
- `save_settings` keeps the file's capability fields whatever the payload says.
- `set_capability` on and off ordering, `grant_requested` refuses a stale fingerprint, rename
  re-keys, delete removes, create grants `process_env` only.
- Migration: grandfathers current requests once, fills the notice, never runs when the file exists.
- Deny-all default: a service built without a store refuses everything.
- Guard test for raw reads.

Tauri: `src-tauri/src/mcp/tool_server.rs:1192-1216` fixtures with the allow-all store.

Frontend (Vitest):
- `SandboxPopover`, `AgentAutonomyToggle`: effective state, requested-not-granted state, confirm
  calls `setCollectionCapability`.
- `CollectionTrustBanner`: shown only when pending, "Allow selected" sends the fingerprint, store
  error alert.
- `process-env-gate`: warning when withheld and the text has a placeholder, silent otherwise.
- `buildSettingsForSave` no longer matters for capability fields (backend ignores them), and its
  test covers that `agentAutonomyEnabled` is not dropped by a save.

Manual: clone a repository whose `opencollection.yml` sets Developer mode, agent run and a
`{{process.env.HOME}}` header; confirm Safe mode, the banner, the agent refusal and the unresolved
placeholder; allow each and confirm it takes effect; pull a commit that adds a root and confirm only
that root is pending.

## 15. Staged implementation plan (question 8)

Three tasks, one plan file, run in order with subagent-driven development. Each leaves the app
working. Every task starts with: read `docs/superpowers/specs/opencollection-spec-reference.md`.

### Task 1: Trust store, effective resolution, all backend consumers, migration

Files:
- New: `crates/rocket-collection/src/trust.rs`; `crates/rocket-infra/src/fs_trust_store.rs`;
  `crates/rocket-app/src/collection_trust.rs`; `src-tauri/src/commands/collection_trust.rs`
  (only `set_collection_capability` in this task).
- Edit: `crates/rocket-collection/src/lib.rs`, `repository.rs:163`, `settings.rs:23-25`, `:69-76`;
  `crates/rocket-infra/src/lib.rs`, `fs_collection/mod.rs:124`, `fs_collection/settings.rs`,
  `shared_path_collection_repo.rs:168`; `crates/rocket-app/src/lib.rs`,
  `execution_service.rs:1699`, `:1704-1727`, constructor at `:393-480`;
  `mcp_tool_service.rs:12-15`, `:172-207`, `:493-497`, `:528-532`, `:580`;
  `mcp_tool_service/chips.rs:72`; `mcp_read_views.rs:196-209`; `acp_session_service.rs:75`,
  `:199-209`; `collection_service.rs:137-169`, `:452-462`; `test_doubles.rs`;
  `crates/rocket-shared/src/events.rs:322`; `src-tauri/src/lib.rs` (wiring and migration, see
  Section 12), `src-tauri/src/mcp/tool_server.rs:1192-1216`.
- Frontend, minimal so the app stays usable: `src/lib/tauri-api.ts` (`setCollectionCapability`),
  `SandboxPopover.tsx:69-84`, `AgentAutonomyToggle.tsx:59-74` call it instead of
  `saveCollectionSettings`.

Done when: all backend consumers use the effective value; existing collections behave as before
(grandfathered); a new clone runs Safe until the user enables Developer mode in the popover.
Checks: `cargo check -j4`, `cargo test -j4 -p rocket-collection`, `-p rocket-infra`,
`-p rocket-app` (targeted), `yarn tsc --noEmit`, `yarn check`.

### Task 2: Trust status IPC and UI

Files:
- Edit `src-tauri/src/commands/collection_trust.rs` (the remaining commands from Section 12 except
  the process-env change), `src-tauri/src/lib.rs:821` registration, Tauri event mapping for
  `CollectionTrustChanged`.
- Frontend: `src/lib/tauri-api.ts` (DTOs, commands, event), new
  `src/lib/queries/collection-trust-queries.ts`, new `src/components/collections/CollectionTrustSection.tsx`
  and `CollectionTrustBanner.tsx`, edit `CollectionOverviewTab.tsx`, the Scripts tab host in
  `src/components/request/`, `SandboxPopover.tsx` (effective state, requested state, shadcn
  buttons), `AgentAutonomyToggle.tsx` (effective state, requested line),
  `src/components/assistant/AssistantPermissionsPopover.tsx:29-31` copy.
- Tests for the components above.

Done when: a collection whose file requests more than it has shows the banner, the user can review,
allow and revoke each capability, and every surface shows requested vs allowed.
Checks: `cargo check -j4`, targeted `cargo test -j4`, `yarn tsc --noEmit`, `yarn check`,
`yarn test` for the touched components.

### Task 3: Process environment gate, migration notice, docs

Files:
- Backend: `crates/rocket-app/src/execution_service.rs:893-913`;
  `src-tauri/src/commands/environments.rs:171-174` (collection argument, state);
  `src-tauri/src/commands/collection_trust.rs` (notice commands).
- Frontend: `src/lib/tauri-api.ts:1544`; `src/lib/queries/environment-queries.ts:21`, `:54-60`;
  `src/lib/execute-request.ts:69-71`, `:244`, `:590`; new `src/lib/process-env-gate.ts`, called from
  the HTTP, WebSocket, GraphQL subscription and gRPC send handlers; `RequestPanel.tsx:536`,
  `CollectionOverviewTab.tsx:121`, `AuthNodeEditor.tsx:57`, `EnvironmentDialog.tsx:398`; new
  `src/components/layout/TrustMigrationNotice.tsx` mounted once at app start;
  `src/lib/collection-settings-save.ts` comment.
- Docs: `crates/rocket-infra/CLAUDE.md` (store, identity), `.claude/tauri-commands.md` (new
  commands), `.claude/roadmap.md` (ship entry; add follow-ups: scanner, request-guard from git,
  proxy and certificates if ever honoured), mark Section 1 of
  `docs/superpowers/specs/2026-10-07-js-script-security-design.md` as superseded by this spec.

Done when: `{{process.env.*}}` resolves only for allowed collections on every protocol, with a
visible warning otherwise; upgraded users see the one-time notice.
Checks: `cargo check -j4`, `cargo test -j4 -p rocket-app` (targeted), `yarn tsc --noEmit`,
`yarn check`, `yarn test` for `execute-request`, `process-env-gate`, `variable-context`.

## 16. Deviations from the 2026-10-07 draft

| Draft | This design | Reason |
|---|---|---|
| One fingerprint over mode and roots | Approved value per capability | A new root does not silently drop Developer mode; per-root approval. |
| Key: canonical path | Canonical path plus uid | A different repo in a re-used folder fails closed. |
| Mode and roots only | Plus agent run and host environment | User decision and coordinator addendum. |
| Collections created in Rocket start trusted | They get `process_env` only | They request nothing else; a toggle by the user is the confirmation. |
| No migration (everything untrusted) | Grandfather once with a notice | Section 9. |
| Script baselines in the trust record | Left to the scanner plan | Not needed to close the hole. |
| `trust_collection` all-or-nothing | Per-capability grant and revoke | The user confirms each power separately. |

## 17. Open questions for the user

1. Migration: grandfather once with a notice (recommended), or require re-confirmation of
   Developer mode and agent run while still grandfathering host environment access?
2. Should requests outside any collection (scratch tabs) keep full `{{process.env}}` access? This
   design says yes, since no shared file is involved.
