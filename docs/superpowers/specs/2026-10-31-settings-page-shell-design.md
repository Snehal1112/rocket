# Settings page (Bruno parity), sub-project 1: Shell

Date: 2026-10-31
Status: design approved in chat, spec awaiting review
Roadmap: R-72 (this spec), R-73 to R-77 (later sub-projects)

## 1. Goal

Rocket gets one Settings page with the same sections as Bruno's Preferences. The settings are
global: they apply to every workspace and collection, and the page looks the same wherever it is
opened.

Full parity is too large for one spec, so the work is split into six sub-projects (section 3).
This spec designs the first one, the Shell: the page itself, its section list, the three existing
settings dialogs moved in, and the Support and About sections.

### What the user asked for

- A settings page "just like Bruno", with full Bruno parity as the end goal.
- The page opens over the editor area, not as a modal.
- Simple settings apply at once; forms that can be invalid keep a Save button.
- One gear icon in the title bar. The Proxy (globe) and AI Agents (bot) icons go away.
- Sections that are not built yet stay hidden until their sub-project ships.
- Support links: the GitHub repo and its issues.
- The page is global: it is not tied to a collection's or workspace's tab set.
- Secret Managers and AI Agents are covered in full, with no regression.

### Assumptions

- Rocket keeps two sections Bruno does not have: Secret Managers and About.
- Rocket's AI section holds the existing AI Agents (ACP) configuration. Bruno's AI section
  (providers, autocomplete, redaction) is a different product and is not copied.
- The per-agent model and effort picks of the assistant composer stay in the assistant panel. They
  are stored per agent in `localStorage` and are not moved.

## 2. Bruno's Preferences (reference)

Read from `usebruno/bruno`, `packages/bruno-app/src/components/Preferences/` on 2026-10-31. All ten
sections are always shown, in this order:

| # | Bruno section | Content |
|---|---|---|
| 1 | General | SSL/TLS verification; custom CA certificate (file) and "keep default CA certificates"; store cookies; send cookies; OAuth2 in the system browser; auto save on/off and delay (ms, at least 500); request timeout (ms, 0 or more); default collection location. Auto-saved, debounced 500 ms. |
| 2 | Themes | Mode light, dark or system; a light theme variant and a dark theme variant, picked from preview cards. |
| 3 | Display | Code editor font (free text, default `default`); font size (1 to 32, default 13); interface zoom (50% to 150% in steps of 10, default 100%). |
| 4 | Proxy | Proxy settings. |
| 5 | Client Certificates | Global list per domain: Cert and Key files, or a PFX file; optional passphrase; enable switch per row; remove. |
| 6 | Keybindings | Editable table (command, keybinding). Record up to 3 keys; needs a modifier (F1 to F12 exempt); one non-modifier key; no OS-reserved combo; no duplicates. Reset one row or all. Locked rows. Master on/off. |
| 7 | AI | Providers and models, OpenAI-compatible endpoints, autocomplete, redaction rules. |
| 8 | Cache | File cache (size, clear, on/off) and SSL session cache (on/off, clear). |
| 9 | Support | Docs, issues, GitHub, Discord, X, LinkedIn. |
| 10 | Beta | List of experimental features; one toggle (mock server). |

## 3. Parity map and sub-projects

| Bruno section | Rocket today | Sub-project |
|---|---|---|
| General | Per-request SSL verify and timeout only (default 30 s in `src/lib/execute-request.ts`). Auto-save always on. OAuth2 uses an in-app webview (`src-tauri/src/commands/oauth2.rs`). Cookies always on. | 3, General (R-74) |
| Themes | Light/dark toggle in the status bar, `localStorage` key `rocket-theme` (`src/hooks/useTheme.ts`). | 2, Themes and Display (R-73) |
| Display | Nothing. | 2, Themes and Display (R-73) |
| Proxy | `ProxySettingsDialog`, `~/.rocket-api/proxy.yml`. | 1, Shell (this spec) |
| Client Certificates | Per environment only (`crates/rocket-app/src/client_certificates.rs`, RocketVault included). | 4, Global client certificates (R-75) |
| Keybindings | Hardcoded in `src/hooks/useKeyboardShortcuts.ts`. Needs a shortcut registry first. | 5, Keybindings (R-76) |
| AI | `AgentConfigsDialog`, `~/.rocket-api/agent_configs.yml`. | 1, Shell (this spec) |
| Cache | Nothing. Candidate content: cookie jar view and clear, TLS session reuse. | 6, Cache and Beta (R-77) |
| Support | Nothing. | 1, Shell (this spec) |
| Beta | Nothing. Only useful once Rocket gates features behind it. | 6, Cache and Beta (R-77) |
| (Rocket only) Secret Managers | `SecretManagerConnectionsDialog`, `~/.rocket-api/secret_managers.yml`. | 1, Shell (this spec) |
| (Rocket only) About | Version shown in the status bar only. | 1, Shell (this spec) |

Each later sub-project gets its own brainstorm, spec, plan and build, and adds its section row at
its Bruno position.

## 4. Shell design

### 4.1 Placement: a global full-page view

The Settings page is not a pane tab. Tabs are kept per collection and opening a non-workspace tab
leaves workspace mode (`src/stores/pane-store.ts`, `openTab`), so a tab would not be global.

- `src/App.tsx`: `<main>` becomes `relative`. When Settings is open, `SettingsPage` renders on top
  of it (`absolute inset-0`, own `bg-background`, above the pane area).
- The pane tree stays mounted underneath. Request drafts, WebSocket and GraphQL sessions and Monaco
  editors are not unmounted, so they come back exactly as they were when Settings closes.
- The title bar, sidebar, assistant panel and status bar stay visible and usable.
- No pane-store tab rule changes. The Settings page is not saved in UI state, so it starts closed
  after a restart.

### 4.2 State: `useSettingsStore`

New file `src/stores/settings-store.ts`:

```ts
export type SettingsSection = 'proxy' | 'secret-managers' | 'ai' | 'support' | 'about';

interface SettingsStore {
  open: boolean;
  section: SettingsSection;
  /** True while a section must not be left (an AI Agent save in progress). */
  locked: boolean;
  openSettings: (section?: SettingsSection) => void;
  closeSettings: () => void;
  toggleSettings: () => void;
  setSection: (section: SettingsSection) => void;
  setLocked: (locked: boolean) => void;
}
```

- `openSettings(section)` opens the page and selects `section`, or keeps the current section when
  none is given. The first open shows the first section in the list.
- `closeSettings`, `toggleSettings` and `setSection` do nothing while `locked` is true.
- Components read it with narrow selectors (no full destructuring).

### 4.3 Opening and closing

| Trigger | Effect |
|---|---|
| Title-bar gear (`Settings` icon) | `toggleSettings()` |
| Ctrl/Cmd+, | `toggleSettings()` |
| Close button in the page header | `closeSettings()` |
| Esc | `closeSettings()`, only when focus is not in an input, textarea or editor and no inner dialog or popover is open |
| Any `openTab` call (for example a sidebar click) | `closeSettings()`, so the user lands on what they clicked |
| Assistant "no agents" button | `openSettings('ai')` |
| Workspace or collection switch | No effect. Settings stays open. |

`openTab` in `pane-store.ts` calls `useSettingsStore.getState().closeSettings()` first. If Settings
is locked, the tab still opens underneath and Settings stays on top until the save ends; the save
then unlocks and the user closes Settings normally.

### 4.4 Shortcuts while Settings is open

In `src/hooks/useKeyboardShortcuts.ts`, when `useSettingsStore.getState().open` is true:

- Ctrl/Cmd+Enter and Ctrl/Cmd+S do nothing, so they never act on a hidden tab.
- Ctrl/Cmd+W calls `closeSettings()` instead of closing a hidden tab.
- Ctrl/Cmd+, toggles as above (handled whether Settings is open or not).
- Other existing shortcuts are unchanged.

### 4.5 Page layout

`src/components/settings/SettingsPage.tsx`:

- Header: title "Settings" and a Close button (lucide `X`).
- Left: the section nav, built from the section list. The active row is highlighted. Rows are
  disabled while `locked`.
- Right: the active section's component in a scrolling panel with a readable max width
  (wider for AI Agents and Secret Managers, which hold lists and wide forms).
- shadcn/ui primitives and lucide-react icons only.

`src/components/settings/settings-sections.ts` holds the ordered list:

```ts
interface SettingsSectionDef {
  id: SettingsSection;
  label: string;
  icon: LucideIcon;
  Component: ComponentType;
}
```

Shell order, following Bruno: Proxy, Secret Managers (next to Proxy, Rocket only), AI, Support,
About (Rocket only, last). Later sub-projects insert General, Themes and Display before Proxy,
Client Certificates after Secret Managers, Keybindings before AI, and Cache and Beta after Support.

### 4.6 Sections moved from dialogs

Each dialog becomes a section component with the same state, queries, mutations, validation and
messages. Only the `<Dialog>` wrapper and the `open`/`onOpenChange` props go. The three dialog files
are deleted.

| Dialog | Section | Lifecycle change |
|---|---|---|
| `ProxySettingsDialog` | `ProxySection` | Settings load on mount instead of on `open`. After a successful Save the section stays, shows the existing toast and reloads the saved values (password field cleared, `hasPassword` refreshed). This is the one intended behavior change. A load failure keeps today's rule: Save stays blocked. |
| `SecretManagerConnectionsDialog` | `SecretManagersSection` | The edit form, per-row test vault names and pending delete reset on unmount instead of on close. Unmount happens when the user leaves the section or closes Settings, the same moments the dialog closed. |
| `AgentConfigsDialog` | `AiAgentsSection` | Same reset on unmount. The "no close while saving" rule moves to `setLocked(true)` at the start of a save and `setLocked(false)` when it ends, success or failure, and on unmount. |

Section headers reuse the dialog titles and descriptions as plain headings.

### 4.7 New sections

- `SupportSection`: two links, "Report issues" (`https://github.com/Snehal1112/rocket/issues`) and
  "GitHub" (`https://github.com/Snehal1112/rocket`), opened with `openUrl` from
  `@tauri-apps/plugin-opener`. The links are a constant list so more can be added later.
- `AboutSection`: app name, version (`getVersion` from `@tauri-apps/api/app`), the data folder
  (`get_app_data_dir`, already an IPC command) and the platform (`@tauri-apps/plugin-os`). If a
  value fails to load, its row shows "Unknown" and the rest still render.

`src-tauri/capabilities/default.json` already grants `opener:default`, which allows http and https
URLs, so no capability change is needed.

### 4.8 Other call sites

- `src/components/title-bar/TitleBar.tsx`: the three dialog states and the globe and bot buttons
  are removed. The gear button gets the label "Settings" and calls `toggleSettings()`.
- `src/components/assistant/AssistantStartView.tsx:110`: the text "Add one with the agent button
  in the title bar" becomes a short sentence with a button, "Open AI Agents settings", that calls
  `openSettings('ai')`.
- A grep for `ProxySettingsDialog`, `SecretManagerConnectionsDialog`, `AgentConfigsDialog` and
  "title bar" hints in user-facing text is part of the plan, so no reference is left.

### 4.9 Out of scope for the Shell

- Any new setting (General, Themes, Display, Client Certificates, Keybindings, Cache, Beta).
- Rust changes and new IPC commands.
- Moving the theme toggle out of the status bar (sub-project 2 decides that).

## 5. Error handling

- Load and save errors inside each moved section behave as they do today (toasts, inline errors,
  blocked Save after a failed proxy load).
- About shows "Unknown" for a value it cannot read.
- A Support link that fails to open shows a toast with the URL so the user can copy it.

## 6. Testing

Moved tests (assertions kept, wrapper changed):

- `ProxySettingsDialog.test.tsx` → `ProxySection.test.tsx`, plus a new test that the section stays
  and reloads the saved values after Save.
- `SecretManagerConnectionsDialog.test.tsx` → `SecretManagersSection.test.tsx`.
- `AgentConfigsDialog.test.tsx` → `AiAgentsSection.test.tsx`. The test "does not let the dialog close
  mid-save" becomes "locks the settings store while saving and unlocks after success, failure and
  unmount".

New tests:

- `settings-store`: open, close, toggle, section jump, and that close, toggle and setSection do
  nothing while locked.
- `SettingsPage`: renders the sections in list order, switches panels on click, nav disabled while
  locked, Close button closes.
- Esc closes from the page body but not from an input or an open inner dialog.
- `useKeyboardShortcuts`: Ctrl+, toggles; with Settings open, Ctrl+Enter and Ctrl+S do nothing and
  Ctrl+W closes Settings without closing a tab.
- `pane-store` `openTab` closes Settings.
- `App`: the pane tree stays mounted while Settings is open (a tab's component is not remounted).
- `TitleBar`: one gear button that toggles Settings; no Proxy or AI Agents buttons.
- `AssistantStartView`: the "no agents" button opens Settings at AI.
- `SupportSection`: each link calls `openUrl` with its URL. `AboutSection`: shows the version and
  "Unknown" when a value fails.

Checks before merge: `yarn tsc --noEmit`, `yarn check`, the affected Vitest files, and a manual run
with `yarn tauri dev`:

1. Gear and Ctrl+, open and close Settings; open tabs come back unchanged, including an unsaved
   request draft and a connected WebSocket.
2. Proxy: load, edit, save, reload; the request uses the new proxy.
3. Secret Managers: add, test, edit and delete a connection; External Secrets in an environment
   sees the change.
4. AI Agents: add an agent with a keychain credential; try Close, Esc, Ctrl+W and a sidebar click
   during the save; all are blocked until it ends.
5. Clicking a request in the sidebar closes Settings and shows the request.
6. Switching workspace with Settings open keeps it open.
7. Support links open the browser; About shows the right version and folder.
