# Settings Page Shell Plan Index

Spec: `docs/superpowers/specs/2026-10-31-settings-page-shell-design.md` (roadmap R-72).

Plans run one at a time, in order. Each plan has at most three tasks and ends with working,
checked software.

| # | Plan | Delivers |
|---|---|---|
| 01 | `2026-10-31-settings-shell-plan-01-page-foundation.md` | `useSettingsStore`; the Settings page with its section nav, Support and About; the page drawn over the editor area with the tabs kept mounted; Ctrl/Cmd+, opens and closes it. |
| 02 | `2026-10-31-settings-shell-plan-02-shortcuts-proxy-secrets.md` | Tab shortcuts are off while Settings is open; opening a tab closes Settings; Proxy and Secret Managers move in as sections; the title-bar gear opens Settings. |
| 03 | `2026-10-31-settings-shell-plan-03-ai-agents-and-cleanup.md` | AI Agents moves in with the save lock; the assistant's "no agents" hint opens it; feature doc, CLAUDE.md pointer and roadmap update. |

## Cross-plan facts

- Frontend only. No Rust change, no new IPC command, no change to `src-tauri/app-commands.txt`.
  `src-tauri/capabilities/default.json` already grants `opener:default`, which allows http and
  https URLs for `openUrl`.
- Commands follow the project `CLAUDE.md`: `yarn tsc --noEmit`, `yarn check`, `yarn test <path>`.
  If `yarn check` reports formatting or import order, run `yarn lint` (auto-fix), then `yarn check`
  again.
- Commits go through the `dev-workflow-skills:1-git-commit` skill with conventional subjects. Stage
  explicit paths only (`git add <paths>`), never `-A` or `.`; other sessions share this checkout.
- shadcn/ui primitives and lucide-react icons only. Zustand: narrow selectors, never destructure the
  whole store in a component.
- No task here touches `.yml`, collection, environment, auth or variable resolution, so the
  OpenCollection spec-reference step does not apply.

### Names shared across plans

```ts
// src/stores/settings-store.ts (plan 01)
export type SettingsSection = 'proxy' | 'secret-managers' | 'ai' | 'support' | 'about';
export const useSettingsStore: UseBoundStore<StoreApi<{
  open: boolean;
  section: SettingsSection | null;
  locked: boolean;
  openSettings(section?: SettingsSection): void;
  closeSettings(): void;
  toggleSettings(): void;
  setSection(section: SettingsSection): void;
  setLocked(locked: boolean): void;
  reset(): void;
}>>;

// src/components/settings/settings-sections.ts (plan 01; rows added in 02 and 03)
export interface SettingsSectionDef {
  id: SettingsSection;
  label: string;
  icon: LucideIcon;
  Component: ComponentType;
  wide?: boolean;
}
export const SETTINGS_SECTIONS: SettingsSectionDef[];
export function resolveSection(id: SettingsSection | null, sections?: SettingsSectionDef[]): SettingsSectionDef;

// src/components/settings/SettingsSectionHeader.tsx (plan 01)
export function SettingsSectionHeader(props: { title: string; description?: string }): JSX.Element;

// src/components/settings/SettingsPage.tsx (plan 01)
export function SettingsPage(): JSX.Element; // <section aria-label='Settings'>

// src/components/layout/MainArea.tsx (plan 01)
export function MainArea(props: { children: ReactNode }): JSX.Element;

// src/components/settings/sections/*.tsx
export function SupportSection(): JSX.Element;      // plan 01
export function AboutSection(): JSX.Element;        // plan 01
export function ProxySection(): JSX.Element;        // plan 02
export function SecretManagersSection(): JSX.Element; // plan 02
export function AiAgentsSection(): JSX.Element;     // plan 03
```

Final section order (Bruno's order, Rocket-only rows placed per spec 4.5): Proxy, Secret Managers,
AI Agents, Support, About.

## Deviations from the spec (decided while planning)

- **MainArea instead of an App test.** `App.tsx` runs a dozen event bridges and a startup restore,
  so it has no unit test. The overlay lives in a new `MainArea` component that `App.tsx` uses for
  `<main>`, and "the pane tree stays mounted" is tested there.
- **Tabs underneath are `inert` while Settings is open.** Without it, Tab could move focus into the
  hidden tabs. React 19 supports `inert` and `AssistantPanel.tsx:197` already uses it.
- **`openCollectionTab` also closes Settings.** It focuses an existing collection tab without
  calling `openTab` (`pane-store.ts:1576`), and its callers are user clicks (a toast action, the
  trust notice, the request panel). `openWorkspaceTabs` does not close Settings, since a workspace
  switch must keep it open (spec 4.3).
- **Store `section` starts as `null`.** The page shows the first registered section for `null` or
  for an id that is not registered yet, so plan 01 works with only Support and About.
- **Proxy's Cancel button becomes Reload.** A section has nothing to cancel back to. Reload loads
  the stored setting again, dropping edits, and also retries a failed load. The load-failure text
  changes from "Close this dialog and try again." to "Use Reload to try again."
- **Esc uses a native listener on the page root.** Radix popups (Select, dropdowns) render in
  portals outside the page's DOM subtree, so their Esc never reaches the listener. Esc from an
  input, textarea, combobox or nested dialog inside the page is ignored by a `closest()` check.
- **Section files live in `src/components/settings/sections/`**, with tests in
  `sections/__tests__/`. The three dialog files and their tests are moved with `git mv`, so history
  follows them.
- **The AI Agents test "lays the dialog out as a column…" is deleted.** It asserted dialog-only
  classes (`max-h-[85vh]`); the section scrolls inside the Settings panel instead.

## Carry-over notes

None yet. Reviews of each plan add notes here for the plans after it.
