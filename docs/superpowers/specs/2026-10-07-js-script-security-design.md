# JS Script Security Design

Date: 2026-10-07
Status: draft for review
> **Superseded.** Section 1 (trust model) is replaced by `docs/superpowers/specs/2026-10-11-collection-trust-gate-design.md`. Section 2 (scanner) is unchanged and still pending.

Builds on: `docs/superpowers/specs/2026-10-06-js-script-files-design.md` (script files and local `require`).

## Goal

Make script files, and scripts in general, safe to run from a collection you did not write. Two
parts:

1. **Trust gate.** A collection cannot grant itself extra script power. Until you trust it on this
   machine, its scripts run in Safe mode with no extra roots, whatever its `opencollection.yml` says.
2. **Static scanner.** Rocket scans every script (`.js` files and the inline pre-request,
   post-response and tests scripts) and shows what each one could do, so you can judge it before you
   trust the collection or run a request.

## Why

The security review of the script-files branch found that `extensions.rocketapi.sandboxMode` and
`extensions.rocketapi.scripts.additionalContextRoots` are read straight from the collection's own
`opencollection.yml`. A cloned repository can therefore set `sandboxMode: developer`, which registers
the Developer-only ops (unrestricted file access and process execution), and the first request you
send runs attacker-chosen code as you. The `sandboxMode` part already exists on `main`; script files
add a second privilege setting (`additionalContextRoots`) and a new place to hide code (`.js` files).

## Decisions (made with the user)

| Question | Decision |
|---|---|
| What does "sanitize" mean | Trust gate plus static scan. No rewriting or stripping of code. |
| Untrusted collection behaviour | Scripts still run, forced into Safe mode with no extra roots. A banner offers "Trust this collection". |
| What the scanner does on a finding | Warn only. It never blocks execution. |
| Trust record key | Collection path plus a fingerprint of the privilege settings only (approach D). Script edits do not re-prompt, but the scanner marks findings in scripts that changed since you trusted. |

## Threat model and non-goals

- **In scope:** a collection from someone else (clone, import, shared folder) that tries to run code
  with more power than the user intended: self-granted Developer mode, extra roots, or hidden code in
  script files.
- **The runtime sandbox is the security boundary.** Safe mode never registers the dangerous ops, so
  the scanner is a visibility aid, not a gate. A determined author can hide a call from any static
  scanner (for example `globalThis['pro' + 'cess']`); that is why a finding never blocks and why Safe
  mode is the fallback.
- **Out of scope:** an attacker who already runs code as the user or can edit `~/.rocket-api`;
  rewriting scripts; blocking on findings; trust for the `.yml` form of script files; a global "trust
  everything" switch.
- A trusted Developer-mode collection can still run any script it contains with full power. Trust is
  a decision about the collection's authors, not about each script.

## Section 1: Trust model

- **Trust record.** Per-user local state in `~/.rocket-api/trust.yml`, never inside a collection, so
  a clone cannot grant itself trust. It maps the canonical collection path to:
  - a fingerprint of the privilege settings, and
  - a baseline hash for each script origin at the time of trust (see below).
- **Fingerprint.** SHA-256 over a canonical text form of `sandboxMode` and the sorted, de-duplicated
  `additionalContextRoots`. Script content is not part of it.
- **Effective mode.** Computed in `ExecutionService::begin_phases`, where the mode is read today:
  - the yml says Safe: effective mode is Safe; no trust is needed;
  - the yml says Developer and the stored fingerprint matches: Developer, with its extra roots, as
    today;
  - the yml says Developer and there is no record or the fingerprint differs: effective mode is Safe
    with no extra roots.
  `additionalContextRoots` are already ignored in Safe mode, so they never apply to an untrusted
  collection.
- **Script changes after you trust.** They never change the mode. The scanner uses the baseline hashes
  to mark findings in scripts whose content differs from the trusted baseline as "new since you
  trusted this collection". That gives the warning more weight without a re-prompt.
- **Edits made in Rocket.** Saving a script through Rocket (the script tab or the inline editors) in
  a trusted collection refreshes that script's baseline hash, so your own edits are not reported as
  new. Changes made outside Rocket (a `git pull`, an external editor) are reported.
- **Who is trusted automatically.** Collections created in Rocket by the user start trusted. Cloned,
  imported and linked collections never do. (Open item 2.)
- **UI.** An untrusted collection whose yml asks for Developer mode (or extra roots) shows a banner
  on the collection overview and in the script editor: it runs in Safe mode, why, and a "Trust this
  collection" button. Trusting records the current fingerprint and the baselines.
- **IPC.** `get_collection_trust(collection)` returns `trusted`, `untrusted` or `not_needed`.
  `trust_collection(collection)` records the fingerprint and baselines. Both are thin commands over a
  `rocket-app` service. The trust store is a trait in a domain crate with a filesystem implementation
  in `rocket-infra` (`trust.yml`).

## Section 2: Static scanner

- **Where it lives.** A pure function `scan_script(source: &str) -> Vec<Finding>` in `rocket-scripting`
  (which does no I/O). `rocket-infra` reads files and calls it; `rocket-app` orchestrates. A small
  tokenizer skips comments and the contents of string literals, so a word inside a comment or string
  is not flagged. No heavy parser dependency. Each scan is capped at 2 MB of source.
- **Rules.**
  - High: anything that reaches the Developer-only ops (file access, process and shell execution; the
    list comes from the engine's op registration), `child_process`, and any use of `__ops`, `Deno` or
    `__bootstrap`.
  - Medium: `eval`, `new Function`, dynamic `import()`, `require` with a non-literal argument,
    computed access on `globalThis` or `this`, and `fetch`, `XMLHttpRequest` or `WebSocket` used
    instead of Rocket's own request API.
  - Info: `require('./...')` of a local file, shown so a script's dependency chain is visible.
- **Finding.** Rule name, severity, origin (a `.js` file path, or a request or folder script with its
  phase), line and column, and a snippet of at most 80 characters.
- **When it runs.**
  - Live: `scan_script_source(source)` while typing in the script tab, with a short debounce. Findings
    appear as editor markers and as a count next to the Save button.
  - Collection scan: `scan_collection_scripts(collection)` walks every `.js` file plus the inline
    scripts of every request and folder, and returns findings for the collection overview, beside the
    trust banner.
  - "New since you trusted": findings in scripts whose hash differs from the trusted baseline carry a
    marker.
- **Safe versus Developer.** In Safe mode a high finding is informational, because the op is not
  registered at run time. In Developer mode it is shown more prominently, because that is where it
  would actually run.
- **DTOs.** `Finding` DTOs use camelCase on the IPC boundary only. The scanner reads source text and
  never writes.

## Section 3: Errors, edge cases, testing

Errors and edge cases:

- Missing or corrupt `trust.yml` is treated as "no collection trusted"; the effective mode is Safe.
  It fails closed and never blocks execution.
- If `trust.yml` cannot be written, "Trust this collection" shows an error and the collection stays in
  Safe mode.
- Moving or renaming a collection changes its canonical path, so it is untrusted again. This is
  intended.
- A `.js` file over 2 MB, or one that is not valid UTF-8, is skipped by the scanner and listed as
  "not scanned".
- The scanner must never panic or loop on unterminated strings, template literals, regex-like text or
  garbage input. A source that cannot be tokenised yields no findings plus a "could not scan" note.
- Snippets are cut at 80 characters, are never logged, and script sources are never sent anywhere.
- No panicking unwraps on production paths; failures map to stable IPC error messages.

Testing:

- Trust: a yml-declared Developer mode with no record gives an effective mode of Safe with no extra
  roots; a matching fingerprint gives Developer; changing `sandboxMode` or `additionalContextRoots`
  invalidates trust; a collection cannot trust itself (nothing inside it is read as trust); a corrupt
  trust file fails closed; a plain send and the collection runner get the same effective mode.
- Scanner: every rule has a positive and a negative case, including words inside comments and strings,
  template literals, nested braces and Unicode; a fuzz-style test feeds truncated and garbage input
  and expects no panic; line and column positions are checked.
- UI: the banner appears only for an untrusted collection that asks for Developer mode; trusting
  clears it; editor markers appear; a "new since trusted" finding is marked.
- Regression: script behaviour in Safe mode, and in an already-trusted Developer-mode collection, is
  unchanged.

## Scope and plan split

Two independent pieces, each its own plan of at most three tasks:

1. **Trust model.** Fingerprint and `trust.yml` store; the effective-mode change in `begin_phases`;
   the two IPC commands; the banner and button.
2. **Scanner.** The pure scanner in `rocket-scripting`; the two scan commands; the editor markers,
   collection summary and "new since trusted" marking.

Plan 1 ships first because it closes the real hole.

## Open items

1. **Trust file location.** `~/.rocket-api/trust.yml` is proposed as a new file in the existing data
   directory. Say so if it should live inside an existing settings file instead.
2. **Collections you create.** Proposed: start trusted, because the user created them. Cloned,
   imported and linked collections never start trusted.
3. **Baseline scope.** The baseline covers every `.js` file and every inline script in the collection
   at trust time. Whether it also covers scripts in the collection's environments (if any exist) is to
   be settled in the plan after reading the code.

## Addendum (2026-10-08): network access from Safe mode

rok parity B adds `rok.sendRequest` and `rok.runRequest`, and both work in Safe mode, like Bruno (user ruling). This widens the threat model above. Safe mode is where untrusted collections run, so a script from a cloned or imported collection can now send environment values, RocketVault values and response data to any host it names. The trust gate does not stop this.

- Script requests reuse the calling request's TLS, proxy and client-certificate settings. A client certificate is only presented when its domain matches the host the script picked, as for any send.
- Console lines and rejection messages mask secret values. The requests themselves carry the real values.
- Mitigations to consider later: show script-originated requests in History with a badge, and have the static scanner flag `rok.sendRequest`, `rok.runRequest` and, with part C, `rok.cookies.jar`.
