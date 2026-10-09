# Manual checks — Plan 02 (isolation and lifecycle)

Run these in `yarn tauri dev` with an agent config that uses
`claude-agent-acp` 0.88.0 and an API-key credential (`ANTHROPIC_API_KEY` or
`CLAUDE_CODE_OAUTH_TOKEN` as the credential env var). The isolated session has
an empty `CLAUDE_CONFIG_DIR`, so a login stored only in `~/.claude` is not
available to it.

## 1. Token comparison for a first "hello" turn

The usage numbers come from the `agent-session-usage` event (Plan 01's
`AcpUsage`): `used` is the context tokens after the turn, `size` the context
window, `cost_usd` the turn cost.

1. Open the webview devtools (right click, Inspect) and paste:

   ```js
   const T = window.__TAURI_INTERNALS__;
   T.invoke('plugin:event|listen', {
     event: 'agent-session-usage',
     target: { kind: 'Any' },
     handler: T.transformCallback((e) => console.log('usage', e.payload)),
   });
   ```

   If that call errors, read the numbers from the panel's usage indicator
   once Plan 06 lands, and record them here later.
2. **Before:** check out the Plan 02 Task 1 commit (`feat: add agent isolation
   options and session cleanup port`), which does not isolate yet. Start AI
   Assist on a request, send `hello`, wait for the answer and note `used` and
   `cost_usd`. Do this twice with a fresh session each time.
3. **After:** check out the Plan 02 Task 2 commit (or later) and repeat step 2.
4. Expected: `used` drops by a large factor (the user's CLAUDE.md, plugins,
   skills, hooks and built-in tool definitions are gone). Record both pairs of
   numbers in this file.

| Run | Before `used` | Before cost | After `used` | After cost |
|---|---|---|---|---|
| 1 | | | | |
| 2 | | | | |

## 2. `settingSources: []` drops plugins, skills, hooks and CLAUDE.md

1. Make sure `~/.claude` has something recognisable: a line in
   `~/.claude/CLAUDE.md` such as `Always sign answers with ZEBRA-42.`, at
   least one plugin or skill, and a `SessionStart` hook if you use hooks.
2. In an isolated session (after Task 2), on a collection with the agent
   access switch **on**, send:
   `List every tool you can call, then every skill, slash command, plugin and
   CLAUDE.md instruction you were given. Names only.`
3. Expected:
   - Tools: only `mcp__rocket__...` names. No `Bash`, `Read`, `Write`,
     `Edit`, `WebFetch`, `Task` or `TodoWrite`.
   - No skills, no plugin names, no hook output, and no `ZEBRA-42` signature.
   - With the switch **off**, the agent reports no tools at all.
4. Ask it to list the Rocket requests in the collection. The Rocket tool must
   run without a permission prompt or a hang (`allowedTools` allows it).
5. While the session is open, run `ls -la "${TMPDIR:-/tmp}/rocket-agent-sessions"/*/`.
   Expected: one `<uuid>/` with `cwd/` (empty) and `config/` (Claude Code may
   write `.claude.json` or `projects/` there). `~/.claude/projects/` gets no
   new entry for this session.
6. End the session (End session, or close the tab). The `<uuid>/` directory
   must be gone. Repeat with an idle timeout (stop the network for 120 s
   mid-turn) and with app exit: the directory must be gone each time.

## 3. Fallback when the options are not honoured

- **Plugins, skills or `ZEBRA-42` still appear:** check `CLAUDE_CONFIG_DIR`
  first. In step 2.5, `config/` must be the directory the agent writes to. If
  `~/.claude/projects/` gets a new entry instead, the env var did not reach
  the agent: check `crates/rocket-infra/src/acp_agent_client.rs` passes `env`
  through `.envs(...)` and that the agent command is not a wrapper script that
  resets its environment. With an empty config dir and an empty cwd, user and
  project settings cannot load even if `settingSources` were ignored; only
  managed (enterprise) policy can still apply, and that is intended.
- **Built-in tools still appear:** `tools: []` was not honoured. Record the
  adapter version, then decide with the plan owner whether to add
  `disallowedTools` for the built-ins in `agent_isolation.rs`.
- **`session/new` fails with the `_meta` options:** the adapter rejected an
  option. Record the error, then fall back to the spec's plan B: keep the
  empty `CLAUDE_CONFIG_DIR` and send `_meta.systemPrompt` as one string (this
  replaces the whole `claude_code` preset, so test that tool use still works).
  Changing `isolation_meta` needs an index update, because its shape is locked.

## 4. Stale-session sweep command

Not wired to the UI until Plan 05. In devtools, with one AI Assist session
open:

```js
window.__TAURI_INTERNALS__.invoke('end_stale_assistant_sessions').then(console.log);
```

Expected: logs `1`, the scratch directory disappears, and a new AI Assist
session still starts afterwards.
