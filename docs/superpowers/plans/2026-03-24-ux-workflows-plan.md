# UX Workflows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire up UI stubs, build the environment variable system, and add collection CRUD with auto-save to make Rocket a daily-driver API client.

**Architecture:** Three features delivered sequentially. Feature 1 (stubs) patches existing components. Feature 2 (environments) adds a new Zustand store + two new components + variable substitution in the request execution path. Feature 3 (collection CRUD) adds context menus and hover icons to the sidebar, auto-save side effects in the pane store, and a "Save to Collection" dialog.

**Tech Stack:** React 19, TypeScript, Zustand, shadcn/ui (Radix), Tailwind CSS 4, Tauri 2.0 IPC, Rust DDD backend

**Spec:** `docs/superpowers/specs/2026-03-24-ux-workflows-design.md`

---

## File Structure

### New Files
| File | Responsibility |
|------|---------------|
| `src/lib/execute-request.ts` | Standalone request execution function (usable from hooks and keyboard handlers) |
| `src/hooks/useExecuteRequest.ts` | Thin React hook wrapper around `execute-request.ts` (provides `sending` state) |
| `src/stores/env-store.ts` | Environment Zustand store (CRUD, active env, variable resolution) |
| `src/components/layout/EnvironmentSwitcher.tsx` | Header dropdown for switching active environment |
| `src/components/environments/EnvironmentDialog.tsx` | Environment CRUD dialog with key-value editor |
| `src/components/collections/SaveToCollectionDialog.tsx` | Save draft tab to collection picker |
| `src/lib/auto-save.ts` | Debounced auto-save logic for collection requests |

### Modified Files
| File | Changes |
|------|---------|
| `src/components/request/RequestPanel.tsx` | Extract send logic to `useExecuteRequest`, add env variable resolution |
| `src/hooks/useKeyboardShortcuts.ts` | Wire Cmd+Enter (send), add Cmd+S (save) |
| `src/components/request/AuthEditor.tsx` | Wire OAuth "Get Token" button |
| `src/components/collections/CollectionSettingsDialog.tsx` | Wire `handleSave` to Tauri |
| `src/components/layout/CollectionsSidebar.tsx` | New Collection handler, hover icons, context menus, inline rename |
| `src/components/layout/Header.tsx` | Add EnvironmentSwitcher component |
| `src/stores/pane-store.ts` | Auto-save side effect in `updateRequest`, debounce cleanup in `closeTab` |
| `src/lib/tauri-api.ts` | Add `saveCollectionSettings` function |
| `src-tauri/src/commands/collections.rs` | Add `save_collection_settings` Tauri command |
| `src-tauri/src/lib.rs` | Register `save_collection_settings` command |
| `crates/rocket-app/src/collection_service.rs` | Add `save_settings` public method |

---

## Feature 1: Wire Up Stubs

### Task 1: Extract shared request execution and wire Cmd+Enter

**Files:**
- Create: `src/lib/execute-request.ts`
- Create: `src/hooks/useExecuteRequest.ts`
- Modify: `src/components/request/RequestPanel.tsx:68-88, 90-106, 127, 192-237`
- Modify: `src/hooks/useKeyboardShortcuts.ts:16-21`

- [ ] **Step 1: Create standalone `sendRequest` in `src/lib/execute-request.ts`**

This is a non-hook function callable from both React components and plain event handlers:

```typescript
// src/lib/execute-request.ts
import { usePaneStore } from '@/stores/pane-store';
import {
  executeRequest,
  type Auth,
  type Body,
  type Header,
} from '@/lib/tauri-api';
import type {
  AuthState,
  BodyState,
  RequestState,
  ResponseState,
} from '@/types/pane-types';

function toApiAuth(auth: AuthState): Auth {
  switch (auth.authType) {
    case 'basic':
      return {
        authType: 'basic',
        username: auth.basic?.username ?? '',
        password: auth.basic?.password ?? '',
      };
    case 'bearer':
      return { authType: 'bearer', token: auth.bearer?.token ?? '' };
    case 'api-key':
      return {
        authType: 'api-key',
        key: auth.apiKey?.key ?? '',
        value: auth.apiKey?.value ?? '',
        addTo: auth.apiKey?.addTo ?? 'header',
      };
    case 'oauth2':
      // Send the stored access token as a bearer token.
      return {
        authType: 'bearer',
        token: auth.oauth2?.accessToken ?? '',
      };
    default:
      return { authType: 'none' };
  }
}

function toApiBody(body: BodyState): Body | undefined {
  if (body.mode === 'none') return undefined;
  if (body.mode === 'formdata') {
    return {
      mode: 'formdata',
      formData: body.formData
        .filter((e) => e.enabled)
        .map((e) => ({
          key: e.key,
          value: e.value,
          entryType: 'text' as const,
          enabled: e.enabled,
        })),
    };
  }
  return { mode: body.mode as Body['mode'], content: body.content };
}

export async function sendRequest(tabId: string, request: RequestState): Promise<void> {
  const headers: Header[] = request.headers
    .filter((h) => h.enabled)
    .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled }));

  try {
    const result = await executeRequest({
      method: request.method,
      url: request.url,
      headers,
      body: toApiBody(request.body),
      auth: toApiAuth(request.auth),
      options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
    });

    const responseState: ResponseState = {
      status: result.status,
      statusText: result.statusText,
      headers: result.headers.map((h) => ({
        id: crypto.randomUUID(),
        key: h.key,
        value: h.value,
        enabled: h.enabled,
      })),
      body: result.body,
      durationMs: result.durationMs,
      sizeBytes: result.sizeBytes,
      activeView: 'pretty',
    };
    usePaneStore.getState().setResponse(tabId, responseState);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    usePaneStore.getState().setResponse(tabId, {
      status: 0,
      statusText: 'Error',
      headers: [],
      body: msg,
      durationMs: 0,
      sizeBytes: msg.length,
      activeView: 'raw',
    });
  }
}
```

- [ ] **Step 2: Create thin React hook wrapper in `src/hooks/useExecuteRequest.ts`**

```typescript
// src/hooks/useExecuteRequest.ts
import { useState, useCallback } from 'react';
import { sendRequest } from '@/lib/execute-request';
import type { RequestState } from '@/types/pane-types';

export function useExecuteRequest(tabId: string) {
  const [sending, setSending] = useState(false);

  const send = useCallback(async (request: RequestState) => {
    setSending(true);
    try {
      await sendRequest(tabId, request);
    } finally {
      setSending(false);
    }
  }, [tabId]);

  return { send, sending };
}
```

- [ ] **Step 3: Update `RequestPanel.tsx` to use the hook**

- Remove the `toApiAuth` function (lines 68-88)
- Remove the `toApiBody` function (lines 90-106)
- Remove the `sending` state and `handleSend` callback (lines 127, 192-237)
- Add: `const { send, sending } = useExecuteRequest(tab.id);`
- Add import: `import { useExecuteRequest } from '@/hooks/useExecuteRequest';`
- Update the Send button's `onClick` to: `onClick={() => send(request)}`
- Update the URL input `onKeyDown` to: `if (e.key === 'Enter') send(request);`

- [ ] **Step 4: Wire Cmd+Enter in `useKeyboardShortcuts.ts`**

Replace the Cmd+Enter stub (lines 16-21):

```typescript
// Cmd/Ctrl+Enter — send the active tab's request.
if (e.key === 'Enter') {
  e.preventDefault();
  const tab = activeLeaf.tabs.find((t) => t.id === activeLeaf.activeTabId);
  if (tab) {
    sendRequest(tab.id, tab.request);
  }
  return;
}
```

Add the import at the top:
```typescript
import { sendRequest } from '@/lib/execute-request';
```

- [ ] **Step 5: Verify build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`
Expected: Build succeeds

- [ ] **Step 6: Commit**

```bash
git add src/lib/execute-request.ts src/hooks/useExecuteRequest.ts src/hooks/useKeyboardShortcuts.ts src/components/request/RequestPanel.tsx
git commit -m "feat: extract shared execute-request module, wire Cmd+Enter to send"
```

---

### Task 2: Wire "New Collection" button

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx:309-316`

- [ ] **Step 1: Add inline creation state and handler**

In `CollectionsSidebar`, add state for inline creation mode and the handler. Add these inside the `CollectionsSidebar` component function:

```typescript
const [isCreating, setIsCreating] = useState(false);
const [newName, setNewName] = useState('');
const [createError, setCreateError] = useState('');

const INVALID_CHARS = /[/\\:*?"<>|]/;

const handleCreateCollection = useCallback(async () => {
  const trimmed = newName.trim();
  if (!trimmed) {
    setIsCreating(false);
    setNewName('');
    return;
  }
  if (INVALID_CHARS.test(trimmed)) {
    setCreateError('Name contains invalid characters.');
    return;
  }
  try {
    await createCollection(trimmed);
    setIsCreating(false);
    setNewName('');
    setCreateError('');
  } catch (err) {
    setCreateError(err instanceof Error ? err.message : 'Failed to create collection.');
  }
}, [newName]);
```

Add `createCollection` to the imports from `@/lib/tauri-api`.

- [ ] **Step 2: Replace the Button with inline input when creating**

Replace the `<Button>` at line 309-316 with a conditional:

```tsx
{isCreating ? (
  <div className="px-1">
    <Input
      autoFocus
      className="h-7 text-xs"
      placeholder="Collection name"
      value={newName}
      onChange={(e) => { setNewName(e.target.value); setCreateError(''); }}
      onKeyDown={(e) => {
        if (e.key === 'Enter') handleCreateCollection();
        if (e.key === 'Escape') { setIsCreating(false); setNewName(''); setCreateError(''); }
      }}
      onBlur={() => { setIsCreating(false); setNewName(''); setCreateError(''); }}
    />
    {createError && (
      <p className="text-[10px] text-destructive mt-0.5 px-1">{createError}</p>
    )}
  </div>
) : (
  <Button
    variant="ghost"
    size="sm"
    className="w-full justify-start h-7 text-xs text-muted-foreground hover:text-foreground"
    onClick={() => setIsCreating(true)}
  >
    <Plus className="h-3.5 w-3.5 mr-1.5" />
    New Collection
  </Button>
)}
```

- [ ] **Step 3: Verify build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "feat: wire New Collection button with inline name input"
```

---

### Task 3: Wire collection settings save (Rust + frontend)

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs`
- Modify: `src-tauri/src/commands/collections.rs`
- Modify: `src-tauri/src/lib.rs:103`
- Create: `src/lib/tauri-api.ts` (add function)
- Modify: `src/components/collections/CollectionSettingsDialog.tsx:31-35`

- [ ] **Step 1: Add `save_settings` to `CollectionService`**

Append to `crates/rocket-app/src/collection_service.rs` inside the `impl CollectionService` block:

```rust
pub fn save_settings(
    &self,
    name: &str,
    settings: &rocket_collection::CollectionSettings,
) -> DomainResult<()> {
    self.repo.save_settings(name, settings)?;
    // Use RequestSaved event to trigger collection-changed in the frontend.
    // There is no dedicated SettingsChanged variant yet.
    self.events.publish(DomainEvent::RequestSaved {
        collection: name.to_string(),
        path: "collection.json".to_string(),
    });
    Ok(())
}
```

- [ ] **Step 2: Add Tauri command**

Append to `src-tauri/src/commands/collections.rs`:

```rust
#[tauri::command]
pub fn save_collection_settings(
    collection: String,
    settings: rocket_collection::CollectionSettings,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.save_settings(&collection, &settings)
}
```

- [ ] **Step 3: Register command in `lib.rs`**

Add `commands::collections::save_collection_settings,` to the `invoke_handler` macro in `src-tauri/src/lib.rs` after the `move_item` line.

- [ ] **Step 4: Add TypeScript bridge function**

Add to `src/lib/tauri-api.ts` after the `moveItem` function:

```typescript
export const saveCollectionSettings = (
  collection: string,
  settings: { auth?: any; headers?: { key: string; value: string; enabled: boolean }[] },
) => invoke<void>("save_collection_settings", { collection, settings });
```

- [ ] **Step 5: Wire `handleSave` in `CollectionSettingsDialog.tsx`**

Replace `handleSave` (lines 31-35):

```typescript
async function handleSave() {
  try {
    await saveCollectionSettings(collectionName, {
      auth: auth.authType !== 'none' ? auth : undefined,
      headers: headers.filter((h) => h.key),
    });
    onClose();
  } catch (err) {
    console.error('[CollectionSettings] save failed', err);
  }
}
```

Add import: `import { saveCollectionSettings } from '@/lib/tauri-api';`

- [ ] **Step 6: Verify Rust compiles**

Run: `cd /home/numericlabs/data/Rust/Rocket && cargo check --workspace`
Expected: Compiles with no errors

- [ ] **Step 7: Verify frontend builds**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-app/src/collection_service.rs src-tauri/src/commands/collections.rs src-tauri/src/lib.rs src/lib/tauri-api.ts src/components/collections/CollectionSettingsDialog.tsx
git commit -m "feat: wire collection settings save through Tauri to Rust backend"
```

---

### Task 4: Wire OAuth 2.0 "Get Token" button

**Files:**
- Modify: `src/components/request/AuthEditor.tsx:289-297`

- [ ] **Step 1: Add the token fetch handler**

Add this function inside the `AuthEditor` component, after the `patchAWS` callback:

```typescript
const [tokenError, setTokenError] = useState('');

const handleGetToken = useCallback(async () => {
  const oauth = auth.oauth2;
  if (!oauth || !oauth.tokenUrl) return;
  if (oauth.grantType === 'authorization_code') return;

  setTokenError('');
  const params = new URLSearchParams();
  params.set('grant_type', oauth.grantType);
  params.set('client_id', oauth.clientId);
  params.set('client_secret', oauth.clientSecret);
  // Note: password grant reuses clientId/clientSecret as username/password.
  // This is a v1 limitation; dedicated username/password fields can be added later.
  if (oauth.grantType === 'password') {
    params.set('username', oauth.clientId);
    params.set('password', oauth.clientSecret);
  }
  if (oauth.scope) params.set('scope', oauth.scope);

  try {
    const result = await executeRequest({
      method: 'POST',
      url: oauth.tokenUrl,
      headers: [{ key: 'Content-Type', value: 'application/x-www-form-urlencoded', enabled: true }],
      body: { mode: 'text', content: params.toString() },
      auth: { authType: 'none' },
      options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
    });

    const json = JSON.parse(result.body);
    if (json.error) {
      setTokenError(json.error_description || json.error);
      return;
    }
    patchOAuth2({
      accessToken: json.access_token ?? '',
      refreshToken: json.refresh_token ?? '',
    });
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    setTokenError(msg);
    patchOAuth2({ accessToken: '' });
  }
}, [auth.oauth2, patchOAuth2]);
```

Add imports at top:
```typescript
import { useState } from 'react';
import { executeRequest } from '@/lib/tauri-api';
```

Display `tokenError` below the Get Token button row:
```tsx
{tokenError && (
  <p className="text-[11px] text-destructive mt-1">{tokenError}</p>
)}
```

- [ ] **Step 2: Wire the button and add authorization_code disable**

Replace the "Get Token" button (line 289-297):

```tsx
<Button
  type="button"
  variant="outline"
  size="sm"
  className="h-8 shrink-0 px-2 text-xs"
  disabled={auth.oauth2.grantType === 'authorization_code' || !auth.oauth2.tokenUrl}
  onClick={handleGetToken}
  title={
    auth.oauth2.grantType === 'authorization_code'
      ? 'Authorization code flow coming soon.'
      : undefined
  }
>
  Get Token
</Button>
```

- [ ] **Step 3: Verify build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 4: Commit**

```bash
git add src/components/request/AuthEditor.tsx
git commit -m "feat: wire OAuth 2.0 Get Token for client_credentials and password grants"
```

---

## Feature 2: Environment System

### Task 5: Create environment Zustand store

**Files:**
- Create: `src/stores/env-store.ts`

- [ ] **Step 1: Create the store**

```typescript
// src/stores/env-store.ts
import { create } from 'zustand';
import {
  listEnvironments,
  saveEnvironment,
  deleteEnvironment as deleteEnvApi,
  type Environment,
} from '@/lib/tauri-api';

const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;

export interface EnvState {
  environments: Environment[];
  activeEnvId: string | null;

  loadEnvironments: () => Promise<void>;
  setActiveEnv: (id: string | null) => void;
  createEnvironment: (name: string) => Promise<void>;
  updateEnvironment: (env: Environment) => Promise<void>;
  deleteEnvironment: (name: string) => Promise<void>;
  getActiveVariables: () => Record<string, string>;
  resolveVariables: (text: string) => string;
}

export const useEnvStore = create<EnvState>((set, get) => ({
  environments: [],
  activeEnvId: null,

  async loadEnvironments() {
    try {
      const environments = await listEnvironments();
      set({ environments });
    } catch (err) {
      console.error('[EnvStore] Failed to load environments:', err);
    }
  },

  setActiveEnv(id) {
    set({ activeEnvId: id });
  },

  async createEnvironment(name) {
    const env: Environment = { name, variables: [] };
    await saveEnvironment(env);
    await get().loadEnvironments();
    set({ activeEnvId: name });
  },

  async updateEnvironment(env) {
    await saveEnvironment(env);
    set((state) => ({
      environments: state.environments.map((e) =>
        e.name === env.name ? env : e,
      ),
    }));
  },

  async deleteEnvironment(name) {
    await deleteEnvApi(name);
    set((state) => ({
      environments: state.environments.filter((e) => e.name !== name),
      activeEnvId: state.activeEnvId === name ? null : state.activeEnvId,
    }));
  },

  getActiveVariables() {
    const { environments, activeEnvId } = get();
    if (!activeEnvId) return {};
    const env = environments.find((e) => e.name === activeEnvId);
    if (!env) return {};
    const vars: Record<string, string> = {};
    for (const v of env.variables) {
      if (v.enabled) vars[v.key] = v.value;
    }
    return vars;
  },

  resolveVariables(text) {
    const vars = get().getActiveVariables();
    return text.replace(VAR_REGEX, (match, key) => {
      return key in vars ? vars[key] : match;
    });
  },
}));
```

- [ ] **Step 2: Verify Rust backend has environment commands registered**

Run: `cd /home/numericlabs/data/Rust/Rocket && cargo check --workspace`

Also verify that `src-tauri/src/lib.rs` contains `commands::environments::list_environments`, `save_environment`, `delete_environment` in the `invoke_handler` macro (they should already be there from SP1).

- [ ] **Step 3: Verify frontend build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 4: Commit**

```bash
git add src/stores/env-store.ts
git commit -m "feat: add environment Zustand store with variable resolution"
```

---

### Task 6: Create EnvironmentSwitcher dropdown

**Files:**
- Create: `src/components/layout/EnvironmentSwitcher.tsx`
- Modify: `src/components/layout/Header.tsx`

- [ ] **Step 1: Create the switcher component**

```typescript
// src/components/layout/EnvironmentSwitcher.tsx
import { useEffect, useState } from 'react';
import { Check, ChevronDown, Settings } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';
import { EnvironmentDialog } from '@/components/environments/EnvironmentDialog';

export function EnvironmentSwitcher() {
  const environments = useEnvStore((s) => s.environments);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const setActiveEnv = useEnvStore((s) => s.setActiveEnv);
  const loadEnvironments = useEnvStore((s) => s.loadEnvironments);

  const [dialogOpen, setDialogOpen] = useState(false);

  useEffect(() => {
    void loadEnvironments();
  }, [loadEnvironments]);

  const activeName = activeEnvId ?? 'No Environment';

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 gap-1.5 px-2 text-xs font-normal"
          >
            <span
              className={cn(
                'h-2 w-2 rounded-full',
                activeEnvId ? 'bg-green-500' : 'bg-muted-foreground/50',
              )}
            />
            <span className="max-w-[120px] truncate">{activeName}</span>
            <ChevronDown className="h-3 w-3 text-muted-foreground" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-56">
          <DropdownMenuItem onClick={() => setActiveEnv(null)}>
            <span className="flex-1">No Environment</span>
            {!activeEnvId && <Check className="h-3.5 w-3.5" />}
          </DropdownMenuItem>
          {environments.length > 0 && <DropdownMenuSeparator />}
          {environments.map((env) => (
            <DropdownMenuItem
              key={env.name}
              onClick={() => setActiveEnv(env.name)}
            >
              <span className="flex-1">{env.name}</span>
              {activeEnvId === env.name && <Check className="h-3.5 w-3.5" />}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={() => setDialogOpen(true)}>
            <Settings className="h-3.5 w-3.5 mr-2" />
            Manage Environments...
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <EnvironmentDialog open={dialogOpen} onOpenChange={setDialogOpen} />
    </>
  );
}
```

Note: This references `EnvironmentDialog` which is created in the next task. For now, create a placeholder to allow compilation.

- [ ] **Step 2: Create placeholder EnvironmentDialog**

```typescript
// src/components/environments/EnvironmentDialog.tsx
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';

interface EnvironmentDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function EnvironmentDialog({ open, onOpenChange }: EnvironmentDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Manage Environments</DialogTitle>
        </DialogHeader>
        <p className="text-sm text-muted-foreground">Coming next...</p>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 3: Add EnvironmentSwitcher to Header**

In `src/components/layout/Header.tsx`, add the switcher between the logo section and the spacer div:

```tsx
// After the logo div (line 17), before <div className="flex-1" />:
<EnvironmentSwitcher />
```

Add import: `import { EnvironmentSwitcher } from '@/components/layout/EnvironmentSwitcher';`

- [ ] **Step 4: Verify build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 5: Commit**

```bash
git add src/components/layout/EnvironmentSwitcher.tsx src/components/environments/EnvironmentDialog.tsx src/components/layout/Header.tsx
git commit -m "feat: add environment switcher dropdown in header bar"
```

---

### Task 7: Build EnvironmentDialog with key-value editor

**Files:**
- Modify: `src/components/environments/EnvironmentDialog.tsx` (full rewrite)

- [ ] **Step 1: Implement the full dialog**

Replace `src/components/environments/EnvironmentDialog.tsx` with the complete implementation:

```typescript
// src/components/environments/EnvironmentDialog.tsx
import { useState, useCallback, useRef } from 'react';
import { Plus, Trash2, Eye, EyeOff, Check, X } from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';
import type { Variable, Environment } from '@/lib/tauri-api';

interface EnvironmentDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function EnvironmentDialog({ open, onOpenChange }: EnvironmentDialogProps) {
  const environments = useEnvStore((s) => s.environments);
  const createEnvironment = useEnvStore((s) => s.createEnvironment);
  const updateEnvironment = useEnvStore((s) => s.updateEnvironment);
  const deleteEnvironment = useEnvStore((s) => s.deleteEnvironment);

  const [selectedName, setSelectedName] = useState<string | null>(
    environments[0]?.name ?? null,
  );
  const [isAddingEnv, setIsAddingEnv] = useState(false);
  const [newEnvName, setNewEnvName] = useState('');

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const selectedEnv = environments.find((e) => e.name === selectedName) ?? null;

  const handleAddEnv = useCallback(async () => {
    const trimmed = newEnvName.trim();
    if (!trimmed) { setIsAddingEnv(false); return; }
    await createEnvironment(trimmed);
    setSelectedName(trimmed);
    setIsAddingEnv(false);
    setNewEnvName('');
  }, [newEnvName, createEnvironment]);

  const handleDeleteEnv = useCallback(async () => {
    if (!selectedName) return;
    await deleteEnvironment(selectedName);
    setSelectedName(environments.find((e) => e.name !== selectedName)?.name ?? null);
  }, [selectedName, deleteEnvironment, environments]);

  // Debounced save after variable edits.
  const saveEnv = useCallback(
    (env: Environment) => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        void updateEnvironment(env);
      }, 500);
    },
    [updateEnvironment],
  );

  const updateVariable = useCallback(
    (idx: number, patch: Partial<Variable>) => {
      if (!selectedEnv) return;
      const variables = selectedEnv.variables.slice();
      variables[idx] = { ...variables[idx], ...patch };
      const updated = { ...selectedEnv, variables };
      // Optimistic update in store.
      useEnvStore.setState((s) => ({
        environments: s.environments.map((e) =>
          e.name === updated.name ? updated : e,
        ),
      }));
      saveEnv(updated);
    },
    [selectedEnv, saveEnv],
  );

  const addVariable = useCallback(() => {
    if (!selectedEnv) return;
    const variable: Variable = { key: '', value: '', enabled: true, secret: false };
    const updated = { ...selectedEnv, variables: [...selectedEnv.variables, variable] };
    useEnvStore.setState((s) => ({
      environments: s.environments.map((e) =>
        e.name === updated.name ? updated : e,
      ),
    }));
    saveEnv(updated);
  }, [selectedEnv, saveEnv]);

  const removeVariable = useCallback(
    (idx: number) => {
      if (!selectedEnv) return;
      const variables = selectedEnv.variables.filter((_, i) => i !== idx);
      const updated = { ...selectedEnv, variables };
      useEnvStore.setState((s) => ({
        environments: s.environments.map((e) =>
          e.name === updated.name ? updated : e,
        ),
      }));
      saveEnv(updated);
    },
    [selectedEnv, saveEnv],
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl p-0 gap-0">
        <DialogHeader className="p-4 pb-2">
          <DialogTitle>Manage Environments</DialogTitle>
        </DialogHeader>
        <div className="flex border-t border-border min-h-[350px]">
          {/* Left panel: environment list. */}
          <div className="w-[200px] border-r border-border flex flex-col">
            <ScrollArea className="flex-1">
              <div className="p-2 space-y-0.5">
                {environments.map((env) => (
                  <button
                    key={env.name}
                    type="button"
                    onClick={() => setSelectedName(env.name)}
                    className={cn(
                      'w-full text-left px-2 py-1.5 text-xs rounded-sm truncate',
                      selectedName === env.name
                        ? 'bg-accent text-accent-foreground'
                        : 'text-foreground hover:bg-muted/60',
                    )}
                  >
                    {env.name}
                  </button>
                ))}
                {isAddingEnv && (
                  <Input
                    autoFocus
                    className="h-7 text-xs"
                    placeholder="Environment name"
                    value={newEnvName}
                    onChange={(e) => setNewEnvName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleAddEnv();
                      if (e.key === 'Escape') { setIsAddingEnv(false); setNewEnvName(''); }
                    }}
                    onBlur={() => { setIsAddingEnv(false); setNewEnvName(''); }}
                  />
                )}
              </div>
            </ScrollArea>
            <div className="p-2 border-t border-border flex gap-1">
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={() => setIsAddingEnv(true)}
                title="Add environment"
              >
                <Plus className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7 text-destructive"
                onClick={handleDeleteEnv}
                disabled={!selectedName}
                title="Delete environment"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>

          {/* Right panel: variable editor. */}
          <div className="flex-1 flex flex-col">
            {selectedEnv ? (
              <>
                <ScrollArea className="flex-1 p-3">
                  <div className="space-y-1.5">
                    {selectedEnv.variables.map((variable, idx) => (
                      <div key={idx} className="flex gap-1.5 items-center">
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => updateVariable(idx, { enabled: !variable.enabled })}
                          className={cn(
                            'w-4 h-4 rounded border p-0 shrink-0',
                            variable.enabled
                              ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
                              : 'border-gray-300 hover:bg-muted',
                          )}
                        >
                          {variable.enabled && <Check className="h-3 w-3" />}
                        </Button>
                        <Input
                          placeholder="Key"
                          value={variable.key}
                          onChange={(e) => updateVariable(idx, { key: e.target.value })}
                          className="flex-1 text-xs h-7"
                        />
                        <Input
                          placeholder="Value"
                          type={variable.secret ? 'password' : 'text'}
                          value={variable.value}
                          onChange={(e) => updateVariable(idx, { value: e.target.value })}
                          className="flex-1 text-xs h-7"
                        />
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6 shrink-0"
                          onClick={() => updateVariable(idx, { secret: !variable.secret })}
                          title={variable.secret ? 'Show value' : 'Hide value'}
                        >
                          {variable.secret ? (
                            <EyeOff className="h-3 w-3" />
                          ) : (
                            <Eye className="h-3 w-3" />
                          )}
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6 shrink-0"
                          onClick={() => removeVariable(idx)}
                        >
                          <X className="h-3 w-3" />
                        </Button>
                      </div>
                    ))}
                  </div>
                </ScrollArea>
                <div className="p-3 pt-0">
                  <Button variant="ghost" size="sm" onClick={addVariable} className="text-xs">
                    <Plus className="h-3 w-3 mr-1" />
                    Add Variable
                  </Button>
                </div>
              </>
            ) : (
              <div className="flex-1 flex items-center justify-center text-xs text-muted-foreground">
                Select or create an environment.
              </div>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Verify build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 3: Commit**

```bash
git add src/components/environments/EnvironmentDialog.tsx
git commit -m "feat: build environment management dialog with key-value variable editor"
```

---

### Task 8: Add variable substitution to request execution

**Files:**
- Modify: `src/lib/execute-request.ts`

- [ ] **Step 1: Add env resolution to `sendRequest`**

In `src/lib/execute-request.ts`, import the env store and resolve variables before execution:

Add import:
```typescript
import { useEnvStore } from '@/stores/env-store';
```

At the top of `sendRequest`, before building headers, resolve all variable fields:

```typescript
export async function sendRequest(tabId: string, request: RequestState): Promise<void> {
  const resolve = useEnvStore.getState().resolveVariables;

  // Resolve environment variables in all request fields.
  const resolvedUrl = resolve(request.url);
  const resolvedHeaders: Header[] = request.headers
    .filter((h) => h.enabled)
    .map((h) => ({ key: resolve(h.key), value: resolve(h.value), enabled: h.enabled }));

  const resolvedBody = toApiBody(request.body, resolve);
  const resolvedAuth = toApiAuth(request.auth, resolve);

  try {
    const result = await executeRequest({
      method: request.method,
      url: resolvedUrl,
      headers: resolvedHeaders,
      body: resolvedBody,
      auth: resolvedAuth,
      options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
    });
    // ... rest stays the same
```

Update `toApiAuth` to accept a resolve function:
```typescript
function toApiAuth(auth: AuthState, resolve = (s: string) => s): Auth {
  switch (auth.authType) {
    case 'basic':
      return {
        authType: 'basic',
        username: resolve(auth.basic?.username ?? ''),
        password: resolve(auth.basic?.password ?? ''),
      };
    case 'bearer':
      return { authType: 'bearer', token: resolve(auth.bearer?.token ?? '') };
    case 'api-key':
      return {
        authType: 'api-key',
        key: resolve(auth.apiKey?.key ?? ''),
        value: resolve(auth.apiKey?.value ?? ''),
        addTo: auth.apiKey?.addTo ?? 'header',
      };
    case 'oauth2':
      return {
        authType: 'bearer',
        token: resolve(auth.oauth2?.accessToken ?? ''),
      };
    default:
      return { authType: 'none' };
  }
}
```

Update `toApiBody` to accept a resolve function:
```typescript
function toApiBody(body: BodyState, resolve = (s: string) => s): Body | undefined {
  if (body.mode === 'none') return undefined;
  if (body.mode === 'formdata') {
    return {
      mode: 'formdata',
      formData: body.formData
        .filter((e) => e.enabled)
        .map((e) => ({
          key: resolve(e.key),
          value: resolve(e.value),
          entryType: 'text' as const,
          enabled: e.enabled,
        })),
    };
  }
  return { mode: body.mode as Body['mode'], content: resolve(body.content) };
}
```

- [ ] **Step 2: Verify build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 3: Commit**

```bash
git add src/lib/execute-request.ts
git commit -m "feat: resolve environment variables in URLs, headers, body, and auth before execution"
```

---

## Feature 3: Collection CRUD + Auto-Save

### Task 9: Add hover action icons to sidebar nodes

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1: Add hover icons to `RequestNode`**

Wrap the `<button>` in a `group` div and add hover icons:

```tsx
function RequestNode({ name, method, collectionName, path }: { ... }) {
  // ... existing handleClick

  return (
    <div className="group relative flex items-center">
      <button
        type="button"
        className="flex items-center gap-1.5 w-full px-2 py-1 text-left text-xs rounded-sm hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring cursor-pointer"
        onClick={handleClick}
        aria-label={`Open ${method} ${name}`}
      >
        <FileText className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        <span className={cn('w-9 shrink-0 font-semibold text-[10px]', methodColor(method))}>
          {method}
        </span>
        <span className="truncate text-foreground">{name}</span>
      </button>
      <div className="absolute right-1 flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
        <button
          type="button"
          className="h-5 w-5 flex items-center justify-center rounded-sm hover:bg-muted text-muted-foreground"
          onClick={(e) => { e.stopPropagation(); /* duplicate handler */ }}
          title="Duplicate"
        >
          <Copy className="h-3 w-3" />
        </button>
        <button
          type="button"
          className="h-5 w-5 flex items-center justify-center rounded-sm hover:bg-muted text-destructive"
          onClick={(e) => { e.stopPropagation(); /* delete handler */ }}
          title="Delete"
        >
          <Trash2 className="h-3 w-3" />
        </button>
      </div>
    </div>
  );
}
```

Add imports: `Copy, Trash2, FolderPlus, Settings` from `lucide-react`.

- [ ] **Step 2: Add hover icons to `FolderNode` and `CollectionNode`**

Apply the same `group` + hover icon pattern:
- `CollectionNode`: Plus (new request), FolderPlus (new folder), Settings
- `FolderNode`: Plus (new request), FolderPlus (new folder)

The icon handlers are wired in Task 10 (context menus) — for now they can be empty click stubs with `e.stopPropagation()`.

- [ ] **Step 3: Verify build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "feat: add hover action icons to sidebar collection/folder/request nodes"
```

---

### Task 10: Add right-click context menus

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1: Add ContextMenu imports**

Add to imports:
```typescript
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
```

- [ ] **Step 2: Wrap `CollectionNode` in ContextMenu**

Wrap the collection's outer `<div>` in a `<ContextMenu>`:

```tsx
<ContextMenu>
  <ContextMenuTrigger asChild>
    <div>{/* existing collection button + children */}</div>
  </ContextMenuTrigger>
  <ContextMenuContent className="w-48">
    <ContextMenuItem onClick={() => handleNewRequest(summary.name, '')}>
      New Request
    </ContextMenuItem>
    <ContextMenuItem onClick={() => handleNewFolder(summary.name, '')}>
      New Folder
    </ContextMenuItem>
    <ContextMenuSeparator />
    <ContextMenuItem onClick={() => startRename()}>
      Rename
    </ContextMenuItem>
    <ContextMenuItem
      className="text-destructive"
      onClick={() => handleDeleteCollection(summary.name)}
    >
      Delete
    </ContextMenuItem>
    <ContextMenuSeparator />
    <ContextMenuItem onClick={() => openSettings(summary.name)}>
      Settings
    </ContextMenuItem>
  </ContextMenuContent>
</ContextMenu>
```

- [ ] **Step 3: Wrap `FolderNode` and `RequestNode` similarly**

`FolderNode` context menu: New Request, New Folder, Rename, Delete.

`RequestNode` context menu: Duplicate, Rename, Move to... (sub-menu), Delete.

For the Move to... sub-menu:
```tsx
<ContextMenuSub>
  <ContextMenuSubTrigger>Move to...</ContextMenuSubTrigger>
  <ContextMenuSubContent className="w-48">
    {summaries.map((s) => (
      <ContextMenuItem
        key={s.name}
        onClick={() => handleMove(collectionName, path, s.name, '')}
        disabled={s.name === collectionName}
      >
        {s.name}
      </ContextMenuItem>
    ))}
  </ContextMenuSubContent>
</ContextMenuSub>
```

Note: The `summaries` list needs to be passed down to `RequestNode` or accessed via a shared context. Simplest approach: pass `summaries` as a prop from `CollectionsSidebar`.

- [ ] **Step 4: Add handler functions**

```typescript
const handleNewRequest = async (collection: string, folderPath: string) => {
  const name = `New Request`;
  const path = folderPath ? `${folderPath}/${name}` : name;
  await saveRequest(collection, path, {
    name,
    method: 'GET',
    url: '',
    headers: [],
    auth: { authType: 'none' },
  });
  // Open as a tab and trigger inline rename.
  const tab: Tab = {
    id: `${collection}/${path}`,
    title: name,
    tabType: 'request',
    request: createDefaultRequest(),
    response: null,
    isDirty: false,
    source: { collection, path },
  };
  usePaneStore.getState().openTab(tab);
  // TODO: trigger inline rename on the new tab title (future enhancement).
};

const handleNewFolder = async (collection: string, folderPath: string) => {
  const name = `New Folder ${Date.now()}`;
  const path = folderPath ? `${folderPath}/${name}` : name;
  await createFolder(collection, path);
};

const handleDeleteCollection = async (name: string) => {
  // Will be wrapped in AlertDialog confirmation in Task 13.
  await deleteCollection(name);
};
```

- [ ] **Step 5: Verify build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 6: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "feat: add right-click context menus for collection/folder/request nodes"
```

---

### Task 11: Add inline rename and delete confirmation

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1: Add inline rename state to `CollectionNode`**

```typescript
const [isRenaming, setIsRenaming] = useState(false);
const [renameValue, setRenameValue] = useState(summary.name);

const handleRename = async () => {
  const trimmed = renameValue.trim();
  if (!trimmed || trimmed === summary.name) { setIsRenaming(false); return; }
  try {
    await renameCollection(summary.name, trimmed);
    setIsRenaming(false);
  } catch (err) {
    console.error('Rename failed:', err);
  }
};
```

When `isRenaming` is true, replace the collection name `<span>` with an `<Input>`:

```tsx
{isRenaming ? (
  <Input
    autoFocus
    className="h-6 text-xs flex-1"
    value={renameValue}
    onChange={(e) => setRenameValue(e.target.value)}
    onKeyDown={(e) => {
      if (e.key === 'Enter') handleRename();
      if (e.key === 'Escape') setIsRenaming(false);
    }}
    onBlur={handleRename}
    onClick={(e) => e.stopPropagation()}
  />
) : (
  <span className="truncate font-medium text-foreground">{summary.name}</span>
)}
```

- [ ] **Step 2: Add delete confirmation with AlertDialog**

Add a shared confirmation state at the `CollectionsSidebar` level:

```typescript
const [deleteTarget, setDeleteTarget] = useState<{
  type: 'collection' | 'folder' | 'request';
  collection: string;
  path?: string;
  name: string;
} | null>(null);

const confirmDelete = async () => {
  if (!deleteTarget) return;
  try {
    if (deleteTarget.type === 'collection') {
      await deleteCollection(deleteTarget.collection);
    } else if (deleteTarget.type === 'folder') {
      await deleteFolder(deleteTarget.collection, deleteTarget.path!);
    } else {
      await deleteRequest(deleteTarget.collection, deleteTarget.path!);
    }
    // Close any open tabs for deleted items.
    const store = usePaneStore.getState();
    const closeTabs = (node: PaneNode): void => {
      if (node.type === 'leaf') {
        for (const tab of node.tabs) {
          if (!tab.source) continue;
          const matches =
            (deleteTarget.type === 'collection' && tab.source.collection === deleteTarget.collection) ||
            (deleteTarget.type === 'request' && tab.source.collection === deleteTarget.collection && tab.source.path === deleteTarget.path) ||
            (deleteTarget.type === 'folder' && tab.source.collection === deleteTarget.collection && tab.source.path.startsWith(deleteTarget.path!));
          if (matches) store.closeTab(tab.id, node.groupId);
        }
      } else {
        closeTabs(node.children[0]);
        closeTabs(node.children[1]);
      }
    };
    closeTabs(store.root);
  } catch (err) {
    console.error('Delete failed:', err);
  }
  setDeleteTarget(null);
};
```

Add the `AlertDialog` at the bottom of the `CollectionsSidebar` return:

```tsx
<AlertDialog open={!!deleteTarget} onOpenChange={(open) => !open && setDeleteTarget(null)}>
  <AlertDialogContent>
    <AlertDialogHeader>
      <AlertDialogTitle>Confirm Delete</AlertDialogTitle>
      <AlertDialogDescription>
        {deleteTarget?.type === 'collection'
          ? `Delete collection '${deleteTarget.name}'? This removes all requests inside it.`
          : deleteTarget?.type === 'folder'
          ? `Delete folder '${deleteTarget?.name}' and all requests inside it?`
          : `Delete request '${deleteTarget?.name}'?`}
      </AlertDialogDescription>
    </AlertDialogHeader>
    <AlertDialogFooter>
      <AlertDialogCancel>Cancel</AlertDialogCancel>
      <AlertDialogAction onClick={confirmDelete}>Delete</AlertDialogAction>
    </AlertDialogFooter>
  </AlertDialogContent>
</AlertDialog>
```

Add imports for `AlertDialog` components and `deleteCollection`, `deleteFolder`, `deleteRequest` from `tauri-api`.

- [ ] **Step 3: Wire context menu delete items to `setDeleteTarget`**

Update each Delete menu item to call `setDeleteTarget(...)` instead of directly deleting.

- [ ] **Step 4: Verify build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 5: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "feat: add inline rename and delete confirmation for sidebar items"
```

---

### Task 12: Add auto-save for collection requests

**Files:**
- Create: `src/lib/auto-save.ts`
- Modify: `src/stores/pane-store.ts`

- [ ] **Step 1: Create the auto-save module**

```typescript
// src/lib/auto-save.ts
import { saveRequest, type Request } from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';

const timers = new Map<string, ReturnType<typeof setTimeout>>();

function toApiRequest(name: string, request: RequestState): Request {
  return {
    name,
    method: request.method,
    url: request.url,
    headers: request.headers
      .filter((h) => h.enabled)
      .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
    body: request.body.mode !== 'none'
      ? { mode: request.body.mode, content: request.body.content }
      : undefined,
    auth: { authType: request.auth.authType } as any,
  };
}

export function scheduleAutoSave(
  tabId: string,
  collection: string,
  path: string,
  title: string,
  request: RequestState,
) {
  cancelAutoSave(tabId);
  const timer = setTimeout(async () => {
    timers.delete(tabId);
    try {
      await saveRequest(collection, path, toApiRequest(title, request));
    } catch (err) {
      console.error('[AutoSave] Failed:', err);
    }
  }, 500);
  timers.set(tabId, timer);
}

export function cancelAutoSave(tabId: string) {
  const existing = timers.get(tabId);
  if (existing) {
    clearTimeout(existing);
    timers.delete(tabId);
  }
}
```

- [ ] **Step 2: Wire auto-save into `pane-store.ts`**

In `updateRequest` action, add auto-save trigger after state update:

```typescript
import { scheduleAutoSave } from '@/lib/auto-save';

// Inside updateRequest:
updateRequest(tabId, patch) {
  const { root } = get();
  const newRoot = updateTabInTree(root, tabId, (tab) => {
    const updatedTab = {
      ...tab,
      request: { ...tab.request, ...patch },
      isDirty: true,
    };
    // Auto-save for collection-owned tabs.
    if (tab.source) {
      scheduleAutoSave(
        tabId,
        tab.source.collection,
        tab.source.path,
        tab.title,
        updatedTab.request,
      );
    }
    return updatedTab;
  });
  set({ root: newRoot });
},
```

Update `auto-save.ts` `scheduleAutoSave` to mark the tab clean after a successful save:

```typescript
export function scheduleAutoSave(
  tabId: string,
  collection: string,
  path: string,
  title: string,
  request: RequestState,
) {
  cancelAutoSave(tabId);
  const timer = setTimeout(async () => {
    timers.delete(tabId);
    try {
      await saveRequest(collection, path, toApiRequest(title, request));
      // Mark tab clean after successful save.
      const { usePaneStore } = await import('@/stores/pane-store');
      usePaneStore.getState().markClean(tabId);
    } catch (err) {
      console.error('[AutoSave] Failed:', err);
    }
  }, 500);
  timers.set(tabId, timer);
}
```

In `closeTab` action, add cleanup:

```typescript
import { cancelAutoSave } from '@/lib/auto-save';

// At the start of closeTab:
closeTab(tabId, groupId) {
  cancelAutoSave(tabId);
  // ... rest of existing closeTab logic
```

- [ ] **Step 3: Verify build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 4: Commit**

```bash
git add src/lib/auto-save.ts src/stores/pane-store.ts
git commit -m "feat: add debounced auto-save for collection-owned requests"
```

---

### Task 13: Add Cmd+S and SaveToCollectionDialog

**Files:**
- Create: `src/components/collections/SaveToCollectionDialog.tsx`
- Modify: `src/hooks/useKeyboardShortcuts.ts`
- Modify: `src/App.tsx`

- [ ] **Step 1: Create SaveToCollectionDialog**

```typescript
// src/components/collections/SaveToCollectionDialog.tsx
import { useState, useEffect, useCallback } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import {
  listCollections,
  saveRequest as saveReq,
  type CollectionSummary,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestState } from '@/types/pane-types';

interface SaveToCollectionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tabId: string;
  title: string;
  request: RequestState;
}

export function SaveToCollectionDialog({
  open,
  onOpenChange,
  tabId,
  title,
  request,
}: SaveToCollectionDialogProps) {
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [selectedCollection, setSelectedCollection] = useState('');
  const [requestName, setRequestName] = useState(title || 'New Request');

  useEffect(() => {
    if (open) {
      void listCollections().then(setCollections);
      setRequestName(title || 'New Request');
    }
  }, [open, title]);

  const handleSave = useCallback(async () => {
    if (!selectedCollection || !requestName.trim()) return;
    try {
      await saveReq(selectedCollection, requestName.trim(), {
        name: requestName.trim(),
        method: request.method,
        url: request.url,
        headers: request.headers
          .filter((h) => h.enabled)
          .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
        body: request.body.mode !== 'none'
          ? { mode: request.body.mode, content: request.body.content }
          : undefined,
        auth: { authType: request.auth.authType } as any,
      });
      // Update the tab to be collection-owned.
      const store = usePaneStore.getState();
      const { root } = store;
      // Find and update the tab's source and type.
      // Using updateTabInTree pattern from pane-store.
      store.updateRequest(tabId, {});
      // Directly set source on the tab via store.
      usePaneStore.setState((state) => {
        const updateTab = (node: any): any => {
          if (node.type === 'leaf') {
            const idx = node.tabs.findIndex((t: any) => t.id === tabId);
            if (idx === -1) return node;
            const tabs = [...node.tabs];
            tabs[idx] = {
              ...tabs[idx],
              tabType: 'request',
              title: requestName.trim(),
              isDirty: false,
              source: { collection: selectedCollection, path: requestName.trim() },
            };
            return { ...node, tabs };
          }
          return {
            ...node,
            children: [updateTab(node.children[0]), updateTab(node.children[1])],
          };
        };
        return { root: updateTab(state.root) };
      });
      onOpenChange(false);
    } catch (err) {
      console.error('[SaveToCollection] Failed:', err);
    }
  }, [selectedCollection, requestName, request, tabId, onOpenChange]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>Save to Collection</DialogTitle>
        </DialogHeader>
        <div className="space-y-3">
          <div>
            <label className="text-xs font-medium text-muted-foreground mb-1 block">
              Request Name
            </label>
            <Input
              className="text-xs h-8"
              value={requestName}
              onChange={(e) => setRequestName(e.target.value)}
            />
          </div>
          <div>
            {/* v1: flat collection list. Folder selection within collections is deferred. */}
            <label className="text-xs font-medium text-muted-foreground mb-1 block">
              Collection
            </label>
            <ScrollArea className="h-[150px] border border-border rounded-md">
              <div className="p-1">
                {collections.map((c) => (
                  <button
                    key={c.name}
                    type="button"
                    onClick={() => setSelectedCollection(c.name)}
                    className={cn(
                      'w-full text-left px-2 py-1.5 text-xs rounded-sm',
                      selectedCollection === c.name
                        ? 'bg-accent text-accent-foreground'
                        : 'hover:bg-muted/60',
                    )}
                  >
                    {c.name}
                  </button>
                ))}
                {collections.length === 0 && (
                  <p className="text-xs text-muted-foreground text-center py-4">
                    No collections. Create one first.
                  </p>
                )}
              </div>
            </ScrollArea>
          </div>
        </div>
        <DialogFooter>
          <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={handleSave}
            disabled={!selectedCollection || !requestName.trim()}
          >
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Wire Cmd+S in `useKeyboardShortcuts.ts`**

Add a Cmd+S handler after the Cmd+Enter block. Since the dialog needs React state, we emit a custom event that `App.tsx` listens for:

```typescript
// Cmd/Ctrl+S — save draft to collection.
if (e.key === 's') {
  e.preventDefault();
  const tab = activeLeaf.tabs.find((t) => t.id === activeLeaf.activeTabId);
  if (tab && tab.tabType === 'draft') {
    window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId: tab.id } }));
  }
  return;
}
```

- [ ] **Step 3: Add dialog to `App.tsx`**

In `App.tsx`, add state and event listener for the save dialog:

```typescript
import { SaveToCollectionDialog } from '@/components/collections/SaveToCollectionDialog';

// Inside App():
const [saveDialogTabId, setSaveDialogTabId] = useState<string | null>(null);

useEffect(() => {
  const handler = (e: Event) => {
    const tabId = (e as CustomEvent).detail.tabId;
    setSaveDialogTabId(tabId);
  };
  window.addEventListener('rocket:save-draft', handler);
  return () => window.removeEventListener('rocket:save-draft', handler);
}, []);

// Find the tab for the dialog:
const saveTab = saveDialogTabId
  ? (() => {
      const found = findTabInTree(root, saveDialogTabId);
      return found?.tab ?? null;
    })()
  : null;
```

Add the dialog at the bottom of the JSX, before the closing `</div>`:
```tsx
{saveTab && (
  <SaveToCollectionDialog
    open={!!saveDialogTabId}
    onOpenChange={(open) => !open && setSaveDialogTabId(null)}
    tabId={saveDialogTabId!}
    title={saveTab.title}
    request={saveTab.request}
  />
)}
```

Add import: `import { findTabInTree } from '@/lib/pane-utils';`

- [ ] **Step 4: Verify build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`

- [ ] **Step 5: Commit**

```bash
git add src/components/collections/SaveToCollectionDialog.tsx src/hooks/useKeyboardShortcuts.ts src/App.tsx
git commit -m "feat: add Cmd+S save-draft-to-collection dialog"
```

---

### Task 14: Final verification

- [ ] **Step 1: Run full TypeScript build**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn build`
Expected: No errors

- [ ] **Step 2: Run Rust workspace check**

Run: `cd /home/numericlabs/data/Rust/Rocket && cargo check --workspace`
Expected: No errors

- [ ] **Step 3: Run existing tests**

Run: `cd /home/numericlabs/data/Rust/Rocket && yarn vitest run`
Expected: All existing tests pass

- [ ] **Step 4: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: address build issues from UX workflows implementation"
```
