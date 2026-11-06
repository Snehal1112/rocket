# Team network

Rocket nodes on the same network find each other, pair once with a matching 6-digit code and stay paired until one side removes the pairing. This is the base for sending collections and checking contracts between nodes (later projects). Spec: `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`. Plans: `docs/superpowers/plans/team-network/`. Roadmap: R-83, project CA-5.

## What the user sees

- A radar icon in the status bar with the number of online paired nodes. It opens the Network panel.
- The Network panel (a popover): search box (name, initials or short id), open requests at the top, Paired (online first, offline greyed, menu with Remove) and Nearby (Pair button), and Add by address at the bottom.
- The pairing dialog: both screens show the same code, for example `482 913`. The node that started the pairing sees Accept and Cancel. The other node sees Accept, Decline and Block and also gets a toast.
- A one-time card after the first launch: "You're visible on this network as '<name>'." with Edit name, Hide my node and OK.
- Settings, Team network: Hide my node, display name, port in use, device id (short), blocked devices with Unblock, saved addresses with Remove, warnings.

## Where the code lives

| Layer | Place |
|---|---|
| Domain (ids, pairing state machine, presence, dialer, wire types, events, ports) | `crates/rocket-network` |
| Identity, YAML repos, TLS, peer server and client, mDNS | `crates/rocket-infra/src/network/` |
| `NetworkService` (state, pairing, presence, audit) | `crates/rocket-app/src/network/` (a folder) |
| Tauri commands and DTOs | `src-tauri/src/commands/network.rs` (commands are in `src-tauri/app-commands.txt`) |
| Wrappers, types and listeners | `src/lib/tauri-api.ts`, section "Team network" |
| State | `src/stores/network-store.ts` |
| Events | `src/lib/network-event-bridge.ts`, `src/hooks/useIncomingPairingNotice.tsx` |
| UI | `src/components/network/`, `src/components/status-bar/NetworkStatusItem.tsx`, `src/components/settings/sections/NetworkSection.tsx` |
| Shared pieces | `src/lib/error-text.ts`, `src/lib/listen-scope.ts`, `src/hooks/useAsyncAction.ts`, `src/components/ui/search-input.tsx`, `src/components/ui/initials-avatar.tsx`, `src/components/status-bar/StatusBarItem.tsx`, `StatusBarCount.tsx` |
| Data | `~/.rocket-api/network/` (`paired.yml`, `blocked.yml`, `addresses.yml`, `settings.yml`). Machine-local. The device key is in the OS keychain, service `com.rocketapi.device-identity`. |

## Frontend rules

- The backend owns the state. The store mirrors it. `nodesChanged` and `statusChanged` events trigger a refetch of `network_list_nodes` and `network_status`. A sequence counter drops a stale response.
- `pairingIncoming` and `pairingFinished` feed `incoming` and `activePairing` directly. Event fields are snake_case.
- `incoming` is deduped by pairing id and each entry expires 120 seconds after it arrived. A repeated event neither adds a row nor extends the expiry.
- Only the node that started a pairing gets the "<name> declined." toast.
- The Add by address, Remove and Pairing dialogs are mounted outside the Popover. Closing the panel never closes them.
- Closing the pairing dialog of a pairing you started, before you pressed Accept, cancels it.
- The UI shows the short device id only. Nonces, keys, certificates and the full device id are never shown or logged.
- Mutations are store actions (`removePaired`, `addAddress`, `removeAddress`, `unblock`, `setHidden`, `saveName`, `markIntroSeen`). They call the IPC wrapper, reload the nodes or the status, and reject with the backend error. Components do not call `refresh()` after a call.
- `useAsyncAction` wraps one call with a busy flag and a toast (default) or inline error text. `errorText` and `toastError` clean the backend error text. `createListenerScope` handles the Tauri listener lifecycle.

## Decisions

- Visible to everyone on the network by default, with a Hide my node switch (spec section 2). Hiding stops the announcement and refuses new requests. Paired nodes still connect.
- Pairing lasts until removed. A new key is a new device.
- The deviations from the spec are listed in `docs/superpowers/plans/team-network/00-plan-index.md`: quarantined files instead of `.bak`, coarser events (`nodesChanged`), a settings file, the dial grace period, hyper's low-level client, the pairing request first calls `/hello`.
- Error and empty-state copy comes from spec section 9 and is pinned by tests.

## Limits

- 200 paired devices, 50 saved addresses, 256 connections (32 unpaired), 5 pairing requests a minute.
- A pairing request expires after 2 minutes. A declined device waits 1 minute.
- Same-network discovery only, plus Add by address. No relay, no internet pairing.
- No sending of collections or contracts yet.

## How to test

```bash
bunx vitest run src/lib/__tests__/tauri-api.network.test.ts src/stores/__tests__/network-store.test.ts \
  src/lib/__tests__/network-event-bridge.test.ts src/lib/__tests__/error-text.test.ts \
  src/lib/__tests__/listen-scope.test.ts src/components/network src/components/status-bar \
  src/components/ui src/hooks/__tests__/useIncomingPairingNotice.test.tsx \
  src/hooks/__tests__/useAsyncAction.test.tsx src/components/settings
bun run tsc --noEmit
bun run check
```

Backend tests: `cargo test -j4 -p rocket-network`, `cargo test -j4 -p rocket-infra network`, `cargo test -j4 -p rocket-app network`. Real network checks: `docs/manual-tests/team-network.md`.
