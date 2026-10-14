# Console: mask secrets and auth tokens in request rows

Source: GitHub issue #43 (found during the Flow debug-mode investigation). Re-verified against current code: `sendRequest` in `src/lib/execute-request.ts` still passes the resolved URL, headers (including the auth headers it synthesizes) and body to the Console unmasked, and no frontend masking exists.

## Goal

Stop the Console from showing secret values and auth tokens in plain text for single-request sends, while letting the user reveal local secrets on purpose, per row, and never exposing a value that came from a secret manager.

## Decisions

The interview covered every topic. Each line records the question asked and your answer.

- **Purpose and who benefits.** Question: what outcome do you want, and who are you protecting? Answer: secrets are masked by default and the user can reveal them per row (option B of the first question). The person protected is whoever sees the screen or a screenshot.
- **Reveal mechanism.** Question: how should reveal work? Answer: a per-row toggle, with the raw values kept in frontend memory for the session and never written to disk (option A). Values that came from any secret manager are never shown and never stored raw.
- **Security rule for responses.** Question: should the response be masked too? Answer: yes, request and response (option A). The written-back understanding was confirmed.
- **Constraints and success checks.**
  - Question: what counts as done and how is it checked? Answer: unit tests plus a manual check in the real app (option A).
  - Question: short secrets? Answer: keep the 6-character minimum of history and scripts, so a secret shorter than 6 characters stays visible (option A).
  - Question: header masking? Answer: the header name stays visible and only the value is masked.
- **Edge cases and failure behavior.**
  - Question: which extra places carry a credential? Answer: an API key sent as a query parameter, and credentials in the URL (`user:pass@host`).
  - Question: what do copy actions copy? Answer: what is shown, masked unless the row was revealed (option A). The Console has no copy action today, so this is a rule for any future one.
  - Failure behavior, confirmed in the written-back understanding: masking fails closed. When the secret set cannot be built, for example because the environment is not in the query cache, the row still masks the sensitive header values, URL credentials and the API-key parameter, shows a "may contain secrets" note, and offers no reveal for values it could not classify.
- **Security and compatibility.** Question: what must not weaken? Answer: vault values never leave their unresolved template form in the Console, and the rule is pinned by a test. History and the Rust redaction code are unchanged.
- **Scope against related tickets and branches.** Question: how far should the change reach? Answer: only the single-request send path (option A). Flow rows written by `FlowPane.tsx` stay as they are. The branches `secret-provider-foundation` and `worktree-sp-secret-storage-hardening` have no commits ahead of `main` and touch none of these files. #46 (shared Rust redaction) is left alone.
- **Non-goals.** Question: which candidates stay out? Answer: all five, listed under Non-goals.
- **Alternatives that were rejected, and why.** Question: is the list right? Answer: yes. See Rejected alternatives.

## Rejected alternatives

- **Mask in Rust and return a redacted exchange over IPC.** It changes the IPC contract, the frontend builds this Console row itself, and it leaves no way to reveal.
- **Mask when the row is written, keep no raw value, and re-send to reveal.** It is safer, but you chose the per-row reveal.
- **Store raw only and mask when rendering.** A later environment change could unmask an old row, and the fail-closed rule cannot be applied reliably.
- **Guess secrets by field name** (`token`, `password`). It risks false masking.
- **A global "show secrets" setting.** You ruled out a global setting.

## Behavior that must not change

- A Console row for a request with no secrets looks exactly as it does today.
- Flow rows written by `FlowPane.tsx` render raw as today, because they carry no `masked` field.
- History, the script engine's console redaction and `redact_secrets` in Rust are untouched.
- The response panel shows what the server returned.
- The 200-entry cap of the Console store is unchanged.

## Design

Approach: store the raw fields and a masked copy on each entry, fixed at the moment of the send.

1. **Masking helper.** A new pure module `src/lib/console-redaction.ts` exports `redactConsoleEntry(entry, secrets)` and the list of sensitive header names (`Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie`, `X-Api-Key`, matched case-insensitively).
   - `secrets` holds the secret values from the active environment, collection and folder variables, and the global environment, each included only if it is at least 6 characters.
   - Values are replaced longest first with the marker `••••••`, so a secret that contains another leaves nothing behind.
   - The values of the sensitive headers are masked and their names stay.
   - An API-key query parameter and URL credentials (`user:pass@host`) are masked.
   - It returns the masked fields and an `uncertain` flag that is true when the secret set could not be built.
2. **Entry shape.** `HttpConsoleEntry` in `src/stores/console-store.ts` gets two optional fields: `masked` (URL, request headers, request body, response headers, response body) and `uncertain`. `addHttpEntry` accepts them and nothing else changes.
3. **Call site.** In `sendRequest`, before `addHttpEntry`, the code builds the secret set from the variable sources it already used, plus the auth credentials it synthesized, calls `redactConsoleEntry`, and passes the raw fields and `masked`. The error path gets the same wrapper.
4. **Vault values.** The raw fields keep the unresolved `{{provider.name}}` template, because the frontend never resolves vault references. The helper treats such a reference as never revealable. No resolved vault value can enter either copy.
5. **Console panel.** `HttpEntryDetail` in `src/components/layout/ConsolePanel.tsx` shows `entry.masked` unless the row is revealed. The reveal toggle is a shadcn `Button` with a lucide `Eye` and `EyeOff` icon, local to the open row, and it resets when the row collapses or the panel closes. A row with nothing masked shows no toggle. A row with `uncertain` shows a "may contain secrets" note and reveals only the values the helper could classify. `matchesSearch` searches the displayed text, so a search for a secret value cannot find a masked row.

## Tests and measurement

- **Helper unit tests** in `src/lib/__tests__/console-redaction.test.ts`: a secret value is masked in the URL, headers and bodies of the request and the response; a secret shorter than 6 characters stays visible; a longer secret that contains a shorter one masks fully; the values of the five sensitive headers are masked and the names stay, with case-insensitive matching; an API-key query parameter and URL credentials are masked; a `{{provider.name}}` reference is untouched and no resolved vault value appears in either copy; `uncertain` is true when the environment is missing from the cache and the sensitive headers are still masked.
- **Console tests** next to `ConsolePanel.requestName.test.tsx`: the row shows masked text by default; the toggle shows raw text and a second click masks again; search finds a row by its masked text and not by a masked secret; an entry without `masked` renders unchanged.
- **Manual check** in the real app: send a request that uses a secret env var and a Bearer token, confirm the Console row shows `••••••`, that reveal works, and that collapsing the row masks it again.
- **Verification commands:** `yarn tsc --noEmit`, `yarn check` and `yarn test console`.
- **Docs:** a short note in `.claude/` on the Console masking rule.

## Non-goals

- No change to the Rust redaction code, to history or to #46.
- No masking in the response viewer panel, only in the Console.
- No new setting to turn masking off globally. The per-row reveal is the only override.
- No raw secret written to disk. The Console is not persisted and stays that way.
- No guessing of secret-looking values by name. Only known secret values and the fixed header list are masked.
- Other Console producers (Flow, the Runner) are listed as follow-ups, not changed.

## Acceptance criteria

- A secret environment value is masked as `••••••` in the URL, headers and bodies of both the request and the response in the Console row, and a secret shorter than 6 characters stays visible.
- The values of `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie` and `X-Api-Key` are masked and their names stay visible, including the auth headers the app synthesizes for Bearer, Basic and API key.
- An API key sent as a query parameter and credentials in the URL are masked.
- A per-row toggle reveals the local secrets and masks them again, resets when the row collapses, and the raw values are never written to disk.
- A value that comes from a secret manager is never shown and never stored raw, pinned by a test.
- When the secret set cannot be built, the row masks the sensitive headers, URL credentials and the API-key parameter, shows a "may contain secrets" note, and offers no reveal for values it could not classify.
- Search matches the displayed text, so a masked secret cannot be found by searching for it.
- Rows without a `masked` field, such as Flow rows, render exactly as before.
- `yarn tsc --noEmit`, `yarn check` and `yarn test console` pass, and the manual check in the app is done.
