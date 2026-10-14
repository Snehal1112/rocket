# Rocket: Product Pitch

**Rocket is an open-source API workspace that runs natively on your machine. Collections are plain files, git is built in, and nothing leaves your computer.**

## The problem

Most API clients are Electron apps. They ship a full copy of Chromium, and many tie your collections, tokens and history to accounts and cloud sync. When collections are not plain files, API changes are hard to diff, review and ship alongside the code that uses them.

## What Rocket is

A desktop app for Linux, macOS and Windows, built with Tauri 2 (Rust backend, React 19 frontend). It uses the operating system's WebView instead of bundling a browser.

Four ideas:

- **Native.** Rust and the OS WebView. No bundled browser engine.
- **File-based.** Collections, folders, requests, environments, contracts and flows are OpenCollection YAML files in `~/.rocket-api/`. You can read, diff and commit them.
- **Git-native.** Staging, commits, branches, merges, push/pull/fetch (force-with-lease), stash, remotes, SSH key discovery and per-file conflict resolution, all through libgit2 with no shelling out to the git CLI.
- **Offline-first.** No cloud, no account, no background process phoning home.

## Proof: measured, not estimated

Rocket 0.9.2 (release build) against Bruno 4.2.0 and Postman 11.71.7, on the same machine:

| | Rocket | Bruno | Postman |
|---|---|---|---|
| Install size | **38.9 MB** | 527 MB | 653 MB |
| Startup to window | 6.38 s | 7.86 s | **0.99 s** |
| Idle memory (PSS) | ~332 MB | **169.6 MB** | 507.7 MB |
| Processes | 3 | 6 | 8 |

Rocket ships roughly 14 to 17 times smaller than either Electron competitor and starts faster than Bruno. Postman starts faster than Rocket, and Bruno idles at lower memory. The README documents the method and its caveats; we show the full table because it makes the size claim more credible.

## What you can do today

**Collections and workspaces**
- File-based OpenCollection YAML collections; multiple workspaces with embedded and external collections.
- Folder settings (headers, auth, variables, docs, scripts) in `folder.yml`, inherited by everything beneath the folder.
- Import from Postman, Bruno and WSDL: collections, environments and zipped exports, format auto-detected, optionally into a new workspace.
- Collection trust gate: shows which capabilities a collection requests (such as `{{process.env}}`) and which you have allowed.

**Protocols and requests**
- HTTP, GraphQL (schema introspection and subscriptions), gRPC and WebSocket in one collection tree.
- Multi-tab editor with split panes, auto-save, keyboard navigation, and Monaco for JSON, XML and text bodies.
- Request history with a run-source badge (manual, runner, load test, flow, agent) you can filter on.

**Environments and secrets**
- `{{variable}}` environments with secret masking.
- External secret managers: RocketVault and Azure Key Vault.

**Authentication and transport**
- Basic, Bearer, API Key, Digest, NTLM and WSSE.
- OAuth 1.0 and OAuth 2.0 (client credentials, password, authorization code with PKCE, implicit).
- AWS SigV4, mTLS client certificates, proxy support, and auth inheritance from parent folders.

**Scripting and testing**
- Pre-request, post-response and test scripts with TypeScript IntelliSense for the `rok.*` API, a snippet sidebar, and shared `.js` files via a local `require()`.
- Visual no-code assertions builder.
- Collection Runner with live progress and a results summary.
- Load testing: phase-based or fixed concurrency/RPS, live dashboard with latency, throughput, error-rate and concurrency charts and percentile stats, export to HTML, CSV, JSON and PDF.

**Flow (visual workflow builder)**
- A canvas for wiring saved requests together, with Input, If, Switch, Transform, Auth, Wait-for-callback and Output nodes.
- Run a whole flow or part of it; per-step traces and saved run history; flows are plain YAML in the collection.

**AI Assistant**
- A workspace chat panel that connects to an agent over ACP (Claude, Gemini or a generic agent).
- The agent reads the workspace through MCP tools and proposes changes (requests, folders, script edits, renames, moves, environment variables, folder settings). Nothing is written until you accept each proposal.
- Conversations are stored per workspace with secrets masked and can be resumed. Agent credentials come from the vault, an environment variable or the OS keychain.

**Contracts and compliance**
- Contract Lock attaches SLA and API contracts to collections or folders, tracks drift in a changelog, previews PDFs natively and exports OpenAPI YAML. Contracts are YAML files, so they travel with git.
- Tamper-evident, hash-chained audit log with compliance profiles and evidence export.

**General**
- Light and dark themes that follow the system; native app for Linux, macOS and Windows; data stays under `~/.rocket-api/`.

## Still in progress

The static script scanner for the trust gate, parts of the `rok.*` Bruno-parity work (some exist only as specs), and some later Flow plans. Confirm release status before announcing any of these.

## Call to action

Rocket is MIT licensed. Build it with `yarn install` then `yarn tauri dev`, or `yarn tauri build` for a release build. Source: github.com/snehal1112/rocket

## Suggested one-liners

- Short: "The API client that lives on your machine. Small, fast, git-native, no account."
- Technical: "HTTP, GraphQL, gRPC and WebSocket in a 39 MB native app, with collections you can commit."
- Trust: "An AI assistant that proposes changes and never writes until you accept."
- Honest: "Smallest install of the three, faster to start than Bruno, and every number published."

## Gaps to fill before launch

- Real screenshots or clips: the git panel (deck slide 14 still has a placeholder), Flow canvas, AI Assistant proposal card, load-test dashboard and Contract Lock.
- A download or releases link for the closing call to action.
- Retake the workspace and collection captures with a neutral demo workspace (the current ones show internal names and a home path).
- Source for the benchmark figures is the README benchmark section; the feature list comes from the team's feature summary.
