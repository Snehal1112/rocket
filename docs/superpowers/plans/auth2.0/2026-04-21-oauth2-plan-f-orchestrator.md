# OAuth2 Frontend Redesign — Plan F: Orchestrator & Wiring

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the `OAuth2AuthEditor` orchestrator that assembles all sub-components, wire it into the existing `AuthEditor.tsx` (replacing inline OAuth2 rendering), replace JS-based token logic with Tauri command calls, and add auto-fetch/auto-refresh behavior in `execute-request.ts`.

**Architecture:** `OAuth2AuthEditor` renders sections in order, handles `handleGetToken` / `handleRefreshToken` via Tauri commands, and passes state + callbacks to sub-components. `AuthEditor.tsx` delegates to `OAuth2AuthEditor` when `authType === 'oauth2'`. `execute-request.ts` gains pre-request auto-fetch/refresh logic.

**Tech Stack:** React 18, TypeScript, Zustand, shadcn/ui, Tauri IPC

**Spec:** `docs/superpowers/specs/2026-04-21-oauth2-frontend-redesign-design.md`

**Prerequisite:** Plans D + E complete.

---

## File Map

| File | Action | Purpose |
|---|---|---|
| `src/components/request/oauth2/OAuth2AuthEditor.tsx` | Create | Orchestrator — assembles sections, handles token commands |
| `src/components/request/AuthEditor.tsx` | Modify | Replace inline OAuth2 with `<OAuth2AuthEditor>` |
| `src/lib/execute-request.ts` | Modify | Add auto-fetch/refresh, remove old JS token logic |

---

### Task 1: Create OAuth2AuthEditor orchestrator

**Files:**
- Create: `src/components/request/oauth2/OAuth2AuthEditor.tsx`

- [ ] **Step 1: Read the frontend-design skill**

```bash
cat /mnt/skills/public/frontend-design/SKILL.md
```

- [ ] **Step 2: Create the orchestrator component**

`src/components/request/oauth2/OAuth2AuthEditor.tsx`:

This component:
1. Renders Grant Type dropdown at the top
2. Renders all 6 sub-components in order
3. Handles `handleGetToken` and `handleRefreshToken` via Tauri commands
4. Handles "Clear Cache" button (resets token state)
5. Receives resolution context (collection, environmentName, requestPath) for Tauri commands

Key implementation details:
- `handleGetToken` calls `oauth2GetToken` from tauri-api, stores result via `patchOAuth2`, then calls `oauth2DecodeJwt` if `idToken` is returned
- `handleRefreshToken` calls `oauth2RefreshToken`, stores result
- Grant Type dropdown dispatches `patchOAuth2({ grantType })` 
- Action buttons row at the bottom: "Get Access Token", "Clear Cache"

The component should be approximately 200-250 lines. It imports and renders:
- `OAuth2TokenDisplay` (conditional — only when tokens exist)
- `OAuth2ConfigSection`
- `OAuth2TokenSection`
- `OAuth2AdvancedSection`
- `OAuth2SettingsSection`
- `OAuth2AdditionalParams`

```tsx
// Key structure (pseudocode — implement fully):
export function OAuth2AuthEditor({
  oauth2, patchOAuth2, variableContext, onNavigateToSource,
  collection, environmentName, requestPath,
}: OAuth2AuthEditorProps) {
  const [gettingToken, setGettingToken] = useState(false);
  const [tokenError, setTokenError] = useState('');

  const handleGetToken = useCallback(async () => {
    setGettingToken(true);
    setTokenError('');
    try {
      const result = await oauth2GetToken({
        grantType: oauth2.grantType,
        authorizationUrl: oauth2.authorizationUrl || undefined,
        tokenUrl: oauth2.tokenUrl || undefined,
        callbackUrl: oauth2.callbackUrl || undefined,
        clientId: oauth2.clientId,
        clientSecret: oauth2.clientSecret || undefined,
        scope: oauth2.scope || undefined,
        state: oauth2.state || undefined,
        username: oauth2.username || undefined,
        password: oauth2.password || undefined,
        clientAuthentication: oauth2.clientAuthentication,
        usePkce: oauth2.usePkce,
        useSystemBrowser: oauth2.useSystemBrowser,
        verifySsl: oauth2.verifySsl,
        authParams: oauth2.authParams.length ? oauth2.authParams : undefined,
        tokenParams: oauth2.tokenParams.length ? oauth2.tokenParams : undefined,
        refreshParams: oauth2.refreshParams.length ? oauth2.refreshParams : undefined,
        collection,
        environmentName,
        requestPath,
      });
      patchOAuth2({
        accessToken: result.accessToken,
        refreshToken: result.refreshToken || '',
        expiresIn: result.expiresIn ?? null,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
        idToken: result.idToken || '',
        tokenType: result.tokenType || '',
        responseScope: result.scope || '',
      });
      // Decode ID token if present
      if (result.idToken) {
        try {
          const claims = await oauth2DecodeJwt(result.idToken);
          patchOAuth2({ idTokenClaims: claims });
        } catch { /* non-critical */ }
      }
    } catch (err) {
      setTokenError(err instanceof Error ? err.message : String(err));
    } finally {
      setGettingToken(false);
    }
  }, [oauth2, patchOAuth2, collection, environmentName, requestPath]);

  const handleRefreshToken = useCallback(async () => {
    // Similar pattern — calls oauth2RefreshToken
  }, [oauth2, patchOAuth2, collection, environmentName, requestPath]);

  const handleClearCache = useCallback(() => {
    patchOAuth2({
      accessToken: '', refreshToken: '', expiresIn: null,
      tokenAcquiredAt: null, idToken: '', tokenType: '',
      responseScope: '', idTokenClaims: null,
    });
    setTokenError('');
  }, [patchOAuth2]);

  return (
    <div className='space-y-4'>
      {/* Grant Type */}
      {/* Token Display */}
      {/* Configuration */}
      {/* Token */}
      {/* Advanced Settings */}
      {/* Settings */}
      {/* Additional Parameters */}
      {/* Action Buttons + Error */}
    </div>
  );
}
```

- [ ] **Step 3: Verify TypeScript**

```bash
npx tsc --noEmit 2>&1 | head -20
```

- [ ] **Step 4: Commit**

```bash
git add src/components/request/oauth2/OAuth2AuthEditor.tsx
git commit -m "feat: OAuth2AuthEditor orchestrator with Tauri command wiring"
```

---

### Task 2: Wire OAuth2AuthEditor into AuthEditor.tsx

**Files:**
- Modify: `src/components/request/AuthEditor.tsx`

- [ ] **Step 1: Read the current AuthEditor.tsx**

Understand the full file structure. Identify:
- Where the OAuth2 section starts (the `{auth.authType === 'oauth2' && auth.oauth2 && (() => { ... })()}` block)
- Where `handleGetToken` and `handleRefreshToken` are defined
- Where `patchOAuth2` is defined
- What props `AuthEditor` receives

- [ ] **Step 2: Add OAuth2AuthEditor import**

```typescript
import { OAuth2AuthEditor } from '@/components/request/oauth2/OAuth2AuthEditor';
```

- [ ] **Step 3: Add resolution context props to AuthEditorProps**

```typescript
interface AuthEditorProps {
  auth: AuthState;
  onChange: (auth: AuthState) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
  // NEW: resolution context for OAuth2 Tauri commands
  collection?: string;
  environmentName?: string;
  requestPath?: string;
}
```

- [ ] **Step 4: Replace inline OAuth2 rendering**

Find the `{auth.authType === 'oauth2' && auth.oauth2 && (() => { ... })()}` block (this is the large inline IIFE that renders all OAuth2 fields).

Replace it entirely with:

```tsx
{auth.authType === 'oauth2' && auth.oauth2 && (
  <OAuth2AuthEditor
    oauth2={auth.oauth2}
    patchOAuth2={patchOAuth2}
    variableContext={variableContext}
    onNavigateToSource={onNavigateToSource}
    collection={collection}
    environmentName={environmentName}
    requestPath={requestPath}
  />
)}
```

- [ ] **Step 5: Remove old handleGetToken and handleRefreshToken**

Delete the `handleGetToken` and `handleRefreshToken` useCallback blocks from `AuthEditor`. These are now handled inside `OAuth2AuthEditor`.

Also remove the related state: `tokenError`, `gettingToken`, `advancedOpen` — these move to `OAuth2AuthEditor`.

Keep `patchOAuth2` — it's still used by the new component via props.

- [ ] **Step 6: Remove unused imports**

Remove `executeRequest` import (no longer needed in AuthEditor for token acquisition). Remove `oauth2AuthCodeFlow` import if no longer referenced. Keep `ChevronDown`, `ChevronRight` only if used by non-OAuth2 sections.

- [ ] **Step 7: Update RequestPanel to pass resolution context**

Find where `<AuthEditor>` is rendered in `RequestPanel.tsx` (or wherever it's mounted). Add the resolution context props:

```tsx
<AuthEditor
  auth={request.auth}
  onChange={handleAuthChange}
  variableContext={variableContext}
  onNavigateToSource={onNavigateToSource}
  collection={tab.source?.collection}
  environmentName={activeEnvId}
  requestPath={tab.source?.path}
/>
```

Read the file to find the exact prop names and how `tab.source` and `activeEnvId` are accessed in the current code.

- [ ] **Step 8: Verify TypeScript**

```bash
npx tsc --noEmit
```

Expected: Clean compilation.

- [ ] **Step 9: Verify frontend build**

```bash
yarn build
```

- [ ] **Step 10: Commit**

```bash
git add src/components/request/AuthEditor.tsx src/components/request/RequestPanel.tsx
git commit -m "refactor: replace inline OAuth2 rendering with OAuth2AuthEditor component"
```

---

### Task 3: Auto-fetch/refresh in execute-request.ts

**Files:**
- Modify: `src/lib/execute-request.ts`

- [ ] **Step 1: Read the current sendRequest function**

Understand how auth is resolved in `sendRequest`. Find where `toApiAuth` is called and how the auth state flows into the request.

- [ ] **Step 2: Add auto-fetch/refresh logic before request execution**

In `sendRequest`, after resolving the auth but before calling `executeRequest`, add:

```typescript
// Auto-fetch/refresh OAuth2 token if configured.
if (authToResolve.authType === 'oauth2' && authToResolve.oauth2) {
  const oauth = authToResolve.oauth2;
  const tokenExpired = oauth.expiresIn && oauth.tokenAcquiredAt
    ? Math.floor(Date.now() / 1000) >= oauth.tokenAcquiredAt + oauth.expiresIn
    : false;

  let tokenUpdated = false;

  // Auto-refresh: token expired + refresh token available
  if (oauth.autoRefreshToken && tokenExpired && oauth.refreshToken && (oauth.refreshTokenUrl || oauth.tokenUrl)) {
    try {
      const result = await oauth2RefreshToken({
        refreshToken: oauth.refreshToken,
        tokenUrl: oauth.tokenUrl,
        refreshTokenUrl: oauth.refreshTokenUrl || undefined,
        clientId: oauth.clientId,
        clientSecret: oauth.clientSecret || undefined,
        scope: oauth.scope || undefined,
        clientAuthentication: oauth.clientAuthentication,
        verifySsl: oauth.verifySsl,
        refreshParams: oauth.refreshParams.length ? oauth.refreshParams : undefined,
        collection,
        environmentName: envStore.activeEnvId ?? undefined,
        requestPath,
      });
      // Update pane store with new token.
      const { updateRequest } = usePaneStore.getState();
      updateRequest(tabId, {
        auth: {
          ...authToResolve,
          oauth2: {
            ...oauth,
            accessToken: result.accessToken,
            refreshToken: result.refreshToken || oauth.refreshToken,
            expiresIn: result.expiresIn ?? null,
            tokenAcquiredAt: Math.floor(Date.now() / 1000),
            idToken: result.idToken || oauth.idToken,
            tokenType: result.tokenType || oauth.tokenType,
            responseScope: result.scope || oauth.responseScope,
          },
        },
      });
      tokenUpdated = true;
    } catch (err) {
      console.warn('[OAuth2] Auto-refresh failed:', err);
      // Proceed without token — user will see auth error in response.
    }
  }

  // Auto-fetch: no token at all
  if (!tokenUpdated && oauth.autoFetchToken && !oauth.accessToken) {
    try {
      const result = await oauth2GetToken({
        grantType: oauth.grantType,
        authorizationUrl: oauth.authorizationUrl || undefined,
        tokenUrl: oauth.tokenUrl || undefined,
        callbackUrl: oauth.callbackUrl || undefined,
        clientId: oauth.clientId,
        clientSecret: oauth.clientSecret || undefined,
        scope: oauth.scope || undefined,
        state: oauth.state || undefined,
        username: oauth.username || undefined,
        password: oauth.password || undefined,
        clientAuthentication: oauth.clientAuthentication,
        usePkce: oauth.usePkce,
        useSystemBrowser: oauth.useSystemBrowser,
        verifySsl: oauth.verifySsl,
        authParams: oauth.authParams.length ? oauth.authParams : undefined,
        tokenParams: oauth.tokenParams.length ? oauth.tokenParams : undefined,
        collection,
        environmentName: envStore.activeEnvId ?? undefined,
        requestPath,
      });
      const { updateRequest } = usePaneStore.getState();
      updateRequest(tabId, {
        auth: {
          ...authToResolve,
          oauth2: {
            ...oauth,
            accessToken: result.accessToken,
            refreshToken: result.refreshToken || '',
            expiresIn: result.expiresIn ?? null,
            tokenAcquiredAt: Math.floor(Date.now() / 1000),
            idToken: result.idToken || '',
            tokenType: result.tokenType || '',
            responseScope: result.scope || '',
          },
        },
      });
      tokenUpdated = true;
    } catch (err) {
      console.warn('[OAuth2] Auto-fetch failed:', err);
    }
  }

  // Re-read auth from pane store if token was updated.
  if (tokenUpdated) {
    const { root } = usePaneStore.getState();
    const refreshed = findTabInTree(root, tabId);
    if (refreshed?.tab.request) {
      authToResolve = refreshed.tab.request.auth;
    }
  }
}
```

- [ ] **Step 3: Add imports**

Add to the imports in `execute-request.ts`:

```typescript
import { oauth2GetToken, oauth2RefreshToken } from '@/lib/tauri-api';
```

- [ ] **Step 4: Remove old executeRequest-based token logic**

If there's any remaining JS-based token acquisition code in `execute-request.ts` (there shouldn't be after AuthEditor changes, but verify), remove it.

- [ ] **Step 5: Verify TypeScript**

```bash
npx tsc --noEmit
```

- [ ] **Step 6: Run tests**

```bash
npx vitest run
```

Expected: All existing tests pass. The auto-fetch/refresh logic doesn't break existing flows because it only activates when `authType === 'oauth2'` and the specific settings are enabled.

- [ ] **Step 7: Commit**

```bash
git add src/lib/execute-request.ts
git commit -m "feat: auto-fetch and auto-refresh OAuth2 tokens before request execution"
```
