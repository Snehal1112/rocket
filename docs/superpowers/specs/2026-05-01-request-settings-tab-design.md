# Request Settings Tab — Full Feature Design

**Date:** 2026-05-01  
**Status:** Approved

---

## Goal

Extend the existing request Settings tab to match Bruno's feature set: Tags (per-request labels), URL Encoding toggle, Follow Redirects toggle, Max Redirects input, and Timeout input. All settings must persist to the `.yml` collection file on disk via existing auto-save and manual-save paths.

---

## Background & Current State

The Settings tab already exists in `RequestPanel.tsx` with three fields:

| Field | Frontend type | IPC field | Status |
|---|---|---|---|
| Verify SSL | `verifySsl: boolean` | `verifySsl` | ✅ Exists |
| Follow Redirects | `followRedirects: boolean` | `followRedirects` | ✅ Exists |
| Timeout (ms) | `timeoutMs: number` | `timeout` | ✅ Exists |

Missing fields per Bruno / OpenCollection spec:

| Field | Location in spec | Where it lives in YAML |
|---|---|---|
| Tags | `info.tags: [string]` | `RequestState.tags` (separate from settings) |
| URL Encoding | `settings.encodeUrl: bool\|"inherit"` | `ApiRequestSettings.encodeUrl` |
| Max Redirects | `settings.maxRedirects: number\|"inherit"` | `ApiRequestSettings.maxRedirects` |

The OpenCollection YAML structs (`OcHttpRequestSettings`) and domain type (`RequestSettings` in `rocket-shared`) already have `encode_url` and `max_redirects` fields per Plan P15. The frontend just doesn't surface or serialize them yet.

---

## Data Model Changes

### 1. `src/types/pane-types.ts`

**`RequestSettings`** — add two new fields:

```ts
export interface RequestSettings {
  verifySsl: boolean;
  followRedirects: boolean;
  maxRedirects: number;    // NEW — default: 5
  timeoutMs: number;
  encodeUrl: boolean;      // NEW — default: true
}
```

**`RequestState`** — add `tags`:

```ts
export interface RequestState {
  requestType: 'http' | 'graphql' | 'grpc' | 'websocket';
  method: HttpMethod;
  url: string;
  pathParams: KeyValueEntry[];
  queryParams: KeyValueEntry[];
  headers: KeyValueEntry[];
  body: BodyState;
  auth: AuthState;
  settings: RequestSettings;
  docs: string | null;
  tags: string[];          // NEW — default: []
}
```

### 2. `src/lib/tauri-api.ts`

**`ApiRequestSettings`** — add two new optional fields:

```ts
export interface ApiRequestSettings {
  timeout?: number;
  followRedirects?: boolean;
  verifySsl?: boolean;
  maxRedirects?: number;   // NEW
  encodeUrl?: boolean;     // NEW
}
```

`Request.tags` already exists on the interface as `tags?: string[]` — no change needed.

### 3. `src/lib/pane-utils.ts`

**`createDefaultRequest()`** — add defaults:

```ts
tags: [],
settings: {
  verifySsl: true,
  followRedirects: true,
  maxRedirects: 5,
  timeoutMs: 0,
  encodeUrl: true,
},
```

**`mapApiRequestToState()`** — map new fields:

```ts
settings: {
  verifySsl: req.settings?.verifySsl ?? true,
  followRedirects: req.settings?.followRedirects ?? true,
  maxRedirects: req.settings?.maxRedirects ?? 5,
  timeoutMs: req.settings?.timeout ?? 0,
  encodeUrl: req.settings?.encodeUrl ?? true,
},
tags: req.tags ?? [],
```

---

## Serialization Changes

### `src/lib/auto-save.ts` — `toApiRequest()`

Must include `encodeUrl`, `maxRedirects`, and `tags` in the serialized payload:

```ts
function toApiRequest(uid: string, name: string, request: RequestState): Request {
  const s = request.settings;
  return {
    uid,
    name,
    method: request.method,
    url: request.url,
    headers: request.headers.filter((h) => h.enabled).map(...),
    body: ...,
    auth,
    tags: request.tags.length > 0 ? request.tags : undefined,
    settings: {
      timeout: s.timeoutMs,
      followRedirects: s.followRedirects,
      verifySsl: s.verifySsl,
      maxRedirects: s.maxRedirects,
      encodeUrl: s.encodeUrl,
    },
  };
}
```

### `src/components/request/SaveRequestButton.tsx` — `buildPayloadFromTab()`

Same additions — `encodeUrl`, `maxRedirects`, `tags`:

```ts
settings: s ? {
  timeout: s.timeoutMs,
  followRedirects: s.followRedirects,
  verifySsl: s.verifySsl,
  maxRedirects: s.maxRedirects,
  encodeUrl: s.encodeUrl,
} : undefined,
tags: tab.request.tags?.length > 0 ? tab.request.tags : undefined,
```

---

## UI Design

### Layout — Settings section in `RequestPanel.tsx`

The existing `activeSection === 'settings'` block is replaced with three cards in a `ScrollArea`:

```
┌─ Tags ─────────────────────────────────────────────────────┐
│  [Input: e.g. smoke, regression etc]                       │
│  [tag-badge ×] [tag-badge ×]                               │
└────────────────────────────────────────────────────────────┘

┌─ Request Settings ─────────────────────────────────────────┐
│  URL Encoding                            [Switch: ON/OFF]  │
│  Automatically encode query parameters                     │
│                                                            │
│  Automatically Follow Redirects          [Switch: ON/OFF]  │
│  Follow HTTP redirects automatically                       │
│                                                            │
│  Max Redirects                                 [Input: 5]  │
│  Set a limit for the number of redirects to follow         │
│                                                            │
│  Timeout (ms)                                  [Input: 0]  │
│  Max time to wait before aborting the request              │
└────────────────────────────────────────────────────────────┘

┌─ Security ─────────────────────────────────────────────────┐
│  ☑ Verify SSL certificate                                  │
│  Validate the server's TLS certificate chain               │
└────────────────────────────────────────────────────────────┘
```

**Key UI rules:**
- Toggles use shadcn `Switch` (not `Checkbox`) — matches Bruno's pink toggle style
- Verify SSL keeps shadcn `Checkbox` (it's a less-commonly changed security guard)
- Max Redirects and Timeout are shadcn `Input` with `type="number"`, `min={0}`
- The existing `Checkbox` for Follow Redirects is replaced by `Switch`
- Tags section uses a controlled `Input` — pressing `Enter` or `,` adds a tag; each tag renders as a `Badge` with a `×` remove button using Lucide `X` icon
- Max Redirects is disabled (grayed, `disabled` prop) when Follow Redirects switch is OFF

### Tags component behavior

- Input: placeholder `"e.g. smoke, regression etc"`
- `onKeyDown`: `Enter` or `,` adds the trimmed value as a new tag (ignores blank/duplicate)
- `onChange`: strip `,` from the typed value to prevent confusion
- Tag list renders below the input as `Badge` elements with `variant="secondary"`
- Each `Badge` has an inline `X` button (Lucide `X`, 10px) that removes the tag
- Tags stored on `RequestState.tags: string[]`
- Update via `updateRequest(tab.id, { tags: [...] })`

### Dirty dot indicator

The "Settings" tab label in `RocketTabBar` shows a dot indicator when settings differ from defaults. Update `settingsModified` to also consider the new fields and tags:

```ts
const settingsModified =
  !settings.verifySsl ||
  !settings.followRedirects ||
  settings.timeoutMs !== 0 ||
  settings.maxRedirects !== 5 ||
  !settings.encodeUrl ||
  request.tags.length > 0;
```

---

## File Map

| File | Change |
|---|---|
| `src/types/pane-types.ts` | Add `encodeUrl`, `maxRedirects` to `RequestSettings`; add `tags: string[]` to `RequestState` |
| `src/lib/pane-utils.ts` | Add defaults in `createDefaultRequest()`; add field mapping in `mapApiRequestToState()` |
| `src/lib/auto-save.ts` | Include `encodeUrl`, `maxRedirects`, `tags` in `toApiRequest()` |
| `src/components/request/SaveRequestButton.tsx` | Include `encodeUrl`, `maxRedirects`, `tags` in `buildPayloadFromTab()` |
| `src/components/request/RequestPanel.tsx` | Redesign Settings section: Tags card + Request Settings card (Switch-based) + Security card |

No Rust changes needed — `OcHttpRequestSettings` and `RequestSettings` domain types already have `encode_url` and `max_redirects` per Plan P15. The `Request` IPC type already has `tags?: string[]`. This is a pure frontend feature.

---

## What Is NOT in Scope

- Inherit mode for settings (the `"inherit"` variant in the OC spec — treated as defaults for now)
- Tags on folders or collections (only request-level tags)
- Tag filtering/search in the sidebar (future)
- `verifySSL` UI redesign to Switch (keep as Checkbox for intentional visual distinction)
