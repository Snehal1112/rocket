# AI Assistant

The AI Assistant is a workspace chat panel that connects to an agent over
**ACP** (Agent Client Protocol). You bring your own agent and model; Rocket
isn't locked to one vendor.

![The AI Assistant panel — Ask, Write and Organize, with the agent picker and Start button](/screenshots/ai-assistant-panel.png)

The panel frames what it can do in three lines: **Ask** (questions about your
collections and requests), **Write** (scripts and tests for your requests),
and **Organize** (move and group requests into folders) — and states its one
hard rule up front: **"Every change is a proposal you review before anything
is written."**

## Quickstart: configure an agent and start a session

1. Open **Settings** (the gear icon, or Ctrl/Cmd+,) → **AI Agents**.
2. Press **+ Add Agent**, and fill in:
   - **Label** — the name shown in the agent picker (for example "Claude Code (work)").
   - **Agent type** — **Claude Code**, **Gemini CLI**, or **Generic ACP agent**
     (the route for anything else ACP-compatible, OpenAI-compatible bridges
     included — a label like "Rocket-OpenAI" is a name *you* give a Generic
     agent, not a fourth built-in type).
   - **Command** — the program Rocket starts for this agent, found on the PATH
     or given as a full path (for example `claude-agent-acp`), plus any
     **Arguments** it needs, one per row.
   - **Credential → Source** — **Secret manager** (reads from a connected
     manager), **Environment** (a variable from Rocket's own environment), or
     **Keychain** (your OS keychain) — then pick the **Connection** if you
     chose Secret manager. See [Secrets & vault](/guide/secrets-and-vault) for
     how each of these actually stores the value.

   ![The Add Agent form — type, launch command, and where its credential comes from](/screenshots/add-agent-dialog.png)
3. Press **Save**, then **Test** on the new agent's row to confirm it launches
   and authenticates correctly before you rely on it.

   ![Settings → AI Agents, listing configured agents with their launch command](/screenshots/ai-agents.png)
4. Open the **AI Assistant** panel — its icon sits next to the Settings gear,
   at the top-right of the window.

   ![The AI Assistant panel's toggle icon, next to the Settings gear](/screenshots/open-ai-assistant-panel.png)
5. Pick your agent from the **Agent** dropdown at the bottom of the panel, and
   press **Start**.
6. Choose a mode — **Ask**, **Edit** or **Agent** — before you start typing;
   see below for what each one allows. Type your question or request and
   press Enter.

With no agents configured yet, the Start view offers an "Open AI Agents
settings" shortcut that jumps straight to step 1.

## What the agent can see

The agent reads your workspace through an **MCP** (Model Context Protocol)
tool server that Rocket exposes — your requests, collections, environments,
history, and test results.

## Nothing is written until you approve it

The agent **proposes** changes — new requests or folders, script edits,
renames, moves, environment variables, folder settings — as a reviewable card.
It never writes directly. Three modes control how far it can go, each
strictly more permissive than the last:

1. **Ask** — read-only.
2. **Edit** — can propose changes.
3. **Agent** — can also run requests.

## Multiple agents, one workspace, and where credentials come from

Each configured agent is launched via its own command, and switching between
them in the composer's agent picker changes nothing else — the workspace, the
approval flow, and the masking rules stay identical regardless of which agent
is behind the conversation. Pressing **Start** signs in using the credential
that agent's configuration points at — a connected secret manager, an
environment variable, or the OS keychain — explicitly *not* a stored CLI login
session. See [Secrets & vault](/guide/secrets-and-vault) for how each of those
actually stores the value.

## The composer

- Mentioning **`#`** in the message box adds a chip referencing a request,
  folder, collection, environment, or the last response — up to 8 chips per
  message. The currently open request is added as a focus chip by default.
  Chip content is masked by the backend before it ever reaches the agent.
- **Enter** sends, **Shift+Enter** adds a line, **Esc** stops a running turn.
- A context indicator shows how much of the conversation window is used, with
  token count and cost on hover.

## Conversation history

Every conversation is saved per workspace (as YAML files Rocket keeps out of
git on purpose) up to 50 conversations and 500 messages each, with the same
secret-masking rule applied before anything is written to disk. The panel's
**History** button lists past conversations — opening one from the *current*
workspace with its agent still configured carries it on in place; otherwise it
opens read-only, with a **Continue** option that replays enough of the earlier
conversation for the agent to pick up context.

## The collection trust gate

Before an agent (or a script) can use elevated capabilities, a trust gate
shows exactly what a collection is **requesting** — developer-mode script
access, extra script folders, letting the agent run requests, host
environment-variable access, RocketVault secret or certificate access — against
what you've actually **allowed**, per computer. A collection you imported or
cloned starts with the risky capabilities denied by default, even if the
collection file itself requests them. See
[Security & compliance](/guide/security-and-compliance) for the full detail on
how that gate works.
