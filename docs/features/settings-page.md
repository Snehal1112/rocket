# Settings page

One global Settings page, modelled on Bruno's Preferences. Spec:
`docs/superpowers/specs/2026-10-31-settings-page-shell-design.md` (parity map and later
sub-projects R-73 to R-77). Plans: `docs/superpowers/plans/settings-page-shell/`.

## Where it is

- The title-bar gear and Ctrl/Cmd+, open and close it. It covers the editor area only; the title
  bar, sidebar, assistant panel and status bar stay usable.
- The tabs underneath stay mounted and `inert` (`src/components/layout/MainArea.tsx`), so drafts,
  sessions and editors come back unchanged.
- It is global: a workspace or collection switch keeps it open.
- It is not saved in UI state, so Settings starts closed after a restart.
- Settings is the sunken elevation level and the Collections sidebar is raised. See `docs/features/elevation.md`.

## Sections

Order follows Bruno: Proxy, Secret Managers, AI Agents, Support, About. The list is
`SETTINGS_SECTIONS` in `src/components/settings/settings-sections.ts`; section components live in
`src/components/settings/sections/`. Secret Managers and AI Agents use the wide panel.

To add a section: add its id to `SettingsSection` in `src/stores/settings-store.ts`, add a row to
`SETTINGS_SECTIONS` at its Bruno position (General, Themes and Display before Proxy; Client
Certificates after Secret Managers; Keybindings before AI Agents; Cache and Beta after Support), and
start the component with `SettingsSectionHeader`. Simple settings apply at once; forms that can be
invalid keep a Save button.

## State and rules

- `useSettingsStore` (`src/stores/settings-store.ts`): `open`, `section`, `locked`. The page shows
  the first section until one is picked.
- Closing: the Close button, Esc, Ctrl/Cmd+W, Ctrl/Cmd+, the gear, and any `openTab` or
  `openCollectionTab` call.
- Esc inside an input, textarea, select, contenteditable element, combobox, listbox, menu, dialog or
  alertdialog does not close Settings. That Esc belongs to the field or popup.
- Key isolation: while Settings is mounted, a `keydown` listener on `document.body` stops keys
  without Ctrl, Meta or Alt. Hidden tabs listen on `document` or `window` (Flow's Delete and
  Backspace, the Contracts j/k/n/e/p/c/Delete keys), so they never see them. The listener is on
  `body` on purpose: React handles events at `#root`, below `body`, so React handlers in Settings
  (Radix Select, RadioGroup, menus) still run. Modified keys still bubble, so Ctrl/Cmd+, Ctrl/Cmd+W
  and the other global shortcuts work. The listener is removed on unmount.
- Sidebar clicks on the workspace home button and the audit log shortcut call `closeSettings()` in
  their handlers, so the user lands on the tab they asked for. `openWorkspaceTabs` itself does not
  close Settings: a workspace switch keeps it open. A locked Settings stays open.
- Focus goes back to the element that had it before Settings opened, when Settings closes. It is
  restored only if focus is still inside Settings or on the page body, so a fresh click elsewhere
  keeps its focus.
- While open, Ctrl/Cmd+Enter and Ctrl/Cmd+S do nothing, so they never act on a hidden tab.
  Ctrl/Cmd+W closes Settings.
- Ctrl+Tab, Ctrl+Shift+Tab and Ctrl+1..9 still switch the hidden tabs while Settings stays on top.
  The spec leaves other shortcuts unchanged.
- While an AI Agents save runs, Settings is locked. It cannot be closed (button, Esc, Ctrl+W,
  Ctrl+, gear) and cannot switch sections, and `openSettings` does nothing. Opening a tab does not
  close it; the tab opens underneath and Settings stays on top until the save ends. The lock is set
  at the start of the save, released when it ends, and released on unmount.
- A section's form state resets when it unmounts (leaving the section or closing Settings). In
  Secret Managers this drops the connection form.
- Proxy: Save stays on the section and refreshes the stored setting in place. A failed refresh only
  shows a toast and keeps the form. Reload drops edits, or retries a failed load.
- AI Agents has a sticky Save footer, so Save stays visible on a long form.
- With no agents, the assistant start view shows an "Open AI Agents settings" button
  (`openSettings('ai')`).

## Manual checks

1. The gear, Ctrl+, and Esc open and close Settings; open tabs come back unchanged, including an
   unsaved request draft and a connected WebSocket.
2. Proxy: load, edit, Save, then Reload; a request uses the new proxy.
3. Secret Managers: add, test, edit and delete a connection; External Secrets in an environment sees
   the change.
4. AI Agents: add an agent with a keychain credential; try Close, Esc, Ctrl+W and a sidebar click
   during the save. All are blocked until it ends.
5. AI Agents on a long form: the Save footer stays pinned at the bottom (check in WebKitGTK).
6. Clicking a request in the sidebar closes Settings and shows the request. So do the workspace home
   button and the audit log shortcut.
7. Switching workspace with Settings open keeps it open.
8. Support links open the browser; About shows the right version and folder.
9. With a Flow tab (selected nodes) or a Contracts tab open underneath, Backspace and Delete in
   Settings do not touch it, and Ctrl+, and Ctrl+W still work.

## Follow-ups

- R-73 Themes and Display, R-74 General, R-75 global client certificates, R-76 Keybindings,
  R-77 Cache and Beta (`docs/roadmaps/roadmap.md`, section 8f).
- The theme toggle still lives in the status bar until R-73 decides where it goes.
