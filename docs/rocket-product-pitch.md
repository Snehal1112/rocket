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

Rocket 2.1.1 against Bruno 4.2.0 and Postman 11.71.7, on the same Linux machine, 2026-10-21:

| | Rocket 2.1.1 | Bruno 4.2.0 | Postman 11.71.7 |
|---|---|---|---|
| Install size on disk | **146 MB** | 527 MB | 653 MB |
| Download size | **48.4 MB** (.deb) | not measured | 174 MB (snap) |
| Idle memory (PSS) | **~262 MB** (3 processes) | ~566 MB (7 processes) | ~516 MB (7 processes) |
| Idle CPU | ~1% | **~0%** | ~7% |
| Startup to window | 0.43 s (blank) | ~3.5 s | **~1.0 s** |
| Startup to UI drawn | ~10.4 s | ~4.2 s | **~2.2 s** |

Rocket uses about half the memory of either Electron app and takes roughly 3.6 to 4.5 times less disk space once installed. Startup is its weak spot: the UI is drawn after about 10.4 s, against about 4.2 s for Bruno and 2.2 s for Postman. Postman was signed in during the run, which likely explains its higher idle CPU. The README documents the method; we show the full table because it makes the memory and size claims more credible.

## What you can do today

**Collections and workspaces**
- File-based OpenCollection YAML collections; multiple workspaces with embedded and external collections.
- Folder settings (headers, auth, variables, docs, scripts) in `folder.yml`, inherited by everything beneath the folder.
- Import from Postman, Bruno, WSDL and OpenAPI (3.0, 3.1, Swagger 2.0; JSON or YAML, from a file or a link including GitHub links). Optionally create a new workspace, and group imported folders by tag or path. Private-repo tokens are kept in the system keychain, never in the collection.
- OpenAPI sync: link a collection to a spec, check it every 5 minutes while the app is open, see how many endpoints are in sync and which you changed, and review spec updates and your own changes in separate tabs. Each link is approved per machine and can be untrusted at any time.
- Collection trust gate: shows which capabilities a collection requests (such as `{{process.env}}`) and which you have allowed.

**Protocols and requests**
- HTTP, GraphQL (schema introspection and subscriptions), gRPC and WebSocket in one collection tree.
- Multi-tab editor with split panes, auto-save, keyboard navigation, and Monaco for JSON, XML and text bodies.
- Request history with a run-source badge (manual, runner, load test, flow, agent) you can filter on.

**Environments and secrets**
- `{{variable}}` environments with secret masking.
- External secret managers: RocketVault and Azure Key Vault.
- Environments can inherit from another environment and load a `.env` file.
- Client certificates per environment (PEM, PKCS12 or RocketVault), matched to hosts by domain with wildcards.

**Authentication and transport**
- Basic, Bearer, API Key, Digest, NTLM and WSSE.
- OAuth 1.0 and OAuth 2.0 (client credentials, password, authorization code with PKCE, implicit).
- AWS SigV4, mTLS client certificates, proxy support, and auth inheritance from parent folders.

**Scripting and testing**
- Pre-request, post-response and test scripts with TypeScript IntelliSense for the `rok.*` API, a snippet sidebar, and shared `.js` files via a local `require()`.
- Scripts run in a sandbox: Safe Mode (default, no filesystem or system access) or Developer Mode (full access) as an explicit opt-in.
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

Rocket is MIT licensed. Build it with `bun install` then `bun run tauri dev`, or `bun run tauri build` for a release build. Source: github.com/snehal1112/rocket

## Suggested one-liners

- Short: "The API client that lives on your machine. Small, fast, git-native, no account."
- Technical: "HTTP, GraphQL, gRPC and WebSocket in a native app under 50 MB to download, with collections you can commit."
- Trust: "An AI assistant that proposes changes and never writes until you accept."
- Honest: "Smallest install and about half the memory of the Electron apps, with every number published, including our slow startup."

## Gaps to fill before launch

- Still missing screenshots or clips: an AI Assistant proposal card being accepted, the load-test dashboard and a clean audit log (the current audit capture shows repeated "undefined" events and a console error, so it was left out).
- The benchmark is for Rocket 2.1.1 and the latest screenshots show v2.1.4, so re-measure if the numbers matter for the launch.
- Captures used in the deck have local paths, tokens and one environment name blurred. Retaking them in a neutral demo workspace is safer for a public launch.
- A download or releases link for the closing call to action.
- Source for the benchmark figures is the README benchmark section; the feature list comes from the team's feature summary.
