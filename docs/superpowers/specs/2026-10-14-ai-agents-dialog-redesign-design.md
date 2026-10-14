# AI Agents dialog redesign

Date: 2026-10-14. Status: approved by the user, ready for plans.

## Goal

Make the AI Agents dialog (`src/components/settings/AgentConfigsDialog.tsx`) faster to fill in and harder to get wrong. The trigger was the "Credential source: RocketVault secret" label sitting above a connection list that holds every secret manager.

Scope: dialog redesign plus provider-aware credentials. Out of scope: in-form Test, optional credentials for Generic agents, any persistence migration.

## Constraints

- Every control comes from `src/components/ui/` and follows the Rocket DSM (`docs/dsm/index.html`). Tokens only, no hard-coded colours.
- Any new component gets a DSM entry in the same plan that builds it.
- Hard rules from `CLAUDE.md` apply: shadcn primitives, lucide icons, narrow Zustand selectors, explicit error mapping in production Rust.
- Persisted shape is unchanged: `CredentialSource { vault, env, keychain }` and the `vault_*` fields stay.

## Findings that drive the design

All from a code read on 2026-10-14. Nothing was run.

1. The label is stale, not the data model. `CredentialSource::Vault` is a generic connection + scope + secret source (`crates/rocket-acp/src/agent_config.rs:32-40`). The provider lives on the connection (`crates/rocket-environment/src/secret_manager.rs:13-75`). Azure already works end to end through the same fields. AWS, HashiCorp and GCP are enum values with no fetcher (`crates/rocket-infra/src/secret_providers.rs:31-60`).
2. The dialog lists all connections unfiltered and shows only the label (`AgentConfigsDialog.tsx:842-846`). Copy and service errors say "RocketVault" (`AgentConfigsDialog.tsx:816-820`, `crates/rocket-app/src/agent_config_service.rs`).
3. Vault Name is required by both the dialog and the Rust validator but ignored by the Azure fetcher (`crates/rocket-infra/src/azurekeyvault/mod.rs:464-513`).
4. Credential Env Var and Environment variables come before Credential source, and their help text points down at it.
5. Reasoning is shown for every kind but only takes effect for Gemini (settings file) and for agents that expose an ACP `effort` option. Model is stored for every kind but used only for Gemini.
6. `working_dir` is stored and passed through the IPC DTO, but a grep of `crates/` and `src-tauri/src` finds no consumer at session start.
7. The Gemini reserved env names are enforced in Rust only. The dialog shows the raw backend error.
8. The dialog deviates from the DSM: custom width, no description or footer, plain number input, `gap-1` radios, unlabelled selects, hand-built error blocks.

## Design

### Layout

One scrolling form in three sections, in a fixed-width dialog (`max-w-lg`) with a scrolling body and a right-aligned footer.

1. Agent: Label, Agent type with a one-line hint per kind, Command, Arguments (one per row, directly under Command). A new agent starts with the Claude preset. Changing the agent type replaces a field only when it is empty or still equals the previous type's preset.
2. Credential: Source picker, source-specific fields, Key name.
3. Advanced (collapsed, shows a one-line summary): Idle timeout, Reasoning (Claude and Gemini only), Extra environment variables. Gemini model stays where it is today.

The approved mockup was published as an artifact in this session.

`working_dir` (finding 6): hide it from the form and preserve any stored value on save. Decided 2026-10-14. If it should be honoured at session start later, that is a separate Rust change.

Credential Key name stays required for every kind.

### Credential section

- Source picker labels: Secret manager, Environment, Keychain. Persisted values unchanged.
- Connection select lists every connection as `label (Provider name)`. Providers without a fetcher are listed disabled with "not available yet". Availability comes from `src/lib/secret-providers.ts`, so no new IPC.
- Scope field uses the provider descriptor's `scopeLabel` and `scopePlaceholder`. Hidden for Azure. For Azure the form saves the connection label as `vault_name` to satisfy the Rust validator.
- Secrets load automatically when connection and scope are set. The Fetch Secrets button is removed, and the Secret select gets a refresh icon button (lucide `RefreshCw`, with a tooltip) for secrets created after the dialog opened. The stale-response generation guard stays. Changing the scope keeps the chosen secret until the new list arrives. A chosen secret missing from the new list shows a warning and is not silently cleared. Errors show in the inline state block with Retry.
- Two names, labelled as a pair. Key name is labelled "Agent reads it as" for every source: the variable the agent receives (`session_env`, `acp_session_service.rs:570-589`). For the Environment source a second field "Rocket reads it from" names the variable in Rocket's own environment (`agent_config_service.rs:163-176`). It is prefilled from "Agent reads it as" until edited, and the help text spells out the mapping: "Rocket reads MY_KEY and gives it to the agent as GEMINI_API_KEY".
- Keychain source: saved-state pill with Replace and Remove text buttons. No icon-only toggle.

### Help text

The current dialog explains every field, and the redesign keeps that.

- Every field has always-visible help of one to three short sentences: what it is, an example, and what happens if it is wrong. Existing wording is reused where it is already good (the Gemini and Generic kind notes, idle timeout, reasoning, the keychain notes, the exported-in-the-shell note).
- The Form field component takes a required `help` prop, so a field cannot ship without it. DSM rule: write the NAME of a variable in capitals when it is a name and not a value.
- Notes that apply only sometimes appear only then: the `*_BASE_URL` redirect warning when an extra variable name ends in `_URL`, the exported-in-the-shell note only for the Environment source, the short-lived-token note only under Key name.
- Help is composed from live state where that avoids confusion, for example the sentence that shows the mapping between the two environment names.

### Validation

- Validators move out of the component into a pure module and keep their current rules (env names, secret-looking heuristic, duplicates, idle 30-1800, model pattern).
- Add the Gemini reserved names (`GEMINI_CLI_HOME`, `GEMINI_CLI_TRUST_WORKSPACE`, `GEMINI_CLI_IDE_WORKSPACE_PATH`) to the dialog check.
- Errors show inline per field through the new Form field component. Save is disabled until the form is valid. The aggregate toast goes away.
- Fix the misleading message that says to put the secret "in the RocketVault secret below" whatever the source.

### Rust changes (wording only)

Provider-neutral error text in `agent_config_service.rs` and `secret_manager_service.rs` ("credential no longer exists in the secret manager", "any configured secret manager connection"). Update the tests that assert the old strings. No type, enum or serde change.

Not in scope: a save-time check that a connection's provider has a fetcher. The picker already prevents choosing an unavailable one.

### New DSM components

Built in `src/components/ui/`, documented in `docs/dsm/index.html`:

1. Form field (label, control, helper, error, `aria-invalid`, `aria-describedby`).
2. Source picker (segmented radio group).
3. Collapsible group with a summary row.
4. Inline state block (loading, empty, error).
5. Secret field with saved state (extends `saved-pill`).
6. Key and value rows, reused for arguments.
7. Dialog body and footer pattern in `ui/dialog.tsx`.

### File structure

Split the 1034-line component:

- `AgentConfigsDialog.tsx`: shell and list view.
- `AgentConfigForm.tsx`: sections and state.
- `AgentCredentialSection.tsx`: source picker and per-source fields.
- `AgentAdvancedSection.tsx`.
- `use-secret-listing.ts`: connection and scope to secrets, with the generation guard.
- `agent-config-validation.ts`: pure validators and per-kind presets.

## Testing

- Vitest: validation module, credential section (provider gating, Azure hides scope, auto-load, stale response ignored, missing secret warning), form (Save disabled, inline errors, per-kind fields). Update `AgentConfigsDialog.test.tsx`.
- `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`.
- `cargo check -j4` and focused `cargo test -j4 -p rocket-app` for the reworded errors.
- Manual check in the real app against an Azure connection and a RocketVault connection.

## Plan split

At most three tasks per plan, run one at a time:

1. DSM components and page entries (7 components, grouped into three tasks).
2. Credential section and secret listing hook.
3. Form restructure, validation and Advanced section.
4. Rust wording and test updates.

Plans 1 and 4 are independent. Plan 2 depends on 1. Plan 3 depends on 1 and 2.

## Open items

- `working_dir`: decided, hide and preserve. Wiring it into session start is a possible follow-up.
- AWS, HashiCorp and GCP stay disabled until their fetchers exist.
- Optional credentials for Generic agents: follow-up, not this project.
