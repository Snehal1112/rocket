# Request Settings Tab — Tags, URL Encoding, Max Redirects Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the request Settings tab with Tags, URL Encoding, and Max Redirects fields — matching Bruno's feature set — and persist them to `.yml` collection files via existing auto-save and manual-save paths.

**Architecture:** Pure frontend change across 6 files. No Rust changes needed. `save_request` receives `rocket_collection::Request` directly; that struct already has `settings: Option<RequestSettings>` with `encode_url`, `max_redirects`, `verify_ssl` (all camelCase via `#[serde(rename_all = "camelCase")]`). The only bridge gap is `ApiRequestSettings` in `tauri-api.ts` — it currently omits `encodeUrl` and `maxRedirects`, so the TypeScript types don't reflect what Rust already accepts. Fix that first, then extend the remaining TS types, wire serialization, and redesign the UI.

**Tech Stack:** React 18, TypeScript, Zustand, shadcn/ui (Switch, Badge, Input, Card, CardContent, ScrollArea, Checkbox), Lucide React icons

**Spec:** `docs/superpowers/specs/2026-05-01-request-settings-tab-design.md`

---

## File Map

| File | Change |
|---|---|
| `src/lib/tauri-api.ts` | Add `encodeUrl?` and `maxRedirects?` to `ApiRequestSettings` |
| `src/types/pane-types.ts` | Add `encodeUrl`, `maxRedirects` to `RequestSettings`; add `tags: string[]` to `RequestState` |
| `src/lib/pane-utils.ts` | Add defaults in `createDefaultRequest()`; extend `mapApiRequestToState()` |
| `src/lib/auto-save.ts` | Include `encodeUrl`, `maxRedirects`, `tags` in `toApiRequest()` |
| `src/components/request/SaveRequestButton.tsx` | Include `encodeUrl`, `maxRedirects`, `tags` in `buildPayloadFromTab()` |
| `src/components/request/RequestPanel.tsx` | Redesign Settings section: Tags card + Request Settings card (Switch-based) + Security card |

---

## Chunk 1: IPC bridge fix + type extensions + serialization

### Task 1: Fix ApiRequestSettings IPC bridge + extend frontend types

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/types/pane-types.ts`
- Modify: `src/lib/pane-utils.ts`

- [ ] **Step 1: Verify the gap in `ApiRequestSettings`**

```bash
grep -n "ApiRequestSettings" src/lib/tauri-api.ts
```

Expected: interface with only `timeout`, `followRedirects`, `verifySsl` — `encodeUrl` and `maxRedirects` missing.

- [ ] **Step 2: Add `encodeUrl` and `maxRedirects` to `ApiRequestSettings` in `tauri-api.ts`**

Find:

```ts
export interface ApiRequestSettings {
  /** Timeout in milliseconds. */
  timeout?: number;
  followRedirects?: boolean;
  verifySsl?: boolean;
}
```

Replace with:

```ts
export interface ApiRequestSettings {
  /** Timeout in milliseconds. */
  timeout?: number;
  followRedirects?: boolean;
  verifySsl?: boolean;
  maxRedirects?: number;
  encodeUrl?: boolean;
}
```

This is the only change needed in `tauri-api.ts`. The Rust `save_request` command already accepts these fields — `rocket_shared::types::RequestSettings` has `encode_url`, `max_redirects`, `verify_ssl` with `#[serde(rename_all = "camelCase")]`, so they deserialize from the camelCase IPC payload transparently. No Rust edits required.

- [ ] **Step 3: Add `encodeUrl` and `maxRedirects` to `RequestSettings` in `pane-types.ts`**

Find:

```ts
export interface RequestSettings {
  verifySsl: boolean;
  followRedirects: boolean;
  timeoutMs: number;
}
```

Replace with:

```ts
export interface RequestSettings {
  verifySsl: boolean;
  followRedirects: boolean;
  maxRedirects: number;
  timeoutMs: number;
  encodeUrl: boolean;
}
```

- [ ] **Step 4: Add `tags` field to `RequestState` in `pane-types.ts`**

Find the `RequestState` interface. Add `tags: string[];` as the last field (after `docs`):

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
  tags: string[];
}
```

- [ ] **Step 5: Update `createDefaultRequest()` in `pane-utils.ts`**

```bash
grep -n "createDefaultRequest\|verifySsl\|timeoutMs\|settings" src/lib/pane-utils.ts | head -20
```

Find the `settings:` block in `createDefaultRequest()` and update it; add `tags`:

```ts
settings: {
  verifySsl: true,
  followRedirects: true,
  maxRedirects: 5,
  timeoutMs: 0,
  encodeUrl: true,
},
tags: [],
```

- [ ] **Step 6: Update `mapApiRequestToState()` in `pane-utils.ts`**

Find the `settings:` block inside `mapApiRequestToState()`. Replace with:

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

- [ ] **Step 7: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: errors only from `auto-save.ts`, `SaveRequestButton.tsx`, and `RequestPanel.tsx` — all fixed in the next two tasks.

- [ ] **Step 8: Commit**

```bash
git add src/lib/tauri-api.ts src/types/pane-types.ts src/lib/pane-utils.ts
git commit -m "feat(types): fix ApiRequestSettings IPC bridge; add encodeUrl/maxRedirects/tags to frontend types"
```

---

### Task 2: Wire new fields into serialization paths

**Files:**
- Modify: `src/lib/auto-save.ts`
- Modify: `src/components/request/SaveRequestButton.tsx`

- [ ] **Step 1: Read current `toApiRequest()` in `auto-save.ts`**

```bash
cat src/lib/auto-save.ts
```

Note: the current function omits `settings` and `tags` from the returned object entirely.

- [ ] **Step 2: Update `toApiRequest()` in `auto-save.ts`**

Replace the full function body:

```ts
function toApiRequest(uid: string, name: string, request: RequestState): Request {
  const auth =
    request.auth.authType === 'oauth2' && request.auth.oauth2
      ? oauth2StateToApiAuth(request.auth.oauth2)
      : toApiAuth(request.auth);

  const s = request.settings;

  return {
    uid,
    name,
    method: request.method,
    url: request.url,
    headers: request.headers
      .filter((h) => h.enabled)
      .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
    body:
      request.body.mode !== 'none'
        ? { mode: request.body.mode, content: request.body.content }
        : undefined,
    auth,
    tags: request.tags && request.tags.length > 0 ? request.tags : undefined,
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

- [ ] **Step 3: Read current `buildPayloadFromTab()` in `SaveRequestButton.tsx`**

```bash
grep -n "buildPayloadFromTab\|settings\|tags" src/components/request/SaveRequestButton.tsx
```

- [ ] **Step 4: Update `buildPayloadFromTab()` in `SaveRequestButton.tsx`**

Replace the full function:

```ts
function buildPayloadFromTab(tab: RequestTab): ApiRequest {
  const body = tab.request.body;
  const s = tab.request.settings;
  return {
    uid: tab.id,
    name: tab.title,
    method: tab.request.method,
    url: tab.request.url,
    headers: tab.request.headers
      .filter((h) => h.key)
      .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
    body: body.mode !== 'none' ? { mode: body.mode, content: body.content } : undefined,
    auth: authForSave(tab.request.auth),
    tags: tab.request.tags && tab.request.tags.length > 0 ? tab.request.tags : undefined,
    settings: s
      ? {
          timeout: s.timeoutMs,
          followRedirects: s.followRedirects,
          verifySsl: s.verifySsl,
          maxRedirects: s.maxRedirects,
          encodeUrl: s.encodeUrl,
        }
      : undefined,
    docs: tab.request.docs ?? null,
  };
}
```

- [ ] **Step 5: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: only remaining errors in `RequestPanel.tsx` (fixed in Task 3).

- [ ] **Step 6: Commit**

```bash
git add src/lib/auto-save.ts src/components/request/SaveRequestButton.tsx
git commit -m "feat(save): include encodeUrl, maxRedirects, tags in auto-save and manual save payloads"
```

---

## Chunk 2: Settings section UI redesign

### Task 3: Redesign Settings section in RequestPanel

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

- [ ] **Step 1: Verify required shadcn components are installed**

```bash
ls src/components/ui/ | grep -E "switch|badge"
```

If `switch.tsx` is missing:

```bash
yarn dlx shadcn@latest add switch
```

If `badge.tsx` is missing:

```bash
yarn dlx shadcn@latest add badge
```

- [ ] **Step 2: Read the current Settings section and imports**

```bash
grep -n "activeSection === 'settings'\|settingsModified\|handleSettingsChange\|import.*lucide\|import.*@/components/ui" src/components/request/RequestPanel.tsx | head -30
```

- [ ] **Step 3: Add new imports**

Add `Tag` and `X` to the existing lucide-react import (merge, don't add a second import line):

```ts
import { ..., Tag, X } from 'lucide-react';
```

Add `Switch` and `Badge` alongside existing shadcn/ui imports:

```ts
import { Switch } from '@/components/ui/switch';
import { Badge } from '@/components/ui/badge';
```

- [ ] **Step 4: Add `tagInput` state**

Inside the `RequestPanel` function body, after the existing `useState` declarations, add:

```tsx
const [tagInput, setTagInput] = useState('');
```

- [ ] **Step 5: Update `settingsModified` and the settings fallback**

Find these two lines (they appear together):

```ts
const settings = request.settings ?? { verifySsl: true, followRedirects: true, timeoutMs: 30000 };
const settingsModified =
  !settings.verifySsl || !settings.followRedirects || settings.timeoutMs !== 30000;
```

Replace with:

```ts
const settings = request.settings ?? {
  verifySsl: true,
  followRedirects: true,
  maxRedirects: 5,
  timeoutMs: 0,
  encodeUrl: true,
};
const settingsModified =
  !settings.verifySsl ||
  !settings.followRedirects ||
  settings.timeoutMs !== 0 ||
  settings.maxRedirects !== 5 ||
  !settings.encodeUrl ||
  (request.tags ?? []).length > 0;
```

- [ ] **Step 6: Add tag helper callbacks**

After the existing `handleSettingsChange` callback, add:

```tsx
const handleAddTag = useCallback(
  (raw: string) => {
    const tag = raw.trim().replace(/,/g, '');
    if (!tag) return;
    const current = request.tags ?? [];
    if (current.includes(tag)) return;
    updateRequest(tab.id, { tags: [...current, tag] });
    setTagInput('');
  },
  [tab.id, updateRequest, request.tags],
);

const handleRemoveTag = useCallback(
  (tag: string) => {
    const current = request.tags ?? [];
    updateRequest(tab.id, { tags: current.filter((t) => t !== tag) });
  },
  [tab.id, updateRequest, request.tags],
);
```

- [ ] **Step 7: Replace the Settings section JSX**

Find the entire `{activeSection === 'settings' && ( ... )}` block and replace it with:

```tsx
{activeSection === 'settings' && (
  <ScrollArea className='h-full'>
    <div className='p-6 max-w-2xl mx-auto space-y-4'>

      {/* Tags */}
      <Card>
        <CardContent className='p-4 space-y-3'>
          <div className='flex items-center gap-2 mb-1'>
            <Tag className='h-3.5 w-3.5 text-muted-foreground' />
            <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
              Tags
            </span>
          </div>
          <Input
            placeholder='e.g. smoke, regression etc'
            value={tagInput}
            onChange={(e) => setTagInput(e.target.value.replace(/,/g, ''))}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ',') {
                e.preventDefault();
                handleAddTag(tagInput);
              }
            }}
            className='h-8 text-sm'
          />
          {(request.tags ?? []).length > 0 && (
            <div className='flex flex-wrap gap-1.5 pt-0.5'>
              {(request.tags ?? []).map((tag) => (
                <Badge
                  key={tag}
                  variant='secondary'
                  className='gap-1 pl-2 pr-1 py-0.5 text-xs font-normal'
                >
                  <Tag className='h-2.5 w-2.5 text-muted-foreground' />
                  {tag}
                  <button
                    type='button'
                    onClick={() => handleRemoveTag(tag)}
                    className='ml-0.5 rounded-sm opacity-60 hover:opacity-100 focus:outline-none'
                    aria-label={`Remove tag ${tag}`}
                  >
                    <X className='h-2.5 w-2.5' />
                  </button>
                </Badge>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Request settings */}
      <Card>
        <CardContent className='p-4 space-y-0 divide-y divide-border'>

          {/* URL Encoding */}
          <div className='flex items-center justify-between py-3'>
            <div>
              <p className='text-sm font-medium'>URL Encoding</p>
              <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                Automatically encode query parameters in the URL
              </p>
            </div>
            <Switch
              checked={settings.encodeUrl}
              onCheckedChange={(checked) => handleSettingsChange({ encodeUrl: checked })}
            />
          </div>

          {/* Follow Redirects */}
          <div className='flex items-center justify-between py-3'>
            <div>
              <p className='text-sm font-medium'>Automatically Follow Redirects</p>
              <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                Follow HTTP redirects automatically
              </p>
            </div>
            <Switch
              checked={settings.followRedirects}
              onCheckedChange={(checked) => handleSettingsChange({ followRedirects: checked })}
            />
          </div>

          {/* Max Redirects */}
          <div className='flex items-center justify-between py-3'>
            <div>
              <p className={cn('text-sm font-medium', !settings.followRedirects && 'text-muted-foreground')}>
                Max Redirects
              </p>
              <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                Set a limit for the number of redirects to follow
              </p>
            </div>
            <Input
              type='number'
              min={0}
              disabled={!settings.followRedirects}
              className='h-8 w-24 text-sm text-right tabular-nums'
              value={settings.maxRedirects}
              onChange={(e) => {
                const val = Number(e.target.value);
                if (!Number.isNaN(val) && val >= 0) {
                  handleSettingsChange({ maxRedirects: val });
                }
              }}
            />
          </div>

          {/* Timeout */}
          <div className='flex items-center justify-between py-3'>
            <div>
              <p className='text-sm font-medium'>Timeout (ms)</p>
              <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                Set maximum time to wait before aborting the request
              </p>
            </div>
            <div className='flex items-center gap-1.5'>
              <Input
                type='number'
                min={0}
                className='h-8 w-24 text-sm text-right tabular-nums'
                value={settings.timeoutMs}
                onChange={(e) => {
                  const val = Number(e.target.value);
                  if (!Number.isNaN(val) && val >= 0) {
                    handleSettingsChange({ timeoutMs: val });
                  }
                }}
              />
              {settings.timeoutMs > 0 && (
                <button
                  type='button'
                  onClick={() => handleSettingsChange({ timeoutMs: 0 })}
                  className='text-muted-foreground hover:text-foreground'
                  aria-label='Clear timeout'
                >
                  <X className='h-3.5 w-3.5' />
                </button>
              )}
            </div>
          </div>

        </CardContent>
      </Card>

      {/* Security */}
      <Card>
        <CardContent className='p-4 space-y-3'>
          <div className='flex items-center gap-2 mb-1'>
            <ShieldCheck className='h-3.5 w-3.5 text-muted-foreground' />
            <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
              Security
            </span>
          </div>
          <label
            htmlFor='verify-ssl'
            className='flex items-center gap-2.5 rounded-md px-2 py-1.5 -mx-1 cursor-pointer transition-colors hover:bg-muted/60'
          >
            <Checkbox
              id='verify-ssl'
              checked={settings.verifySsl}
              onCheckedChange={(checked) => handleSettingsChange({ verifySsl: !!checked })}
            />
            <div>
              <span className='text-sm'>Verify SSL certificate</span>
              <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                Validate the server's TLS certificate chain.
              </p>
            </div>
          </label>
        </CardContent>
      </Card>

    </div>
  </ScrollArea>
)}
```

- [ ] **Step 8: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: zero errors.

- [ ] **Step 9: Lint check**

```bash
yarn check 2>&1 | head -20
```

Expected: zero new warnings.

- [ ] **Step 10: Commit**

```bash
git add src/components/request/RequestPanel.tsx
git commit -m "feat(ui): redesign Settings tab — Tags, URL Encoding, Max Redirects, Switch toggles"
```

---

## Smoke Test Checklist

After all tasks complete, manually verify:

- [ ] Open any saved request in a collection — Settings tab renders without errors
- [ ] Add tag: type `smoke` + Enter → badge appears; add `regression` + Enter → second badge; click `×` → badge removes
- [ ] Toggle URL Encoding OFF → auto-saved; check `.yml` has `encodeUrl: false` under `settings:`
- [ ] Toggle Follow Redirects OFF → Max Redirects input becomes disabled/grayed
- [ ] Set Max Redirects to `3` → auto-saved; check `.yml` has `maxRedirects: 3`
- [ ] Set Timeout to `5000` → clear `×` button appears; click → resets to `0`
- [ ] Settings dot indicator appears when any non-default value set; disappears when all reset to defaults
- [ ] Cmd+S manual save persists tags + all settings; verify `.yml` has `info.tags` and `settings.*`
- [ ] Reopen saved request in new tab — all settings + tags load correctly from disk

---

## Milestone Checklist

- [ ] `ApiRequestSettings` has `encodeUrl?` and `maxRedirects?`
- [ ] `RequestSettings` has `encodeUrl` and `maxRedirects`
- [ ] `RequestState` has `tags: string[]`
- [ ] `createDefaultRequest()` defaults: `encodeUrl: true`, `maxRedirects: 5`, `timeoutMs: 0`, `tags: []`
- [ ] `mapApiRequestToState()` maps all 5 settings fields + tags
- [ ] `auto-save.ts` serializes all 5 settings + tags
- [ ] `SaveRequestButton.tsx` serializes all 5 settings + tags on manual save
- [ ] Settings section: Tags card + Request Settings card (Switch toggles) + Security card
- [ ] Dirty dot accounts for `encodeUrl`, `maxRedirects`, `tags`
- [ ] `yarn tsc --noEmit` passes clean
- [ ] Zero Rust changes required
