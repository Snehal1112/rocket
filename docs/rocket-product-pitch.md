# Rocket: Product Pitch

**Rocket is an open-source API workspace that runs natively on your machine. Collections are plain files, git is built in, and nothing leaves your computer.**

## The problem

Most API clients are Electron apps. They ship a full copy of Chromium, and many tie your collections, tokens and history to accounts and cloud sync. When collections are not plain files, API changes are hard to diff, review and ship alongside the code that uses them.

## What Rocket is

A desktop app for Linux, macOS and Windows, built with Tauri 2 (Rust backend, React 19 frontend). It uses the operating system's WebView instead of bundling a browser.

Four ideas:

- **Native.** Rust and the OS WebView. No bundled browser engine.
- **File-based.** Collections, folders, requests, environments and contracts are OpenCollection YAML files in `~/.rocket-api/`. You can read, diff and commit them.
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

- **Request workspace.** Multi-tab editor with split panes and auto-save, params, headers, body (Monaco), auth, response viewer, light and dark themes, keyboard-driven navigation.
- **Import.** Postman and Bruno collections, environments and zipped exports.
- **Environments.** `{{variable}}` templates, secret masking, optional external secret-manager integration.
- **Auth.** Basic, Bearer, API Key, OAuth 2.0 (client credentials) and AWS SigV4.
- **Scripting and tests.** Pre-request, post-response and test scripts with TypeScript IntelliSense, plus a visual assertions builder.
- **Collection Runner.** Run a collection or folder sequentially with live progress and a results summary.
- **Load testing.** Phase-based or fixed concurrency/RPS runs, a live dashboard with latency, throughput and error charts, and export to HTML, CSV, JSON and PDF.
- **Contract Lock.** Attach SLAs and API contracts to a collection, folder or request. Track provider, consumer, version and expiry, record drift in a changelog, and export as OpenAPI YAML. Contracts are YAML files, so they travel with git.
- **Security and compliance.** A tamper-evident, hash-chained audit trail with compliance profiles and evidence export.

## What is coming

*In development, confirm release status before announcing.* A workspace AI assistant that talks to ACP agents (Claude, Gemini), the Flow visual workflow builder, and more of the `rok.*` scripting API.

## Call to action

Rocket is MIT licensed. Build it with `yarn install` then `yarn tauri dev`, or `yarn tauri build` for a release build. Source: github.com/snehal1112/rocket

## Suggested one-liners

- Short: "The API client that lives on your machine. Small, fast, git-native, no account."
- Technical: "Postman-style workflows with Bruno-style files, in a 39 MB native app."
- Honest: "Smallest install of the three, faster to start than Bruno, and every number published."

## Gaps to fill before launch

- Real screenshots (request editor, git panel, load-test dashboard, Contract Lock) for the deck's slots on slides 4 and 8.
- A download or releases link for the closing call to action.
- Confirm which roadmap items (AI assistant, Flow) are released before keeping the "coming next" slide.
- Source for every figure above is the README benchmark section.
