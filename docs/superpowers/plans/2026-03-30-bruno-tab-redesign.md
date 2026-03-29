# Bruno-Style Request Panel Tab Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the shadcn pill tab bar in the request panel with a flat Bruno-style underline tab bar, move the Body mode selector and Auth type selector to the right side of the tab bar, and add count badges and status dots to tab labels.

**Architecture:** A new `BrunoTabBar` component renders the flat underline tab strip using plain `button` elements driven by `activeSection` state from `RequestPanel`. The shadcn `Tabs` root and `TabsContent` are removed in favour of direct conditional rendering. Body mode and Auth type selectors are lifted from their editor components into `RequestPanel` where they are passed as `rightContent` to `BrunoTabBar`.

**Tech Stack:** React 19, TypeScript 5.8, TailwindCSS 4.2, shadcn/ui (Select only — no Tabs primitives in this component), Zustand pane-store.

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/components/request/BrunoTabBar.tsx` | Create | Flat underline tab strip with right slot |
| `src/components/request/BodyEditor.tsx` | Modify | Remove mode selector UI + related code |
| `src/components/request/AuthEditor.tsx` | Modify | Remove auth type selector UI + related code |
| `src/components/request/RequestPanel.tsx` | Modify | Wire BrunoTabBar; own Body mode + Auth type selectors; drop shadcn Tabs |

---

## Task 1: Create BrunoTabBar component

**Files:**
- Create: `src/components/request/BrunoTabBar.tsx`

- [ ] **Step 1: Write the component**

```tsx
import { cn } from '@/lib/utils';

export interface BrunoTabDef {
  value: string;
  label: React.ReactNode;
  isActive: boolean;
  onClick: () => void;
}

interface BrunoTabBarProps {
  tabs: BrunoTabDef[];
  rightContent?: React.ReactNode;
}

export function BrunoTabBar({ tabs, rightContent }: BrunoTabBarProps) {
  return (
    <div className="flex items-center border-b border-border px-3 shrink-0">
      {tabs.map((tab) => (
        <button
          key={tab.value}
          type="button"
          onClick={tab.onClick}
          className={cn(
            'py-2 mr-4 text-sm border-b-2 -mb-px transition-colors',
            tab.isActive
              ? 'border-primary text-foreground font-semibold'
              : 'border-transparent text-muted-foreground hover:text-foreground',
          )}
        >
          {tab.label}
        </button>
      ))}
      {rightContent && (
        <div className="ml-auto flex items-center gap-2">
          {rightContent}
        </div>
      )}
    </div>
  );
}
```

Note: `-mb-px` pulls each button 1 px down so its 2 px active border overlaps the container's 1 px `border-b`, creating the flush underline effect.

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/request/BrunoTabBar.tsx
git commit -m "feat(request): add BrunoTabBar flat underline tab component"
```

---

## Task 2: Remove mode selector from BodyEditor

**Files:**
- Modify: `src/components/request/BodyEditor.tsx`

- [ ] **Step 1: Remove the mode selector block and its supporting code**

Open `src/components/request/BodyEditor.tsx`. Make the following changes:

**Remove** the `MODES` constant (lines 30–37):
```tsx
// DELETE THIS BLOCK:
const MODES: { label: string; value: BodyMode }[] = [
  { label: 'None', value: 'none' },
  { label: 'JSON', value: 'json' },
  { label: 'XML', value: 'xml' },
  { label: 'Text', value: 'text' },
  { label: 'Form Data', value: 'formdata' },
  { label: 'Binary', value: 'binary' },
];
```

**Remove** the `BodyMode` type alias (line 23):
```tsx
// DELETE THIS LINE:
type BodyMode = BodyState['mode'];
```

**Remove** the `setMode` callback (lines 40–43):
```tsx
// DELETE THIS BLOCK:
const setMode = useCallback(
  (mode: BodyMode) => onChange({ ...body, mode }),
  [body, onChange],
);
```

**Remove** the mode selector `<div>` (lines 77–90 in the original, the block under `{/* Mode selector dropdown. */}`):
```tsx
// DELETE THIS BLOCK:
{/* Mode selector dropdown. */}
<div className="flex items-center gap-2 shrink-0">
  <Select value={body.mode} onValueChange={(val) => setMode(val as BodyMode)}>
    <SelectTrigger className="w-[140px] h-7 text-xs">
      <SelectValue />
    </SelectTrigger>
    <SelectContent>
      {MODES.map((m) => (
        <SelectItem key={m.value} value={m.value} className="text-xs">
          {m.label}
        </SelectItem>
      ))}
    </SelectContent>
  </Select>
</div>
```

**Remove** all Select imports (no other Select usage remains in this file after the above change):
```tsx
// DELETE THESE LINES from the import block:
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
```

The final `BodyEditor.tsx` should look like this:

```tsx
import { useCallback, lazy, Suspense } from 'react';
import { FileUp } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import type { BodyState, KeyValueEntry } from '@/types/pane-types';
import { KeyValueEditor } from './KeyValueEditor';

// Lazy-load Monaco so it stays out of the initial JS bundle.
const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({
    default: m.MonacoWrapper,
  })),
);

interface BodyEditorProps {
  body: BodyState;
  onChange: (body: BodyState) => void;
}

export function BodyEditor({ body, onChange }: BodyEditorProps) {
  const setContent = useCallback(
    (content: string) => onChange({ ...body, content }),
    [body, onChange],
  );

  const setFormData = useCallback(
    (formData: KeyValueEntry[]) => onChange({ ...body, formData }),
    [body, onChange],
  );

  const handlePickFile = useCallback(async () => {
    const result = await open({
      multiple: false,
      title: 'Select file for request body',
    });
    if (result) {
      const path = result as string;
      onChange({
        ...body,
        filePath: path,
        fileName: path.split('/').pop() ?? 'unknown',
      });
    }
  }, [body, onChange]);

  const handleClear = useCallback(() => {
    onChange({ ...body, filePath: undefined, fileName: undefined });
  }, [body, onChange]);

  return (
    <div className="flex h-full flex-col space-y-2">
      {/* Content area — fills remaining height. */}
      {body.mode === 'none' && (
        <div className="flex items-center justify-center h-32 text-muted-foreground text-sm">
          No body content
        </div>
      )}

      {(body.mode === 'json' || body.mode === 'xml' || body.mode === 'text') && (
        <div className="flex-1 border rounded min-h-[200px]">
          <Suspense
            fallback={
              <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
                Loading editor...
              </div>
            }
          >
            <MonacoWrapper
              value={body.content}
              onChange={(val) => setContent(val)}
              bodyMode={body.mode}
              height="100%"
            />
          </Suspense>
        </div>
      )}

      {body.mode === 'formdata' && (
        <KeyValueEditor
          entries={body.formData}
          onChange={setFormData}
          keyPlaceholder="Field name"
          valuePlaceholder="Value"
          addLabel="Add Field"
        />
      )}

      {body.mode === 'binary' && (
        body.filePath ? (
          <Card className="max-w-sm">
            <CardContent className="flex items-center gap-3 p-4">
              <FileUp className="size-5 shrink-0 text-muted-foreground" />
              <span className="flex-1 truncate text-sm">{body.fileName}</span>
              <Button variant="ghost" size="sm" onClick={handleClear}>
                Clear
              </Button>
            </CardContent>
          </Card>
        ) : (
          <Button variant="outline" onClick={handlePickFile}>
            <FileUp className="mr-2 size-4" />
            Choose file
          </Button>
        )
      )}
    </div>
  );
}
```

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/request/BodyEditor.tsx
git commit -m "refactor(request): move body mode selector out of BodyEditor to RequestPanel"
```

---

## Task 3: Remove auth type selector from AuthEditor

**Files:**
- Modify: `src/components/request/AuthEditor.tsx`

- [ ] **Step 1: Remove auth type selector and all supporting code**

Open `src/components/request/AuthEditor.tsx`. Apply the following removals:

**Remove** the `AuthType` type alias (line 17):
```tsx
// DELETE THIS LINE:
type AuthType = AuthState['authType'];
```

**Remove** `BASE_AUTH_TYPES` constant (lines 27–34):
```tsx
// DELETE THIS BLOCK:
const BASE_AUTH_TYPES: { label: string; value: AuthType }[] = [
  { label: 'None', value: 'none' },
  { label: 'Basic', value: 'basic' },
  { label: 'Bearer', value: 'bearer' },
  { label: 'API Key', value: 'api-key' },
  { label: 'OAuth 2.0', value: 'oauth2' },
  { label: 'AWS Sig v4', value: 'aws-sig-v4' },
];
```

**Remove** `INHERIT_OPTION` constant (lines 36–39):
```tsx
// DELETE THIS BLOCK:
const INHERIT_OPTION: { label: string; value: AuthType } = {
  label: 'Inherit from parent', value: 'inherit',
};
```

**Update** `AuthEditorProps` — remove the `showInherit` prop:
```tsx
// BEFORE:
interface AuthEditorProps {
  auth: AuthState;
  onChange: (auth: AuthState) => void;
  /** Show "Inherit from parent" option. Use for request-level auth only. */
  showInherit?: boolean;
}

// AFTER:
interface AuthEditorProps {
  auth: AuthState;
  onChange: (auth: AuthState) => void;
}
```

**Update** function signature — remove `showInherit` parameter and the `authTypes` derivation:
```tsx
// BEFORE:
export function AuthEditor({ auth, onChange, showInherit = false }: AuthEditorProps) {
  const authTypes = showInherit ? [INHERIT_OPTION, ...BASE_AUTH_TYPES] : BASE_AUTH_TYPES;
  const setType = useCallback(
    (authType: AuthType) => {
      const next: AuthState = { authType };
      if (authType === 'basic') next.basic = { username: '', password: '' };
      if (authType === 'bearer') next.bearer = { token: '' };
      if (authType === 'api-key')
        next.apiKey = { key: '', value: '', addTo: 'header' };
      if (authType === 'oauth2')
        next.oauth2 = {
          grantType: 'client_credentials',
          authorizationUrl: '',
          tokenUrl: '',
          callbackUrl: 'https://exchange4all.local/webapp/#oidc-callback',
          clientId: '',
          clientSecret: '',
          scope: '',
          state: '',
          username: '',
          password: '',
          clientAuthentication: 'body',
          headerPrefix: 'Bearer',
          addTokenTo: 'header',
          verifySsl: true,
          accessToken: '',
          refreshToken: '',
          expiresIn: null,
          tokenAcquiredAt: null,
        };
      if (authType === 'aws-sig-v4')
        next.awsSigV4 = {
          accessKey: '',
          secretKey: '',
          region: '',
          service: '',
          sessionToken: '',
        };
      onChange(next);
    },
    [onChange],
  );

// AFTER:
export function AuthEditor({ auth, onChange }: AuthEditorProps) {
```

**Remove** the auth type `<Select>` block at the top of the JSX (inside the returned `<div className="space-y-4">`):
```tsx
// DELETE THIS BLOCK:
{/* Auth type selector. */}
<Select value={auth.authType} onValueChange={(val) => setType(val as AuthType)}>
  <SelectTrigger className="w-[200px] text-sm">
    <SelectValue />
  </SelectTrigger>
  <SelectContent>
    {authTypes.map((t) => (
      <SelectItem key={t.value} value={t.value} className="text-sm">
        {t.label}
      </SelectItem>
    ))}
  </SelectContent>
</Select>
```

The `{/* Inherit: ... */}`, `{/* None: ... */}`, and all other type-specific sections remain. `Select` imports stay because they are used throughout the OAuth2 and API Key sections.

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors. (RequestPanel still passes `showInherit` at this point — that will be cleaned up in Task 4.)

- [ ] **Step 3: Commit**

```bash
git add src/components/request/AuthEditor.tsx
git commit -m "refactor(request): move auth type selector out of AuthEditor to RequestPanel"
```

---

## Task 4: Wire BrunoTabBar in RequestPanel

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

This is the largest task. Read the current file before editing. The changes are:
1. Update imports
2. Add body mode and auth type data + handler
3. Build tab definitions with badges and dots
4. Build `rightContent` for body and auth tabs
5. Replace the `Tabs` root + `TabsList`/`TabsTrigger`/`TabsContent` block with `BrunoTabBar` + conditional renders

- [ ] **Step 1: Update imports**

Replace the tabs import block and add new imports. The final import section should be:

```tsx
import { useState, useCallback, useRef, useEffect, useMemo } from 'react';
import { Send, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { RocketLiftOff } from '@/components/illustrations';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { cn } from '@/lib/utils';
import { usePaneStore } from '@/stores/pane-store';
import { parseQueryParams, buildUrl, splitUrl, extractPathParams } from '@/lib/url-params';
import { useExecuteRequest } from '@/hooks/useExecuteRequest';
import { QueryParamsEditor } from './QueryParamsEditor';
import { PathParamsPanel } from './PathParamsPanel';
import { HeadersEditor } from './HeadersEditor';
import { BodyEditor } from './BodyEditor';
import { AuthEditor } from './AuthEditor';
import { ResponseBodyViewer } from '@/components/response/ResponseBodyViewer';
import { SaveRequestButton } from './SaveRequestButton';
import { VariableAwareUrlInput } from './VariableAwareUrlInput';
import { BrunoTabBar } from './BrunoTabBar';
import { METHOD_TEXT_COLOR } from '@/lib/colors';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import type {
  RequestTab,
  HttpMethod,
  KeyValueEntry,
  BodyState,
  AuthState,
} from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';
import { findTabInTree } from '@/lib/pane-utils';
import type { ParsedCurl } from '@/lib/curl-parser';
import { getCollectionSettings } from '@/lib/tauri-api';
```

Note: `Tabs`, `TabsList`, `TabsTrigger`, `TabsContent` are removed. `BodyState` and `AuthState` are added.

- [ ] **Step 2: Add body mode data constant and auth type data constant**

After the `METHODS` constant (around line 47) and before the `SectionTab` type, add:

```tsx
const BODY_MODES: { label: string; value: BodyState['mode'] }[] = [
  { label: 'None', value: 'none' },
  { label: 'JSON', value: 'json' },
  { label: 'XML', value: 'xml' },
  { label: 'Text', value: 'text' },
  { label: 'Form Data', value: 'formdata' },
  { label: 'Binary', value: 'binary' },
];

const BASE_AUTH_TYPES: { label: string; value: AuthState['authType'] }[] = [
  { label: 'None', value: 'none' },
  { label: 'Basic', value: 'basic' },
  { label: 'Bearer', value: 'bearer' },
  { label: 'API Key', value: 'api-key' },
  { label: 'OAuth 2.0', value: 'oauth2' },
  { label: 'AWS Sig v4', value: 'aws-sig-v4' },
];

const INHERIT_AUTH_OPTION = { label: 'Inherit from parent', value: 'inherit' as AuthState['authType'] };
```

- [ ] **Step 3: Add handleAuthTypeChange inside the component function**

Inside `RequestPanel`, after the existing hooks (after `queryParamMap` memoization), add:

```tsx
const authTypeOptions = useMemo(
  () => (tab.source ? [INHERIT_AUTH_OPTION, ...BASE_AUTH_TYPES] : BASE_AUTH_TYPES),
  [tab.source],
);

const handleAuthTypeChange = useCallback(
  (authType: AuthState['authType']) => {
    const next: AuthState = { authType };
    if (authType === 'basic') next.basic = { username: '', password: '' };
    if (authType === 'bearer') next.bearer = { token: '' };
    if (authType === 'api-key') next.apiKey = { key: '', value: '', addTo: 'header' };
    if (authType === 'oauth2')
      next.oauth2 = {
        grantType: 'client_credentials',
        authorizationUrl: '',
        tokenUrl: '',
        callbackUrl: 'https://exchange4all.local/webapp/#oidc-callback',
        clientId: '',
        clientSecret: '',
        scope: '',
        state: '',
        username: '',
        password: '',
        clientAuthentication: 'body',
        headerPrefix: 'Bearer',
        addTokenTo: 'header',
        verifySsl: true,
        accessToken: '',
        refreshToken: '',
        expiresIn: null,
        tokenAcquiredAt: null,
      };
    if (authType === 'aws-sig-v4')
      next.awsSigV4 = { accessKey: '', secretKey: '', region: '', service: '', sessionToken: '' };
    updateRequest(tab.id, { auth: next });
  },
  [tab.id, updateRequest],
);
```

- [ ] **Step 4: Build tab definitions and right-content memos**

Add after `handleAuthTypeChange`:

```tsx
const tabDefs = useMemo(
  () => [
    {
      value: 'params',
      label: (
        <>
          Params
          {enabledParamCount > 0 && (
            <span className="ml-1 inline-flex items-center justify-center min-w-[18px] h-[18px] rounded-full bg-muted px-1.5 text-xs font-semibold">
              {enabledParamCount}
            </span>
          )}
        </>
      ),
      isActive: activeSection === 'params',
      onClick: () => setActiveSection('params'),
    },
    {
      value: 'headers',
      label: (
        <>
          Headers
          {enabledHeaderCount > 0 && (
            <span className="ml-1 inline-flex items-center justify-center min-w-[18px] h-[18px] rounded-full bg-muted px-1.5 text-xs font-semibold">
              {enabledHeaderCount}
            </span>
          )}
        </>
      ),
      isActive: activeSection === 'headers',
      onClick: () => setActiveSection('headers'),
    },
    {
      value: 'body',
      label: (
        <>
          Body
          {request.body.mode !== 'none' && (
            <span className="ml-1.5 inline-block w-1.5 h-1.5 rounded-full bg-primary" />
          )}
        </>
      ),
      isActive: activeSection === 'body',
      onClick: () => setActiveSection('body'),
    },
    {
      value: 'auth',
      label: (
        <>
          Auth
          {request.auth.authType !== 'none' && (
            <span className="ml-1.5 inline-block w-1.5 h-1.5 rounded-full bg-primary" />
          )}
        </>
      ),
      isActive: activeSection === 'auth',
      onClick: () => setActiveSection('auth'),
    },
  ],
  [activeSection, enabledParamCount, enabledHeaderCount, request.body.mode, request.auth.authType],
);

const tabRightContent = useMemo(() => {
  if (activeSection === 'body') {
    return (
      <Select
        value={request.body.mode}
        onValueChange={(val) =>
          updateRequest(tab.id, { body: { ...request.body, mode: val as BodyState['mode'] } })
        }
      >
        <SelectTrigger className="h-7 w-[120px] text-xs">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {BODY_MODES.map((m) => (
            <SelectItem key={m.value} value={m.value} className="text-xs">
              {m.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    );
  }
  if (activeSection === 'auth') {
    return (
      <Select
        value={request.auth.authType}
        onValueChange={(val) => handleAuthTypeChange(val as AuthState['authType'])}
      >
        <SelectTrigger className="h-7 w-[160px] text-xs">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {authTypeOptions.map((t) => (
            <SelectItem key={t.value} value={t.value} className="text-xs">
              {t.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    );
  }
  return undefined;
}, [activeSection, request.body, request.auth.authType, tab.id, updateRequest, handleAuthTypeChange, authTypeOptions]);
```

- [ ] **Step 5: Replace the Tabs block in JSX**

Find the `{/* Section tabs. */}` block in the JSX, which currently looks like:

```tsx
{/* Section tabs. */}
<Tabs
  value={activeSection}
  onValueChange={(val) => setActiveSection(val as SectionTab)}
  className="flex-1 flex flex-col min-h-0"
>
  <TabsList>
    <TabsTrigger value="params">
      Params
      {enabledParamCount > 0 && (
        <span className="ml-1 text-2xs text-muted-foreground">
          ({enabledParamCount})
        </span>
      )}
    </TabsTrigger>
    <TabsTrigger value="headers">
      Headers
      {enabledHeaderCount > 0 && (
        <span className="ml-1 text-2xs text-muted-foreground">
          ({enabledHeaderCount})
        </span>
      )}
    </TabsTrigger>
    <TabsTrigger value="body">Body</TabsTrigger>
    <TabsTrigger value="auth">Auth</TabsTrigger>
  </TabsList>

  <div className="flex-1 overflow-auto p-3">
    <TabsContent value="params" className="mt-0 h-full">
      <div className="space-y-2">
        <PathParamsPanel
          params={request.pathParams}
          onChange={(pathParams) => updateRequest(tab.id, { pathParams })}
        />
        <QueryParamsEditor params={request.queryParams} onChange={handleParamsChange} />
      </div>
    </TabsContent>
    <TabsContent value="headers" className="mt-0 h-full">
      <HeadersEditor
        headers={request.headers}
        onChange={(headers) => updateRequest(tab.id, { headers })}
      />
    </TabsContent>
    <TabsContent value="body" className="mt-0 h-full">
      <BodyEditor
        body={request.body}
        onChange={(body) => updateRequest(tab.id, { body })}
      />
    </TabsContent>
    <TabsContent value="auth" className="mt-0 h-full">
      <AuthEditor
        auth={request.auth}
        onChange={(auth) => updateRequest(tab.id, { auth })}
        showInherit={!!tab.source}
      />
    </TabsContent>
  </div>
</Tabs>
```

Replace it with:

```tsx
{/* Section tabs. */}
<div className="flex-1 flex flex-col min-h-0">
  <BrunoTabBar tabs={tabDefs} rightContent={tabRightContent} />
  <div className="flex-1 overflow-auto p-3">
    {activeSection === 'params' && (
      <div className="space-y-2">
        <PathParamsPanel
          params={request.pathParams}
          onChange={(pathParams) => updateRequest(tab.id, { pathParams })}
        />
        <QueryParamsEditor params={request.queryParams} onChange={handleParamsChange} />
      </div>
    )}
    {activeSection === 'headers' && (
      <HeadersEditor
        headers={request.headers}
        onChange={(headers) => updateRequest(tab.id, { headers })}
      />
    )}
    {activeSection === 'body' && (
      <BodyEditor
        body={request.body}
        onChange={(body) => updateRequest(tab.id, { body })}
      />
    )}
    {activeSection === 'auth' && (
      <AuthEditor
        auth={request.auth}
        onChange={(auth) => updateRequest(tab.id, { auth })}
      />
    )}
  </div>
</div>
```

Note: `showInherit` prop is removed from `<AuthEditor>` because the type selector (which used `showInherit`) now lives in `RequestPanel`.

- [ ] **Step 6: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 7: Build check**

```bash
yarn build
```

Expected: successful build with no errors.

- [ ] **Step 8: Commit**

```bash
git add src/components/request/RequestPanel.tsx
git commit -m "feat(request): replace shadcn tabs with Bruno-style flat underline tab bar"
```

---

## Manual Smoke Test (after all tasks complete)

1. Open the app (`yarn tauri dev`) and open a request tab.
2. Verify the tab bar renders as a flat underline strip — no pill background, no rounded container.
3. Active tab shows a bold label and a colored bottom border. Inactive tabs show muted text.
4. Add at least one query param — count badge `(n)` appears on the Params tab.
5. Add at least one header — count badge appears on the Headers tab.
6. Switch to Body, change mode to JSON — status dot `●` appears on Body tab; mode Select (showing "JSON") is visible at the right end of the tab bar. Switch to another tab and back — mode persists.
7. Switch to Auth, select Bearer — status dot appears on Auth tab; type Select (showing "Bearer") is visible at the right end of the tab bar. Enter a token value — it persists.
8. Verify Body and Auth type selectors are gone from inside the content areas.
9. Light mode and dark mode — tab colors use CSS variables, dot and border use `primary` color in both modes.
10. Keyboard nav — pressing Tab moves focus to the next tab button.
