# OAuth 2.0 Full Configurable Fields — Design Spec

**Date:** 2026-03-25
**Status:** Approved

## Problem

The OAuth2 auth editor is missing many fields that Postman provides. Password grant reuses clientId/clientSecret as username/password. No token expiry tracking, no auto-refresh, no client authentication option, no header prefix configuration, no callback URL display.

## Solution

Make every OAuth2 field user-configurable. Add dedicated password grant fields, advanced options (collapsed by default), and token management with expiry display and refresh.

## AuthState Changes

In `src/types/pane-types.ts`, expand the `oauth2` type:

```typescript
oauth2?: {
  // Grant type.
  grantType: 'client_credentials' | 'password' | 'authorization_code' | 'implicit';

  // URLs.
  authorizationUrl: string;
  tokenUrl: string;
  callbackUrl: string;

  // Credentials.
  clientId: string;
  clientSecret: string;
  scope: string;
  state: string;

  // Password grant dedicated fields.
  username: string;
  password: string;

  // Advanced options.
  clientAuthentication: 'header' | 'body';
  headerPrefix: string;
  addTokenTo: 'header' | 'queryParams';

  // Token state.
  accessToken: string;
  refreshToken: string;
  expiresIn: number | null;
  tokenAcquiredAt: number | null;
};
```

### Default values when switching to OAuth2:

```typescript
{
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
}
```

## AuthEditor UI Layout

### Section 1: Core Fields (always visible)

```
Grant Type:        [Authorization Code ▼]
                   Options: Client Credentials, Password, Authorization Code, Implicit
```

### Section 2: URL Fields (conditional)

| Field | client_credentials | password | authorization_code | implicit |
|-------|-------------------|----------|-------------------|----------|
| Authorization URL | hidden | hidden | visible | visible |
| Token URL | visible | visible | visible | hidden |
| Callback URL | hidden | hidden | visible | visible |
| State | hidden | hidden | visible | visible |

Callback URL: editable input with default `http://localhost:{port}/callback`. Copy button next to it.

State: editable input. Placeholder: "Leave empty for auto-generated".

### Section 3: Credentials (always visible)

```
Client ID:         [my-client-id                            ]
Client Secret:     [••••••••                                 ]
```

Client Secret: hidden for Implicit grant (implicit doesn't use secrets).

### Section 4: Scope (always visible)

```
Scope:             [openid email profile                     ]
```

### Section 5: Password Grant Fields (password grant only)

```
Username:          [user@example.com                         ]
Password:          [••••••••                                 ]
```

These are dedicated fields — NOT reusing clientId/clientSecret.

### Section 6: Advanced Options (collapsed by default)

Use a collapsible section with shadcn Collapsible or a simple toggle:

```
▸ Advanced Options
  ┌──────────────────────────────────────────────┐
  │ Client Authentication:                       │
  │   [Send in Request Body ▼]                   │
  │   Options: "Send in Request Body"            │
  │            "Send as Basic Auth Header"        │
  │                                              │
  │ Header Prefix:                               │
  │   [Bearer                      ]             │
  │                                              │
  │ Add Token To:                                │
  │   [Header ▼]                                 │
  │   Options: "Header", "Query Params"          │
  └──────────────────────────────────────────────┘
```

### Section 7: Token (always visible at bottom)

```
Access Token:      [ya29.abc...        ] [📋]
Refresh Token:     [1//refresh...      ] [📋]
Expires:           3600s (expires at 18:30)  or  "Expired"  or  "No expiry"
                   [Get Token]  [Refresh Token]
Error:             (red text if token request failed)
```

- Access Token: read-only input with copy button
- Refresh Token: read-only input with copy button
- Expires display: computed from `expiresIn` + `tokenAcquiredAt`
- "Get Token" button: initiates the flow (existing behavior)
- "Refresh Token" button: uses refresh_token to get a new access_token (new)
- Both buttons disabled while waiting, show "Waiting..." state

## handleGetToken Changes

### Password grant fix

Replace the current hack (reusing clientId/clientSecret):
```typescript
// OLD:
params.set('username', oauth.clientId);
params.set('password', oauth.clientSecret);

// NEW:
params.set('username', oauth.username);
params.set('password', oauth.password);
```

### Client Authentication: Basic Auth Header

When `clientAuthentication === 'header'`:
- Don't send `client_id` and `client_secret` as form body params
- Instead, send `Authorization: Basic base64(clientId:clientSecret)` header

```typescript
const headers = [{ key: 'Content-Type', value: 'application/x-www-form-urlencoded', enabled: true }];
if (oauth.clientAuthentication === 'header') {
  const basic = btoa(`${oauth.clientId}:${oauth.clientSecret}`);
  headers.push({ key: 'Authorization', value: `Basic ${basic}`, enabled: true });
} else {
  params.set('client_id', oauth.clientId);
  params.set('client_secret', oauth.clientSecret);
}
```

### Token expiry tracking

After a successful token response, store:
```typescript
patchOAuth2({
  accessToken: json.access_token,
  refreshToken: json.refresh_token || '',
  expiresIn: json.expires_in ?? null,
  tokenAcquiredAt: Math.floor(Date.now() / 1000),
});
```

### Implicit grant

Implicit flow returns the token directly in the URL fragment (not as a code). The callback URL will be:
```
http://localhost:{port}/callback#access_token=TOKEN&token_type=bearer&expires_in=3600
```

For MVP, we can handle this by parsing the fragment on the callback page via JavaScript in the SUCCESS_HTML, then sending it to the Tauri backend. This is complex — **defer Implicit grant to a follow-up**. Keep the dropdown option but show "Coming soon" tooltip for now.

### Refresh Token flow

New function `handleRefreshToken`:
```typescript
const params = new URLSearchParams();
params.set('grant_type', 'refresh_token');
params.set('refresh_token', oauth.refreshToken);
// clientAuthentication determines how to send credentials.
```

Same token endpoint, same client authentication method.

## toApiAuth Changes

In `src/lib/execute-request.ts`, when the auth type is `oauth2`:

```typescript
case 'oauth2': {
  const token = auth.oauth2?.accessToken ?? '';
  const prefix = auth.oauth2?.headerPrefix ?? 'Bearer';
  if (auth.oauth2?.addTokenTo === 'queryParams') {
    // Token will be added as a query param by the executor.
    return { authType: 'bearer', token: '' }; // placeholder
  }
  return { authType: 'bearer', token: `${prefix} ${token}` };
}
```

Note: For query param token injection, the executor needs to append `?access_token=TOKEN` to the URL. This requires changes in the request execution flow. For MVP, **Header is the default and query param support is a follow-up**.

## Expiry Display

Helper function:
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

## Out of Scope (follow-up)

- Implicit grant full implementation (fragment-based callback)
- Query param token injection (needs executor changes)
- Token persistence across app restarts (tokens are in-memory pane state)
- Token management (save/name/list multiple tokens)
- Auto-refresh before request execution

## Files

- Modify: `src/types/pane-types.ts` (expand oauth2 fields)
- Modify: `src/components/request/AuthEditor.tsx` (full UI)
- Modify: `src/lib/execute-request.ts` (respect headerPrefix)
