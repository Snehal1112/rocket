# Collection trust gate

Spec: `docs/superpowers/specs/2026-10-11-collection-trust-gate-design.md`.

A collection file only requests elevated access. `~/.rocket-api/trust.yml` holds what the user allowed on this computer. What runs is the effective value.

## Capabilities

| Capability | Requested by | Gate |
|---|---|---|
| Developer mode | `sandboxMode: developer` | Script ops and `rok.getProcessEnv` |
| Extra script folders | `additionalContextRoots` | `require()` roots, per approved root |
| Agent request runs | `agentAutonomyEnabled: true` | Assistant `run_request`, ACP MCP servers |
| Host environment | none (implicit) | `{{process.env.NAME}}` on every protocol |

## Rules

- Never read `.sandbox_mode`, `.script_context_roots` or `.agent_autonomy_enabled` outside `collection_trust.rs` (a guard test enforces it). Use `effective_capabilities`.
- `save_collection_settings` never grants and keeps the three file fields as they are. Only `CollectionTrustService` writes capabilities.
- The banner approves with the fingerprint the user saw (`grant_requested_capabilities`). Freeze it when the review opens.
- Services built without a trust store deny everything (`DenyAllTrustStore`). Tests that need access use `InMemoryTrustStore::allow_all()`.

## Host environment (`{{process.env.*}}`)

- Backend (WebSocket, GraphQL subscriptions, gRPC): `build_variable_context_with_process_env` fills `process.env.*` only when `RequestExecutionService::process_env_allowed(collection)`.
- HTTP: `get_process_env_vars(collection?)` returns an empty map when the collection is not allowed. `useProcessEnvVars(collection)` and `execute-request.ts` pass the collection.
- A request with no collection (scratch) keeps full access.
- Before a send, `warnIfProcessEnvWithheld` (`src/lib/process-env-gate.ts`) adds a console line and a toast when the text uses a placeholder and access is withheld. The send is never blocked.
- Who has it: collections that existed at the upgrade (grandfathered), collections created in Rocket. Clones and imports have none until allowed.

## Migration

First start after the upgrade grandfathers every existing collection once. `TrustMigrationNotice` lists the ones that kept Developer mode, extra folders or agent runs until the user clicks OK.

## Commands

`get_collection_trust`, `set_collection_capability`, `set_collection_context_roots`, `grant_requested_capabilities`, `revoke_collection_trust`, `get_trust_migration_notice`, `dismiss_trust_migration_notice`, `get_process_env_vars`. The event is `collection-trust-changed`.

## Follow-ups

- Per-row revoke of one extra script folder (today only "Forget permissions").
- A revoke that clears the grant without writing the collection file.
- Host environment count in the migration notice.
- Static script scanner and request guard from git (separate specs).
- If `config.proxy` or `config.clientCertificates` in `opencollection.yml` are ever honoured, they must join this gate.
