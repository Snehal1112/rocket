# Settings

The gear icon (or Ctrl/Cmd+,) opens a single, global **Settings** page. It's
global on purpose — switching workspaces or collections keeps it open — and it
covers only the editor area, so the sidebar, assistant panel and status bar
stay usable while it's open. Five sections:

## Proxy

How Rocket reaches the network for requests and OAuth 2.0 token calls:
**System** (use `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY`), **None** (always
connect directly), or **Custom** (one proxy URL, used for both HTTP and
HTTPS).

![Settings → Proxy](/screenshots/settings-proxy.png)

## Secret Managers

Connections to external secret stores — RocketVault and Azure Key Vault today
— used by both environment External Secrets bindings and AI Agent
credentials. See [Secrets & vault](/guide/secrets-and-vault).

## AI Agents

The agents Rocket can start for the workspace assistant — **Claude Code**,
**Gemini CLI**, or a **Generic ACP agent** for anything else, each launched via
its own command, with its credential pointed at a secret manager connection,
an environment variable, or your OS keychain. See
[AI Assistant](/guide/ai-assistant).

## Support and About

Support links open in your browser; About shows the installed version and the
data folder Rocket is using on this machine.

## A couple of behaviors worth knowing

- Settings isn't remembered across restarts — it always starts closed.
- While an AI Agents save is in progress, Settings locks: it can't be closed
  or switched away from until the save finishes, so a slow save can't be
  interrupted halfway through.
- Clicking a request, the workspace home button, or the Audit Log shortcut in
  the sidebar closes Settings automatically and takes you there.
