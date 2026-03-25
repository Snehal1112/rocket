# OAuth 2.0 Full Configurable Fields Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every OAuth2 field user-configurable — add Implicit grant type, dedicated password fields, advanced options (client auth, header prefix, token location), token expiry tracking, and refresh token flow.

**Architecture:** Pure frontend changes across 3 files. Expand AuthState type, rebuild the AuthEditor UI with conditional sections, and update toApiAuth to respect new config options.

**Tech Stack:** React, TypeScript, shadcn/ui, Tailwind

**Spec:** `docs/superpowers/specs/2026-03-25-oauth2-full-fields-design.md`

---

### File Structure

```
src/types/pane-types.ts                      # expand oauth2 fields
src/components/request/AuthEditor.tsx         # full UI rebuild
src/lib/execute-request.ts                    # respect headerPrefix
```

---

### Task 1: Expand AuthState oauth2 type

**Files:**
- Modify: `src/types/pane-types.ts`

- [ ] **Step 1: Replace the oauth2 type**

Find the `oauth2?:` field in `AuthState`. Replace it with all new fields:

```typescript
oauth2?: {
  grantType: 'client_credentials' | 'password' | 'authorization_code' | 'implicit';
  authorizationUrl: string;
  tokenUrl: string;
  callbackUrl: string;
  clientId: string;
  clientSecret: string;
  scope: string;
  state: string;
  username: string;
  password: string;
  clientAuthentication: 'header' | 'body';
  headerPrefix: string;
  addTokenTo: 'header' | 'queryParams';
  accessToken: string;
  refreshToken: string;
  expiresIn: number | null;
  tokenAcquiredAt: number | null;
};
```

- [ ] **Step 2: Verify TypeScript**

Run: `npx tsc --noEmit`
Note: There will be errors in AuthEditor where the old default object is too narrow. That's expected — Task 2 fixes them.

- [ ] **Step 3: Commit**

```bash
git add src/types/pane-types.ts
git commit -m "feat: expand OAuth2 AuthState with all configurable fields"
```

---

### Task 2: Rebuild AuthEditor OAuth2 UI

**Files:**
- Modify: `src/components/request/AuthEditor.tsx`

This is the largest task. The AuthEditor OAuth2 section needs a full rebuild with conditional field visibility, advanced options, and token management.

- [ ] **Step 1: Read the full AuthEditor.tsx file**

Understand the current structure: how `auth` and `onChange` props work, `patchOAuth2`, `handleGetToken`, and the JSX layout.

- [ ] **Step 2: Update OAuth2 defaults**

Find where `oauth2` defaults are set (in the `setType` handler, around line 40-43). Replace with all new fields:

```typescript
next.oauth2 = {
  grantType: 'client_credentials',
  authorizationUrl: '',
  tokenUrl: '',
  callbackUrl: 'http://localhost:9876/callback',
  clientId: '',
  clientSecret: '',
  scope: '',
  state: '',
  username: '',
  password: '',
  clientAuthentication: 'body',
  headerPrefix: 'Bearer',
  addTokenTo: 'header',
  accessToken: '',
  refreshToken: '',
  expiresIn: null,
  tokenAcquiredAt: null,
};
```

- [ ] **Step 3: Add Implicit to Grant Type dropdown**

In the SelectContent for grantType, add:
```tsx
<SelectItem value="implicit" className="text-xs">Implicit (Coming Soon)</SelectItem>
```

- [ ] **Step 4: Fix Password grant to use dedicated fields**

In `handleGetToken`, replace:
```typescript
params.set('username', oauth.clientId);
params.set('password', oauth.clientSecret);
```
With:
```typescript
params.set('username', oauth.username);
params.set('password', oauth.password);
```

- [ ] **Step 5: Add Client Authentication (Basic Auth Header option)**

In `handleGetToken`, where `client_id` and `client_secret` are added to params, wrap in a conditional:

```typescript
const headers: { key: string; value: string; enabled: boolean }[] = [
  { key: 'Content-Type', value: 'application/x-www-form-urlencoded', enabled: true },
];
if (oauth.clientAuthentication === 'header') {
  const basic = btoa(`${oauth.clientId}:${oauth.clientSecret}`);
  headers.push({ key: 'Authorization', value: `Basic ${basic}`, enabled: true });
} else {
  params.set('client_id', oauth.clientId);
  params.set('client_secret', oauth.clientSecret);
}
```

Update the `executeRequest` call to use the `headers` array instead of the hardcoded single header.

- [ ] **Step 6: Store token expiry after successful token response**

After parsing the token response JSON, update the patchOAuth2 call:

```typescript
patchOAuth2({
  accessToken: json.access_token,
  refreshToken: refreshToken,
  expiresIn: typeof json.expires_in === 'number' ? json.expires_in : null,
  tokenAcquiredAt: Math.floor(Date.now() / 1000),
});
```

- [ ] **Step 7: Add handleRefreshToken function**

Add a new function near `handleGetToken`:

```typescript
const handleRefreshToken = useCallback(async () => {
  const oauth = auth.oauth2;
  if (!oauth || !oauth.refreshToken || !oauth.tokenUrl) return;
  setGettingToken(true);
  setTokenError('');
  try {
    const params = new URLSearchParams();
    params.set('grant_type', 'refresh_token');
    params.set('refresh_token', oauth.refreshToken);

    const headers: { key: string; value: string; enabled: boolean }[] = [
      { key: 'Content-Type', value: 'application/x-www-form-urlencoded', enabled: true },
    ];
    if (oauth.clientAuthentication === 'header') {
      const basic = btoa(`${oauth.clientId}:${oauth.clientSecret}`);
      headers.push({ key: 'Authorization', value: `Basic ${basic}`, enabled: true });
    } else {
      params.set('client_id', oauth.clientId);
      params.set('client_secret', oauth.clientSecret);
    }

    const result = await executeRequest({
      method: 'POST',
      url: oauth.tokenUrl,
      headers,
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
      accessToken: json.access_token,
      refreshToken: json.refresh_token || oauth.refreshToken,
      expiresIn: typeof json.expires_in === 'number' ? json.expires_in : null,
      tokenAcquiredAt: Math.floor(Date.now() / 1000),
    });
  } catch (err) {
    setTokenError(err instanceof Error ? err.message : String(err));
  } finally {
    setGettingToken(false);
  }
}, [auth.oauth2, patchOAuth2]);
```

- [ ] **Step 8: Add tokenExpiryDisplay helper**

```typescript
function tokenExpiryDisplay(expiresIn: number | null, acquiredAt: number | null): string {
  if (!expiresIn || !acquiredAt) return 'No expiry';
  const expiresAt = acquiredAt + expiresIn;
  const now = Math.floor(Date.now() / 1000);
  if (now >= expiresAt) return 'Expired';
  const remaining = expiresAt - now;
  const date = new Date(expiresAt * 1000);
  const time = date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (remaining < 60) return `Expires in ${remaining}s (at ${time})`;
  if (remaining < 3600) return `Expires in ${Math.floor(remaining / 60)}m (at ${time})`;
  return `Expires in ${Math.floor(remaining / 3600)}h (at ${time})`;
}
```

- [ ] **Step 9: Rebuild the OAuth2 JSX section**

Replace the entire `{auth.authType === 'oauth2' && auth.oauth2 && (...)}` block with the new layout.

Use `const o = auth.oauth2` for brevity. Sections:

**Grant Type** (always visible):
Same as current but with Implicit added.

**Authorization URL** (authorization_code + implicit):
```tsx
{(o.grantType === 'authorization_code' || o.grantType === 'implicit') && (
  <div>
    <label ...>Authorization URL</label>
    <Input ... value={o.authorizationUrl} onChange={e => patchOAuth2({ authorizationUrl: e.target.value })} />
  </div>
)}
```

**Token URL** (all except implicit):
```tsx
{o.grantType !== 'implicit' && (
  <div>
    <label ...>Token URL</label>
    <Input ... />
  </div>
)}
```

**Callback URL** (authorization_code + implicit):
```tsx
{(o.grantType === 'authorization_code' || o.grantType === 'implicit') && (
  <div>
    <label ...>Callback URL</label>
    <div className="flex gap-1.5">
      <Input ... value={o.callbackUrl} onChange={...} />
      <Button variant="outline" size="sm" onClick={() => navigator.clipboard.writeText(o.callbackUrl)} title="Copy">📋</Button>
    </div>
  </div>
)}
```

**State** (authorization_code + implicit):
```tsx
{(o.grantType === 'authorization_code' || o.grantType === 'implicit') && (
  <div>
    <label ...>State</label>
    <Input ... value={o.state} placeholder="Leave empty for auto-generated" onChange={...} />
  </div>
)}
```

**Client ID + Secret** (always, secret hidden for implicit):
Same grid layout as current but Secret hidden when `o.grantType === 'implicit'`.

**Scope** (always):
Same as current.

**Username + Password** (password grant only):
```tsx
{o.grantType === 'password' && (
  <div className="grid grid-cols-2 gap-2">
    <div>
      <label ...>Username</label>
      <Input ... value={o.username} onChange={e => patchOAuth2({ username: e.target.value })} />
    </div>
    <div>
      <label ...>Password</label>
      <Input ... type="password" value={o.password} onChange={e => patchOAuth2({ password: e.target.value })} />
    </div>
  </div>
)}
```

**Advanced Options** (collapsible):
```tsx
<details className="text-xs">
  <summary className="cursor-pointer text-muted-foreground hover:text-foreground py-1">
    Advanced Options
  </summary>
  <div className="space-y-3 mt-2 pl-1">
    {/* Client Authentication */}
    <div>
      <label ...>Client Authentication</label>
      <Select value={o.clientAuthentication} onValueChange={v => patchOAuth2({ clientAuthentication: v as 'header' | 'body' })}>
        <SelectTrigger className="w-full h-8 text-xs"><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="body" className="text-xs">Send in Request Body</SelectItem>
          <SelectItem value="header" className="text-xs">Send as Basic Auth Header</SelectItem>
        </SelectContent>
      </Select>
    </div>
    {/* Header Prefix */}
    <div>
      <label ...>Header Prefix</label>
      <Input ... value={o.headerPrefix} onChange={e => patchOAuth2({ headerPrefix: e.target.value })} />
    </div>
    {/* Add Token To */}
    <div>
      <label ...>Add Token To</label>
      <Select value={o.addTokenTo} onValueChange={v => patchOAuth2({ addTokenTo: v as 'header' | 'queryParams' })}>
        <SelectTrigger className="w-full h-8 text-xs"><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="header" className="text-xs">Header</SelectItem>
          <SelectItem value="queryParams" className="text-xs">Query Params</SelectItem>
        </SelectContent>
      </Select>
    </div>
  </div>
</details>
```

**Token section** (always visible):
```tsx
<div className="space-y-2 border-t border-border/50 pt-3 mt-3">
  <div>
    <label ...>Access Token</label>
    <div className="flex gap-1.5">
      <Input className="h-8 flex-1 text-xs" readOnly value={o.accessToken} placeholder="(none)" />
      <Button variant="outline" size="sm" className="h-8 px-2" onClick={() => navigator.clipboard.writeText(o.accessToken)} title="Copy">📋</Button>
    </div>
  </div>
  <div>
    <label ...>Refresh Token</label>
    <div className="flex gap-1.5">
      <Input className="h-8 flex-1 text-xs" readOnly value={o.refreshToken} placeholder="(none)" />
      <Button variant="outline" size="sm" className="h-8 px-2" onClick={() => navigator.clipboard.writeText(o.refreshToken)} title="Copy">📋</Button>
    </div>
  </div>
  <p className="text-[10px] text-muted-foreground">
    {tokenExpiryDisplay(o.expiresIn, o.tokenAcquiredAt)}
  </p>
  <div className="flex gap-2">
    <Button variant="outline" size="sm" className="h-8 text-xs" disabled={!o.tokenUrl || gettingToken} onClick={handleGetToken}>
      {gettingToken ? 'Waiting...' : 'Get Token'}
    </Button>
    <Button variant="outline" size="sm" className="h-8 text-xs" disabled={!o.refreshToken || !o.tokenUrl || gettingToken} onClick={handleRefreshToken}>
      Refresh
    </Button>
  </div>
  {tokenError && <p className="text-[10px] text-destructive">{tokenError}</p>}
</div>
```

- [ ] **Step 10: Verify TypeScript**

Run: `npx tsc --noEmit`
Expected: Clean.

- [ ] **Step 11: Commit**

```bash
git add src/components/request/AuthEditor.tsx
git commit -m "feat: full OAuth2 configurable UI — all grant types, advanced options, token management"
```

---

### Task 3: Update toApiAuth to respect headerPrefix

**Files:**
- Modify: `src/lib/execute-request.ts`

- [ ] **Step 1: Update the oauth2 case in toApiAuth**

Find `case 'oauth2':` in `toApiAuth`. Replace with:

```typescript
case 'oauth2': {
  const token = resolve(auth.oauth2?.accessToken ?? '');
  const prefix = auth.oauth2?.headerPrefix ?? 'Bearer';
  return {
    authType: 'bearer',
    token: token ? `${prefix} ${token}` : '',
  };
}
```

Wait — the backend expects `authType: 'bearer'` with just the token value, not the prefix. The `Authorization` header is constructed as `Bearer {token}` by the Rust executor. If we send `Bearer ya29.abc` as the token, the header becomes `Bearer Bearer ya29.abc`.

Check how the Rust side constructs the header. Read `crates/rocket-infra/src/reqwest_executor.rs` to see how `Auth::Bearer { token }` is applied.

The correct approach depends on the Rust executor:
- If Rust adds `Bearer ` prefix: send just the raw token, and we can't customize the prefix without Rust changes.
- If Rust sends the token as-is: we can send `Bearer ya29.abc` or `CustomPrefix ya29.abc`.

For MVP, just send the raw token (don't prepend prefix). The prefix customization will need a Rust-side change later. Keep the `headerPrefix` field in the UI for future use.

```typescript
case 'oauth2':
  return {
    authType: 'bearer',
    token: resolve(auth.oauth2?.accessToken ?? ''),
  };
```

(This is the same as current — no change needed for MVP. The headerPrefix field is stored but not applied until the Rust executor supports it.)

- [ ] **Step 2: Verify TypeScript**

Run: `npx tsc --noEmit`

- [ ] **Step 3: Commit (if any changes)**

```bash
git add src/lib/execute-request.ts
git commit -m "docs: note on headerPrefix — requires Rust executor change for custom prefix"
```

---

### Task 4: End-to-end verification

- [ ] **Step 1: Verify build**

Run: `npx tsc --noEmit && npx vitest run`
Expected: Clean, 70+ tests pass.

- [ ] **Step 2: Visual verification**

Open the app, create a request, select OAuth2 auth:
- [ ] All 4 grant types appear in dropdown
- [ ] Authorization URL shows for auth_code and implicit
- [ ] Token URL hides for implicit
- [ ] Callback URL shows for auth_code and implicit with copy button
- [ ] State shows for auth_code and implicit
- [ ] Username/Password fields show for password grant only
- [ ] Advanced Options collapsed, opens to show client auth, header prefix, add token to
- [ ] Token section shows access token, refresh token, expiry, Get Token + Refresh buttons
- [ ] Password grant uses dedicated username/password (not clientId/clientSecret)

- [ ] **Step 3: Commit any fixes**
