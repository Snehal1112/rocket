# Spec: Bruno-Style Request Panel Tab Redesign

**Date:** 2026-03-30
**Scope:** `src/components/request/` — RequestPanel, BodyEditor, AuthEditor

---

## Problem

The request panel tab bar uses shadcn's default pill/background tab style (`TabsList` with a rounded container and highlighted active background). This does not match the Bruno API client's flat, minimal underline style. Additionally, the Body mode selector and Auth type selector are buried inside content areas, whereas Bruno surfaces them in the tab bar for faster access.

---

## Goals

1. Replace shadcn pill tabs with a flat underline tab bar matching Bruno's design.
2. Float the Body mode selector to the right side of the tab bar when the Body tab is active.
3. Float the Auth type selector to the right side of the tab bar when the Auth tab is active.
4. Add count badges on Params and Headers tabs when entries are present.
5. Add a StatusDot on Body (when mode ≠ `'none'`) and Auth (when authType ≠ `'none'`).

---

## Out of Scope

- `ResponseBodyViewer.tsx` — recently migrated to shadcn Tabs; different visual context.
- Any tab beyond the existing four: Params, Headers, Body, Auth.
- State management, Tauri commands, or backend logic.
- `QueryParamsEditor`, `PathParamsPanel`, `HeadersEditor`, `SaveRequestButton` — no changes.

---

## Architecture

Purely presentational. The shadcn `Tabs` root and `TabsContent` are kept unchanged — Radix UI continues to own keyboard navigation and ARIA state. Only the visual tab strip (`TabsList`/`TabsTrigger`) is replaced with a new `BrunoTabBar` component.

The Body mode selector and Auth type selector move from their respective editor components up to `RequestPanel`, where they are passed as `rightContent` to `BrunoTabBar`.

---

## Design Details

### Tab bar visual spec

```
┌─ Params (3) · Headers (2) · Body ● · Auth ────────── [JSON ▾] ┐
```

- Container: `flex items-center border-b border-border`
- Each tab: `py-2 mr-4 text-sm border-b-2`
  - Inactive: `border-transparent text-muted-foreground hover:text-foreground`
  - Active: `border-primary text-foreground font-semibold`
- Count badge: small rounded pill, shown when count > 0
  - `ml-1 text-xs font-semibold bg-muted rounded-full px-1.5 min-w-[18px] h-[18px] inline-flex items-center justify-center`
- StatusDot: 6px filled circle using `bg-primary`, shown inline after tab label
  - Body tab: visible when `request.body.mode !== 'none'`
  - Auth tab: visible when `request.auth.authType !== 'none'`
- Right slot: `ml-auto flex items-center` — renders `rightContent` prop

### BrunoTabBar component API

```tsx
interface BrunoTabProps {
  value: string;
  label: ReactNode;        // tab label + badge/dot
  onClick: () => void;
  isActive: boolean;
}

interface BrunoTabBarProps {
  tabs: BrunoTabProps[];
  rightContent?: ReactNode;
}
```

The component renders a plain `div` + `button` elements. It does NOT use Radix tab primitives — active state is driven by `isActive` prop from the parent's `useState`. `TabsContent` in `RequestPanel` is keyed off the same `activeSection` state.

### RequestPanel changes

- Remove shadcn `TabsList`/`TabsTrigger`; replace with `<BrunoTabBar>`.
- Render Body mode `Select` as `rightContent` when `activeSection === 'body'`.
- Render Auth type `Select` as `rightContent` when `activeSection === 'auth'`.
- Body mode change: `updateRequest(tab.id, { body: { ...request.body, mode: val } })`.
- Auth type change: `updateRequest(tab.id, { auth: { ...request.auth, authType: val } })`.

### BodyEditor changes

- Remove the mode selector (`Select` for none/json/xml/text/formdata/binary) from the top.
- Props unchanged: `{ body, onChange }`. Mode is already set when the component renders.

### AuthEditor changes

- Remove the auth type selector (`Select` for none/basic/bearer/apikey/oauth2/awsSigV4) from the top.
- Props unchanged: `{ auth, onChange, showInherit }`. Auth type is already set when the component renders.

---

## Files Changed

| File | Change |
|------|--------|
| `src/components/request/BrunoTabBar.tsx` | New — flat underline tab bar with right slot |
| `src/components/request/RequestPanel.tsx` | Replace TabsList/TabsTrigger with BrunoTabBar; own Body mode and Auth type selectors |
| `src/components/request/BodyEditor.tsx` | Remove mode selector from top |
| `src/components/request/AuthEditor.tsx` | Remove auth type selector from top |

---

## Testing

Pre-merge checks:
```bash
yarn tsc --noEmit
yarn build
```

Manual smoke test:
1. Open a request tab — flat underline tab bar renders; active tab shows bottom border + bold; inactive tabs show muted text.
2. Add query params — count badge appears on Params tab.
3. Add headers — count badge appears on Headers tab.
4. Switch to Body, set mode to JSON — StatusDot appears on Body tab; mode Select floats right in tab bar; switch away and back, mode persists.
5. Switch to Auth, set type to Bearer — StatusDot appears on Auth tab; type Select floats right in tab bar.
6. Keyboard nav — Tab/arrow keys still move between tabs.
7. Light and dark mode — underline and dot use CSS variables, render correctly in both.
